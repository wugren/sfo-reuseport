---
module: sfo-reuseport
submodule:
version: v0.1
status: approved
approved_by: auto-pipeline
approved_at: 2026-06-01T00:00:00+08:00
---

# sfo-reuseport 设计

> 本文件只说明实现设计。完整测试策略保留在 `testing.md` 中。

## 设计范围
### 目标
- 将 v0.1 设计为 library crate，提供 TCP 与 UDP multi-worker socket 服务 API。
- 通过互斥 feature 隔离 tokio、async-std 与 tokio-uring；TCP 公开回调使用当前 runtime 的原生 stream 类型，UDP 公开回调使用 crate 统一 `UdpSocket` 抽象。
- 将实现划分为三层内部模块：
  - 异步运行时抽象。
  - 领域实现抽象层，包含共用业务逻辑以及平台实现接口。
  - 平台具体实现，封装 Linux、macOS、FreeBSD 和 Windows socket 行为差异。
- 定义 worker 生命周期、worker thread runtime、TCP accept、UDP packet 交付、Linux 兼容内部调度、受控 socket 选项和错误语义。
- 定义 `TcpServiceConfig`/`UdpServiceConfig` 的 socket 创建后初始化回调；该回调默认不存在，在底层 socket 创建后、内部 socket 选项和 bind/listen 前同步执行。
- 将公开 service config 拆分为 `TcpServiceConfig` 和 `UdpServiceConfig`；TCP 入口只接受 TCP 配置，UDP/QUIC 入口只接受 UDP 配置，UDP/QUIC 专属 routed packet channel capacity 不进入 TCP 配置类型。
- 定义 `TcpServiceConfig::max_concurrency_per_worker` 和 `UdpServiceConfig::max_concurrency_per_worker`；`TcpServer`、`UdpServer` 和 `QuicServer` 的 handler 型 `serve` 在每个 worker 线程内独立限制已交付 handler future 数量，默认和显式 0 均不限制；permit helper 不维护跨线程共享锁或 waiter 列表。
- 定义 `ServerRuntime` 运行期抽象；TCP/UDP/QUIC-aware UDP server task 通过单协议 `serve` 入口投递到 runtime worker 线程，`serve` 返回对应 server 对象，server 对象负责显式关闭自身 task。
- 定义 `UdpServer::serve_socket` 和 `QuicServer::serve_socket` socket-only serve 入口；该入口创建监听 socket 并在监听 worker 线程通过用户回调交付统一 `UdpSocket` 和该 socket 所属 worker id，不调用数据包 handler。
- 定义默认关闭的 `quinn` feature；启用后在统一 `UdpSocket` 上暴露调用方实现 quinn `AsyncUdpSocket` adapter 所需的非阻塞收发和 readiness 接口，不让本 crate 直接依赖 quinn。
- 定义 `QuicServer` 作为 QUIC-aware UDP 包分配入口；该入口只解析足够的 QUIC header 路由字段来选择 worker，不实现 TLS、handshake、connection 或 stream。
- 定义 fallback 用户态 UDP/QUIC routed packet channel 的有界容量配置；默认容量常量在接口层定义为 4096，Windows 目标调用方可通过 `UdpServiceConfig` 覆盖，非 Windows 目标不暴露该公开容量接口，所有 runtime 后端的 routed packet channel 都必须使用该容量创建有界队列；`TcpServer` 和 `TcpServiceConfig` 不读取、不校验该容量。
- 定义 `QuicCidGenerator`，为上层 QUIC/Quinn adapter 生成符合 `QuicServer` 固定 2 字节 worker index 前缀 layout 的 CID bytes。
- 定义一个 `examples/hyper_static.rs` 示例，用 hyper 在 `TcpServer` 回调中处理 HTTP 静态文件请求，并允许通过命令行参数设置静态文件根目录和监听地址。
- 定义 crates.io 发布所需的 Cargo package 元数据和 package 文件包含边界，不改变 crate 公开 API、依赖或运行行为。

### 非目标
- 不实现协议解析、TLS、连接池、限流、超时、重试或跨 worker 通信。
- 不在常规 TCP/UDP/QUIC 数据处理回调中暴露 worker id，不暴露 raw fd/raw socket escape hatch 或平台特定 reuse-port 细节；socket-only serve 回调按 proposal 例外暴露 socket 所属 worker id。
- 不允许同时启用 tokio、async-std 与 tokio-uring 中的多个 runtime feature。
- 不在设计阶段修改测试策略、测试计划或实现代码。
- 不提供配置文件热加载、外部配置订阅或按协议独立线程池。
- 不把 `QuicServer` 设计成完整 QUIC 协议栈，不引入 quinn、TLS 配置、QUIC connection/stream API 或应用协议处理。
- 不在本 crate 中实现 quinn `AsyncUdpSocket`，不把 quinn 或 quinn-udp 类型放入公开 API；`quinn` feature 只暴露本 crate 自有适配接口。
- 不把 hyper 静态文件能力提升为 library API；示例不提供生产级缓存、压缩、range 请求、目录浏览、TLS 或完整 MIME 数据库。
- 不执行 `cargo publish`，不引入发布自动化脚本，不用 package 元数据承诺 HTTP、TLS 或完整 QUIC server 能力。

## 总体方案
`sfo-reuseport` 公开一个小型 builder/config API。调用方创建 `ServerRuntime`，由 runtime 初始化共享 worker 数，再通过单协议 TCP/UDP/QUIC-aware UDP 入口显式借用该 runtime 投递 server task。公开 API 中每个 server 类型只保留一个同步 `serve(&ServerRuntime, <typed config>, handler)` 方法，其中 TCP 使用 `TcpServiceConfig`，UDP/QUIC 使用 `UdpServiceConfig`；不再提供无 runtime 参数的默认启动入口、`serve_with_runtime` 并列入口或 `add_*_listener` 动态新增入口。`serve` 完成 socket bind 和 worker task 提交后分别返回 `Result<TcpServer, Error>`、`Result<UdpServer, Error>` 或 `Result<QuicServer, Error>`，不在方法内部 await `pending` 或其他永不完成的 lifecycle future；返回的 server 对象持有关闭信号和监听 socket 集合，负责显式关闭自身 server task。

`TcpServiceConfig::max_concurrency_per_worker` 和 `UdpServiceConfig::max_concurrency_per_worker` 是 handler 型 `serve` 的 per-server 配置。值为 `None` 或 `Some(0)` 时不限制并发；值为 `Some(n)` 且 `n > 0` 时，每个 worker 线程为该 server 维护独立 permit 计数，最多允许该 worker 同时运行 `n` 个已交付用户 handler future。该计数不跨 worker 汇总，不创建全局共享池，也不改变 `ServerRuntimeConfig` 的 worker 数量。native per-worker TCP、UDP 和 QUIC-aware UDP listener loop 在执行 accept/recv 前先等待本 worker 的 permit；permit helper 只需要支持每个 worker limit 的单个 listener loop waiter，因此内部不维护 `Mutex` 保护的共享状态或 waiter 列表，可用原子计数和单个 waker 表达可用许可通知。用户态模拟 TCP/UDP/QUIC reuse-port 或路由 loop 已经 accept 连接或 recv 数据包之后才知道目标 worker，因此不能在目标 worker 满载时等待 permit；这些路径需要对目标 worker 执行非阻塞 permit 获取，拿不到 permit 时关闭/丢弃该连接或数据包并继续 accept/recv。permit 可用时才投递 handler。handler future 完成、返回错误或 panic unwind 被 runtime 结束时释放对应 permit。server close/drop 或 runtime shutdown 会唤醒正在等待 permit 的 native listener loop，使其在未取得新工作前退出；已经交付的 handler future 仍不被强制取消。socket-only `serve_socket` 不使用该限制，因为数据读取和应用 handler 生命周期由调用方自行管理。

`UdpServer` 和 `QuicServer` 返回对象还提供获取监听 socket 的公开方法。该方法优先检查调用线程是否是该 server 的监听 worker 线程并拥有正在监听的 socket；若是，则返回本线程 socket。若调用线程没有该 server 的监听 socket，则从该 server 持有的监听 socket 集合中随机选择一个返回。无可用监听 socket、server 已关闭或 socket 无法按当前 runtime feature 安全 clone/share 时返回 `Error`，具体错误变体在实现阶段复用 `Error::Runtime` 或补充不扩大 proposal 的内部错误上下文。

`UdpSocket` 是 crate 公开的统一 UDP socket 抽象，由 core 层提供并在 crate 根 re-export。它内部持有所选 runtime feature 的 UDP socket handle；Linux/BSD reuse-port 路径可以包装每 worker 的真实监听 socket，Windows 或其他 fallback 路径可以包装单个共享 socket 或模拟后端。公开方法至少覆盖应用自读和响应所需的 `recv_from`、`send_to` 和 `local_addr`；调用方不需要按平台或 runtime feature 分支。

`UdpServer::serve_socket(runtime, config, callback)` 和 `QuicServer::serve_socket(runtime, config, callback)` 是 socket-only serve 入口。Rust 不支持按参数数量重载同名 associated function，因此该入口使用 `serve_socket` 命名，归属同一 serve-family。它复用 `UdpServiceConfig` 校验、socket 创建、socket 初始化回调和平台 bind 逻辑，创建后在监听 worker 线程通过 `callback(UdpSocket, usize)` 交付 socket 和该 socket 所属 worker id，并返回对应 `UdpServer` 或 `QuicServer` 生命周期对象。支持 `SO_REUSEPORT` worker socket 的平台为每个 worker 创建监听 socket，并在该 worker 线程交付该 worker 的真实监听 socket和对应 worker id。不支持该能力的平台创建内部接收后端，并向每个 worker 回调交付一个统一 `UdpSocket` 视图和对应 worker id；这些 fallback socket 视图只能接收按内部 Linux 兼容调度或 QUIC worker index 前缀路由属于该 worker 的数据。该入口不调用 UDP/QUIC 数据包 handler；返回后应用在 socket 回调中自行调用 `recv_from` 和协议处理，并可用 worker id 选择 worker-local 状态。`QuicServer::serve_socket` 的 fallback 路径仍使用固定 QUIC worker index 前缀规则隔离 worker 数据，但不把该规则暴露为可配置策略。

`quinn` feature 是默认关闭的公开 API 扩展 feature，不加入默认 feature 集。启用后，`UdpSocket` 额外提供标准库类型表达的 adapter helper：非阻塞 `try_send_to`、`poll_send_ready`、单 buffer `poll_recv_from` 和可选 vectored `poll_recv_from_vectored`。这些方法不引用 quinn crate，也不要求本 crate 依赖 quinn；调用方 adapter 负责把这些方法桥接到具体 quinn 版本的 `AsyncUdpSocket`、`Transmit` 和 `RecvMeta`。`runtime-tokio` 真实监听 socket 路径直接委托 tokio socket readiness 和非阻塞 I/O；fallback `RoutedUdpSocket` 路径从内部 routed packet queue poll 接收，并用内部真实 `std::net::UdpSocket` clone 执行非阻塞发送。`runtime-async-std` 和 `runtime-tokio-uring` 组合必须保持 `quinn` feature 可编译；当对应 runtime socket 不提供无 unsafe、无新增依赖的 readiness/nonblocking 能力时，这些 helper 返回明确 `Unsupported`。该设计保证 `QuicServer::serve_socket` 交付的 tokio 真实 socket 和 routed socket 视图都可通过同一组接口适配 quinn，同时不承诺 routed 路径支持 GSO/GRO/ECN fast path。quinn adapter 的 `max_transmit_segments`、`max_receive_segments` 和 `may_fragment` 能力声明由调用方常量实现，不进入本 crate API。

