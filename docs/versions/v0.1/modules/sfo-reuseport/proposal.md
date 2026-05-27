---
module: sfo-reuseport
submodule:
version: v0.1
status: approved
approved_by: auto-pipeline
approved_at: 2026-05-27T13:32:51Z
---

# sfo-reuseport 提案

## 背景与目标
Rust 网络生态在应用框架、协议实现和 syscall 封装之间缺少一个小而稳定的跨平台抽象层，用于处理与协议无关的 multi-worker socket 服务。`sfo-reuseport` 的目标是填补这一层：对调用方屏蔽平台 socket 差异、worker 创建、连接或数据包分发，以及 async runtime 差异，并提供稳定的 async 回调式 API。

v0.1 的目标是提供一个协议无关的 Rust 库，支持 TCP accept 均衡和 UDP 数据包均衡，能够把工作分发到多个 worker，并通过互斥 feature 支持 tokio、async-std 与 tokio-uring。

## 范围
### 范围内
- 提供用于 TCP 与 UDP multi-worker 服务的 library crate API。
- 通过 feature 选择 runtime，默认 `runtime-tokio`，可替换为 `runtime-async-std` 或 `runtime-tokio-uring`。
- 编译期拒绝同时启用多个 runtime feature。
- 启用 `runtime-tokio-uring` 时，公开回调类型必须使用 tokio-uring 的原生 TCP/UDP socket 类型或等价接口，使调用方能够直接使用 tokio-uring 提供的相关异步 I/O API。
- TCP worker 服务：每个被 accept 的连接都通过 async 用户回调交付，回调参数包含当前 runtime 的原生 `TcpStream`，但不包含 `worker_id`。
- UDP worker 服务：每个收到的数据包都通过 async 用户回调交付，回调参数包含当前 runtime 的原生 `UdpSocket` 以及由后续设计定义的数据包数据/来源元信息，但不包含 `worker_id`。
- 不提供 `BalancedUdpSocket` 公开类型；UDP 回调直接使用所选 runtime 的原生 `UdpSocket`，并由实现保持 balancer 必需的 bind/reuse-port 状态不被公开配置覆盖。
- 对 Linux、macOS、FreeBSD 和 Windows 提供跨平台行为，平台差异不暴露到公开 API。
- 不提供公开可配置 Dispatcher 或分发策略；所有需要用户态模拟的系统必须使用与 Linux `SO_REUSEPORT` 语义一致的内部调度算法。
- 支持配置 worker 数量，默认使用 `num_cpus::get()`。
- worker 数量属于 `ServerRuntime` 级别配置；所有投递到同一 `ServerRuntime` 的 TCP/UDP/QUIC server task 必须共享同一套 worker，不在每个 server config 中单独设置 worker 数量。
- `TcpServer`、`UdpServer` 和 `QuicServer` 的单协议公开入口各自只提供一个同步 `serve(runtime: &ServerRuntime, config: ServiceConfig, handler: F)` 方法；调用方必须显式传入已创建的 `ServerRuntime`，不提供无 runtime 参数的默认 `serve` 或 `serve_with_runtime` 并列入口；`serve` 完成服务 task 投递后返回对应的 server 对象类型，不在方法内部通过 `pending` 或等价 future 挂起。
- 每个 worker 必须对应一个独立 OS 线程；每个 worker 线程内运行一个单线程 async runtime，用于驱动被投递到该 worker 的 TCP/UDP/QUIC server task 和用户 handler future。
- 支持通过受控配置设置底层 socket 选项；v0.1 至少覆盖 `reuse_address` 以及 IPv4/IPv6 transparent 相关能力，并允许后续设计定义更多不会破坏 balancer 状态的选项。
- `ServiceConfig` 支持一个可选 socket 创建后回调，默认 `None`；crate 在创建底层 socket 后、执行 bind/listen 或 runtime socket 转换前调用该回调，让调用方可以设置未被 `SocketOptions` 直接覆盖的 socket 参数。
- worker 标识属于内部实现细节，不作为用户回调参数或公开 API 契约暴露。
- TCP/UDP/QUIC-aware UDP server task 只能由 `TcpServer::serve`、`UdpServer::serve` 和 `QuicServer::serve` 投递到 `ServerRuntime` 的 worker 线程中执行；`serve` 返回的 `TcpServer`、`UdpServer` 或 `QuicServer` 对象必须提供显式关闭方法，关闭后相关 task 停止接受新连接或新数据包，不影响同一 runtime 中其他 server task。
- `UdpServer` 和 `QuicServer` 对象必须提供获取监听 socket 的公开方法；获取时优先返回调用该方法的当前线程正在监听的 socket，如果当前线程没有该 server 的监听 socket，则从该 server 持有的监听 socket 集合中随机返回一个。
- 同一套 worker 线程集合必须能够同时承载 TCP listener 和 UDP listener；协议类型属于监听项属性，而不是独立服务线程池边界。
- 提供专门的 `QuicServer` UDP 包分配入口，用于按 QUIC Destination Connection ID 或等价 QUIC 路由键把 UDP 数据包稳定分配到 worker；该入口只处理 UDP 包路由，不实现 QUIC 协议栈。
- 提供一个基于 hyper 的 HTTP 静态文件服务器示例，用于展示如何在 `examples/` 中把本 crate 的 TCP 服务能力接入上层 HTTP 协议处理；静态文件根目录必须可通过命令行参数设置。
- 在 `Cargo.toml` 中声明发布到 crates.io 所需的 package 元数据，包括 description、license、readme、repository 或 homepage/documentation、keywords、categories，以及必要时的 include/exclude 边界。

### 范围外
- 不解析 HTTP 或自定义应用协议；除 `QuicServer` 为 UDP 包分配所需的 QUIC header 路由字段外，不解析 QUIC 协议语义。
- HTTP 静态文件服务器仅作为示例程序存在，不把 HTTP 解析、静态文件服务、目录索引、缓存策略、压缩、范围请求或 MIME 完整识别提升为 library crate 的公开 API 义务。
- 不实现 QUIC TLS、handshake、connection、stream、flow control、congestion control、retransmission、ALPN、transport parameter、stateless reset 或应用层协议逻辑。
- 不提供 TLS 终止或 TLS 配置。
- 不提供连接池、限流、超时策略、重试策略，或超出 worker 循环运行所必需范围的背压策略。
- 不提供跨 worker 通信原语。
- 不在公开 API 中暴露平台特定的 reuse-port 设置细节。
- 不提供可接管、替换、长期持有或在 bind 之后修改 balancer socket 状态的 raw socket escape hatch；socket 创建后回调只能作为一次性初始化钩子进入，并且不得覆盖 balancer 必需的内部 reuse-port 或 bind 状态。
- 不支持同时启用 tokio、async-std 和 tokio-uring runtime feature 中的多个。
- 不提供文件监听、配置中心订阅或自动热加载机制。
- 不提供除 `serve` 投递 server task 和通过返回的 server 对象显式关闭该 task 之外的通用动态管理面；不提供公开 `add_tcp_listener`、`add_udp_listener` 或 `add_quic_listener` 并列入口。
- server 对象关闭不强制取消已经交付给用户回调的工作；已交付的工作随用户 future 自身生命周期完成或失败。

