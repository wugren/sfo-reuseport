---
module: sfo-reuseport
submodule:
version: v0.1
status: approved
approved_by: auto-pipeline
approved_at: 2026-05-26T08:09:05Z
---

# sfo-reuseport 设计

> 本文件只说明实现设计。完整测试策略保留在 `testing.md` 中。

## 设计范围
### 目标
- 将 v0.1 设计为 library crate，提供 TCP 与 UDP multi-worker socket 服务 API。
- 通过互斥 feature 隔离 tokio、async-std 与 tokio-uring，并让公开回调使用当前 runtime 的原生 stream/socket 类型。
- 将实现划分为三层内部模块：
  - 异步运行时抽象。
  - 领域实现抽象层，包含共用业务逻辑以及平台实现接口。
  - 平台具体实现，封装 Linux、macOS、FreeBSD 和 Windows socket 行为差异。
- 定义 worker 生命周期、worker thread runtime、TCP accept、UDP packet 交付、Linux 兼容内部调度、受控 socket 选项和错误语义。
- 定义 `ServiceConfig` 的 socket 创建后初始化回调；该回调默认不存在，在底层 socket 创建后、内部 socket 选项和 bind/listen 前同步执行。
- 定义 `ServerRuntime` 运行期抽象、动态 listener 注册/删除接口，并允许一个 runtime 实例在同一 worker 线程集合中同时承载 TCP 与 UDP listener。
- 定义 `QuicServer` 作为 QUIC-aware UDP 包分配入口；该入口只解析足够的 QUIC header 路由字段来选择 worker，不实现 TLS、handshake、connection 或 stream。

### 非目标
- 不实现协议解析、TLS、连接池、限流、超时、重试或跨 worker 通信。
- 不在公开 API 中暴露 worker id、raw fd/raw socket escape hatch 或平台特定 reuse-port 细节。
- 不允许同时启用 tokio、async-std 与 tokio-uring 中的多个 runtime feature。
- 不在设计阶段修改测试策略、测试计划或实现代码。
- 不提供配置文件热加载、外部配置订阅或按协议独立线程池。
- 不把 `QuicServer` 设计成完整 QUIC 协议栈，不引入 quinn、TLS 配置、QUIC connection/stream API 或应用协议处理。

## 总体方案
`sfo-reuseport` 公开一个小型 builder/config API。调用方创建 `ServerRuntime`，由 runtime 初始化共享 worker 数，再在运行期间增加、删除 TCP/UDP listener。单协议 TCP/UDP/QUIC-aware UDP 入口必须显式借用调用方创建的 `ServerRuntime`，公开 API 中每个 server 类型只保留一个 `serve(&ServerRuntime, ServiceConfig, handler)` 方法，不再提供无 runtime 参数的默认启动入口或 `serve_with_runtime` 并列入口。crate 在内部完成 socket 创建、平台能力适配、worker 启动和事件循环运行。

`QuicServer` 复用 UDP bind、worker runtime 和 runtime 原生 `UdpSocket` 回调形态，但使用固定的 QUIC-aware worker 选择规则替代普通 UDP worker 选择：从 UDP payload 中读取 QUIC Destination Connection ID 的前 2 字节 big-endian `u16` worker shard，并把 packet 投递到对应 worker。这个设计只处理 UDP 包分配；QUIC 协议状态仍由调用方或上层 QUIC crate 管理。

内部结构按三层组织：

1. `runtime`：对当前 feature 选中的 async runtime 做薄封装，提供类型别名、spawn/block_on/sleep 等最小适配，以及 TCP/UDP socket 从标准 socket 转换到 runtime socket 的入口；`runtime-tokio-uring` 仅在 Linux 上启用 tokio-uring 的 current-thread driver 和原生 net 类型。
2. `core`：领域实现抽象层，持有公开配置、动态 listener registry、worker 模型、Linux 兼容内部调度、错误类型、TCP/UDP 服务循环、runtime 原生 UDP socket 交付逻辑，以及面向平台层的 trait 接口。
3. `platform`：平台具体实现，负责 bind 前 socket 创建、reuse-port/reuse-address/transparent 等选项设置，以及 Windows 或其他 fallback 用户态模拟所需的收包适配。

公开 API 不暴露上述内部层级。公开类型保留在 crate 根或 `api` 模块中，再 re-export 给调用方；内部模块负责降低实现耦合。

## 简化检查
- 最小充分方案：使用 feature-gated runtime 模块、一个共享 core 层和 cfg-gated platform 层，不引入插件系统或动态分发平台后端；tokio-uring 作为第三个互斥 runtime adapter，而不是新增一套 public server API。
- 复用的既有组件或模式：Rust 标准库 socket 地址类型、feature gating、`cfg(target_os)`、`Arc` 和 async callback future。
- 新增抽象：
  - `runtime` 抽象：隔离 tokio/async-std/tokio-uring 类型、单线程 worker runtime 启动方式和 socket 转换方式。
  - `PlatformSocketOps`：让 core 层不分支平台 syscall 细节。
  - Linux 兼容内部调度函数：统一 TCP/UDP 在没有可用 `SO_REUSEPORT` worker 分配能力时的 worker 选择语义。
  - socket 初始化回调：让调用方在不接管 socket 所有权的前提下设置尚未成为稳定 `SocketOptions` 字段的创建期 socket 参数。
  - `QuicServer`：复用 UDP socket handle 但提供独立公开入口，避免把普通 UDP 回调语义和 QUIC-aware 包路由语义混在一个 bool 配置里。