`QuicServer` handler 型 `serve` 复用 UDP bind、worker runtime 和统一 `UdpSocket` 回调形态，但使用 QUIC-aware worker 选择规则替代普通 UDP worker 选择。所有可解析 DCID 的 QUIC long header 和 short header packet 都从 Destination Connection ID 开头读取固定 2 字节网络字节序 worker index 前缀，并按 `worker_index % worker_count` 选择 worker；Initial/0-RTT 的随机 DCID 也使用该前缀取模作为首包分布规则。这个设计只处理 UDP 包分配；QUIC 协议状态仍由调用方或上层 QUIC crate 管理。为减少上层适配错误，crate 同时提供 `QuicCidGenerator` 生成符合该 layout 的 CID bytes，但不实现 quinn trait 或暴露 quinn 类型。

内部结构按三层组织：

1. `runtime`：对当前 feature 选中的 async runtime 做薄封装，提供类型别名、spawn/block_on/sleep 等最小适配，以及 TCP/UDP socket 从标准 socket 转换到 runtime socket 的入口；`runtime-tokio-uring` 仅在 Linux 上启用 tokio-uring 的 current-thread driver 和原生 net 类型。
2. `core`：领域实现抽象层，持有公开配置、worker 模型、Linux 兼容内部调度、错误类型、TCP/UDP 服务循环、统一 `UdpSocket` 抽象和交付逻辑，以及面向平台层的 trait 接口。
3. `platform`：平台具体实现，负责 bind 前 socket 创建、reuse-port/reuse-address/transparent 等选项设置，以及 Windows 或其他 fallback 用户态模拟所需的收包适配。

公开 API 不暴露上述内部层级。公开类型保留在 crate 根或 `api` 模块中，再 re-export 给调用方；内部模块负责降低实现耦合。

`examples/hyper_static.rs` 是 consumer 示例，不进入 crate 公开 API。示例创建 `ServerRuntime`，通过 `TcpServer::serve(&runtime, config, handler)` 接收 runtime 原生 `TcpStream`，再使用 hyper 的 HTTP/1 connection 服务处理请求。示例参数支持 `--root <path>` 和 `--addr <socket-addr>`；未提供时 `--root` 默认当前目录，`--addr` 默认 `127.0.0.1:8080`。请求路径按 URL path segment 逐段解析并拒绝 `..`、Windows prefix/root、空 NUL 等逃逸静态根目录的输入；目录请求尝试追加 `index.html`；普通文件返回 `200`，缺失路径返回 `404`，非法路径返回 `403`。

发布元数据只修改 `Cargo.toml` 的 package metadata。`description` 使用一句准确描述 multi-worker TCP/UDP reuse-port runtime 的短句；`license` 指向根目录 MIT 许可证；`readme` 指向 `README.md`；`repository` 和 `homepage` 使用 Git remote `https://github.com/wugren/sfo-reuseport`；`documentation` 使用 docs.rs 的 crate 页面 `https://docs.rs/sfo-reuseport`；`keywords` 和 `categories` 选择 crates.io 接受的少量检索项，限定在 async、networking、reuse-port、socket 和 runtime 能力内。为避免发布包包含 Harness 缓存、review 流程产物或本地生成文件，manifest 使用 `include` 显式保留 `src/`、`examples/`、`README.md`、`LICENSE` 和 `Cargo.toml`；验证时允许 Cargo 自动加入 `.cargo_vcs_info.json`、`Cargo.lock` 和规范化 manifest 用的 `Cargo.toml.orig`。

## 简化检查
- 最小充分方案：使用 feature-gated runtime 模块、一个共享 core 层和 cfg-gated platform 层，不引入插件系统或动态分发平台后端；tokio-uring 作为第三个互斥 runtime adapter，而不是新增一套 public server API。
- 复用的既有组件或模式：Rust 标准库 socket 地址类型、feature gating、`cfg(target_os)`、`Arc` 和 async callback future。
- 新增抽象：
  - `runtime` 抽象：隔离 tokio/async-std/tokio-uring 类型、单线程 worker runtime 启动方式和 socket 转换方式。
  - `PlatformSocketOps`：让 core 层不分支平台 syscall 细节。
  - Linux 兼容内部调度函数：统一 TCP/UDP 在没有可用 `SO_REUSEPORT` worker 分配能力时的 worker 选择语义。
  - socket 初始化回调：让调用方在不接管 socket 所有权的前提下设置尚未成为稳定 `SocketOptions` 字段的创建期 socket 参数。
  - server 对象：`TcpServer`、`UdpServer` 和 `QuicServer` 既是类型化 handler serve 入口，也是 serve 返回的生命周期控制对象。
  - 统一 `UdpSocket`：屏蔽 runtime UDP socket、Linux/BSD reuse-port socket 和 Windows/fallback 模拟 socket 的差异，为 handler、`listener_socket` 和 socket-only serve 提供同一公开类型。
  - `QuicServer`：复用 UDP socket handle 但提供独立公开入口，避免把普通 UDP 回调语义和 QUIC-aware 包路由语义混在一个 bool 配置里。
  - `QuicCidGenerator`：把固定 CID layout 的生成逻辑放在 crate 内，避免调用方重复手写 worker index 前缀、长度校验和随机填充。
  - 无新增发布抽象：package metadata 是 Cargo 原生 manifest 字段，不需要额外脚本或配置层。
- 每个新增抽象的必要性：
  - runtime 抽象是互斥 feature 和同形 API 的直接要求；tokio-uring 的 driver 初始化和 socket API 与 tokio 不同，必须由独立 adapter 隔离。
  - 平台接口是跨平台屏蔽 socket 差异的直接要求。
  - Linux 兼容内部调度函数是 fallback 平台可预测 worker 选择的集中点；它不是公开配置项，也不提供用户自定义策略。
  - 不再引入 `BalancedUdpSocket` 公开封装；UDP 回调接收名为 `UdpSocket` 的统一抽象，避免 Linux 直接返回原生 socket 而 Windows/fallback 无法等价返回时泄漏平台差异。
  - socket 初始化回调是 `CHG-socket-init-callback` 的直接公开契约；使用一次性闭包比为每个底层选项新增稳定字段更小，同时仍避免长期 raw socket escape hatch。
  - `ServerRuntime` 是混合协议共享 worker 配置和 server task 投递目标的直接需求；v0.1 不需要单 listener registry 或 listener id 管理面。
  - server 对象是 `serve` 返回类型和显式关闭能力的直接契约；复用 `TcpServer`、`UdpServer`、`QuicServer` 三个公开类型比新增独立 stop handle 更小。
  - `serve_socket` 是 `CHG-udp-quic-listener-serve` 的直接公开契约；由于 Rust 不支持同名重载，独立命名比把 handler 参数做成 enum 或 builder 更简单。该入口通过 socket 回调、socket 所属 worker id 和返回的 server 生命周期对象表达每 worker socket 交付与关闭边界。
  - `quinn` feature 下的 `UdpSocket` adapter helper 是 `CHG-quinn-udp-socket-compat` 的直接公开契约；将适配所需 poll/readiness 能力放在现有统一 socket 上，比新增 quinn 专用 wrapper 更小，也避免本 crate 绑定 quinn trait 版本。
  - `QuicServer` 是 `CHG-quic-routed-udp` 的直接公开契约；独立类型可以清楚表达 QUIC-aware UDP routing，同时保持 `UdpServer` 的裸 UDP 包交付模型不变。
  - hyper 静态文件服务器只作为示例 binary 存在；直接在示例内实现少量参数解析和路径解析即可，不为 crate 增加 HTTP 抽象。

## 当前结构
- `Cargo.toml` 声明 library crate、runtime features 和实现依赖。
- `src/lib.rs` 是公开 library 入口。
- `src/core/`、`src/runtime/` 和 `src/platform/` 分别承载领域逻辑、async runtime 适配和平台 socket 行为。
- `examples/tcp_echo.rs` 是示例 binary，必须使用 `ServerRuntimeConfig` 配置 worker。
- `examples/hyper_static.rs` 是新增示例 binary，必须使用 `ServerRuntime` 和 `TcpServer`，并把 HTTP/静态文件逻辑限制在示例内。
- `Cargo.toml` 还需要声明 crates.io 发布元数据；当前 manifest 已有 name/version/edition/features/dependencies，但缺少 description、license、readme、repository/homepage/documentation、keywords/categories 和明确 package include 边界。

## 模块拆分
这些是 crate 内部 Rust 模块，不是 Harness 直接子模块包。

| 模块 | 类型 | 职责 | 输入 | 输出 | 依赖 | 独立文档 |
|------|------|------|------|------|------|----------|
| `runtime` | internal | 异步运行时抽象，按 feature 暴露当前 runtime 类型与 spawn/转换入口。 | feature、标准 socket、future | runtime 原生 stream/socket、task handle | tokio、async-std 或 tokio-uring | no |
| `core` | internal | 需求实现抽象层，包含配置、worker、TCP/UDP 服务循环、Linux 兼容内部调度、错误、公共业务逻辑和平台 trait。 | 公开配置、回调、platform ops | worker 运行、回调交付、统一错误 | `runtime`、`platform` trait | no |
| `platform` | internal | 平台具体实现的 cfg 分发入口。 | bind 地址、socket 选项、协议类型 | 已配置 socket 或模拟后端 | OS cfg、`socket2`/std | no |
| `platform::unix` | internal | Linux/macOS/FreeBSD 共享 socket 设置基础。 | socket config | 已设置 socket | `socket2` | no |
| `platform::linux` | internal | Linux reuse-port 和 IPv4/IPv6 transparent 细节。 | socket config | Linux socket 设置结果 | `platform::unix` | no |
| `platform::bsd` | internal | macOS/FreeBSD reuse-port 行为封装。 | socket config | BSD socket 设置结果 | `platform::unix` | no |
| `platform::windows` | internal | Windows 用户态模拟路径和 socket 创建。 | socket config | Windows socket 或模拟接收入口 | std/runtime | no |
| none | Harness submodule | 当前 crate 仍由根模块包表示。 | n/a | n/a | n/a | no |

## 大模块子模块决策
当前仓库只有一个小型 Rust crate，v0.1 的 runtime、core 和 platform 分层共享同一公开 crate 边界。它们应作为 crate 内部模块记录在根模块 `design.md` 中，不拆成 Harness 直接子模块包。若未来出现独立协议适配、benchmark harness 或平台专用子项目，再建立直接子模块包。