### 与相邻模块的边界
- 本 crate 负责 `src/` 下的 socket 均衡 API 和 worker 生命周期。
- 协议 crate 和应用框架是消费者；它们接收 stream 或 UDP packet 回调，并自行处理协议逻辑。
- `socket2` 或等价 syscall 层依赖属于实现细节；除非后续设计明确暴露，否则不构成公开 API 义务。

## 假设与歧义
- 假设：
  - v0.1 可以暴露一个小型 `ServerRuntime` 配置类型，用于描述共享 worker 数量；单个 server 配置只描述监听地址和 socket 选项。
  - `TcpServer`、`UdpServer` 和 `QuicServer` 的 convenience 入口借用调用方已创建的 `ServerRuntime`，并把相关 server future task 发送到该 runtime 的 worker 线程中执行；runtime 生命周期、server task 生命周期、server 对象关闭语义和 handler future bounds 由后续设计同步收敛。
  - 从库的角度看，每个 worker 是 share-nothing；如果用户回调需要共享状态，必须由用户通过显式传入的可 clone 状态对象自行承担，而不是依赖回调参数中的 worker id。
  - Linux 兼容调度算法的精确输入、hash 边界、TCP/UDP 元信息使用方式和版本兼容策略由后续设计定义；公开 API 不暴露 Dispatcher 或策略选择入口。
  - UDP 回调形态需要包含足够的数据包元信息以支持实际处理，同时直接使用所选 runtime 的原生 `UdpSocket`。
  - socket 选项配置需要区分跨平台选项、平台 gated 选项和需要特权或系统配置的选项；不可用或设置失败时的错误语义由后续设计定义。
  - socket 创建后回调需要由后续设计定义准确类型、调用时机、错误传播、TCP/UDP 复用方式，以及与内部必需 socket 选项冲突时的处理。
  - server task 在 `ServerRuntime` 的 worker 线程中执行；v0.1 需要让 `serve` 返回对应的 `TcpServer`、`UdpServer` 或 `QuicServer` 对象，调用方可以通过该对象显式关闭 server task；关闭只停止该 server task 的后续 accept/recv 工作，已经交付给用户回调的工作按用户 future 自身生命周期完成或失败。
  - `UdpServer` 和 `QuicServer` 返回对象需要能暴露一个当前可用于响应或发送的监听 socket；当调用线程本身正在监听该 server 的 socket 时，应优先返回本线程 socket，否则从该 server 的监听 socket 集合中随机选择一个。
  - 同一套线程同时处理 TCP 与 UDP 时，公平性、唤醒和错误隔离由后续设计定义；v0.1 不要求跨协议流量的负载感知调度。
  - `QuicServer` 由本 crate 负责 UDP 包分配与 worker 稳定路由，QUIC 协议状态由调用方或上层 QUIC crate 负责；如果上层需要连接迁移或 NAT rebinding，必须使用能够让本 crate 从数据包中恢复目标 worker 的 QUIC 路由键。
  - `QuicServer` 的公开命名表示 QUIC-aware UDP routing，不表示本 crate 提供完整 QUIC server 协议栈。
  - `runtime-tokio-uring` 可作为 Linux 定向 runtime feature 提供；非 Linux 平台上的编译期或运行时行为由后续设计定义，但必须给出明确错误或 cfg 边界。
  - HTTP 静态文件服务器示例只覆盖常见本地演示场景：监听地址和静态根目录可通过参数指定，普通文件请求返回文件内容，目录请求可尝试 `index.html`，越过静态根目录的路径必须拒绝。
  - crates.io 发布元数据应来自仓库内稳定事实：README 作为 readme，MIT 许可来自根目录 `LICENSE`，描述、keywords 和 categories 只表达本 crate 的 multi-worker socket/reuse-port/runtime 能力，不暗示完整 HTTP、TLS 或 QUIC 协议栈。
- 未决歧义：
  - crate 是否应保持 library-only，还是保留 `src/main.rs` 中的演示 binary。
  - `repository`、`homepage` 和 `documentation` URL 需要在设计或实现前确认；不能用占位 URL 发布。
  - `TcpServer`、`UdpServer` 和 `QuicServer` 的 handler trait bounds、返回 future 生命周期和停止语义需要在设计阶段与唯一 `serve` 签名同步。
  - 最低 Rust 版本和支持的 OS 版本矩阵。
  - IPv4/IPv6 transparent 等平台能力在非 Linux 平台上应表现为编译期不可用、运行时 `Unsupported`，还是由更通用的 capability 机制表达。
  - `TcpServer`、`UdpServer` 和 `QuicServer` 返回对象的关闭方法签名、关闭幂等性、关闭错误语义以及与 runtime drop 的先后关系需要在设计阶段定义。
  - server 对象关闭后的监听 socket 关闭、server task 退出和并发关闭边界需要在设计阶段定义。
  - `UdpServer` 和 `QuicServer` 获取监听 socket 方法的返回类型、随机选择算法、无可用 socket 时的错误语义，以及跨 runtime feature 的 socket clone/share 方式需要在设计阶段定义。
  - 同一 socket 地址上同时存在 TCP 与 UDP listener 时，配置 API 是否允许复用同一个逻辑名称。
  - `QuicServer` 在 Linux eBPF reuse-port selector 不可用时应返回 `Unsupported`，还是退回用户态 UDP 包分发。
  - 初始 QUIC 数据包或缺少可识别 worker 路由键的数据包应被丢弃、按来源地址分配，还是交给调用方指定的默认 worker。
  - QUIC CID 中 worker 标识固定为 DCID 前 2 字节 big-endian `u16` worker shard；上层 QUIC crate 必须按该 layout 生成 server CID，`QuicServer` 不提供其他 layout 或自动推断。
  - tokio-uring 的 current-thread 运行时、socket 类型和 future `Send` 边界是否能完全复用现有 worker thread runtime 约束，需要在设计阶段确认并定义不兼容时的 API 或实现边界。
- 批准前需确认：
  - 上述歧义可在设计阶段解决，且不会改变本提案范围。

## 约束
- 可用库/组件：
  - Rust 标准库中的网络和线程设施。
  - 由互斥 feature 选择的 runtime crate：tokio、async-std 或 tokio-uring。
  - 经设计说明后可使用 `socket2` 等低层 socket 设置 crate。
  - 经设计说明后可使用 `num_cpus` 等 CPU 数量发现 crate。