- 每个新增抽象的必要性：
  - runtime 抽象是互斥 feature 和同形 API 的直接要求；tokio-uring 的 driver 初始化和 socket API 与 tokio 不同，必须由独立 adapter 隔离。
  - 平台接口是跨平台屏蔽 socket 差异的直接要求。
  - Linux 兼容内部调度函数是 fallback 平台可预测 worker 选择的集中点；它不是公开配置项，也不提供用户自定义策略。
  - 不再引入 `BalancedUdpSocket` 公开封装；UDP 回调直接接收所选 runtime 的原生 `UdpSocket`，减少公开 API 面并保持与 TCP 的 runtime 原生类型风格一致。
  - socket 初始化回调是 `CHG-socket-init-callback` 的直接公开契约；使用一次性闭包比为每个底层选项新增稳定字段更小，同时仍避免长期 raw socket escape hatch。
  - `ServerRuntime`/listener registry 是运行期增删监听和混合协议共享 worker 配置的直接需求。
  - `QuicServer` 是 `CHG-quic-routed-udp` 的直接公开契约；独立类型可以清楚表达 QUIC-aware UDP routing，同时保持 `UdpServer` 的裸 UDP 包交付模型不变。

## 当前结构
- `Cargo.toml` 声明 library crate、runtime features 和实现依赖。
- `src/lib.rs` 是公开 library 入口。
- `src/core/`、`src/runtime/` 和 `src/platform/` 分别承载领域逻辑、async runtime 适配和平台 socket 行为。
- `examples/tcp_echo.rs` 是示例 binary，必须使用 `ServerRuntimeConfig` 配置 worker。

## 模块拆分
这些是 crate 内部 Rust 模块，不是 Harness 直接子模块包。

| 模块 | 类型 | 职责 | 输入 | 输出 | 依赖 | 独立文档 |
|------|------|------|------|------|------|----------|
| `runtime` | internal | 异步运行时抽象，按 feature 暴露当前 runtime 类型与 spawn/转换入口。 | feature、标准 socket、future | runtime 原生 stream/socket、task handle | tokio、async-std 或 tokio-uring | no |
| `core` | internal | 需求实现抽象层，包含配置、动态 listener registry、worker、TCP/UDP 服务循环、Linux 兼容内部调度、错误、公共业务逻辑和平台 trait。 | 公开配置、回调、platform ops | worker 运行、回调交付、listener 管理、统一错误 | `runtime`、`platform` trait | no |
| `platform` | internal | 平台具体实现的 cfg 分发入口。 | bind 地址、socket 选项、协议类型 | 已配置 socket 或模拟后端 | OS cfg、`socket2`/std | no |
| `platform::unix` | internal | Linux/macOS/FreeBSD 共享 socket 设置基础。 | socket config | 已设置 socket | `socket2` | no |
| `platform::linux` | internal | Linux reuse-port 和 IPv4 transparent 细节。 | socket config | Linux socket 设置结果 | `platform::unix` | no |
| `platform::bsd` | internal | macOS/FreeBSD reuse-port 行为封装。 | socket config | BSD socket 设置结果 | `platform::unix` | no |
| `platform::windows` | internal | Windows 用户态模拟路径和 socket 创建。 | socket config | Windows socket 或模拟接收入口 | std/runtime | no |
| none | Harness submodule | 当前 crate 仍由根模块包表示。 | n/a | n/a | n/a | no |

## 大模块子模块决策
当前仓库只有一个小型 Rust crate，v0.1 的 runtime、core 和 platform 分层共享同一公开 crate 边界。它们应作为 crate 内部模块记录在根模块 `design.md` 中，不拆成 Harness 直接子模块包。若未来出现独立协议适配、benchmark harness 或平台专用子项目，再建立直接子模块包。