## Directly Mapped Change Items
| change_id | proposal_id | Design Coverage | Scope Paths | Interface/Boundary Impact | Notes |
|-----------|-------------|-----------------|-------------|---------------------------|-------|
| CHG-runtime-features | P-runtime | `Cargo.toml` features、`runtime` 模块、互斥 compile_error、runtime 原生类型别名。 | `Cargo.toml`、`src/runtime.rs` 或 `src/runtime/`、`src/lib.rs` | 公开回调类型随 feature 变化。 | 默认 `runtime-tokio`。 |
| CHG-tokio-uring-runtime | P-tokio-uring-runtime | 新增 `runtime-tokio-uring` feature、`tokio-uring` 可选依赖、Linux-only cfg 边界、`src/runtime/tokio_uring.rs` adapter、公开 `TcpStream` 类型映射到 tokio-uring net 类型或等价接口、统一 `UdpSocket` 在 tokio-uring feature 下包装 tokio-uring UDP I/O、标准 socket 转换入口和每 worker current-thread tokio-uring driver 启动方式。 | `Cargo.toml`、`src/lib.rs`、`src/runtime/mod.rs`、`src/runtime/tokio_uring.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`examples/` | 第三个互斥 runtime feature；启用后用户 TCP handler 可直接调用 tokio-uring stream API，UDP handler 通过统一 `UdpSocket` 使用等价 UDP I/O；非 Linux 平台编译期拒绝或明确 unsupported。 | 不新增 server API；不允许与 tokio/async-std 同时启用；tokio-uring 非 Linux 可运行支持不属于 v0.1 承诺；不要求 UDP 公开类型直接等于 tokio-uring 原生 socket。 |
| CHG-server-runtime | P-server-runtime | `ServerRuntimeConfig` 持有共享 worker 数，server config 不含 worker 数量或调度策略；`TcpServer`、`UdpServer`、`QuicServer` 单协议入口只接受显式 `&ServerRuntime`，并作为同步 server task 投递方法返回对应 server 对象。 | `src/core/config.rs`、`src/core/server_runtime.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/lib.rs` | 公开运行时入口命名为 `ServerRuntime`；worker 配置从 server config 移到 runtime config；移除无 runtime 参数 `serve` 和 `serve_with_runtime`；`serve` 返回值不是 future，而是 `TcpServer`、`UdpServer` 或 `QuicServer`。 | 不提供每 server 独立 worker 池、隐式默认 runtime 入口或公开 `add_*_listener` API；不让 `serve` 通过 `pending` 挂起。 |
| CHG-service-config-split | P-service-config-split | 新增公开 `TcpServiceConfig` 和 `UdpServiceConfig`；`TcpServer::serve` 接受 `TcpServiceConfig`；`UdpServer::serve`、`UdpServer::serve_socket`、`QuicServer::serve` 和 `QuicServer::serve_socket` 接受 `UdpServiceConfig`；平台 bind helper 通过内部 `SocketConfig` trait 复用 bind address、socket options 和 socket init callback；crate root re-export 两个配置类型且不再 re-export 共用 service config。 | `src/core/config.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/quic.rs`、`src/platform/`、`src/lib.rs`、`tests/unit/`、`tests/integration/`、`examples/` | 公开 API 拆分 TCP 与 UDP/QUIC 配置；`TcpServiceConfig` 不包含 routed packet channel capacity；`UdpServiceConfig` 仅在 Windows 公开 routed packet channel capacity builder/getter，默认容量 4096；共享字段保持相同行为。 | 不保留公共 service config 兼容别名；不把 UDP/QUIC 专属配置暴露给 TCP；不改变 `SocketOptions` 和 `SocketInitCallback` 的语义。 |
| CHG-worker-thread-runtime | P-worker-thread-runtime | `runtime` 模块提供 worker thread 启动入口；每个 worker loop 在独立 OS 线程中初始化并运行单线程 async runtime。 | `src/runtime/`、`src/core/worker.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/dynamic.rs` | worker loop 不再直接依赖调用方当前 runtime 的 `spawn` 代表 worker。 | 不提供 work stealing 或多线程 per-worker runtime。 |
| CHG-worker-model | P-workers | `ServerRuntimeConfig` worker 数量、默认 CPU 数、worker spawn/join、内部 worker id；socket-only serve 回调接收 socket 所属 worker id。 | `src/core/worker.rs`、`src/core/udp.rs`、`src/core/quic.rs` | 常规 TCP/UDP/QUIC handler 回调不包含 worker id；`UdpServer::serve_socket` 和 `QuicServer::serve_socket` 回调包含 worker id。 | worker id 仅用于内部状态和 socket-only 应用自读场景，不进入常规 packet/connection handler。 |
| CHG-tcp-serve | P-tcp | 同步 `TcpServer::serve(&ServerRuntime, TcpServiceConfig, handler) -> Result<TcpServer, Error>` 创建 TCP listener、提交 accept task、每连接 async 回调；task 投递完成后立即返回 `TcpServer` 对象。 | `src/core/tcp.rs`、`src/platform/` | TCP serve API 只通过显式 runtime 入口暴露，调用方不需要 `.await` 才能完成启动，返回对象可显式关闭 TCP server task。 | 回调接收 runtime 原生 `TcpStream`；不保留 `serve_with_runtime`；不在 `serve` 内部使用 `pending`。 |
| CHG-udp-runtime-socket | P-udp | 同步 `UdpServer::serve(&ServerRuntime, UdpServiceConfig, handler) -> Result<UdpServer, Error>` 提交 UDP recv task、交付 packet metadata，并把 crate 统一 `UdpSocket` 交给 handler；不导出 `BalancedUdpSocket`；task 投递完成后立即返回 `UdpServer` 对象；`UdpServer` 可按本线程优先、否则随机的规则获取监听 socket。 | `src/core/udp.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | UDP serve API 只通过显式 runtime 入口暴露，UDP handler 与统一 `UdpSocket` 绑定并隐藏平台差异，调用方不需要 `.await` 才能完成启动，返回对象可显式关闭 UDP server task并获取监听 socket。 | 不保留 `BalancedUdpSocket` 或 `serve_with_runtime`；不要求 UDP 公开类型直接等于各 runtime 或平台原生 socket；实现仍负责保护内部 bind/reuse-port 状态不被配置覆盖；不在 `serve` 内部使用 `pending`。 |
| CHG-udp-quic-listener-serve | P-udp-quic-listener-serve | 新增 `UdpServer::serve_socket(&ServerRuntime, UdpServiceConfig, callback) -> Result<UdpServer, Error>` 和 `QuicServer::serve_socket(&ServerRuntime, UdpServiceConfig, callback) -> Result<QuicServer, Error>`，复用 UDP socket 创建、配置校验和 socket 初始化回调，在监听 worker 线程通过回调交付统一监听 `UdpSocket` 和 socket 所属 worker id，不调用数据包 handler。 | `src/core/udp.rs`、`src/core/quic.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | 新增 UDP/QUIC socket-only serve-family 入口；调用方显式传入 `&ServerRuntime`、`TcpServiceConfig`/`UdpServiceConfig` 和 socket 回调，在每个监听 worker 中获得统一 `UdpSocket` 和 worker id 后自行读取数据；`SO_REUSEPORT` 路径交付每 worker 真实 socket及对应 worker id，Windows/fallback 路径交付按 worker 隔离收包的 socket 视图及对应 worker id，内部机制不暴露。 | 不为 TCP 增加 socket-only serve；不提供公开可配置普通 UDP packet 分发或 QUIC routing 策略；不调用数据包 handler；不提供无 runtime 参数入口；不改变 handler 型 `serve` 签名；不向常规 UDP/QUIC packet handler 传递 worker id。 |
| CHG-linux-compatible-scheduling | P-linux-compatible-scheduling | 删除公开 `DispatchPolicy` 和 dispatcher 配置；`ServerRuntimeConfig` 不包含调度字段；fallback 用户态路径使用内部 Linux 兼容 hash 选择 worker；`QuicServer` 继续使用固定 2 字节 worker index 前缀规则。 | `src/core/config.rs`、`src/core/schedule.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/mod.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | 公开 API 不导出 Dispatcher/DispatchPolicy，不提供 `Auto`、`RoundRobin`、`SrcHash`、`Custom` 或自定义 selector；平台 fallback 行为保持内部实现细节。 | 不提供可配置、load-aware、adaptive 或用户自定义 scheduler。 |
| CHG-platform-behavior | P-platform | `PlatformSocketOps` trait 和 cfg-gated Linux/BSD/Windows 实现。 | `src/platform/` | 平台差异不进入公开 API。 | Windows 走用户态模拟。 |
| CHG-socket-options | P-socket-options | `SocketOptions`、IPv4/IPv6 transparent 能力检查、设置时机和错误分类。 | `src/core/config.rs`、`src/platform/` | 配置层新增受控 socket 选项。 | 不允许覆盖内部 reuse-port/bind 状态。 |
| CHG-socket-init-callback | P-socket-init-callback | `TcpServiceConfig` 和 `UdpServiceConfig` 持有默认 `None` 的 socket 创建后回调；平台层创建 `socket2::Socket` 后、内部选项和 bind/listen 前调用；回调错误转换为 `Error::SocketInitCallback` 并阻止服务启动。 | `src/core/config.rs`、`src/core/error.rs`、`src/platform/`、`src/core/tcp.rs`、`src/core/udp.rs` | 公开配置层新增一次性初始化钩子；不暴露 socket 所有权，不允许回调返回后继续持有可变访问权。 | 回调接收借用的 `socket2::Socket`，可调用 socket2 支持的 setter；跨平台可用性由调用方和底层 OS 负责。 |
| CHG-dynamic-listeners | P-dynamic-listeners | 不保留 `ServerRuntime` 内部 listener registry、`ListenerId`、`ListenerProtocol`、`remove_listener` 和公开 `add_*_listener` 管理面；TCP/UDP/QUIC-aware UDP server task 只通过 `serve` 投递；`serve` 返回的 `TcpServer`、`UdpServer` 或 `QuicServer` 对象提供显式关闭能力。 | `src/core/config.rs`、`src/core/server_runtime.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | 公开 API 不提供 listener 动态新增入口；内部 helper 只需服务 `serve` task 投递、server 对象关闭和 runtime drop 停止边界。 | 不提供独立 stop handle 或按 listener id 删除；已经交付的 handler future 不由 balancer 强制取消。 |
| CHG-server-concurrency-limit | P-server-concurrency-limit | `TcpServiceConfig::max_concurrency_per_worker` 和 `UdpServiceConfig::max_concurrency_per_worker` 配置 handler 型 `TcpServer::serve`、`UdpServer::serve` 和 `QuicServer::serve` 的每 worker handler 并发上限；默认 `None` 和显式 `Some(0)` 均不限制；每个 worker 为每个 server 独立维护 permit，不做全局共享池；listener loop 在 accept/recv 前等待 permit；close/drop 唤醒等待 permit 的 loop 并退出；permit helper 不提供 `Mutex` 保护的跨线程共享状态或多 waiter 队列。 | `src/core/config.rs`、`src/core/concurrency.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/quic.rs`、`tests/unit/`、`tests/integration/` | 公开配置层新增 per-server 字段和 builder；上限不改变 `ServerRuntimeConfig` worker 数量，不向 handler 暴露 worker id；handler 型 serve 达到上限时等待新许可，不主动丢弃连接或数据包；`serve_socket` 不纳入该限制；实现使用每 worker 单 listener waiter 模型，不维护 waiter 列表。 | 不提供请求速率限制、跨 worker 全局并发池、按客户端配额、优先级队列、超时、自适应调度或多等待者公平队列；不改变已交付 handler future 的关闭语义。 |
| CHG-mixed-protocol-workers | P-mixed-protocol-workers | TCP/UDP server task 可通过各自 `serve` 入口投递到同一 `ServerRuntime` 实例并在同一 runtime executor 上运行。 | `src/core/tcp.rs`、`src/core/udp.rs`、`src/runtime/` | 保持混合协议共享 worker，不依赖公开 add listener API。 | 不提供按协议独立线程池或负载感知调度。 |
| CHG-quic-routed-udp | P-quic-routed-udp | 同步 `QuicServer::serve(&ServerRuntime, UdpServiceConfig, handler) -> Result<QuicServer, Error>`、QUIC long/short header DCID 解析、long/short header 统一 DCID 固定 2 字节 worker index 前缀解析、非法 packet 丢弃、Linux reuse-port eBPF selector 的 best-effort worker 预分配、CBPF fallback、用户态稳定 worker 投递 fallback、`QuicServer` 关闭语义和监听 socket 获取语义；task 投递完成后立即返回 `QuicServer` 对象。 | `src/core/udp.rs`、`src/core/quic.rs`、`src/core/mod.rs`、`src/lib.rs`、`src/platform/`、`tests/unit/`、`tests/dv/`、`tests/integration/` | 新增 QUIC-aware UDP routing API；不改变 `UdpServer` 裸 UDP API；`QuicServer` 也只通过显式 runtime 同步 `serve` 暴露；Linux 可用时优先尝试内核 reuse-port eBPF 预分配，eBPF 加载或 attach 失败时退回 CBPF，再失败时退回用户态路由；外部使用者必须为后续 server CID 按固定 CID layout 写入接收握手首包的 worker index 前缀；返回对象可显式关闭并按本线程优先、否则随机的规则获取监听 socket。 | 不实现 TLS、handshake、connection、stream、congestion control 或 quinn 集成；不支持可配置 CID layout；不支持 worker index 超过 16 bit 的 CID 路由前缀；不把 eBPF/CBPF 加载失败暴露为公开 API 或强制启动失败；不保留 `serve_with_runtime`；不在 `serve` 内部使用 `pending`；不使用来源四元组 hash、随机 worker 或连接状态表作为 QUIC 握手 fallback。 |
| CHG-routed-packet-channel-limit | P-routed-packet-channel-limit | `UdpServiceConfig` 在 Windows 目标公开 `routed_packet_channel_capacity` builder/getter；接口层定义 `DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY: usize = 4096`；`UdpServer`/`QuicServer` 入口校验容量大于 0，fallback routed socket 创建时把该容量传入 `routed_packet_channel(capacity)`；async-std 使用 `bounded(capacity)`，tokio 使用 `mpsc::channel(capacity)`，tokio-uring/std fallback 使用 `std::sync::mpsc::sync_channel(capacity)`；满队列时分发 loop 等待发送完成以形成背压，关闭时 sender/receiver 断开使等待退出；`TcpServer` 不读取、不校验该字段。 | `src/core/config.rs`、`src/core/udp.rs`、`src/core/quic.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | Windows 公开配置层新增 routed packet channel 容量 builder/getter 和默认常量；非 Windows 不暴露公开容量接口；默认行为从无界队列变为 4096 容量有界队列；Windows 显式配置可覆盖容量；fallback 路径不再允许 routed packet channel 无界增长；TCP 服务不因该配置失败。 | 不为 TCP 增加 routed packet channel 语义；不为测试夹具 channel 增加容量配置；不改变 runtime executor task submission channel；不引入通用跨 worker channel API；容量必须大于 0，显式 0 只在 UDP/QUIC 服务入口是配置错误。 |
| CHG-quic-cid-generator | P-quic-cid-generator | 新增 `QuicCidGenerator` 公开类型；默认生成 8 字节 CID；允许配置 8..=20 字节 CID 长度；worker index 按网络字节序写入固定 2 字节前缀；剩余字节通过 `getrandom` 从 OS 随机源填充；提供 `new(worker_index)`, `for_worker(worker_index)`, `with_cid_len`, `generate` 和 `generate_into`。 | `Cargo.toml`、`src/core/cid.rs`、`src/core/mod.rs`、`src/lib.rs`、`tests/unit/` | 公开 API 新增 crate 自有 CID byte generator；调用方可把返回 bytes 包装成 quinn 或其他 QUIC 库的 CID 类型；生成结果与 `QuicServer` 固定 worker index 前缀路由一致。 | 不实现 quinn trait，不引入 quinn 依赖，不支持可配置 CID layout、HMAC、防伪、超过 16 bit 的 worker index、CID 生命周期管理或连接状态表。 |
| CHG-quinn-udp-socket-compat | P-quinn-udp-socket-compat | 新增默认关闭的 `quinn` feature；启用该 feature 后，统一 `UdpSocket` 暴露本 crate 自有的非阻塞发送、发送 ready poll、接收 poll 和 vectored 接收 helper，接口只使用标准库和本 crate 类型，`runtime-tokio` 真实 socket 与 fallback routed socket 视图都可用，其他 runtime 组合保持可编译并在不具备 readiness/nonblocking 能力时返回 unsupported。 | `Cargo.toml`、`src/core/udp.rs`、`src/runtime/`、`tests/unit/`、`tests/integration/` | 默认 API 不增加 quinn adapter helper；启用 `quinn` feature 后，调用方可基于同一 `UdpSocket` 为 quinn `AsyncUdpSocket` 编写 adapter；tokio native 和 routed socket 不需要调用方按平台或 native/routed 分支；本 crate 不实现 quinn trait。 | 不把 quinn 或 quinn-udp 加入默认依赖；不在本 crate 中实现 quinn `AsyncUdpSocket`；不暴露 quinn 类型；不承诺 async-std/tokio-uring native socket 提供 readiness/nonblocking quinn adapter fast path；不提供 GSO/GRO/ECN fast path 承诺；不新增能力查询方法。 |
| CHG-hyper-static-example | P-hyper-static-example | `examples/hyper_static.rs` 使用 hyper HTTP/1 connection API 包装 `TcpServer` 交付的 TCP stream；示例解析 `--root <path>` 和 `--addr <socket-addr>`，将 URL path 映射到静态根目录内文件，目录请求尝试 `index.html`，非法路径拒绝，缺失文件返回 404。 | `Cargo.toml`、`examples/hyper_static.rs`、`tests/` 或 harness 可达的示例验证 | 新增示例 binary 和 example-only 依赖；不改变 `src/` 公开 API。 | 依赖允许使用 `hyper`、`hyper-util`、`http-body-util` 和 `bytes`；参数解析用标准库，避免新增 CLI 依赖。 |
| CHG-publish-metadata | P-publish-metadata | `Cargo.toml` `[package]` 增加 description、license、readme、repository、homepage、documentation、keywords、categories、rust-version 和 package include 边界。 | `Cargo.toml` | 只影响 Cargo 发布页面和 package 文件列表；不改变公开 Rust API、feature、依赖解析或运行时行为。 | `license = "MIT"` 对应根目录 `LICENSE`；`repository/homepage = "https://github.com/wugren/sfo-reuseport"`；`documentation = "https://docs.rs/sfo-reuseport"`；`readme = "README.md"`；`rust-version = "1.85"` 匹配 Rust 2024 edition；`include` 保留 `src/**`、`examples/**`、`README.md`、`LICENSE`、`Cargo.toml`，并允许 Cargo 自动加入 `.cargo_vcs_info.json`、`Cargo.lock` 和 `Cargo.toml.orig`。 |