- 禁止方案：
  - 不提供 `BalancedUdpSocket` 公开封装类型。
  - 不允许用户通过 socket 选项配置覆盖 balancer 必需的内部 reuse-port 或 worker 绑定状态。
  - 不允许 socket 创建后回调替换底层 socket、延长持有 socket 可变访问权，或在回调返回后继续修改 balancer 拥有的 socket 状态。
  - 不允许为 `TcpServer`、`UdpServer` 或 `QuicServer` 同时提供无 runtime 参数的 `serve`、`serve_with_runtime`、按 server 独立 runtime config 启动的并列入口，或其他绕过显式 `&ServerRuntime` 的 convenience 入口。
  - 不要求用户协议代码按 Linux、macOS、FreeBSD 或 Windows 分支。
  - 除 crate feature 选择外，不要求用户协议代码按 tokio、async-std 或 tokio-uring 分支。
  - 不在均衡层实现协议特定状态机；`QuicServer` 仅允许实现足够的 QUIC header 路由字段读取和 worker 选择。
  - 不在 package 元数据中声明超出 crate 实际能力的协议栈、生产级静态文件服务、TLS 或完整 QUIC server 能力。
- 系统约束：
  - Linux 在可用且合适时使用原生 `SO_REUSEPORT`。
  - macOS 和 FreeBSD 使用原生 `SO_REUSEPORT`；当目标系统不能提供可用的 `SO_REUSEPORT` worker 分配能力时，回退到 Linux 兼容的内部用户态调度。
  - Windows 使用 Linux 兼容的内部用户态调度模拟，同时保持公开 API 行为一致。
  - 公开的 runtime-specific stream 和 socket 类型必须对应所选择的 runtime feature。

## 大模块子模块决策
| 子模块 | 新建或既有 | 职责 | 提案包 | 原因 |
|--------|------------|------|--------|------|
| none | none | 当前 crate 仍足够小，可由根模块包表示。 | n/a | v0.1 中 TCP、UDP、runtime 选择和内部 worker 调度共享同一个公开 crate 边界。 |

## 高层结果
- Rust 用户可以在不手写平台特定 socket 设置的情况下，把 TCP 连接或 UDP 数据包服务运行在多个 worker 上。
- 公开 API 与协议无关，通过用户 async 回调交付工作，不强加 HTTP、QUIC 或自定义协议假设。
- tokio、async-std 与 tokio-uring 通过互斥 crate feature 提供同形 API；启用 tokio-uring 时，用户回调获得 tokio-uring 原生 socket/stream 能力。
- UDP 用户获得与 Linux `SO_REUSEPORT` 语义一致的确定性 worker 分配；公开 API 不提供策略选择或自定义分发回调。
- QUIC UDP 用户可以使用专门的 `QuicServer` 入口，让具备 QUIC 路由键的数据包在多 worker reuse-port 场景下稳定进入同一 worker，而无需本 crate 接管 QUIC 协议状态。
- 用户可以通过 `serve` 入口把 TCP/UDP/QUIC-aware UDP server task 投递到 `ServerRuntime` 的 worker 线程，并让这些 task 共享同一套 worker 线程集合；`serve` 返回的 `TcpServer`、`UdpServer` 或 `QuicServer` 对象可以显式关闭对应 task。
- 用户可以显式创建一个 `ServerRuntime`，并把 TCP/UDP/QUIC server task 投递给它执行；worker 数量在 `ServerRuntime` 初始化时确定，并被该 runtime 下的所有 server task 共享。
- 用户通过同步的 `TcpServer::serve(&runtime, config, handler)`、`UdpServer::serve(&runtime, config, handler)` 或 `QuicServer::serve(&runtime, config, handler)` 使用单协议入口时，仍然复用同一个显式 `ServerRuntime`，公开 API 表面不出现第二套 runtime 创建方式，且调用完成 task 投递后立即返回对应 server 对象。
- `UdpServer` 和 `QuicServer` 调用方可以从返回对象中获取监听 socket；获取方法在 worker 线程内调用时优先返回本线程监听 socket，在非监听线程或本线程无监听 socket 时从该 server 的监听 socket 集合中随机返回一个。
- 用户可以运行 `examples/` 下的 hyper 静态文件服务器示例，并通过命令行参数选择静态文件根目录和监听地址。
- crate manifest 包含足够的发布元数据，使 crates.io 使用者能从 package 页面理解用途、许可、README、源码位置和检索分类。