## Directly Mapped Change Items
| change_id | proposal_id | Design Coverage | Scope Paths | Interface/Boundary Impact | Notes |
|-----------|-------------|-----------------|-------------|---------------------------|-------|
| CHG-runtime-features | P-runtime | `Cargo.toml` features、`runtime` 模块、互斥 compile_error、runtime 原生类型别名。 | `Cargo.toml`、`src/runtime.rs` 或 `src/runtime/`、`src/lib.rs` | 公开回调类型随 feature 变化。 | 默认 `runtime-tokio`。 |
| CHG-tokio-uring-runtime | P-tokio-uring-runtime | 新增 `runtime-tokio-uring` feature、`tokio-uring` 可选依赖、Linux-only cfg 边界、`src/runtime/tokio_uring.rs` adapter、公开 `TcpStream`/`UdpSocket` 类型映射到 tokio-uring net 类型或共享等价包装、标准 socket 转换入口和每 worker current-thread tokio-uring driver 启动方式。 | `Cargo.toml`、`src/lib.rs`、`src/runtime/mod.rs`、`src/runtime/tokio_uring.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`examples/` | 第三个互斥 runtime feature；启用后用户 handler 可直接调用 tokio-uring socket API；非 Linux 平台编译期拒绝或明确 unsupported。 | 不新增 server API；不允许与 tokio/async-std 同时启用；tokio-uring 非 Linux 可运行支持不属于 v0.1 承诺。 |
| CHG-server-runtime | P-server-runtime | `ServerRuntimeConfig` 持有共享 worker 数，`ServerRuntime` 持有 listener registry 和注册入口，server/listener config 不含 worker 数量或调度策略；`TcpServer`、`UdpServer`、`QuicServer` 单协议入口只接受显式 `&ServerRuntime`。 | `src/core/config.rs`、`src/core/server_runtime.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/lib.rs` | 公开动态服务入口命名为 `ServerRuntime`；worker 配置从 server/listener config 移到 runtime config；移除无 runtime 参数 `serve` 和 `serve_with_runtime`。 | 不提供每 server 独立 worker 池或隐式默认 runtime 入口。 |
| CHG-worker-thread-runtime | P-worker-thread-runtime | `runtime` 模块提供 worker thread 启动入口；每个 worker loop 在独立 OS 线程中初始化并运行单线程 async runtime。 | `src/runtime/`、`src/core/worker.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/dynamic.rs` | worker loop 不再直接依赖调用方当前 runtime 的 `spawn` 代表 worker。 | 不提供 work stealing 或多线程 per-worker runtime。 |
| CHG-worker-model | P-workers | `ServerRuntimeConfig` worker 数量、默认 CPU 数、worker spawn/join、内部 worker id。 | `src/core/worker.rs` | 回调不包含 worker id。 | worker id 仅用于日志/分发内部状态。 |
| CHG-tcp-serve | P-tcp | `TcpServer::serve(&ServerRuntime, ServiceConfig, handler)` 创建 TCP listener、运行 accept loop、每连接 async 回调。 | `src/core/tcp.rs`、`src/platform/` | TCP serve API 只通过显式 runtime 入口暴露。 | 回调接收 runtime 原生 `TcpStream`；不保留 `serve_with_runtime`。 |
| CHG-udp-runtime-socket | P-udp | `UdpServer::serve(&ServerRuntime, ServiceConfig, handler)` 运行 UDP recv loop、交付 packet metadata，并把当前 runtime 的原生 `UdpSocket` 交给 handler；不导出 `BalancedUdpSocket`。 | `src/core/udp.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | UDP serve API 只通过显式 runtime 入口暴露，UDP handler 与 runtime feature 选择绑定。 | 不保留 `BalancedUdpSocket` 或 `serve_with_runtime`；实现仍负责保护内部 bind/reuse-port 状态不被配置覆盖。 |
| CHG-linux-compatible-scheduling | P-linux-compatible-scheduling | 删除公开 `DispatchPolicy` 和 dispatcher 配置；`ServerRuntimeConfig` 不包含调度字段；fallback 用户态路径使用内部 Linux 兼容 hash 选择 worker；`QuicServer` 继续使用固定 16-bit shard 规则。 | `src/core/config.rs`、`src/core/schedule.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/mod.rs`、`src/lib.rs`、`tests/unit/`、`tests/integration/` | 公开 API 不导出 Dispatcher/DispatchPolicy，不提供 `Auto`、`RoundRobin`、`SrcHash`、`Custom` 或自定义 selector；平台 fallback 行为保持内部实现细节。 | 不提供可配置、load-aware、adaptive 或用户自定义 scheduler。 |
| CHG-platform-behavior | P-platform | `PlatformSocketOps` trait 和 cfg-gated Linux/BSD/Windows 实现。 | `src/platform/` | 平台差异不进入公开 API。 | Windows 走用户态模拟。 |
| CHG-socket-options | P-socket-options | `SocketOptions`、能力检查、设置时机和错误分类。 | `src/core/config.rs`、`src/platform/` | 配置层新增受控 socket 选项。 | 不允许覆盖内部 reuse-port/bind 状态。 |
| CHG-socket-init-callback | P-socket-init-callback | `ServiceConfig` 持有默认 `None` 的 socket 创建后回调；平台层创建 `socket2::Socket` 后、内部选项和 bind/listen 前调用；回调错误转换为 `Error::SocketInitCallback` 并阻止服务启动。 | `src/core/config.rs`、`src/core/error.rs`、`src/platform/`、`src/core/tcp.rs`、`src/core/udp.rs` | 公开配置层新增一次性初始化钩子；不暴露 socket 所有权，不允许回调返回后继续持有可变访问权。 | 回调接收借用的 `socket2::Socket`，可调用 socket2 支持的 setter；跨平台可用性由调用方和底层 OS 负责。 |
| CHG-dynamic-listeners | P-dynamic-listeners | `ServerRuntime`、`ListenerId`、listener registry、运行期 TCP/UDP bind/unbind、删除唤醒和停止新工作交付。 | `src/core/config.rs`、`src/core/dynamic.rs`、`src/core/tcp.rs`、`src/core/udp.rs` | 新增动态 listener 管理 API，不改变既有单协议入口。 | 删除 listener 不强制中断已交付 handler future。 |
| CHG-mixed-protocol-workers | P-mixed-protocol-workers | TCP/UDP listener 注册到同一 `ServerRuntime` 实例并在同一 runtime executor 上运行。 | `src/core/dynamic.rs`、`src/runtime/` | 新增混合协议 runtime 入口。 | 不提供按协议独立线程池或负载感知调度。 |
| CHG-quic-routed-udp | P-quic-routed-udp | `QuicServer::serve(&ServerRuntime, ServiceConfig, handler)`、固定 QUIC DCID 前 2 字节 big-endian `u16` worker shard 解析、非法 packet 丢弃、Linux reuse-port eBPF selector 的 best-effort worker 预分配、CBPF fallback、用户态稳定 worker 投递 fallback 和 listener 删除语义。 | `src/core/udp.rs`、`src/core/mod.rs`、`src/lib.rs`、`src/platform/`、`tests/unit/`、`tests/dv/`、`tests/integration/` | 新增 QUIC-aware UDP routing API；不改变 `UdpServer` 裸 UDP API；`QuicServer` 也只通过显式 runtime `serve` 暴露；Linux 可用时优先尝试内核 reuse-port eBPF 预分配，eBPF 加载或 attach 失败时退回 CBPF，再失败时退回用户态路由；外部使用者必须按固定 CID layout 生成 server CID。 | 不实现 TLS、handshake、connection、stream、congestion control 或 quinn 集成；不支持可配置 CID layout；不把 eBPF/CBPF 加载失败暴露为公开 API 或强制启动失败；不保留 `serve_with_runtime`。 |