## 实施顺序
| 阶段 | 目标 | 前置条件 | 输出 | 依赖 | 可并行 |
|------|------|----------|------|------|--------|
| 1 | 建立 library crate、features 和 runtime 抽象。 | proposal/design approved，schema-check 与 admission-check 通过。 | `src/lib.rs`、`runtime`、feature gating。 | none | no |
| 2 | 建立公开配置、错误、worker 模型和 Linux 兼容内部调度。 | 阶段 1 | `ServerRuntimeConfig`、`TcpServiceConfig`、`UdpServiceConfig`、`SocketOptions`、worker core、内部 scheduling helper。 | runtime | no |
| 3 | 建立平台接口和 Linux/BSD/Windows 后端骨架。 | 阶段 2 | `PlatformSocketOps` 和 cfg-gated platform modules。 | core config | yes |
| 4 | 实现 TCP 服务路径。 | 阶段 1-3 | TCP bind、accept loop、回调交付、`TcpServer` 返回对象和关闭方法。 | runtime、platform、worker | yes |
| 5 | 实现 UDP 服务路径。 | 阶段 1-3 | 统一 `UdpSocket` handler 参数、packet loop、send/response API 使用方式、`UdpServer` 返回对象、关闭方法、监听 socket 获取方法和回调式 `serve_socket`。 | runtime、platform、内部 scheduling helper | yes |
| 6 | 收敛错误语义、文档注释和示例。 | 阶段 4-5 | 一致的 public API 和 docs。 | all | no |
| 7 | 实现 `ServerRuntime` 和混合协议服务入口。 | 阶段 1-6 | `ServerRuntime`、serve task 投递、server 对象生命周期、runtime 生命周期停止、混合 TCP/UDP 验证。 | tcp、udp、worker、runtime | no |
| 8 | 实现 `QuicServer` QUIC-aware UDP 包路由入口。 | 阶段 1-7 | `QuicServer`、QUIC DCID worker index 前缀解析、跨 worker 稳定投递验证、关闭方法和监听 socket 获取方法。 | udp、worker、runtime | no |
| 9 | 增加 tokio-uring runtime adapter。 | 阶段 1-8，`CHG-tokio-uring-runtime` admission 通过。 | `runtime-tokio-uring` feature、tokio-uring socket 类型映射、Linux cfg 编译边界和 handler API 验证。 | runtime、tcp、udp、platform | no |
| 10 | 增加 quinn UDP socket adapter helper。 | `CHG-quinn-udp-socket-compat` admission 通过，统一 `UdpSocket` 和 `serve_socket` 可用。 | 默认关闭的 `quinn` feature、feature-gated `UdpSocket` poll/readiness helper、tokio native/routed 统一行为，以及其他 runtime 的 unsupported 边界。 | udp、runtime、Cargo feature | no |
| 11 | 增加 hyper 静态文件服务器示例。 | `CHG-hyper-static-example` admission 通过，`TcpServer` 与 `ServerRuntime` 可用。 | `examples/hyper_static.rs`、必要 example 依赖和示例验证。 | tcp、runtime、Cargo example deps | no |
| 12 | 补齐 crates.io 发布元数据。 | `CHG-publish-metadata` admission 通过。 | `Cargo.toml` package metadata 和 include 边界。 | none | yes |
| 13 | 增加每 worker handler 并发数限制。 | `CHG-server-concurrency-limit` admission 通过，`TcpServer`、`UdpServer`、`QuicServer` handler 型 serve 可用。 | `TcpServiceConfig::max_concurrency_per_worker`、`UdpServiceConfig::max_concurrency_per_worker`、每 worker permit helper、native TCP/UDP/QUIC accept/recv 前等待、用户态模拟 TCP/UDP/QUIC 路径非阻塞获取 permit 且满载时丢弃已收工作并继续循环、close/drop 唤醒等待 loop；permit helper 去除 `Mutex` 和 waiter 列表。 | config、concurrency、tcp、udp、quic、runtime close signal | no |
| 14 | 增加 routed packet channel 容量限制。 | `CHG-routed-packet-channel-limit` admission 通过，fallback routed socket 路径可用。 | `DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY`、Windows-only `UdpServiceConfig::routed_packet_channel_capacity`、有界 routed channel 创建和满队列背压。 | config、udp、runtime feature 后端 | no |