## Proposal Items
| proposal_id | change_id | Outcome | Scope Boundary | Success Evidence | Explicit Non-goals |
|-------------|-----------|---------|----------------|------------------|--------------------|
| P-runtime | CHG-runtime-features | 提供互斥 runtime feature，默认 tokio，可选 async-std 或 tokio-uring。 | Cargo features 以及公开 runtime type aliases 或等价机制。 | 编译检查证明同一时间只有一个 runtime 激活，且公开回调类型匹配所选 runtime。 | 不支持同时构建多个 runtime。 |
| P-tokio-uring-runtime | CHG-tokio-uring-runtime | 增加 `runtime-tokio-uring` 互斥 runtime feature，使用户在启用该 feature 时可通过公开回调类型使用 tokio-uring 的原生 TCP/UDP I/O 接口。 | Cargo features、runtime 类型别名或适配层、worker thread runtime 启动方式、TCP/UDP/QUIC-aware UDP 回调类型映射，以及非 Linux 平台 cfg 或错误边界。 | 编译检查证明 `runtime-tokio-uring` 与其他 runtime feature 互斥；启用后公开 `TcpStream`/`UdpSocket` 类型或等价接口来自 tokio-uring；示例或测试证明用户 handler 可以调用 tokio-uring 相关 socket API。 | 不引入第四种 runtime；不支持同时启用 tokio-uring 与 tokio 或 async-std；不要求 tokio-uring 在非 Linux 平台提供可运行实现。 |
| P-server-runtime | CHG-server-runtime | 提供 `ServerRuntime` 作为共享 worker 运行时抽象，worker 数量在 runtime 初始化时确定。 | `ServerRuntime` 配置、共享 worker 持有、server task 投递入口；server config 不再拥有 worker 数量；`TcpServer`、`UdpServer`、`QuicServer` 的单协议入口只接受显式 `&ServerRuntime`，且 `serve` 是同步 task 投递方法并返回对应 server 对象。 | 测试或示例展示多个 TCP/UDP/QUIC-aware UDP server task 共享同一 `ServerRuntime` worker 配置，公开 server config 不暴露 worker 设置，不存在无 runtime 参数的 `serve` 或 `serve_with_runtime` 并列入口，且 `serve` 返回值不是 future 而是 `TcpServer`、`UdpServer` 或 `QuicServer` 对象。 | 不提供每 server 独立 worker 池作为 v0.1 默认模型；不提供隐式默认 runtime convenience API；不让 `serve` 内部长期挂起代表服务生命周期。 |
| P-worker-thread-runtime | CHG-worker-thread-runtime | 每个 worker 是一个独立线程，线程内运行单线程 async runtime。 | worker 线程创建、单线程 runtime 初始化、listener loop 和 handler future 调度边界。 | 测试或可验证实现展示 worker 启动路径使用每 worker 独立线程和单线程 runtime，而不是把 worker loop 直接 spawn 到调用方 runtime。 | 不提供 work stealing、多线程 per-worker runtime 或跨 worker task migration。 |
| P-workers | CHG-worker-model | 提供可配置的 share-nothing worker 线程，worker 标识不暴露为用户回调参数。 | Worker 生命周期和回调调用。 | 测试或示例展示显式 worker 数量与默认 CPU 数量行为，且公开回调签名不包含 worker id。 | 不提供跨 worker 消息 API。 |
| P-tcp | CHG-tcp-serve | 提供 TCP multi-worker accept 服务和 async 回调。 | 同步 `TcpServer::serve(runtime: &ServerRuntime, config: ServiceConfig, handler: F) -> Result<TcpServer, Error>`、TCP listener 设置、accept task 投递、回调交付、`TcpServer` 关闭方法。 | 测试或可运行验证展示连接被 accept，并通过不含 worker id 的回调交付；API 编译覆盖确认 `TcpServer` 只有一个同步 `serve` 入口、必须传入 `&ServerRuntime`，方法内部不使用 `pending` 挂起，且返回 `TcpServer` 可显式关闭 TCP server task。 | 不解析协议，不处理 TLS；不提供无 runtime 参数的 TCP serve 入口；不把 `serve` 做成异步 lifecycle future。 |
| P-udp | CHG-udp-runtime-socket | 通过当前 runtime 的原生 `UdpSocket` 提供 UDP 数据包服务。 | 同步 `UdpServer::serve(runtime: &ServerRuntime, config: ServiceConfig, handler: F) -> Result<UdpServer, Error>`、UDP receive/send API 表面、runtime 原生 `UdpSocket` 回调交付、`UdpServer` 关闭方法、监听 socket 获取方法；不提供 `BalancedUdpSocket` 公开类型。 | 测试或可运行验证展示数据包到达回调，handler 接收所选 runtime 的原生 `UdpSocket`，公开 API 不导出 `BalancedUdpSocket`，API 编译覆盖确认 `UdpServer` 只有一个同步 `serve` 入口且必须传入 `&ServerRuntime`，方法内部不使用 `pending` 挂起，返回 `UdpServer` 可显式关闭 UDP server task，并可按本线程优先、否则随机的规则获取监听 socket。 | 不提供 `BalancedUdpSocket` 封装类型；不提供无 runtime 参数的 UDP serve 入口；不允许通过 socket 选项配置覆盖 balancer 必需的内部 bind/reuse-port 状态；不把 `serve` 做成异步 lifecycle future。 |
| P-linux-compatible-scheduling | CHG-linux-compatible-scheduling | 移除公开 Dispatcher/DispatchPolicy 逻辑；没有可用 `SO_REUSEPORT` worker 分配能力的系统使用与 Linux `SO_REUSEPORT` 语义一致的内部调度算法。 | 内部 worker 选择、非 `SO_REUSEPORT` 平台模拟路径、TCP/UDP 元信息到调度输入的映射；公开 API 不暴露策略配置或自定义分发回调。 | 测试展示非 `SO_REUSEPORT` fallback 路径与 Linux 兼容调度结果一致，且公开 API 不导出 Dispatcher/DispatchPolicy 或 `Auto`、`RoundRobin`、`SrcHash`、`Custom` 策略。 | v0.1 不提供可配置、load-aware、adaptive 或用户自定义 scheduler。 |
| P-platform | CHG-platform-behavior | 用相同 API 屏蔽 Linux、macOS、FreeBSD 和 Windows socket 行为差异。 | 平台特定 socket 设置和 Windows 模拟路径。 | 平台 gated 编译检查或文档化验证覆盖每类支持平台。 | 不提供公开的平台特定 API 变体。 |
| P-socket-options | CHG-socket-options | 提供受控且可扩展的 socket 选项配置，至少支持 `reuse_address` 与 IPv4/IPv6 transparent 相关设置。 | TCP/UDP bind 前 socket 选项配置、平台能力表达、错误语义和与 balancer 内部状态的冲突处理。 | 测试或文档化验证展示支持的选项被应用，不支持或无权限的选项产生明确错误，且调用方不能通过选项破坏 balancer 状态。 | 不提供任意 raw fd/raw socket 访问，不把未设计的系统 socket 选项承诺为稳定 API。 |
| P-socket-init-callback | CHG-socket-init-callback | `ServiceConfig` 提供默认 `None` 的 socket 创建后回调；socket 创建完成后、bind/listen 或 runtime 转换前调用该回调，使调用方可设置额外 socket 参数。 | `ServiceConfig` 配置表面、TCP/UDP socket 创建路径、回调错误传播，以及与内部必需 socket 状态的冲突边界。 | 测试展示默认 `None` 不改变现有行为；配置回调时 TCP/UDP socket 创建路径都会调用它；回调返回错误会阻止服务启动并保留错误信息。 | 不允许回调接管或替换 socket，不允许在回调返回后继续修改 balancer 拥有的 socket 状态，不承诺任意平台私有选项跨平台可用。 |
| P-dynamic-listeners | CHG-dynamic-listeners | 支持通过 `serve` 把 TCP/UDP/QUIC-aware UDP server task 投递到 `ServerRuntime` worker 线程，并通过 `serve` 返回的 server 对象显式关闭该 task。 | server task 生命周期、公开 server 对象 API 表面、runtime drop 停止边界、worker 投递状态、server 对象关闭后 socket/task 停止边界。 | API 编译覆盖证明 `TcpServer::serve`、`UdpServer::serve` 和 `QuicServer::serve` 分别返回 `TcpServer`、`UdpServer` 和 `QuicServer` 对象且对象可关闭；运行期验证关闭单个 TCP/UDP/QUIC server 对象后该 task 停止接收新工作，其他 server task 和已交付 handler future 不被强制取消。 | 不提供 `add_tcp_listener`、`add_udp_listener` 或 `add_quic_listener` 并列入口；不提供配置文件热加载或外部配置订阅；不强制取消已经交付给用户回调的工作。 |
| P-mixed-protocol-workers | CHG-mixed-protocol-workers | 同一套 worker 线程集合同时支持 TCP 和 UDP 监听项。 | worker 池、协议 server task 投递、跨协议事件调度和错误隔离。 | 测试或可运行验证展示一个服务实例内 TCP 与 UDP server task 同时工作，并共享同一 worker 配置。 | 不提供按协议独立线程池作为 v0.1 默认模型，不实现跨协议自适应调度。 |
| P-quic-routed-udp | CHG-quic-routed-udp | 提供专门的 `QuicServer` UDP 包分配入口，按 QUIC DCID 前 2 字节 big-endian `u16` worker shard 将同一逻辑连接的数据包稳定交付到同一 worker。 | 同步 `QuicServer::serve(runtime: &ServerRuntime, config: ServiceConfig, handler: F) -> Result<QuicServer, Error>`、QUIC header 路由字段读取、16-bit worker shard 选择、非法或缺失路由键处理、Linux 高性能 reuse-port 路径、非 Linux 或不可用路径的行为边界、`QuicServer` 关闭方法和监听 socket 获取方法。 | 测试或可运行验证展示具备 16-bit QUIC 路由键的数据包被稳定路由到目标 worker，不满足固定 CID layout 的数据包被丢弃，且该入口不提供 TLS、handshake、connection 或 stream API；API 编译覆盖确认 `QuicServer` 只有一个同步 `serve` 入口、必须传入 `&ServerRuntime`，方法内部不使用 `pending` 挂起，返回 `QuicServer` 可显式关闭 QUIC-aware UDP server task，并可按本线程优先、否则随机的规则获取监听 socket。 | 不实现 QUIC 协议栈，不替代 quinn 等上层 QUIC crate，不提供 TLS 配置或 QUIC stream/connection 抽象；不支持可配置 CID layout；不提供无 runtime 参数的 QUIC-aware UDP serve 入口；不把 `serve` 做成异步 lifecycle future。 |
| P-hyper-static-example | CHG-hyper-static-example | 提供一个 hyper HTTP 静态文件服务器示例，展示上层协议如何使用本 crate 的 TCP 服务入口处理 HTTP 请求。 | `examples/` 示例代码、示例所需 Cargo 依赖或 example target 配置、命令行参数解析、静态根目录解析、基础 HTTP 响应和错误响应。 | `cargo check --example <name>` 或 harness 测试证明示例可编译；可运行验证证明 `--root <path>` 能设置静态根目录，普通文件返回 200，缺失文件返回 404，路径遍历请求不逃逸静态根目录。 | 不把 HTTP 静态服务变成 library API；不承诺生产级静态服务器能力、目录浏览、缓存控制、压缩、range 请求、TLS 或完整 MIME 数据库。 |
| P-publish-metadata | CHG-publish-metadata | 补齐 crates.io 发布所需的 `Cargo.toml` package 元数据。 | `Cargo.toml` `[package]` 元数据字段、README/LICENSE 引用、源码或文档 URL、keywords/categories，以及不会把无关开发产物打入 package 的发布边界。 | `cargo package --list` 或等价检查证明 manifest 元数据可被 Cargo 接受，README/LICENSE 被包含，package 文件列表不包含 Harness 运行缓存或无关本地产物；人工检查 description/keywords/categories 不扩大 crate 能力承诺。 | 不发布 crate，不修改 crate 公开 API，不改变 runtime feature、依赖、示例行为或测试策略。 |