## 实施顺序
| 阶段 | 目标 | 前置条件 | 输出 | 依赖 | 可并行 |
|------|------|----------|------|------|--------|
| 1 | 建立 library crate、features 和 runtime 抽象。 | proposal/design approved，schema-check 与 admission-check 通过。 | `src/lib.rs`、`runtime`、feature gating。 | none | no |
| 2 | 建立公开配置、错误、worker 模型和 Linux 兼容内部调度。 | 阶段 1 | `ServerRuntimeConfig`、`ServiceConfig`、`SocketOptions`、worker core、内部 scheduling helper。 | runtime | no |
| 3 | 建立平台接口和 Linux/BSD/Windows 后端骨架。 | 阶段 2 | `PlatformSocketOps` 和 cfg-gated platform modules。 | core config | yes |
| 4 | 实现 TCP 服务路径。 | 阶段 1-3 | TCP bind、accept loop、回调交付。 | runtime、platform、worker | yes |
| 5 | 实现 UDP 服务路径。 | 阶段 1-3 | runtime 原生 `UdpSocket` handler 参数、packet loop、send/response API 使用方式。 | runtime、platform、内部 scheduling helper | yes |
| 6 | 收敛错误语义、文档注释和示例。 | 阶段 4-5 | 一致的 public API 和 docs。 | all | no |
| 7 | 实现 `ServerRuntime`、动态 listener registry 和混合协议服务入口。 | 阶段 1-6 | `ServerRuntime`、listener add/remove、混合 TCP/UDP 验证。 | tcp、udp、worker、runtime | no |
| 8 | 实现 `QuicServer` QUIC-aware UDP 包路由入口。 | 阶段 1-7 | `QuicServer`、QUIC DCID worker shard 解析、跨 worker 稳定投递验证。 | udp、worker、runtime | no |
| 9 | 增加 tokio-uring runtime adapter。 | 阶段 1-8，`CHG-tokio-uring-runtime` admission 通过。 | `runtime-tokio-uring` feature、tokio-uring socket 类型映射、Linux cfg 编译边界和 handler API 验证。 | runtime、tcp、udp、platform | no |