## 关键决策
- 使用 compile-time feature 选择 runtime，而不是 runtime trait object。原因是公开 TCP 回调必须包含当前 runtime 的原生 `TcpStream` 类型；UDP 通过统一 `UdpSocket` 抽象在 core 层封装 runtime 差异。
- `runtime-tokio-uring` 使用独立 feature 和独立 adapter，不复用 `runtime-tokio` adapter。每个 worker OS 线程创建并持有一个 `tokio_uring::Runtime`；executor 记录 owner thread id，本线程提交的 local task 直接调用 tokio-uring 的 local spawn，非本线程提交时通过 task channel 投递到 owner thread 后再 local spawn。公开 `TcpStream` 映射到 `tokio_uring::net::TcpStream`，公开 `UdpSocket` 由 core 包装 tokio-uring UDP handle 或等价可 clone handle，保持现有 UDP handler 可复制 socket handle 的调用形态。
- `runtime-tokio-uring` 是 Linux 定向 feature。非 Linux 目标启用该 feature 时，crate 在 `src/lib.rs` 或 `runtime` 模块中通过 `compile_error!` 明确拒绝；这比运行时 `Unsupported` 更早暴露平台边界，也避免非 Linux 构建拉入不可用的 io_uring API。
- tokio-uring adapter 的跨线程投递闭包保持 `Send + 'static` bounds；闭包只携带标准 socket、handler 和控制状态，tokio-uring socket/future 的创建和 poll 必须发生在目标 worker thread runtime 内。tokio-uring handler future 不要求 `Send`，以匹配 tokio-uring 原生 socket 的 current-thread 边界。
- `core` 依赖平台 trait，不直接写 `cfg(target_os)` 分支。原因是 TCP/UDP/worker 业务逻辑应只关注 socket 能力结果，平台差异应集中在 `platform`。
- UDP handler 直接接收 crate 统一 `UdpSocket`。crate 不额外提供 `BalancedUdpSocket` 封装；bind、reuse-port 和 server task 生命周期仍由 `ServerRuntime`、平台 bind 路径、server 对象关闭、返回 socket drop 和 worker shutdown 控制，公开配置不得覆盖这些内部状态。
- `serve_socket` 返回对应 server 生命周期对象并注册 socket 回调 task；回调参数为统一 `UdpSocket` 和该 socket 所属 worker id。关闭该 server 对象会停止后续 socket 读取或使 fallback socket 视图结束。应用需要停止读取时可以结束自身回调 future，或通过返回的 server 对象关闭该 socket-only serve。
- `quinn` feature 不引入 quinn 依赖。`Cargo.toml` 中该 feature 是空 feature 或仅用于启用 cfg-gated API；默认 feature 列表不包含它。适配接口命名保持通用 UDP 语义，避免把 quinn 版本细节写入方法签名。
- 不提供公开 `DispatchPolicy` 或 dispatcher 配置。支持内核 `SO_REUSEPORT` worker 分配的平台优先使用平台路径；没有可用 `SO_REUSEPORT` worker 分配能力的系统使用内部 Linux 兼容调度函数保持公开 API 行为一致。
- Linux 兼容内部调度以连接或数据包的四元组元信息为输入，使用稳定 hash 映射到 `worker_count`；缺少 peer 地址时回退到本地地址和协议类别可用信息。该函数只在 fallback 用户态路径使用，不进入公开 API。
- server task 生命周期由 `serve` 返回的 server 对象和 `ServerRuntime` 共同控制；server 对象关闭时只停止自身 task 的后续 accept/recv，runtime drop 时关闭 worker executor，并通过共享运行状态让不在 worker executor 内的模拟 accept/recv 线程退出。v0.1 不提供按 listener id 删除单个 listener 的公开能力，已经交付给 handler 的 TCP/UDP work item 不由 balancer 强制取消。
- handler 并发限制使用 server task 私有的每 worker permit helper。`TcpServiceConfig::max_concurrency_per_worker` 和 `UdpServiceConfig::max_concurrency_per_worker` 为 `None` 或 `Some(0)` 时不创建有效上限；`Some(n)` 且 `n > 0` 时，native per-worker listener loop 在 accept/recv 前异步等待本 worker 的 permit。用户态模拟 TCP/UDP/QUIC reuse-port 或路由 loop 先 accept/recv 并选择目标 worker，再用非阻塞 permit 获取检查目标 worker 容量；目标 worker 满载时关闭/丢弃该连接或数据包并继续 accept/recv，不等待该 worker 释放 permit。每个 worker limit 只有一个 native listener loop 会等待许可，helper 不维护多等待者队列；实现使用原子 active 计数和单个 waker，释放许可时唤醒该 worker 的等待 loop。取得 permit 后把 permit guard 随 handler future 一起移动，future 完成后 drop guard 释放许可。close/drop 设置 server closed 状态并通过 socket/runtime 关闭或 permit 释放唤醒等待者；等待者观察 closed 后退出 listener loop，不再执行新的 accept/recv。
- routed packet channel 容量限制只作用于 fallback 用户态 UDP/QUIC 分发路径。`DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY` 在公开接口层定义为 4096，并由 `UdpServiceConfig::new` 写入内部容量。Windows 目标公开 `UdpServiceConfig::with_routed_packet_channel_capacity(capacity)` 和 getter 以写入显式容量，非 Windows 目标不暴露这两个公开接口；`capacity == 0` 在 `UdpServer`/`QuicServer` 服务入口校验时返回 `Error::InvalidConfig`；`TcpServer` 和 `TcpServiceConfig` 不读取、不校验该字段。分发 loop 发送到满队列时等待可用容量，不丢弃已经通过路由解析的 packet；server close/drop 导致 active flag 关闭、socket 关闭或 channel 断开后，等待发送返回错误并退出 loop。
- `ServerRuntime` 使用同一份 worker 数执行 TCP 与 UDP server task。每个 worker 是一个独立 OS 线程，线程内由当前 feature 选择的 runtime 初始化一个单线程 async runtime；TCP 与 UDP listener loop 作为 server task 投递到这些 worker thread runtime 上，而不是创建按协议隔离的线程池。
- `QuicServer` 使用统一 worker 选择规则：long header packet 读取 DCID length 和完整 DCID，short header packet 读取首字节后的 DCID；只要 DCID 含有至少 2 字节，就从 DCID 开头读取固定 2 字节 worker index 前缀并计算 `worker_index % worker_count`。Initial/0-RTT 的客户端随机 DCID 因此前 2 字节天然形成首包分布；server CID 回填后的 Initial/Handshake/short packet 也继续按相同前缀回到生成该 CID 的 worker。payload 太短、DCID length 为 0、DCID 超出 payload 边界或缺少完整 2 字节 worker index 前缀时丢弃该 packet，不调用用户 handler。
- 强约束由固定公开契约和运行时拒绝共同形成：上层 QUIC crate 必须在 server CID 的 DCID 开头写入接收 server 侧 Initial fallback 的 worker index 前缀；对端回填该 CID 后，后续 packet 通过该前缀路由回到同一 worker。worker index 使用固定 16-bit 网络字节序编码，超过 16 bit 的 worker index 不支持。`QuicServer` 不提供可配置 layout、来源地址兜底或连接状态表，所有不满足 layout 且不属于 Initial fallback 的 packet 都不会进入用户 handler。
- `QuicCidGenerator` 生成与 `QuicServer` 匹配的 server CID bytes。`QuicCidGenerator::new(worker_index)` 使用默认 8 字节长度并校验 worker index 不超过 `0xffff`；`for_worker(worker_index)` 与 `new` 使用同一边界；`with_cid_len` 只接受 8..=20；`generate` 分配 `Vec<u8>` 并填充，`generate_into` 要求 buffer 长度等于配置长度。生成时 worker index 固定写入前 2 字节，随机数从第三字节开始。随机数来源使用 `getrandom`，失败时返回 `Error::Runtime`。
- `QuicServer` v0.1 在 Linux 上先尝试 best-effort reuse-port eBPF selector：为每个 worker 创建绑定到同一地址的 UDP socket，加载 `BPF_PROG_TYPE_SK_REUSEPORT` 程序并通过 `SO_ATTACH_REUSEPORT_EBPF` 附加到 reuse-port group。eBPF 程序对 long header 和 short header 都读取 QUIC 固定 worker index 前缀并返回 `worker_index % worker_count`，让内核优先把 packet 送到目标 worker socket。eBPF 加载、verifier、权限、attach 或一致性探测失败时不改变公开 API，继续尝试当前 classic BPF selector；CBPF 也失败、平台不支持、socket 组创建失败或 selector 与用户态 fallback 算法不一致时退回可移植用户态稳定分发路径。`SFO_REUSEPORT_DISABLE_QUIC_BPF` 可作为内部测试和故障隔离开关强制走用户态 fallback。worker loop 仍在用户态解析 route key；非法 packet、BPF fallback packet 或未进入目标 worker 的 packet 不调用 handler。
- hyper 静态文件服务器示例使用标准库解析命令行，避免新增 CLI 依赖。路径解析基于 URL path segment，不对请求路径做字符串拼接；示例只从静态根目录读取普通文件或目录下的 `index.html`。Content-Type 可以用少量扩展名映射提供常见值，未知扩展名使用 `application/octet-stream`。
- 发布元数据不增加新依赖。`keywords` 使用 `reuseport`、`socket`、`udp`、`tcp`、`runtime`，`categories` 使用 `network-programming` 和 `asynchronous`。这些字段只用于检索，不作为功能开关或 API 承诺。

## 数据与状态
### 配置类型
- `ServerRuntimeConfig`：包含共享 `workers: WorkerCount`，用于同一 runtime 实例内的所有 server task；不包含调度策略字段。
- `TcpServiceConfig`：包含 TCP 入口的 `bind_addr: SocketAddr`、`socket_options: SocketOptions`、`socket_init_callback: Option<SocketInitCallback>` 和 `max_concurrency_per_worker: Option<usize>`；不包含 worker 数量或 routed packet channel 容量。
- `UdpServiceConfig`：包含 UDP/QUIC 入口的 `bind_addr: SocketAddr`、`socket_options: SocketOptions`、`socket_init_callback: Option<SocketInitCallback>`、`max_concurrency_per_worker: Option<usize>` 和内部 `routed_packet_channel_capacity: usize`；不包含 worker 数量。
- `UdpServiceConfig::routed_packet_channel_capacity`：Windows-only 公开接口，配置 fallback routed socket 每 worker 接收队列容量。默认值来自 `DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY == 4096`；显式值在 `UdpServer`/`QuicServer` 中必须大于 0。该字段只影响 UDP/QUIC fallback routed packet channel，不影响 `TcpServer`、runtime executor task channel、handler 并发 permit 或测试夹具 channel；非 Windows 目标使用默认容量且不暴露公开 builder/getter。
- `TcpServiceConfig::socket_init_callback` / `UdpServiceConfig::socket_init_callback`：`Option<SocketInitCallback>`，默认 `None`。当存在时，平台层在创建 TCP/UDP `socket2::Socket` 后立即调用。该回调必须同步完成，只接收借用，不可替换 socket 或保存可变访问权；返回错误时服务启动失败。
- `TcpServiceConfig::max_concurrency_per_worker` / `UdpServiceConfig::max_concurrency_per_worker`：默认 `None`，表示不限制；显式 `Some(0)` 也表示不限制；`Some(n)` 且 `n > 0` 表示每个 worker 线程最多同时运行该 server 的 `n` 个常规 handler future。该字段只适用于 `TcpServer::serve`、`UdpServer::serve` 和 `QuicServer::serve`，不适用于 `serve_socket`。
- `WorkerCount`：支持 `Default` 和显式正整数。`Default` 在构建服务时解析为 `num_cpus::get()`；显式 0 是配置错误。
- `SocketOptions`：包含 `reuse_address: bool`、`ipv4_transparent: TransparentMode`、`ipv6_transparent: TransparentMode`。后续选项只能通过该受控类型加入。
- `TransparentMode`：`Disabled`、`Required`、`BestEffort`。`Required` 在不支持或无权限时返回错误；`BestEffort` 记录 unsupported/permission-denied 结果但不阻止服务启动。