## 成功标准
- 用户可见或系统可见结果：
  - 用户可以依赖 `sfo-reuseport`，选择 tokio、async-std 或 tokio-uring，初始化带有共享 worker 数量的 `ServerRuntime`，并提供 async TCP 或 UDP 回调。
  - 回调接收与所选 feature 匹配的 runtime 原生 socket/stream 值，且不暴露 worker id。
  - UDP 回调接收所选 feature 匹配的 runtime 原生 `UdpSocket`，公开 API 不导出 `BalancedUdpSocket`。
  - 用户可以在创建 TCP 或 UDP 服务时声明受控 socket 选项，例如 `reuse_address` 和 IPv4/IPv6 transparent；平台不支持或权限不足时得到明确错误。
  - 用户可以在 `ServiceConfig` 中选择性提供 socket 创建后初始化回调；未提供时行为与默认配置一致，提供时回调在 TCP/UDP 底层 socket 创建后被调用，回调失败会使服务启动失败。
  - 用户不能通过 `add_tcp_listener`、`add_udp_listener` 或 `add_quic_listener` 并列入口动态新增 listener；TCP/UDP/QUIC-aware UDP server task 通过 `TcpServer::serve`、`UdpServer::serve` 和 `QuicServer::serve` 投递到 `ServerRuntime` worker 线程，并可通过 `serve` 返回的 server 对象显式关闭。
  - `UdpServer` 和 `QuicServer` 返回对象可以获取监听 socket；如果调用线程正在监听该 server 的 socket，则优先返回本线程 socket，否则从该 server 持有的监听 socket 中随机返回一个。
  - 同一个服务实例和同一套 worker 配置可以同时处理 TCP 连接和 UDP 数据包。
  - 用户可以使用 `QuicServer` 入口获得 QUIC-aware UDP 包分配能力；同一 16-bit QUIC worker shard 对应的数据包稳定进入同一 worker，且公开 API 不包含 TLS、handshake、connection 或 stream 配置。
  - 用户可以通过 hyper 静态文件服务器示例指定静态文件根目录并验证基础 HTTP 静态文件响应，同时该示例不改变 crate 公开 API。
  - crate 的 `Cargo.toml` 包含 crates.io 发布所需 package 元数据，README/LICENSE/source URL 和检索分类准确反映本 crate 能力。
  - server config 不能单独覆盖 worker 数量；worker 数量只由 `ServerRuntime` 配置决定。
  - `TcpServer`、`UdpServer` 和 `QuicServer` 各自只暴露一个同步 `serve` 方法，签名必须接受 `runtime: &ServerRuntime`、`config: ServiceConfig` 和对应 handler；公开 API 不保留 `serve_with_runtime` 或隐式默认 runtime 入口，且 `serve` 内部不通过 `pending` 或等价 future 阻塞调用方，返回值必须分别是可显式关闭的 `TcpServer`、`UdpServer` 或 `QuicServer` 对象。
  - 每个 worker 在独立 OS 线程内运行单线程 async runtime；实现不得依赖调用方当前 runtime 的多线程调度来代表 worker。
  - 平台差异不出现在公开 API 中；公开 API 不包含 Dispatcher/DispatchPolicy 或可配置调度策略。
- 必需证据：
  - 实现前，approved `proposal.md` 和 `design.md` 都直接映射相关 `change_id`，并通过 schema/admission 检查。
  - 实现后，测试阶段通过生成或更新测试实现、可选 `testing.md`、可选 `testplan.yaml`，覆盖 runtime feature gating、tokio-uring 公开接口可用性、worker 行为、TCP 服务、UDP 服务、QUIC-aware UDP 包分配、serve 返回 server 对象的显式关闭能力、UDP/QUIC server 对象获取监听 socket 能力、混合协议 worker、Linux 兼容内部调度、socket 选项配置和平台特定编译验证。
  - `CHG-hyper-static-example` 实现后，测试阶段覆盖示例编译、可配置静态根目录、基础 200/404 响应和路径遍历拒绝。
  - `CHG-publish-metadata` 实现后，验证 package manifest 与文件列表满足发布前检查，并确认元数据没有扩大公开 API 或协议能力承诺。
  - 已实现变更集的 canonical harness 测试命令通过；若某个平台特定验证路径为 manual，必须明确记录原因。
- 明确非目标：
  - 不提供通用协议解析器、TLS 栈、QUIC 协议栈、连接池、限流器、超时管理器、重试系统或跨 worker channel 抽象。