## 关键决策
- 使用 compile-time feature 选择 runtime，而不是 runtime trait object。原因是公开回调必须包含当前 runtime 的原生 `TcpStream`/UDP socket 类型，动态抽象会迫使 API 失去原生类型。
- `runtime-tokio-uring` 使用独立 feature 和独立 adapter，不复用 `runtime-tokio` adapter。每个 worker OS 线程创建并持有一个 `tokio_uring::Runtime`；executor 记录 owner thread id，本线程提交的 local task 直接调用 tokio-uring 的 local spawn，非本线程提交时通过 task channel 投递到 owner thread 后再 local spawn。公开 `TcpStream` 映射到 `tokio_uring::net::TcpStream`，公开 `UdpSocket` 使用可 clone 的共享包装 `Rc<tokio_uring::net::UdpSocket>` 或等价类型别名，保持现有 UDP handler 可复制 socket handle 的调用形态。
- `runtime-tokio-uring` 是 Linux 定向 feature。非 Linux 目标启用该 feature 时，crate 在 `src/lib.rs` 或 `runtime` 模块中通过 `compile_error!` 明确拒绝；这比运行时 `Unsupported` 更早暴露平台边界，也避免非 Linux 构建拉入不可用的 io_uring API。
- tokio-uring adapter 的跨线程投递闭包保持 `Send + 'static` bounds；闭包只携带标准 socket、handler 和控制状态，tokio-uring socket/future 的创建和 poll 必须发生在目标 worker thread runtime 内。tokio-uring handler future 不要求 `Send`，以匹配 tokio-uring 原生 socket 的 current-thread 边界。
- `core` 依赖平台 trait，不直接写 `cfg(target_os)` 分支。原因是 TCP/UDP/worker 业务逻辑应只关注 socket 能力结果，平台差异应集中在 `platform`。
- UDP handler 直接接收 runtime 原生 `UdpSocket`。crate 不额外提供 `BalancedUdpSocket` 封装；bind、reuse-port 和 listener 生命周期仍由 `ServerRuntime`、平台 bind 路径和 listener registry 控制，公开配置不得覆盖这些内部状态。
- 不提供公开 `DispatchPolicy` 或 dispatcher 配置。支持内核 `SO_REUSEPORT` worker 分配的平台优先使用平台路径；没有可用 `SO_REUSEPORT` worker 分配能力的系统使用内部 Linux 兼容调度函数保持公开 API 行为一致。
- Linux 兼容内部调度以连接或数据包的四元组元信息为输入，使用稳定 hash 映射到 `worker_count`；缺少 peer 地址时回退到本地地址和协议类别可用信息。该函数只在 fallback 用户态路径使用，不进入公开 API。
- 动态 listener 删除采用受控关闭信号加本地 wake-up，保证删除返回后该 listener 不再有意接收新工作；已经交付给 handler 的 TCP/UDP work item 不由 balancer 强制取消。
- `ServerRuntime` 使用同一份 worker 数注册 TCP 与 UDP listener。每个 worker 是一个独立 OS 线程，线程内由当前 feature 选择的 runtime 初始化一个单线程 async runtime；TCP 与 UDP listener loop 注册到这些 worker thread runtime 上，而不是创建按协议隔离的线程池。
- `QuicServer` 使用固定 worker shard 规则：long header packet 读取 DCID length 后取 DCID 前 2 字节作为 big-endian `u16` worker shard；short header packet 取首字节之后的 2 字节作为 big-endian `u16` worker shard。worker index 为 `shard % worker_count`。payload 太短、DCID length 小于 2 或 DCID 超出 payload 边界时丢弃该 packet，不调用用户 handler。
- 强约束由固定公开契约和运行时拒绝共同形成：上层 QUIC crate 必须在 server CID 的 DCID 前 2 字节写入 big-endian worker shard；`QuicServer` 不提供可配置 layout、fallback 推断或来源地址兜底，所有不满足 layout 的 packet 都不会进入用户 handler。
- `QuicServer` v0.1 在 Linux 上先尝试 best-effort reuse-port eBPF selector：为每个 worker 创建绑定到同一地址的 UDP socket，加载 `BPF_PROG_TYPE_SK_REUSEPORT` 程序并通过 `SO_ATTACH_REUSEPORT_EBPF` 附加到 reuse-port group。eBPF 程序只读取 QUIC 固定 shard layout 并返回 `shard % worker_count`，让内核优先把合法 packet 送到目标 worker socket。eBPF 加载、verifier、权限或 attach 失败时不改变公开 API，继续尝试当前 classic BPF selector；CBPF 也失败、平台不支持或 socket 组创建失败时退回可移植用户态稳定分发路径。worker loop 仍在用户态解析 route key；非法 packet、BPF fallback packet 或未进入目标 worker 的 packet 不调用 handler。

## 数据与状态
### 配置类型
- `ServerRuntimeConfig`：包含共享 `workers: WorkerCount`，用于同一 runtime 实例内的所有 server/listener；不包含调度策略字段。
- `ServiceConfig`：包含单协议 convenience 入口的 `bind_addr: SocketAddr` 和 `socket_options: SocketOptions`；不包含 worker 数量。
- `ServiceConfig::socket_init_callback`：`Option<SocketInitCallback>`，默认 `None`。当存在时，平台层在创建 TCP/UDP `socket2::Socket` 后立即调用。该回调必须同步完成，只接收借用，不可替换 socket 或保存可变访问权；返回错误时服务启动失败。
- `WorkerCount`：支持 `Default` 和显式正整数。`Default` 在构建服务时解析为 `num_cpus::get()`；显式 0 是配置错误。
- `SocketOptions`：包含 `reuse_address: bool`、`ipv4_transparent: TransparentMode`。后续选项只能通过该受控类型加入。
- `TransparentMode`：`Disabled`、`Required`、`BestEffort`。`Required` 在不支持或无权限时返回错误；`BestEffort` 记录 unsupported/permission-denied 结果但不阻止服务启动。
- `ListenerConfig`：包含单个 listener 的 `bind_addr: SocketAddr` 和 `socket_options: SocketOptions`。
- `ListenerId`：由 crate 分配的稳定不透明标识，用于删除 listener。
- `ListenerProtocol`：`Tcp` 或 `Udp`，用于 registry 内部记录和错误上下文。