### Linux 兼容内部调度
- 公开 API 不包含 `DispatchPolicy`、dispatcher 类型、策略枚举、custom selector 或 `ServerRuntimeConfig::with_dispatch`。
- fallback 用户态 TCP/UDP worker 选择由内部函数完成，建议命名为 `linux_reuseport_select(meta, worker_count)` 或等价私有 helper。
- 内部调度输入使用 `PacketMeta` 中的 peer/local socket address 和协议类别；实现必须稳定、确定且不依赖随机种子。
- worker index 为稳定 hash 对 `worker_count` 取模；`worker_count == 0` 仍由 `WorkerCount`/runtime config 验证拒绝。
- `QuicServer` 不使用普通 fallback hash 选择合法 packet，而继续使用固定 2 字节 QUIC worker index 前缀规则；非法 packet 仍丢弃。

### Worker 生命周期
- runtime 启动时先解析 `ServerRuntimeConfig`，再为每个 worker 启动一个 OS 线程和线程内单线程 async runtime；单个 server 启动时由 platform 层创建 socket/backend，并把相关 accept/recv task 投递到 worker。
- worker 内部 id 用于内部调度、任务命名或错误上下文；常规 TCP/UDP/QUIC 数据处理回调不接收 worker id，socket-only serve 回调接收 socket 所属 worker id。
- worker 回调 future 必须是 `Send + 'static`，以便跨线程移动到对应 worker thread runtime；进入 worker 后在该线程的单线程 async runtime 内 poll。若未来支持非 `Send` handler，应另行提案。
- `TcpServer`、`UdpServer` 和 `QuicServer` 对象提供显式关闭方法；关闭方法触发该 server task 的 graceful stop 信号，并关闭或释放相关监听 socket，使后续 accept/recv 退出。
- `UdpServer` 和 `QuicServer` 对象保存监听 socket 集合以及 worker 线程标识到 socket 的映射。获取监听 socket 时，若当前线程 id 命中该 server 的监听 worker 线程映射，则返回该 socket；否则从集合中随机选择一个 socket 返回。随机选择可使用无新增依赖的简单轮转或系统时间/计数器混合，只要公开语义是“不保证固定返回某一 socket”。
- `serve_socket` 使用相同的 UDP bind 和 runtime socket 转换路径，并复用 `UdpServerState` 保存生命周期、任务句柄和可关闭 socket。`SO_REUSEPORT` 路径为每个 worker 创建 socket 并在该 worker 线程以 `(UdpSocket, worker_id)` 调用 socket 回调。fallback 路径由一个内部接收后端读取真实 socket，按 `linux_reuseport_select` 或 QUIC worker index 前缀规则把 packet 投递到每个 worker 对应的统一 `UdpSocket` 视图，并以该视图所属 worker id 调用回调；应用从该视图读取时只能看到本 worker 应接收的数据。
- `ServerRuntime` 启动时不绑定任何 listener；调用方通过 `TcpServer::serve`、`UdpServer::serve` 或 `QuicServer::serve` 投递 server task。
- `ServerRuntime` 内部不保存公开 listener registry；worker executor 内的 listener loop 随对应 server 对象关闭或 worker shutdown 停止，模拟 accept 线程只持有 worker executor handles、server task 运行状态和共享运行状态，不持有 `ServerRuntime` clone。

## 接口与依赖
### 公开接口概要
建议公开接口保持在 crate 根 re-export：

```rust
pub use crate::core::{
    Error, PacketMeta, QuicCidGenerator, QuicServer,
    SocketOptions, ServerRuntime, ServerRuntimeConfig, TcpServer, TcpServiceConfig,
    TransparentMode, UdpServer, UdpServiceConfig, UdpSocket, WorkerCount,
};
pub use crate::runtime::TcpStream;
```

`TcpServiceConfig` / `UdpServiceConfig` socket 创建后回调：

```rust
pub type SocketInitCallback =
    Arc<dyn Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static>;

impl TcpServiceConfig {
    pub fn with_socket_init_callback<F>(self, callback: F) -> Self
    where
        F: Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static;

    pub fn without_socket_init_callback(self) -> Self;

    pub fn with_max_concurrency_per_worker(self, max: usize) -> Self;

    pub fn max_concurrency_per_worker(&self) -> Option<usize>;
}

impl UdpServiceConfig {
    pub fn with_socket_init_callback<F>(self, callback: F) -> Self
    where
        F: Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static;

    pub fn without_socket_init_callback(self) -> Self;

    pub fn with_max_concurrency_per_worker(self, max: usize) -> Self;

    pub fn max_concurrency_per_worker(&self) -> Option<usize>;

    #[cfg(windows)]
    pub fn with_routed_packet_channel_capacity(self, capacity: usize) -> Self;

    #[cfg(windows)]
    pub fn routed_packet_channel_capacity(&self) -> usize;
}
```

回调字段默认是 `None`。`with_socket_init_callback` 将闭包包装为 `Arc`，从而允许每 worker socket 创建路径复用同一回调。回调执行顺序是：创建 `socket2::Socket`，执行用户 socket 初始化回调，执行 crate 内部 `reuse_address`、`reuse_port`、transparent 等必需选项，然后 bind/listen。内部必需选项保留最终控制权，用户回调不得依赖覆盖这些状态。

`with_max_concurrency_per_worker(max)` 将 `max` 写入 `TcpServiceConfig::max_concurrency_per_worker` 或 `UdpServiceConfig::max_concurrency_per_worker`；`max == 0` 表示不限制，getter 返回保存的配置值。实现可在内部把 `None` 与 `Some(0)` 统一归一为无限制，但公开配置语义必须保持二者都不限制。

Windows 目标下，`UdpServiceConfig::with_routed_packet_channel_capacity(capacity)` 将 fallback routed packet channel 容量写入内部容量，默认 getter 返回 4096。非 Windows 目标不暴露这两个公开接口并使用默认容量。`capacity == 0` 不在 builder 中 panic，而是在 `UdpServer`/`QuicServer` 服务入口校验中返回 `Error::InvalidConfig`。`TcpServiceConfig` 不提供该 builder，`TcpServer` 不使用 routed packet channel，因此不会读取或校验该字段。

### 公开代码接口细节
TCP 入口：

```rust
pub struct TcpServer;

impl TcpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: TcpServiceConfig,
        handler: F,
    ) -> Result<TcpServer, Error>
    where
        F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn close(&self) -> Result<(), Error>;
}
```

UDP 入口：

```rust
pub struct UdpServer;
pub struct UdpSocket;

impl UdpSocket {
    pub async fn recv_from(&self, buffer: &mut [u8]) -> Result<(usize, SocketAddr), Error>;
    pub async fn send_to(&self, buffer: &[u8], target: SocketAddr) -> Result<usize, Error>;
    pub fn local_addr(&self) -> Result<SocketAddr, Error>;

    #[cfg(feature = "quinn")]
    pub fn try_send_to(&self, buffer: &[u8], target: SocketAddr) -> std::io::Result<usize>;

    #[cfg(feature = "quinn")]
    pub fn poll_send_ready(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>>;

    #[cfg(feature = "quinn")]
    pub fn poll_recv_from(
        &self,
        cx: &mut std::task::Context<'_>,
        buffer: &mut [u8],
    ) -> std::task::Poll<std::io::Result<(usize, SocketAddr)>>;

    #[cfg(feature = "quinn")]
    pub fn poll_recv_from_vectored(
        &self,
        cx: &mut std::task::Context<'_>,
        buffers: &mut [std::io::IoSliceMut<'_>],
    ) -> std::task::Poll<std::io::Result<(usize, SocketAddr)>>;
}

impl UdpServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: UdpServiceConfig,
        handler: F,
    ) -> Result<UdpServer, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn serve_socket<F, Fut>(
        runtime: &ServerRuntime,
        config: UdpServiceConfig,
        callback: F,
    ) -> Result<UdpServer, Error>
    where
        F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn close(&self) -> Result<(), Error>;

    pub fn listener_socket(&self) -> Result<UdpSocket, Error>;
}
```

QUIC-aware UDP 包分配入口：

```rust
pub struct QuicServer;

impl QuicServer {
    pub fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: UdpServiceConfig,
        handler: F,
    ) -> Result<QuicServer, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn serve_socket<F, Fut>(
        runtime: &ServerRuntime,
        config: UdpServiceConfig,
        callback: F,
    ) -> Result<QuicServer, Error>
    where
        F: Fn(UdpSocket, usize) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn close(&self) -> Result<(), Error>;

    pub fn listener_socket(&self) -> Result<UdpSocket, Error>;
}
```

`TcpServer`、`UdpServer` 和 `QuicServer` 的 `serve` 都借用已启动的 `ServerRuntime`，并且都是同步方法。它们完成配置校验、socket bind 和 worker task 提交后立即返回对应 server 对象；listener loop 在 `ServerRuntime` worker thread runtime 内继续运行，`serve` 内部不得调用 `std::future::pending` 或等价永不完成 future 来占住调用方。accept/recv 等 listener loop 自身的后续异步错误按 listener loop 停止处理；已经交付到独立 handler task 的用户 future 返回错误时只结束该 handler task、释放对应 permit，不把 `serve` 转换为 lifecycle future。这三个类型不再创建自己的默认 runtime，不接受 `ServerRuntimeConfig`，也不提供 `serve_with_runtime`。返回对象的 `close` 方法只关闭对应 server task，不影响同一 `ServerRuntime` 中其他 server task；`close` 后已经交付给 handler 的 future 不被强制取消。

handler 型 `serve` 在 native per-worker listener loop 中按顺序执行：检查 server 是否关闭，等待本 worker 的 concurrency permit，取得 permit 后执行 accept/recv，随后把 permit guard 放入用户 handler future 的 wrapper 中。用户态模拟 TCP/UDP/QUIC reuse-port 或路由 loop 的顺序不同：先 accept/recv，按 Linux 兼容调度或 QUIC worker index 前缀选择目标 worker，再尝试非阻塞获取目标 worker permit；若目标 worker 已达上限，关闭/丢弃该连接或数据包并继续 accept/recv，不等待新许可。每个 permit helper 只记录一个等待 waker，不提供多等待者公平队列。native listener 等待 permit 时若 server close/drop 或 runtime shutdown 发生，等待 loop 在被 socket/runtime 关闭或下一次许可释放唤醒后观察关闭状态并退出，不再执行 accept/recv。`serve_socket` 不等待该 permit。