## 风险
- 如果公开类型没有通过 feature 仔细隔离，runtime 抽象可能泄漏。
- tokio-uring 的平台支持、current-thread runtime 模型和 socket API 与 tokio/async-std 不完全一致，若设计未明确 `Send`、worker thread 和非 Linux 行为，可能导致 API 承诺无法实现。
- 如果未精确定义 Linux 兼容调度语义，Windows 或其他 fallback 用户态模拟可能偏离 Linux `SO_REUSEPORT` 行为。
- Linux 兼容调度的 hash 输入必须确定且有界，避免平台间 worker 选择漂移。
- 如果 API bounds 过于含糊，多线程回调执行会掩盖 runtime 原生 `UdpSocket` 的 clone/share 和生命周期要求。
- `serve` 返回 server 对象的 API 如果过宽，调用方会误以为 crate 支持任意动态 listener 管理、配置热加载或强制取消已交付 handler；设计和测试必须限制公开面只覆盖对应 server task 的显式关闭，以及 UDP/QUIC 监听 socket 获取能力。
- 如果 server 对象关闭与 worker task、socket 关闭或 runtime drop 的并发边界定义不足，可能出现重复关闭、关闭后仍接收新工作、其他 server task 被误停或 handler future 被意外取消。
- 如果 `UdpServer` 或 `QuicServer` 获取监听 socket 的线程优先和随机 fallback 规则定义不足，可能导致响应路径使用错误 socket、跨 worker 路由不稳定或关闭后返回无效 socket。
- TCP 与 UDP 共享同一 worker 池后，若调度和错误隔离设计不足，单一协议的高负载或 handler 错误可能影响另一协议的可用性。
- 如果 `QuicServer` 的命名没有清楚限定为 UDP 包分配入口，调用方可能误以为本 crate 提供完整 QUIC server、TLS 或 stream 语义。
- QUIC 路由字段来自未受信任网络输入；如果解析边界、非法长度、版本差异或防伪规则定义不足，可能导致错误路由、worker 倾斜或拒绝服务风险。
- Linux 高性能 reuse-port 路径若依赖 eBPF/CBPF，权限、内核版本、加载失败和非 Linux fallback 行为必须被设计和测试明确覆盖。

## 触发器记录
| Trigger Category | Applies? | Evidence | Required Checks | Completed Checks | Deferred Checks and Reason | Residual Risk |
|------------------|----------|----------|-----------------|------------------|----------------------------|---------------|
| contract/protocol | yes | 新增 `QuicServer` 公开入口并定义 QUIC-aware UDP routing 语义；收敛 `TcpServer`、`UdpServer`、`QuicServer` 为唯一显式 `&ServerRuntime` 的 `serve` 入口；`serve` 分别返回可显式关闭对应 server task 的 `TcpServer`、`UdpServer` 和 `QuicServer` 对象；UDP/QUIC server 对象可获取监听 socket。 | design 需列出公开 API、兼容性、server 对象方法、关闭方法、UDP/QUIC socket 获取方法、并发关闭和错误语义；post-implementation testing 需覆盖有效、非法和缺失路由键，不存在 `serve_with_runtime` 或隐式默认 runtime 入口的 API 编译契约，TCP/UDP/QUIC server 对象关闭行为，以及 UDP/QUIC 获取监听 socket 的线程优先和随机 fallback 行为。 | none | design 阶段先补齐实现边界；实现后 testing 阶段补齐验证。 | API 命名可能被误解为完整 QUIC server，或单协议入口与 server 对象生命周期、监听 socket 获取边界不一致，需在设计中继续约束。 |
| data/schema | no | 不涉及持久化数据、schema、缓存键或迁移。 | none | none | none | none |
| security/privacy/permission | yes | `QuicServer` 解析来自网络的不可信 UDP payload，Linux 高性能路径可能涉及 eBPF/CBPF 加载权限。 | design 需定义输入信任边界、非法包拒绝路径和 eBPF 权限失败处理；post-implementation testing 至少包含负例或滥用路径。 | none | design 阶段先补齐实现边界；实现后 testing 阶段补齐验证。 | 解析或权限失败策略未定前不能进入实现。 |
| runtime/integration | yes | 变更 UDP/TCP worker 分配、reuse-port 行为、Linux 兼容 fallback 调度、server task lifecycle、server 对象关闭路径和 UDP/QUIC 监听 socket 获取路径。 | design 需描述 Linux 兼容调度输入、路由生命周期、server 对象关闭停止边界、监听 socket 选择规则、失败行为和 fallback；post-implementation testing 需覆盖 unit/DV/integration 路径。 | none | design 阶段先补齐实现边界；实现后 testing 阶段补齐验证。 | 高负载、关闭并发、socket 选择和平台差异风险需后续验证。 |
| build/dependency/config/deployment | yes | Linux 高性能路径可能引入 eBPF/CBPF 相关 build、feature 或平台配置。 | design 需声明 feature/dependency/config 表面；post-implementation testing 需包含可复现构建或配置验证。 | none | design 阶段先补齐实现边界；实现后 testing 阶段补齐验证。 | eBPF 依赖和权限模型未定。 |
| build/dependency/config/deployment | yes | 新增 `runtime-tokio-uring` feature 会引入 tokio-uring 依赖、Linux 平台 cfg 和互斥 feature 组合。 | design 需声明依赖版本、feature 互斥规则、非 Linux 行为和 worker runtime 启动方式；post-implementation testing 需覆盖 feature 编译矩阵和 tokio-uring handler API。 | none | design 阶段先补齐实现边界；实现后 testing 阶段补齐验证。 | tokio-uring 依赖和平台限制未设计前不能进入实现。 |
| build/dependency/config/deployment | yes | crates.io 发布元数据会改变 Cargo package manifest 和发布前检查结果。 | design 需明确具体 package metadata 字段、发布文件包含/排除边界和验证命令；implementation 需只改 manifest/package 资源；post-implementation testing 或 acceptance 需记录 package manifest/list 检查证据。 | none | design 阶段先补齐发布元数据字段；实现后验证 package 列表。 | URL、keywords 或 categories 若未确认，可能导致发布页面误导或发布失败。 |
| ui/datamodel/workflow | no | crate 无 UI。 | none | none | none | none |
| harness/process | no | 不修改 harness rules、scripts、schema 或流程。 | none | none | none | none |