### Linux 兼容内部调度
- 公开 API 不包含 `DispatchPolicy`、dispatcher 类型、策略枚举、custom selector 或 `ServerRuntimeConfig::with_dispatch`。
- fallback 用户态 TCP/UDP worker 选择由内部函数完成，建议命名为 `linux_reuseport_select(meta, worker_count)` 或等价私有 helper。
- 内部调度输入使用 `PacketMeta` 中的 peer/local socket address 和协议类别；实现必须稳定、确定且不依赖随机种子。
- worker index 为稳定 hash 对 `worker_count` 取模；`worker_count == 0` 仍由 `WorkerCount`/runtime config 验证拒绝。
- `QuicServer` 不使用普通 fallback hash 选择合法 packet，而继续使用固定 16-bit QUIC worker shard 规则；非法 packet 仍丢弃。

### Worker 生命周期
- 服务启动时先解析 `ServerRuntimeConfig`，再由 platform 层创建 socket/backend，最后为每个 worker 启动一个 OS 线程和线程内单线程 async runtime。
- worker 内部 id 只用于内部调度、任务命名或错误上下文，不进入用户回调签名。
- worker 回调 future 必须是 `Send + 'static`，以便跨线程移动到对应 worker thread runtime；进入 worker 后在该线程的单线程 async runtime 内 poll。若未来支持非 `Send` handler，应另行提案。
- 服务 handle 提供 graceful stop 的内部信号；v0.1 公开 API 是否暴露 stop handle 由实现阶段按最小可用 API 决定，不新增未批准的生命周期管理功能。
- `ServerRuntime` 启动时不绑定任何 listener；调用方通过 `add_tcp_listener` 或 `add_udp_listener` 增加监听。
- `remove_listener` 从 registry 移除对应 listener，设置停止信号，并向 listener 本地地址发送 TCP/UDP wake-up 以促使阻塞的 accept/recv loop 退出。

## 接口与依赖
### 公开接口概要
建议公开接口保持在 crate 根 re-export：

```rust
pub use crate::core::{
    Error, ListenerConfig, ListenerId, ListenerProtocol, PacketMeta, QuicServer,
    ServiceConfig, SocketOptions, ServerRuntime, ServerRuntimeConfig, TcpServer,
    TransparentMode, UdpServer, WorkerCount,
};
pub use crate::runtime::{TcpStream, UdpSocket};
```

`ServiceConfig` socket 创建后回调：

```rust
pub type SocketInitCallback =
    Arc<dyn Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static>;

impl ServiceConfig {
    pub fn with_socket_init_callback<F>(self, callback: F) -> Self
    where
        F: Fn(&socket2::Socket) -> Result<(), Error> + Send + Sync + 'static;

    pub fn without_socket_init_callback(self) -> Self;
}
```

回调字段默认是 `None`。`with_socket_init_callback` 将闭包包装为 `Arc`，从而允许每 worker socket 创建路径复用同一回调。回调执行顺序是：创建 `socket2::Socket`，执行用户 socket 初始化回调，执行 crate 内部 `reuse_address`、`reuse_port`、transparent 等必需选项，然后 bind/listen。内部必需选项保留最终控制权，用户回调不得依赖覆盖这些状态。

### 公开代码接口细节
TCP 入口：

```rust
pub struct TcpServer;

impl TcpServer {
    pub async fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<(), Error>
    where
        F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;
}
```

UDP 入口：

```rust
pub struct UdpServer;

impl UdpServer {
    pub async fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<(), Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;
}
```

QUIC-aware UDP 包分配入口：

```rust
pub struct QuicServer;

impl QuicServer {
    pub async fn serve<F, Fut>(
        runtime: &ServerRuntime,
        config: ServiceConfig,
        handler: F,
    ) -> Result<(), Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;
}
```

`TcpServer`、`UdpServer` 和 `QuicServer` 的 `serve` 都借用已启动的 `ServerRuntime`。这三个类型不再创建自己的默认 runtime，不接受 `ServerRuntimeConfig`，也不提供 `serve_with_runtime`。`UdpServer` 和 `QuicServer` 的 handler 接收 UDP packet 级别参数和所选 runtime 的原生 `UdpSocket`，便于用户直接使用 runtime UDP API 发送响应；公开 API 不导出 `BalancedUdpSocket`。`QuicServer` 不暴露 TLS、ALPN、QUIC transport config、connection 或 stream API。上层必须生成满足 `DCID[0..2] = worker_shard_be_u16` 的 CID；不满足该 layout 的 packet 被视为不可路由。