`UdpServer` 和 `QuicServer` 的 handler 接收 UDP packet 级别参数和 crate 统一 `UdpSocket`，便于用户用同一 API 发送响应或自行读取；公开 API 不导出 `BalancedUdpSocket`。`UdpServer::listener_socket` 和 `QuicServer::listener_socket` 返回当前可用于发送响应的监听 socket：当前线程若是该 server 的监听 worker 线程并拥有监听 socket，则返回本线程 socket；否则从该 server 的监听 socket 集合中随机返回一个。关闭后调用或没有可用 socket 时返回 `Error`。`UdpServer::serve_socket` 和 `QuicServer::serve_socket` 通过回调交付统一监听 `UdpSocket` 和 socket 所属 worker id，并返回 server 生命周期对象，不调用数据包 handler。`QuicServer` 不暴露 TLS、ALPN、QUIC transport config、connection 或 stream API。上层必须在 server CID 开头生成满足固定 worker index 前缀 layout 的 bytes；缺少完整 2 字节 DCID 前缀的 packet 被 handler 型 `QuicServer::serve` 和 fallback socket-only 后端视为不可路由；native `SO_REUSEPORT` socket-only 路径依赖系统 socket 分配，不额外解析或丢弃 packet。

QUIC CID generator:

```rust
pub struct QuicCidGenerator;

impl QuicCidGenerator {
    pub const DEFAULT_CID_LEN: usize = 8;
    pub const MIN_CID_LEN: usize = 8;
    pub const MAX_CID_LEN: usize = 20;

    pub fn new(worker_index: usize) -> Result<Self, Error>;
    pub fn for_worker(worker_index: usize) -> Result<Self, Error>;
    pub fn with_cid_len(self, cid_len: usize) -> Result<Self, Error>;
    pub fn worker_index(&self) -> usize;
    pub fn cid_len(&self) -> usize;
    pub fn generate(&self) -> Result<Vec<u8>, Error>;
    pub fn generate_into(&self, buffer: &mut [u8]) -> Result<(), Error>;
}
```

`quinn` feature 下的 `UdpSocket` helper 语义：
- `try_send_to` 必须是非阻塞发送；底层暂不可写时返回 `io::ErrorKind::WouldBlock`，不等待。
- `poll_send_ready` 在底层或 routed sender 可能可写时返回 `Ready(Ok(()))`；routed 视图可用保守实现返回 ready，tokio 真实 runtime socket 应委托 runtime readiness。async-std/tokio-uring 原生 socket 如果无法提供该语义，返回 `Unsupported`。
- `poll_recv_from` 在没有 packet 时注册当前 waker 并返回 `Pending`；tokio 真实 socket 委托 runtime recv readiness，routed 视图 poll 内部 routed packet receiver。async-std/tokio-uring 原生 socket 如果无法提供完整语义，返回 `Unsupported` 或保持编译可用的明确错误。
- `poll_recv_from_vectored` 可先作为单 datagram helper 实现：把一个 UDP datagram 写入提供的 buffer 切片序列，返回 datagram 长度和 peer 地址。当前 proposal 不要求 GRO segment 批量返回。
- `local_addr` 已是既有公开方法，不受 `quinn` feature gate 影响。

Linux QUIC reuse-port selector 内部接口：

```rust
pub(crate) fn bind_quic_udp_reuseport_workers(
    config: &UdpServiceConfig,
    workers: usize,
) -> Result<Option<Vec<std::net::UdpSocket>>, Error>;
```

该接口只在平台层内部使用。返回 `Ok(Some(_))` 表示 Linux reuse-port eBPF 或 CBPF selector 已附加并可由每 worker socket 接收；返回 `Ok(None)` 表示当前平台或当前内核/socket 组合不可用，调用方必须退回用户态 QuicServer 路由；返回 `Err(_)` 只用于普通 UDP bind 级别的不可恢复错误。BPF selector 不成为公开配置项，不引入新 crate feature。eBPF 实现使用手写 Linux syscall 和内核 BPF 指令，不新增 `aya`、`libbpf-rs` 或构建期 C 工具链；所有 eBPF 加载、verifier 和权限错误只影响最佳性能路径，不改变公开 API。

运行期服务入口：

```rust
pub struct ServerRuntime;
pub struct ServerRuntimeConfig;
impl ServerRuntime {
    pub fn start(config: ServerRuntimeConfig) -> Result<Self, Error>;
}
```

`ServerRuntime` 不公开 `add_tcp_listener`、`add_udp_listener`、`add_quic_listener` 或 `remove_listener`。TCP/UDP/QUIC-aware UDP server task 只能通过 `TcpServer::serve`、`UdpServer::serve` 和 `QuicServer::serve` 的单协议入口投递；实现可保留私有 helper 复用 bind 和 worker 投递逻辑，但不得保留公开 listener registry 或公开 listener id 管理面。常规 handler 签名继续不包含 worker id，socket-only serve 回调按其公开契约接收 worker id。`TcpServiceConfig` 和 `UdpServiceConfig` 都不提供 `with_workers` 或 worker 字段；共享 worker 数只能通过 `ServerRuntimeConfig` 设置。

错误类型：
- `Error::InvalidConfig`
- `Error::UnsupportedPlatformOption`
- `Error::PermissionDenied`
- `Error::SocketInitCallback`
- `Error::Socket`
- `Error::Runtime`
- `Error::Handler`

错误应保留源错误信息，但公开枚举不得要求调用方按平台分支。

### 平台接口
core 层使用如下内部 trait：

```rust
pub(crate) trait PlatformSocketOps {
    fn bind_tcp(config: &TcpServiceConfig) -> Result<PlatformTcpBackend, Error>;
    fn bind_udp(config: &UdpServiceConfig) -> Result<PlatformUdpBackend, Error>;
    fn capabilities() -> PlatformCapabilities;
}
```

`PlatformTcpBackend` 与 `PlatformUdpBackend` 表示平台创建后的后端：
- Unix reuse-port 路径可以返回每 worker 一个 listener/socket。
- Windows 模拟路径可以返回一个共享接收入口；core 层使用 Linux 兼容内部调度把工作交付到目标 worker。
- core 层只消费后端枚举暴露的统一方法，不依赖 OS cfg。

### 依赖接口和外部约束
- `tokio`：仅在 `runtime-tokio` feature 下启用，需要 net、rt、macros 或等价最小 feature；worker thread 使用 `tokio::runtime::Builder::new_current_thread().enable_all()`。
- `async-std`：仅在 `runtime-async-std` feature 下启用，需要 async net/task 能力。
- `tokio-uring`：仅在 `runtime-tokio-uring` feature 下启用，目标平台必须是 Linux；worker thread 创建并持有 `tokio_uring::Runtime` 作为 current-thread io_uring driver。标准 TCP/UDP socket 转换入口必须先设置 nonblocking 或采用 tokio-uring 支持的 from-std 路径；若 tokio-uring 无法接管已绑定 socket，implementation 必须在 adapter、统一 `UdpSocket` 或 platform 层调整创建顺序，但不得改变公开 server API。
- `quinn`：crate feature 名称只用于打开 `UdpSocket` 适配 helper。该 feature 不要求引入 quinn 依赖；若测试阶段需要编译一个外部 adapter，可作为 dev-dependency 或 compile fixture 处理，不能进入默认依赖或公开类型签名。
- `socket2`：用于 bind 前创建 socket 和设置 reuse-address/reuse-port/transparent 等选项。
- `getrandom`：用于 `QuicCidGenerator` 从 OS 随机源填充 CID 的随机部分；不用于调度 hash 或任何可复现 worker 选择。
- `num_cpus`：用于默认 worker 数。
- `hyper`：仅作为 `examples/hyper_static.rs` 示例依赖，用于 HTTP/1 server connection 和 request/response 类型。
- `hyper-util`：仅作为示例依赖，用于把 runtime TCP stream 适配为 hyper IO。
- `http-body-util` 和 `bytes`：仅作为示例依赖，用于构造固定响应 body。
- 新依赖只在实现阶段按此设计加入；若发现需要额外依赖，必须先更新 design。
- `socket2` 已是平台 socket 设置依赖；公开回调类型可以引用 `socket2::Socket`，不需要新增依赖。
- 发布元数据不改变依赖接口。Cargo package 验证使用 `cargo package --list --allow-dirty` 或等价命令查看发布文件列表；不执行实际发布。

## 实现布局
```text
sfo-reuseport
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── tokio.rs
│   │   ├── async_std.rs
│   │   └── tokio_uring.rs
│   ├── core/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── schedule.rs
│   │   ├── dynamic.rs
│   │   ├── error.rs
│   │   ├── tcp.rs
│   │   ├── udp.rs
│   │   └── worker.rs
│   └── platform/
│       ├── mod.rs
│       ├── unix.rs
│       ├── linux.rs
│       ├── bsd.rs
│       └── windows.rs
├── examples/
│   ├── tcp_echo.rs
│   └── hyper_static.rs
├── docs/
└── harness/
```

| 路径 | 类型 | 职责 | 备注 |
|------|------|------|------|
| `src/lib.rs` | Rust library entry | 公开 API re-export 和 feature 互斥检查 | 新增 |
| `src/runtime/` | Rust module | 异步运行时抽象 | feature-gated |
| `src/core/` | Rust module | 共用业务逻辑、worker、TCP/UDP/QUIC-aware UDP serve、Linux 兼容内部调度和错误 | 无平台 syscall |
| `src/platform/` | Rust module | 平台 socket 行为 | cfg-gated |
| `examples/tcp_echo.rs` | Rust example | 示例 TCP echo server | 使用 `ServerRuntimeConfig` 配置 worker |
| `examples/hyper_static.rs` | Rust example | hyper HTTP 静态文件服务器示例 | 参数支持 `--root <path>` 和 `--addr <socket-addr>`，不改变 library API |

## 文档索引
| 文档 | 主题 | 范围 |
|------|------|------|
| `design.md` | 模块概览和 v0.1 实现设计 | 整个模块 |
| `proposal.md` | 需求、范围、成功标准和 change_id | 整个模块 |
| `testing.md` | 测试策略 | 整个模块，待 downstream 更新 |
| `testplan.yaml` | 机器可读测试计划 | 整个模块，待 downstream 更新 |