## 下游跟进
| follow_up_id | 归属阶段 | 原因 | 触发提案项 | 阻塞 |
|--------------|----------|------|------------|------|
| FU-001 | design | 定义公开 API 命名、回调 trait bounds、runtime 类型映射、worker 生命周期、UDP socket 表面、Linux 兼容内部调度机制和平台策略。 | P-runtime/P-workers/P-tcp/P-udp/P-linux-compatible-scheduling/P-platform | yes |
| FU-002 | testing | 为所有提案 `change_id` 增加直接验证覆盖和 `testplan.yaml` 条目。 | P-runtime/P-workers/P-tcp/P-udp/P-linux-compatible-scheduling/P-platform | yes |
| FU-003 | implementation | 仅在 proposal 和 design 均 approved、schema-check 通过，并且每个相关 `change_id` 的 admission-check 通过后实施；测试实现与测试元数据归属后续 testing 阶段。 | P-runtime/P-workers/P-tcp/P-udp/P-linux-compatible-scheduling/P-platform | yes |
| FU-004 | acceptance | 实现后审计文档、实现和验证之间的一致性。 | P-runtime/P-workers/P-tcp/P-udp/P-linux-compatible-scheduling/P-platform | yes |
| FU-005 | design | 将 TCP/UDP 用户回调签名同步为不含 worker id，并将 UDP handler 参数从 `BalancedUdpSocket` 改为当前 runtime 的原生 `UdpSocket`。 | P-workers/P-tcp/P-udp | yes |
| FU-006 | testing | 增加公开回调签名不含 worker id 的编译期覆盖，以及公开 API 不导出 `BalancedUdpSocket`、UDP handler 接收 runtime 原生 `UdpSocket` 的编译期断言。 | P-workers/P-tcp/P-udp | yes |
| FU-027 | design | 移除 `BalancedUdpSocket` 公开类型设计，更新 `UdpServer`、`QuicServer` 回调签名、re-export 列表、UDP send/response 边界和 balancer socket 状态保护说明。 | P-udp/P-quic-routed-udp/P-runtime | yes |
| FU-028 | testing | 同步测试计划，删除 `BalancedUdpSocket: Send + Sync` 与受限方法表面验证，新增 `UdpSocket` 回调签名、`BalancedUdpSocket` 不可导入和 UDP 发送路径验证。 | P-udp/P-quic-routed-udp/P-runtime | yes |
| FU-029 | implementation | 仅在 proposal/design 重新批准且 `CHG-udp-runtime-socket` 及相关 change_id 实现准入通过后，删除 `BalancedUdpSocket` 代码并迁移回调参数为 `UdpSocket`；相关测试代码和测试元数据在后续 testing 阶段同步。 | P-udp/P-quic-routed-udp | yes |
| FU-007 | design | 定义 socket 选项配置 API、IPv4/IPv6 transparent 平台能力模型、设置时机、错误类型，以及与内部 reuse-port/bind 状态冲突时的处理。 | P-socket-options/P-platform/P-tcp/P-udp | yes |
| FU-008 | testing | 为 `CHG-socket-options` 增加直接测试计划，覆盖 IPv4/IPv6 transparent 成功设置、unsupported/permission-denied 错误和不允许破坏 balancer 状态的负例。 | P-socket-options/P-platform/P-tcp/P-udp | yes |
| FU-009 | design | 定义 `serve` 返回 `TcpServer`、`UdpServer`、`QuicServer` 对象的最小公开 API 形态、显式关闭方法、关闭幂等性、并发边界、socket/server task 停止语义、runtime drop 交互，并继续禁止 `add_tcp_listener`、`add_udp_listener` 和 `add_quic_listener` 并列入口。 | P-dynamic-listeners/P-mixed-protocol-workers/P-workers/P-tcp/P-udp/P-quic-routed-udp | yes |
| FU-010 | testing | 更新测试计划，新增 TCP/UDP/QUIC server 对象关闭验证、关闭后不再接收新工作、其他 server task 不受影响、已交付 handler 不被强制取消，以及未暴露 `add_*_listener` 并列入口的 API 覆盖。 | P-dynamic-listeners/P-mixed-protocol-workers/P-workers/P-tcp/P-udp/P-quic-routed-udp | yes |
| FU-011 | implementation | 仅在 design 重新批准且 `CHG-dynamic-listeners` 实现准入通过后，为 `TcpServer`、`UdpServer` 和 `QuicServer` 的 `serve` 分别返回可显式关闭对应 server task 的 `TcpServer`、`UdpServer` 和 `QuicServer` 对象；相关验证归属后续 testing 阶段。 | P-dynamic-listeners/P-mixed-protocol-workers/P-quic-routed-udp | yes |
| FU-012 | design | 将公开服务 task 投递入口收敛为 `ServerRuntime`，并移除 server config 对 worker 数量的所有权。 | P-server-runtime/P-workers/P-dynamic-listeners/P-mixed-protocol-workers | yes |
| FU-013 | testing | 增加 `ServerRuntime` 共享 worker 配置和 server config 不暴露 worker 设置的直接验证。 | P-server-runtime/P-workers/P-dynamic-listeners/P-mixed-protocol-workers | yes |
| FU-014 | implementation | 在 `CHG-server-runtime` 实现准入通过后，迁移公开 API 与测试到 `ServerRuntime` 模型。 | P-server-runtime | yes |
| FU-015 | design | 定义 worker thread runtime 启动接口、tokio/async-std 单线程运行方式、错误处理和停止边界。 | P-worker-thread-runtime/P-workers/P-server-runtime | yes |
| FU-016 | testing | 增加每 worker 独立线程和单线程 runtime 启动路径的验证覆盖。 | P-worker-thread-runtime/P-workers | yes |
| FU-017 | implementation | 在 `CHG-worker-thread-runtime` 实现准入通过后，将 worker loop 从当前 runtime spawn 迁移到 worker thread runtime。 | P-worker-thread-runtime | yes |
| FU-018 | design | 定义 `ServiceConfig` socket 创建后回调的公开类型、默认 `None` 表达、调用时机、TCP/UDP 共享逻辑、错误传播和与内部必需 socket 选项的冲突处理。 | P-socket-init-callback/P-socket-options/P-platform/P-tcp/P-udp | yes |
| FU-019 | testing | 为 `CHG-socket-init-callback` 增加直接测试计划，覆盖默认 `None`、TCP/UDP 调用路径、回调设置可观测 socket 参数、回调错误传播和冲突边界。 | P-socket-init-callback/P-socket-options/P-platform/P-tcp/P-udp | yes |
| FU-020 | implementation | 仅在 proposal/design 重新批准且 `CHG-socket-init-callback` 实现准入通过后，在 `ServiceConfig` 和平台 socket 创建路径中实现该回调；相关验证归属后续 testing 阶段。 | P-socket-init-callback | yes |
| FU-021 | design | 定义 `QuicServer` 的公开命名、回调形态、QUIC 路由键读取边界、非法包处理、worker 稳定路由、Linux reuse-port 高性能路径、fallback/unsupported 策略和与上层 QUIC crate 的责任边界。 | P-quic-routed-udp/P-udp/P-linux-compatible-scheduling/P-platform/P-workers/P-server-runtime | yes |
| FU-030 | design | 删除 design 中的 `DispatchPolicy`/dispatcher/API 策略配置设计，替换为 Linux 兼容内部调度算法和非 `SO_REUSEPORT` 平台 fallback 边界。 | P-linux-compatible-scheduling/P-platform/P-workers/P-tcp/P-udp | yes |
| FU-031 | testing | 删除 `Auto`、`RoundRobin`、`SrcHash`、`Custom` 策略测试计划，新增 Dispatcher/DispatchPolicy 不公开和 Linux 兼容 fallback 调度一致性验证。 | P-linux-compatible-scheduling/P-platform/P-workers/P-tcp/P-udp | yes |
| FU-032 | implementation | 仅在 design 同步且 `CHG-linux-compatible-scheduling` 实现准入通过后，移除 Dispatcher 相关代码并将非 `SO_REUSEPORT` 系统迁移到 Linux 兼容内部调度；相关验证归属后续 testing 阶段。 | P-linux-compatible-scheduling/P-platform | yes |
| FU-022 | testing | 为 `CHG-quic-routed-udp` 增加直接测试计划和 `testplan.yaml` 条目，覆盖合法路由、非法或缺失路由键、跨 worker 稳定性、平台 fallback/unsupported 和不暴露 TLS/connection/stream API。 | P-quic-routed-udp | yes |
| FU-023 | implementation | 仅在 proposal/design 重新批准且 `CHG-quic-routed-udp` 实现准入通过后，实施 `QuicServer` UDP 包分配入口；相关验证归属后续 testing 阶段。 | P-quic-routed-udp | yes |
| FU-024 | design | 将 `TcpServer`、`UdpServer`、`QuicServer` 的公开 API 同步为唯一 `serve(runtime: &ServerRuntime, config: ServiceConfig, handler: F)`，移除 `serve_with_runtime` 和隐式默认 runtime 入口设计，并定义 runtime 借用、server task 投递、server 对象生命周期，以及 UDP/QUIC 获取监听 socket 方法。 | P-server-runtime/P-tcp/P-udp/P-quic-routed-udp | yes |
| FU-025 | testing | 增加公开 API 编译契约，确认三个 server 类型各自只有一个显式 `&ServerRuntime` 的 `serve` 方法，`serve` 分别返回 `TcpServer`、`UdpServer` 和 `QuicServer` 对象，UDP/QUIC 对象可获取监听 socket，并覆盖不接受无 runtime 参数调用或 `serve_with_runtime` 的负例。 | P-server-runtime/P-tcp/P-udp/P-quic-routed-udp | yes |
| FU-026 | implementation | 仅在 proposal/design 重新批准且相关 `change_id` 实现准入通过后，收敛三个 server 类型的生产代码到唯一 `serve` 入口并返回对应 server 对象，同时为 UDP/QUIC server 对象提供监听 socket 获取能力；相关测试代码和测试元数据在后续 testing 阶段同步。 | P-server-runtime/P-tcp/P-udp/P-quic-routed-udp | yes |
| FU-033 | design | 定义 `runtime-tokio-uring` 的 Cargo feature、依赖版本、公开类型映射、worker thread runtime 启动方式、handler future bounds、非 Linux cfg/错误边界，以及与现有 tokio/async-std 互斥规则的关系。 | P-tokio-uring-runtime/P-runtime/P-worker-thread-runtime/P-tcp/P-udp/P-quic-routed-udp | yes |
| FU-034 | testing | 为 `CHG-tokio-uring-runtime` 增加测试计划，覆盖 feature 互斥、Linux cfg 编译、公开 socket 类型或等价接口、用户 handler 调用 tokio-uring API，以及非 Linux 行为边界。 | P-tokio-uring-runtime | yes |
| FU-035 | implementation | 仅在 design 重新批准且 `CHG-tokio-uring-runtime` 实现准入通过后，实施 `runtime-tokio-uring` 生产代码、Cargo feature 和必要 runtime 适配；相关测试代码和测试元数据在后续 testing 阶段同步。 | P-tokio-uring-runtime | yes |
| FU-036 | acceptance | tokio-uring 实现和验证完成后，审计 proposal、design、代码、测试和运行结果是否一致，特别检查互斥 feature、平台边界和公开 API 承诺。 | P-tokio-uring-runtime | yes |
| FU-037 | design | 定义 hyper 静态文件服务器示例的文件位置、example target、依赖边界、参数格式、请求路径解析、安全边界和与 `TcpServer`/`ServerRuntime` 的连接方式。 | P-hyper-static-example | yes |
| FU-038 | implementation | 仅在 proposal/design 重新批准且 `CHG-hyper-static-example` 实现准入通过后，新增或更新 `examples/` 中的 hyper 静态文件服务器示例及必要 Cargo 示例依赖；相关测试代码和测试元数据在后续 testing 阶段同步。 | P-hyper-static-example | yes |
| FU-039 | testing | 为 `CHG-hyper-static-example` 增加测试计划或测试实现，覆盖示例编译、参数设置静态根目录、普通文件响应、缺失文件响应和路径遍历拒绝。 | P-hyper-static-example | yes |
| FU-040 | acceptance | 示例实现和验证完成后，审计 proposal、design、代码、测试和运行结果是否一致，特别检查示例没有扩大 library API 范围。 | P-hyper-static-example | yes |
| FU-041 | design | 定义 `CHG-publish-metadata` 的具体 Cargo package 字段、URL 取值、keywords/categories、README/LICENSE 引用和 package include/exclude 边界。 | P-publish-metadata | yes |
| FU-042 | implementation | 仅在 proposal/design 重新批准且 `CHG-publish-metadata` 实现准入通过后，更新 `Cargo.toml` 发布元数据和必要 package 边界。 | P-publish-metadata | yes |
| FU-043 | testing | 为 `CHG-publish-metadata` 记录或执行 package manifest/list 验证，确认 README/LICENSE 包含且无本地缓存或无关产物进入包。 | P-publish-metadata | yes |
| FU-044 | acceptance | 发布元数据实现和验证完成后，审计 manifest、README/LICENSE、package 列表和 proposal/design 是否一致。 | P-publish-metadata | yes |

## 提案护栏
- Proposal 阶段任务仅修改 `proposal.md`，除非用户明确要求多阶段更新。
- 如果提案变更需要 design、testing、implementation 或 acceptance 更新，默认只记录所需跟进，不直接编辑下游产物。
- 如果这是包含多个独立子模块的大模块，在 design 或 testing 开始前必须判断该特性是否应成为新的直接子模块。
- 拆分出的子模块 proposal 和 design 文件应放在本模块包下的子模块目录中；若生成 post-implementation testing artifacts，`testing.md` 和 `testplan.yaml` 也放在同一子模块包内。不要使用 `design/<submodule>/` 或 `testing/<submodule>/` 存放独立子模块文档。
- 人工维护的 proposal 文档应尽量低于 1000 行；如果文档会超过该限制，应拆分并更新相关文档索引。
- 如果请求有多个合理解释，记录歧义，不要静默选择。
- Proposal 批准不应依赖仅存在于聊天中的上下文；任务关键假设必须写入本文档。
- 除非约束本身属于需求，否则不要在 proposal 中写实现策略。
- 每个可进入实现的需求必须拥有稳定的 `change_id`。
- 宽泛的模块级说明不足以作为实现准入依据；相关 `change_id` 必须指向具体行为、契约或实现单元。