Linux QUIC reuse-port selector 内部接口：

```rust
pub(crate) fn bind_quic_udp_reuseport_workers(
    config: &ServiceConfig,
    workers: usize,
) -> Result<Option<Vec<std::net::UdpSocket>>, Error>;
```

该接口只在平台层内部使用。返回 `Ok(Some(_))` 表示 Linux reuse-port eBPF 或 CBPF selector 已附加并可由每 worker socket 接收；返回 `Ok(None)` 表示当前平台或当前内核/socket 组合不可用，调用方必须退回用户态 QuicServer 路由；返回 `Err(_)` 只用于普通 UDP bind 级别的不可恢复错误。BPF selector 不成为公开配置项，不引入新 crate feature。eBPF 实现使用手写 Linux syscall 和内核 BPF 指令，不新增 `aya`、`libbpf-rs` 或构建期 C 工具链；所有 eBPF 加载、verifier 和权限错误只影响最佳性能路径，不改变公开 API。

动态服务入口：

```rust
pub struct ServerRuntime;
pub struct ServerRuntimeConfig;
pub struct ListenerConfig;
pub struct ListenerId(/* private */);
pub enum ListenerProtocol { Tcp, Udp }

impl ServerRuntime {
    pub fn start(config: ServerRuntimeConfig) -> Result<Self, Error>;

    pub fn add_tcp_listener<F, Fut>(
        &self,
        config: ListenerConfig,
        handler: F,
    ) -> Result<ListenerId, Error>
    where
        F: Fn(TcpStream) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn add_udp_listener<F, Fut>(
        &self,
        config: ListenerConfig,
        handler: F,
    ) -> Result<ListenerId, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn add_quic_listener<F, Fut>(
        &self,
        config: ListenerConfig,
        handler: F,
    ) -> Result<ListenerId, Error>
    where
        F: Fn(UdpSocket, PacketMeta, Vec<u8>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static;

    pub fn remove_listener(&self, id: ListenerId) -> Result<(), Error>;
}
```

`ServerRuntime` 的 handler 签名继续不包含 worker id。删除 listener 只停止新 accept/recv 交付，不取消已经进入用户 handler 的 future。`ServiceConfig` 和 `ListenerConfig` 不提供 `with_workers` 或 worker 字段；共享 worker 数只能通过 `ServerRuntimeConfig` 设置。

错误类型：
- `Error::InvalidConfig`
- `Error::UnsupportedPlatformOption`
- `Error::PermissionDenied`
- `Error::SocketInitCallback`
- `Error::Socket`
- `Error::Runtime`
- `Error::UnknownListener`
- `Error::Handler`

错误应保留源错误信息，但公开枚举不得要求调用方按平台分支。

### 平台接口
core 层使用如下内部 trait：

```rust
pub(crate) trait PlatformSocketOps {
    fn bind_tcp(config: &ServiceConfig) -> Result<PlatformTcpBackend, Error>;
    fn bind_udp(config: &ServiceConfig) -> Result<PlatformUdpBackend, Error>;
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
- `tokio-uring`：仅在 `runtime-tokio-uring` feature 下启用，目标平台必须是 Linux；worker thread 创建并持有 `tokio_uring::Runtime` 作为 current-thread io_uring driver。标准 TCP/UDP socket 转换入口必须先设置 nonblocking 或采用 tokio-uring 支持的 from-std 路径；若 tokio-uring 无法接管已绑定 socket，implementation 必须在 adapter 或 platform 层调整创建顺序，但不得改变公开 server API。
- `socket2`：用于 bind 前创建 socket 和设置 reuse-address/reuse-port/transparent 等选项。
- `num_cpus`：用于默认 worker 数。
- 新依赖只在实现阶段按此设计加入；若发现需要额外依赖，必须先更新 design。
- `socket2` 已是平台 socket 设置依赖；公开回调类型可以引用 `socket2::Socket`，不需要新增依赖。

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
│   └── tcp_echo.rs
├── docs/
└── harness/
```