## 风险与回滚
- runtime 类型泄漏风险：通过 feature-gated module 和 compile_error 限制。回滚时先保留单 runtime tokio 路径，再恢复 async-std。
- tokio-uring 平台边界风险：通过 Linux-only compile_error 和 feature 编译矩阵验证控制。回滚时删除 `runtime-tokio-uring` feature、依赖和 adapter，不改变 tokio/async-std API。
- tokio-uring socket 转换风险：如果 tokio-uring 版本缺少某个 from-std 接口，implementation 必须优先保持公开 handler 类型和 `ServerRuntime` API 不变，在 runtime/platform 内部调整创建路径；无法保持时返回 design 阶段，不在实现中临时改公开契约。
- 平台行为偏差风险：平台层必须把 unsupported/permission-denied 转成统一错误。回滚时可关闭特定平台选项，不改变公开配置类型。
- Windows 或其他 fallback 模拟与 Linux reuse-port 差异风险：内部 Linux 兼容调度必须稳定定义 hash 输入。回滚时可暂时将对应 fallback 平台标记为 unsupported，但这需要 proposal 更新。
- 统一 `UdpSocket` 状态边界风险：handler 和 socket-only serve 调用方可使用统一 socket 的公开能力，crate 必须在创建和投递 server task 或返回 socket 前完成内部 bind/reuse-port 状态设置，并通过配置 API 禁止覆盖 balancer 必需状态；发现 `BalancedUdpSocket` re-export 或旧封装残留时应先移除旧公开符号。
- quinn adapter helper 风险：如果接口直接使用 quinn 类型，会把本 crate 绑定到 quinn 版本；本设计要求只使用标准库和本 crate 类型。routed fallback 路径不承诺 GSO/GRO/ECN，调用方 adapter 需要用 single-segment 常量能力声明。
- 内部调度偏差风险：hash 输入或取模行为变化会影响 worker 亲和性；回滚时恢复上一版私有调度函数即可，不改变公开 API。
- server 对象关闭风险：返回对象关闭必须只停止对应 server task，不能误停同一 runtime 中的其他 server task；runtime drop 必须关闭 worker executor，模拟 accept/recv 线程不得持有 `ServerRuntime` clone，并应通过共享运行状态退出。
- handler 并发限制风险：permit 必须随 handler future 完成释放，包括返回错误路径；等待 permit 的 native listener loop 必须能在 server close/drop 后退出，避免关闭永久等待。由于 helper 只支持单 listener waiter，后续实现不得让多个 independent acquire future 同时等待同一个 worker limit；用户态模拟 TCP/UDP/QUIC loop 必须使用非阻塞 permit 获取，避免满载 worker 阻塞整个 accept/recv loop。若需要多等待者公平性，必须返回 proposal/design 扩展范围。回滚时可将 `max_concurrency_per_worker` 保留为无效配置并在 design/implementation 回退后恢复不限行为，但不能改变字段语义为全局池。
- UDP/QUIC 监听 socket 获取风险：`listener_socket` 必须优先返回本监听线程 socket，并在非监听线程随机返回同一 server 的 socket；关闭后或无 socket 时不得返回失效 socket。fallback socket-only 入口还必须保证每个 worker 的 socket 视图只接收本 worker 应收数据，不能把全量共享 socket 直接交给每个 worker。
- 混合协议共享 runtime 风险：handler future 必须 `Send + 'static`；若某协议 handler 长时间占用 runtime 线程，调用方需要自行控制 handler 行为，v0.1 不实现负载感知调度。
- `QuicServer` 命名风险：公开文档和测试必须证明它只提供 QUIC-aware UDP routing，不提供完整 QUIC server。回滚时可移除 `QuicServer` re-export 和入口，不影响 `UdpServer`。
- QUIC 路由键解析风险：来自网络的 packet 必须做长度检查，非法、缺失或短于声明的 worker index 前缀长度时丢弃，不得 panic 或调用 handler。eBPF/CBPF selector 仅作为 Linux 内核预分配优化；用户态 worker loop 仍保留 route key 校验，确保 verifier、权限或 selector attach 差异不会改变非法 packet 丢弃语义。
- 静态文件示例路径逃逸风险：示例必须拒绝 `..`、绝对路径、Windows prefix/root 和 NUL 输入，并在最终 canonicalize 后确认目标路径仍位于静态根目录内。回滚时可删除 `examples/hyper_static.rs` 和对应 example 依赖，不影响 library crate。
- 发布元数据风险：URL、keywords 或 categories 不准确会误导 crates.io 使用者。通过使用 git remote、README/LICENSE 和 crates.io 支持的通用分类降低风险；回滚时删除新增 package metadata/include 字段即可，不影响代码。

## 下游跟进
| follow_up_id | 归属阶段 | 原因 | 触发设计项 | 阻塞 |
|--------------|----------|------|------------|------|
| FU-DESIGN-001 | testing | 实现完成后，testing 阶段需要根据 proposal、design 和 delivered code 生成或更新测试实现、`testing.md` 与 `testplan.yaml`。 | 直接映射变更项 | yes |
| FU-DESIGN-002 | implementation | 实现前需通过 schema-check 和 admission-check，且只能修改已准入 `change_id` 对应生产代码或必要非测试运行资源。 | 全部设计项 | yes |
| FU-DESIGN-004 | testing | 为 `CHG-dynamic-listeners` 和 `CHG-mixed-protocol-workers` 更新测试策略与 testplan 条目，覆盖 `serve` 返回 server 对象、对象关闭能力、listener 动态管理 API 不公开以及混合协议 worker 仍可通过 `serve` 使用。 | server 对象 API 表面和混合协议 worker | yes |
| FU-DESIGN-005 | testing | 为 `CHG-server-runtime` 增加直接验证，覆盖 `ServerRuntime` 命名、共享 worker 配置、server config 不含 worker 设置，`TcpServer`/`UdpServer`/`QuicServer` 不存在 `serve_with_runtime` 或隐式默认 runtime 入口，以及 `serve` 返回对应 server 对象。 | `ServerRuntime` runtime 抽象 | yes |
| FU-DESIGN-006 | testing | 为 `CHG-worker-thread-runtime` 增加验证，覆盖 worker thread 启动入口和不使用调用方 runtime spawn 代表 worker。 | worker thread runtime | yes |
| FU-DESIGN-007 | testing | 为 `CHG-socket-init-callback` 增加验证，覆盖默认 `None`、TCP/UDP 创建路径调用、错误传播和内部必需 socket 选项边界。 | socket init callback | yes |
| FU-DESIGN-008 | testing | 为 `CHG-quic-routed-udp` 增加验证，覆盖 QUIC Initial/0-RTT DCID 前缀取模、DCID 固定 2 字节 worker index 前缀解析、server-generated CID 前缀路由一致、非法 packet 丢弃、不暴露 QUIC 协议栈 API、`QuicServer` 关闭和监听 socket 获取规则。 | QuicServer UDP routing | yes |
| FU-DESIGN-020 | implementation | 在 `CHG-quic-routed-udp` admission 通过后，在用户态路由和 Linux reuse-port eBPF/CBPF selector 中同步实现 long/short header 统一固定 2 字节 worker index 前缀路由。 | QuicServer UDP routing | yes |
| FU-DESIGN-021 | testing | 为 `CHG-quic-cid-generator` 增加验证，覆盖默认 CID 长度、固定 2 字节 worker index 前缀、随机部分变化、长度边界、worker index 边界和 crate 根 re-export。 | QUIC CID generator | yes |
| FU-DESIGN-022 | implementation | 在 `CHG-quic-cid-generator` admission 通过后，新增 `QuicCidGenerator`、`getrandom` 依赖和必要 re-export。 | QUIC CID generator | yes |
| FU-DESIGN-009 | testing | 为 `CHG-udp-runtime-socket` 增加验证，覆盖 UDP/QUIC handler 接收统一 `UdpSocket`、`BalancedUdpSocket` 不再公开导出、UDP response 仍可通过统一 socket 发送，以及 `UdpServer` 监听 socket 获取规则。 | UDP unified socket callback | yes |
| FU-DESIGN-023 | implementation | 在 `CHG-server-concurrency-limit` admission 通过后，新增或更新 `TcpServiceConfig::max_concurrency_per_worker` 和 `UdpServiceConfig::max_concurrency_per_worker`、每 worker permit helper；native TCP/UDP/QUIC handler 型 serve 在 accept/recv 前等待许可；用户态模拟 TCP/UDP/QUIC handler 型路径对目标 worker 非阻塞获取许可，满载时丢弃已收工作并继续循环；permit helper 去除 `Mutex` 和 waiter 列表，使用单 listener waiter 模型。 | server handler concurrency limit | yes |
| FU-DESIGN-024 | testing | 为 `CHG-server-concurrency-limit` 增加验证，覆盖 `TcpServiceConfig::max_concurrency_per_worker` 和 `UdpServiceConfig::max_concurrency_per_worker` API、默认和 0 不限制、每 worker 独立上限、handler 完成释放许可、native 上限命中等待许可、用户态模拟 TCP/UDP/QUIC 满载 worker 丢弃已收工作且不阻塞后续 accept/recv、关闭唤醒等待 accept/recv，以及 `serve_socket` 不受该限制影响。 | server handler concurrency limit | yes |
| FU-DESIGN-017 | testing | 为 `CHG-udp-quic-listener-serve` 增加验证，覆盖 `UdpServer::serve_socket`、`QuicServer::serve_socket` 通过回调交付统一 `UdpSocket` 和 socket 所属 worker id、返回 server 生命周期对象、不调用数据包 handler、应用可自行 `recv_from`，以及 Windows/fallback 不暴露平台内部机制且每个 worker socket 视图只接收自身应收数据。 | UDP/QUIC socket-only serve | yes |
| FU-DESIGN-018 | testing | 为 `CHG-quinn-udp-socket-compat` 增加验证，覆盖默认 features 下 quinn helper 不可见、启用 `quinn` feature 后 helper 可见、tokio native 和 routed socket 都可通过 helper 非阻塞收发、其他 runtime 组合保持可编译，以及本 crate 不依赖或实现 quinn trait。 | quinn UDP socket adapter helper | yes |
| FU-DESIGN-019 | implementation | 在 `CHG-quinn-udp-socket-compat` admission 通过后，新增默认关闭的 `quinn` feature，并为统一 `UdpSocket` 实施 feature-gated 非阻塞发送、发送 ready poll、接收 poll 和 vectored 接收 helper。 | quinn UDP socket adapter helper | yes |
| FU-DESIGN-010 | testing | 为 `CHG-linux-compatible-scheduling` 增加验证，覆盖 Dispatcher/DispatchPolicy 不公开、`ServerRuntimeConfig` 不含调度字段，以及 fallback 用户态路径使用 Linux 兼容内部调度。 | Linux 兼容内部调度 | yes |
| FU-DESIGN-011 | implementation | 在 `CHG-tokio-uring-runtime` admission 通过后，新增 `runtime-tokio-uring` feature、tokio-uring adapter、Linux cfg 编译边界和公开 socket 类型映射。 | tokio-uring runtime adapter | yes |
| FU-DESIGN-012 | testing | 为 `CHG-tokio-uring-runtime` 增加验证，覆盖 feature 互斥、非 Linux cfg 边界、公开 socket 类型、handler 调用 tokio-uring API 和 unified harness 可达性。 | tokio-uring runtime adapter | yes |
| FU-DESIGN-013 | implementation | 在 `CHG-hyper-static-example` admission 通过后，新增 hyper 静态文件服务器示例和必要 example 依赖。 | hyper static example | yes |
| FU-DESIGN-014 | testing | 为 `CHG-hyper-static-example` 增加验证，覆盖示例编译、`--root` 参数、200/404/403 响应和 unified harness 可达性。 | hyper static example | yes |
| FU-DESIGN-015 | implementation | 在 `CHG-publish-metadata` admission 通过后，更新 `Cargo.toml` package metadata 和 include 边界。 | publish metadata | yes |
| FU-DESIGN-016 | testing | 为 `CHG-publish-metadata` 记录 package 文件列表验证，确认 README/LICENSE/src/examples/Cargo.toml 被包含且 Harness 缓存不进入 package。 | publish metadata | yes |

## 设计护栏
- 不要在 `design.md` 中改写已批准的 proposal 意图。
- 当前模块没有 Harness 直接子模块；runtime/core/platform 是 Rust 内部模块。
- 对既有代码，先描述当前结构，再描述变更。
- 不要引入 proposal 未批准的理想化架构。
- 优先采用满足 proposal 和文档化约束的最简单设计。
- 不要为单次使用代码增加推测性扩展点、配置或抽象。
- 每个可进入实现的设计项都必须携带 `proposal.md` 中相同的 `change_id`。
- 对多模块或跨边界工作，列出每个受影响模块，并说明是否需要独立实现准入。