| 路径 | 类型 | 职责 | 备注 |
|------|------|------|------|
| `src/lib.rs` | Rust library entry | 公开 API re-export 和 feature 互斥检查 | 新增 |
| `src/runtime/` | Rust module | 异步运行时抽象 | feature-gated |
| `src/core/` | Rust module | 共用业务逻辑、worker、TCP/UDP、动态 listener、Linux 兼容内部调度和错误 | 无平台 syscall |
| `src/platform/` | Rust module | 平台 socket 行为 | cfg-gated |
| `examples/tcp_echo.rs` | Rust example | 示例 TCP echo server | 使用 `ServerRuntimeConfig` 配置 worker |

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
- runtime 原生 `UdpSocket` 状态边界风险：handler 可使用 runtime socket 的公开能力，crate 必须在创建和注册 listener 前完成内部 bind/reuse-port 状态设置，并通过配置 API 禁止覆盖 balancer 必需状态；发现 `BalancedUdpSocket` re-export 或旧封装残留时应先移除旧公开符号。
- 内部调度偏差风险：hash 输入或取模行为变化会影响 worker 亲和性；回滚时恢复上一版私有调度函数即可，不改变公开 API。
- 动态 listener 删除风险：通过停止信号和本地 wake-up 收敛 accept/recv loop；若平台唤醒失败，删除仍从 registry 移除并阻止显式管理面继续引用该 listener。
- 混合协议共享 runtime 风险：handler future 必须 `Send + 'static`；若某协议 handler 长时间占用 runtime 线程，调用方需要自行控制 handler 行为，v0.1 不实现负载感知调度。
- `QuicServer` 命名风险：公开文档和测试必须证明它只提供 QUIC-aware UDP routing，不提供完整 QUIC server。回滚时可移除 `QuicServer` re-export 和入口，不影响 `UdpServer`。
- QUIC 路由键解析风险：来自网络的 packet 必须做长度检查，非法、缺失或短于 16-bit worker shard 的路由键时丢弃，不得 panic 或调用 handler。eBPF/CBPF selector 仅作为 Linux 内核预分配优化；用户态 worker loop 仍保留 route key 校验，确保 verifier、权限或 selector attach 差异不会改变非法 packet 丢弃语义。

## 下游跟进
| follow_up_id | 归属阶段 | 原因 | 触发设计项 | 阻塞 |
|--------------|----------|------|------------|------|
| FU-DESIGN-001 | testing | 实现完成后，testing 阶段需要根据 proposal、design 和 delivered code 生成或更新测试实现、`testing.md` 与 `testplan.yaml`。 | 直接映射变更项 | yes |
| FU-DESIGN-002 | implementation | 实现前需通过 schema-check 和 admission-check，且只能修改已准入 `change_id` 对应生产代码或必要非测试运行资源。 | 全部设计项 | yes |
| FU-DESIGN-004 | testing | 为 `CHG-dynamic-listeners` 和 `CHG-mixed-protocol-workers` 增加测试策略与 testplan 条目。 | 动态 listener 和混合协议 worker | yes |
| FU-DESIGN-005 | testing | 为 `CHG-server-runtime` 增加直接验证，覆盖 `ServerRuntime` 命名、共享 worker 配置、server/listener config 不含 worker 设置，以及 `TcpServer`/`UdpServer`/`QuicServer` 不存在 `serve_with_runtime` 或隐式默认 runtime 入口。 | `ServerRuntime` runtime 抽象 | yes |
| FU-DESIGN-006 | testing | 为 `CHG-worker-thread-runtime` 增加验证，覆盖 worker thread 启动入口和不使用调用方 runtime spawn 代表 worker。 | worker thread runtime | yes |
| FU-DESIGN-007 | testing | 为 `CHG-socket-init-callback` 增加验证，覆盖默认 `None`、TCP/UDP 创建路径调用、错误传播和内部必需 socket 选项边界。 | socket init callback | yes |
| FU-DESIGN-008 | testing | 为 `CHG-quic-routed-udp` 增加验证，覆盖 QUIC DCID worker shard 解析、稳定 worker 投递、非法 packet 丢弃和不暴露 QUIC 协议栈 API。 | QuicServer UDP routing | yes |
| FU-DESIGN-009 | testing | 为 `CHG-udp-runtime-socket` 增加验证，覆盖 UDP/QUIC handler 接收 runtime 原生 `UdpSocket`、`BalancedUdpSocket` 不再公开导出，以及 UDP response 仍可通过 runtime socket 发送。 | UDP runtime socket callback | yes |
| FU-DESIGN-010 | testing | 为 `CHG-linux-compatible-scheduling` 增加验证，覆盖 Dispatcher/DispatchPolicy 不公开、`ServerRuntimeConfig` 不含调度字段，以及 fallback 用户态路径使用 Linux 兼容内部调度。 | Linux 兼容内部调度 | yes |
| FU-DESIGN-011 | implementation | 在 `CHG-tokio-uring-runtime` admission 通过后，新增 `runtime-tokio-uring` feature、tokio-uring adapter、Linux cfg 编译边界和公开 socket 类型映射。 | tokio-uring runtime adapter | yes |
| FU-DESIGN-012 | testing | 为 `CHG-tokio-uring-runtime` 增加验证，覆盖 feature 互斥、非 Linux cfg 边界、公开 socket 类型、handler 调用 tokio-uring API 和 unified harness 可达性。 | tokio-uring runtime adapter | yes |

## 设计护栏
- 不要在 `design.md` 中改写已批准的 proposal 意图。
- 当前模块没有 Harness 直接子模块；runtime/core/platform 是 Rust 内部模块。
- 对既有代码，先描述当前结构，再描述变更。
- 不要引入 proposal 未批准的理想化架构。
- 优先采用满足 proposal 和文档化约束的最简单设计。
- 不要为单次使用代码增加推测性扩展点、配置或抽象。
- 每个可进入实现的设计项都必须携带 `proposal.md` 中相同的 `change_id`。
- 对多模块或跨边界工作，列出每个受影响模块，并说明是否需要独立实现准入。
