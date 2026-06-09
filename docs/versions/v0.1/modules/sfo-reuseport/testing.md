---
module: sfo-reuseport
submodule:
version: v0.1
status: approved
approved_by: auto-pipeline
approved_at: 2026-06-09T15:06:06+08:00
---

# sfo-reuseport 测试

## 测试文档索引
| 文档 | 主题 | 范围 |
|------|------|------|
| `testing.md` | v0.1 测试策略 | 整个模块 |
| `testplan.yaml` | 机器可读测试入口 | 整个模块 |

本测试阶段在实现完成后运行；测试实现、测试夹具、统一测试入口接线、`testing.md` 和 `testplan.yaml` 都属于 testing 阶段产物，不是 implementation admission 的前置条件。

当前设计没有 Harness 直接子模块。`runtime`、`core`、`platform` 是 Rust 内部模块，其测试覆盖保留在根模块测试文档中。

## 统一测试入口
- 机器可读计划：`docs/versions/v0.1/modules/sfo-reuseport/testplan.yaml`
- Unit：`uv run --active python ./harness/scripts/test-run.py sfo-reuseport unit`
- DV：`uv run --active python ./harness/scripts/test-run.py sfo-reuseport dv`
- Integration：`uv run --active python ./harness/scripts/test-run.py sfo-reuseport integration`
- Module all：`uv run --active python ./harness/scripts/test-run.py sfo-reuseport all`
- Project all：`uv run --active python ./harness/scripts/test-run.py all all`
- Root shortcuts：`./test-run.sh [sfo-reuseport <level>]` and `test-run.bat [sfo-reuseport <level>]`

## 子模块测试
| 子模块 | 职责 | 详细测试文档 | 必需行为 | 边界/失败用例 | 测试类型 | 测试文件 | 状态 | 缺口/人工原因 |
|--------|------|--------------|----------|----------------|----------|----------|------|----------------|
| none | 当前没有 Harness 直接子模块。 | n/a | n/a | n/a | n/a | n/a | ready | |

## 模块级测试
| 测试项 | 覆盖边界 | 入口 | 预期结果 | 测试类型 | 测试文件/脚本 | 状态 | 缺口/人工原因 |
|--------|----------|------|----------|----------|----------------|------|----------------|
| runtime feature gating | tokio 默认、async-std 可选、同时启用时报错。 | unit/dv | 默认 feature 可编译；async-std feature 可编译；双 runtime feature compile_fail。 | automated | `tests/unit/runtime_features.rs`、trybuild 或等价 compile-fail 测试 | ready | |
| Tokio io_uring external control | 本 crate 不提供 `runtime-tokio-uring` feature；`runtime-tokio` 可与外部 `tokio/io-uring` feature/cfg 组合编译；公开网络 socket 类型、handler API 和依赖边界保持 tokio 兼容。 | unit/dv | `runtime-tokio,tokio/io-uring` 组合可在 `RUSTFLAGS="--cfg tokio_unstable"` 下编译；公开 `TcpStream` 使用 tokio net 类型且保持 `Send`，统一 `UdpSocket` 可导入并使用 tokio UDP I/O 表面；all-targets 编译覆盖 handler 中不依赖 tokio-uring net API 的类型边界；Cargo metadata 证明本 crate 不定义 `runtime-tokio-uring` feature；依赖反查证明依赖图中不存在 `tokio-uring` crate。 | automated | `tests/unit/runtime_features.rs`、`harness/scripts/assert-no-cargo-feature.py`、`cargo check --no-default-features --features runtime-tokio,tokio/io-uring --all-targets` via `test-run.py` dv、`cargo tree --no-default-features --features runtime-tokio,tokio/io-uring -i tokio-uring` negative check | ready | Tokio `io-uring` feature 当前要求 `tokio_unstable` cfg，由 `test-run.py` 对外部 io_uring 编译命令自动注入。 |
| server runtime API | `ServerRuntime` 命名、共享 worker 配置、server config 不含 worker 设置，以及单协议 server 入口只接受显式 runtime 并同步返回 server 对象。 | unit/integration | `ServerRuntimeConfig` 可设置 worker；`TcpServiceConfig`/`UdpServiceConfig` 不暴露 worker 字段或 `with_workers`；`TcpServer`、`UdpServer`、`QuicServer` 不暴露 `serve_with_runtime` 或无 runtime 参数的 `serve`；`ServerRuntime` 不暴露 `add_tcp_listener`、`add_udp_listener`、`add_quic_listener` 或 `remove_listener`；crate 根不导出 `ListenerId`/`ListenerProtocol`；`serve` 返回 `Result<TcpServer, Error>`、`Result<UdpServer, Error>` 或 `Result<QuicServer, Error>` 而不是 future，且生产代码不使用 `pending` 挂起；server 对象可 `close` 停止对应服务 task；多个 TCP/UDP listener 通过 `serve` 注册到同一 runtime。 | automated | `tests/unit/server_runtime.rs`、`tests/unit/api_signatures.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| server runtime random task submission | `ServerRuntime` 公开随机 worker task factory 投递接口。 | unit | `ServerRuntime::spawn_task` 接受 `FnOnce() -> Pin<Box<dyn Future<Output = ()> + 'static>> + Send + 'static` 等价 factory，随机投递到已有 worker runtime 后在目标 worker 内创建并执行 future，返回 task handle；factory 可跨线程发送，返回的 worker-local future 不要求 `Send` 并可持有 `Rc<RefCell<_>>`；单 worker 配置下任务在线程外的 worker runtime 执行；内部 inactive/shutdown flag 清除后投递返回明确 runtime error；`ServerRuntime` 不公开指定 worker 投递方法。 | automated | `src/core/server_runtime.rs` 内部 unit、`tests/unit/server_runtime.rs` | ready | |
| service config split | TCP 与 UDP/QUIC service config 是不同公开类型。 | unit/integration | `TcpServer::serve` 接受 `TcpServiceConfig`；`UdpServer` 和 `QuicServer` 接受 `UdpServiceConfig`；`TcpServiceConfig` 不提供 routed packet channel capacity；`UdpServiceConfig` 仅在 Windows 公开该配置并默认 4096；crate root 不导出共用 service config。 | automated | `tests/unit/api_signatures.rs`、`tests/unit/server_runtime.rs`、`tests/integration/*` | ready | |
| server handler concurrency limit | `TcpServer`、`UdpServer` 和 `QuicServer` handler 型 `serve` 的每 worker 已交付 handler future 上限。 | unit/integration/source-review | `TcpServiceConfig::max_concurrency_per_worker` / `UdpServiceConfig::max_concurrency_per_worker` 默认 `None` 且显式 0 均不限流；设置为 1 时单 worker TCP/UDP/QUIC 同时运行的 handler 不超过 1；native listener 达到上限后等待许可释放；用户态模拟 TCP/UDP/QUIC 路径映射到满载 worker 的已收工作被丢弃且不会阻塞后续 accept/recv；server close 唤醒等待许可的 native loop；`src/core/concurrency.rs` 不使用 `Mutex` 或 waiter 列表，permit helper 使用单 waiter 通知模型并提供非阻塞 `try_acquire`。 | automated | `tests/unit/api_signatures.rs`、`src/core/concurrency.rs` 内部 unit、`tests/integration/server_concurrency.rs`、`src/core/tcp.rs`/`src/core/udp.rs`/`src/core/quic.rs` source review | ready | |
| non-Send handler futures | server 回调 future 可使用 `!Send` 状态。 | unit/dv/integration | API 编译契约证明 TCP/UDP/QUIC handler closure 和 socket-only callback closure 仍要求 `Send + Sync`，但返回 future 不要求 `Send`，可在 future 内持有 `Rc<RefCell<_>>`；默认和 async-std feature 下 library check 证明 runtime 内部 local task factory 边界可编译；module/project all 覆盖示例、DV 和 integration 路径；`ServerRuntime::spawn_task` 同样通过 Send factory 在目标 worker 内创建 worker-local future，但不支持直接跨线程移动调用线程中已创建的 `!Send` future。 | automated | `tests/unit/api_signatures.rs`、`tests/unit/quic_routed_udp.rs`、`tests/unit/server_runtime.rs`、`cargo check --lib`、`cargo check --no-default-features --features runtime-async-std --lib`、`python3 ./harness/scripts/test-run.py sfo-reuseport all`、`python3 ./harness/scripts/test-run.py all all` | ready | |
| worker thread runtime | 每个 worker 对应独立 OS 线程，线程内运行单线程 async runtime。 | unit/integration | worker 启动路径使用 runtime worker-thread API；TCP/UDP listener loop 不直接使用调用方 runtime spawn 代表 worker。 | automated | `tests/unit/worker_runtime.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| worker model | 默认 CPU 数、显式 worker 数、0 worker 配置错误、常规 handler 回调签名不含 worker id，socket-only 回调包含 socket 所属 worker id。 | unit | worker 数解析符合设计；公开 TCP/UDP/QUIC handler 类型不接收 worker id；`serve_socket` 回调接收 worker id。 | automated | `tests/unit/worker_model.rs`、compile-time API tests | ready | |
| Linux compatible scheduling | Dispatcher/DispatchPolicy 不公开，fallback 用户态路径使用 Linux 兼容内部调度。 | unit/integration | `ServerRuntimeConfig` 不暴露 dispatch 配置；crate 根不能导入 `DispatchPolicy`；内部调度对固定 TCP/UDP metadata 稳定选择 worker；fallback 路径不会调用用户自定义 selector。 | automated | `tests/unit/api_signatures.rs`、`tests/unit/schedule.rs`、`tests/integration/udp_serve.rs` | ready | |
| socket options | `reuse_address`、IPv4/IPv6 transparent mode、unsupported/permission-denied 错误分类。 | unit/dv | 配置转换和错误语义稳定，不允许覆盖内部 reuse-port/bind 状态。 | automated | `tests/unit/socket_options.rs`、`tests/dv/socket_options.rs` | ready | |
| socket init callback | `TcpServiceConfig`/`UdpServiceConfig` 创建后回调的默认值、调用时机和错误传播。 | unit/dv | 默认 `None` 不改变配置；配置回调后 TCP/UDP bind 路径都会调用；回调错误阻止服务启动并保留错误信息。 | automated | `tests/unit/socket_init_callback.rs`、`tests/dv/socket_init_callback.rs` | ready | |
| platform backend selection | Linux/BSD/Windows 后端 cfg 选择、统一错误类型、统一内部能力接口和非公开平台模块边界。 | dv | 当前目标平台可编译；`platform` 不作为 public module 暴露；平台能力通过 crate 内统一 `PlatformCapabilities` 表达；Linux/BSD/Windows 后端保留同名内部接口集合。 | automated/manual | `tests/dv/platform_cfg.rs`；非当前 OS 由 CI matrix 或人工记录验证。 | manual | 单机无法覆盖所有 OS，需要 CI matrix 或人工在目标 OS 运行。 |
| listener API surface | listener 动态新增/删除不作为公开 `ServerRuntime` API。 | unit/integration | `ServerRuntime` 不提供 `add_tcp_listener`、`add_udp_listener`、`add_quic_listener` 或 `remove_listener`；crate 根不导出 `ListenerId`/`ListenerProtocol`；TCP/UDP/QUIC-aware UDP listener 仍可通过 `serve` 注册并接收工作；`TcpServer`/`UdpServer`/`QuicServer` 返回对象可显式 `close`；`UdpServer`/`QuicServer` 可返回已监听的统一 `UdpSocket`。 | automated | `tests/unit/api_signatures.rs`、`tests/unit/server_runtime.rs`、`tests/integration/dynamic_listeners.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| UDP/QUIC socket-only serve | `UdpServer::serve_socket` 和 `QuicServer::serve_socket` 通过回调交付统一监听 `UdpSocket` 和 socket 所属 worker id，返回 server 生命周期对象，不调用数据包 handler。 | unit/integration | API 编译测试确认两个 socket-only serve 入口必须传入 `&ServerRuntime`、`UdpServiceConfig` 和 socket 回调，回调接收 `UdpSocket` 与 worker id，返回 `UdpServer`/`QuicServer`；integration 测试确认应用可在回调获得的 socket 上自行 `recv_from`，单 worker 场景 worker id 为 0；fallback 验证需确认每个 worker socket 视图只接收对应 worker 应收数据。 | automated | `tests/unit/api_signatures.rs`、`tests/integration/udp_serve.rs` | ready | |
| quinn UDP socket compatibility | 默认关闭的 `quinn` feature、启用后统一 `UdpSocket` poll/readiness helper、tokio 真实 socket 和 routed fallback socket 视图可适配上层 quinn adapter。 | unit/dv/integration | 默认 features 下 quinn helper 受 cfg gate；启用 `quinn` feature 后 helper 可编译调用；Cargo feature 不引入 quinn/quinn-udp 默认依赖；tokio native socket 可通过 helper poll recv、poll send ready 和 try send；routed socket 内部测试可通过同一 helper poll 接收；async-std 组合保持可编译且 unsupported 边界明确。 | automated/manual | `tests/unit/api_signatures.rs`、`src/core/udp.rs` 内部 unit、`tests/integration/udp_serve.rs`、`tests/integration/quic_routed_udp.rs`、`harness/scripts/test-run.py` | ready | 完整 Quinn endpoint 互联用例在普通 cargo 调度下可运行，但 canonical integration 的 `--test-threads=1` 下可能 starvation，标记为 ignored/manual；核心 adapter helper 和 routed/native socket 行为仍自动覆盖。 |
| mixed protocol workers | 一个 `ServerRuntime` 实例内 TCP 与 UDP listener 同时工作，并共享同一 worker 配置。 | integration | 同一 runtime 实例同时处理 TCP connection 与 UDP packet。 | automated | `tests/integration/dynamic_listeners.rs` | ready | |
| QUIC routed UDP | `QuicServer` 对 long/short header 都按 QUIC DCID 开头的固定 2 字节 worker index 前缀稳定分配 UDP packet，不提供 QUIC 协议栈 API。 | unit/integration | long/short header worker index 前缀被解析；worker index 使用 16-bit 网络字节序编码；server Initial/0-RTT 随机 DCID 按前 2 字节取模稳定选择 worker；server-generated CID 前缀路由一致；packet 投递到对应 worker；DCID 短于边界、非法或缺失路由键不调用 handler；集成测试通过 `SFO_REUSEPORT_DISABLE_QUIC_BPF` 固定验证用户态 fallback，BPF 指令和 probe shape 保留在 unit 覆盖。 | automated | `tests/unit/quic_routed_udp.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| QUIC CID generator | `QuicCidGenerator` 生成符合 `QuicServer` 固定 worker index 前缀 layout 的 CID bytes。 | unit | 默认 CID 长度为 8；CID 前两个字节按网络字节序等于 worker index；剩余字节由 OS 随机源填充；两次生成结果随机部分不完全相同；长度低于 8 或高于 20 被拒绝；worker index 超过 `0xffff` 被拒绝；crate 根可导入该类型且不依赖 quinn 类型。 | automated | `tests/unit/quic_routed_udp.rs`、`tests/unit/api_signatures.rs` | ready | |
| QUIC Linux reuse-port BPF selector | Linux 上 best-effort 优先附加 reuse-port eBPF selector，失败或 selector 与用户态 DCID 前缀路由算法不一致时退回 CBPF，再失败时保持用户态 QuicServer fallback。 | unit/dv/integration | eBPF 程序指令生成、`BPF_PROG_TYPE_SK_REUSEPORT` load 属性、`SO_ATTACH_REUSEPORT_EBPF` attach 路径、CBPF fallback、long/short header DCID 前缀一致性探测和 worker modulo 覆盖；当前平台编译覆盖平台 cfg；loopback integration 在 selector 可用或 fallback 时均保持稳定投递。 | automated/manual | `src/platform/mod.rs` 内部 unit、`tests/dv/platform_cfg.rs`、`tests/integration/quic_routed_udp.rs` | ready | 非 Linux 只能验证 fallback/cfg；内核拒绝 eBPF/CBPF 加载或附加时以 fallback 行为作为自动验证证据。 |
| udp serve_socket example | `examples/udp_serve_socket.rs` 示例编译、参数设置监听地址和 worker 数，并通过应用自读 `UdpSocket` echo UDP packet。 | dv | `cargo check --example udp_serve_socket` 通过；smoke 脚本启动示例，向临时 UDP 端口发送 datagram，并确认从示例监听地址收到原文 echo。 | automated | `harness/scripts/test-run.py`、`harness/scripts/test-udp-serve-socket-example.py` | ready | |
| hyper static example | `examples/hyper_static.rs` 示例编译、参数设置静态根目录、基础 HTTP 静态文件响应和路径逃逸拒绝。 | dv | `cargo check --example hyper_static` 通过；smoke 脚本用临时静态根目录启动示例，验证 `/hello.txt` 返回 200、`/` 返回 index、缺失文件返回 404、`..` 和 `%2e%2e` 路径返回 403。 | automated | `harness/scripts/test-run.py`、`harness/scripts/test-hyper-static-example.py` | ready | |

## 外部接口测试
| 接口 | 职责 | 成功用例 | 失败/边界用例 | 测试类型 | 测试文档/文件 | 状态 | 缺口/人工原因 |
|------|------|----------|----------------|----------|----------------|------|----------------|
| `TcpServer::serve` | 使用显式 `&ServerRuntime` 的同步 TCP listener 注册、server 对象返回、显式关闭和 async 回调交付。 | `serve` 同步返回 `Result<TcpServer, Error>`；返回对象 `close` 后停止新的 handler work；本地 loopback 多连接被 accept，handler 接收 runtime 原生 `TcpStream`。 | bind 失败、handler 返回错误；无 runtime 参数调用和 `serve_with_runtime` 不属于公开 API；生产 `serve` 代码不使用 `pending` 挂起。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/integration/tcp_serve.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| `UdpServer::serve` | 使用显式 `&ServerRuntime` 的同步 UDP listener 注册、server 对象返回、监听 socket 获取、packet 接收、metadata 和 handler 交付。 | `serve` 同步返回 `Result<UdpServer, Error>`；返回对象 `close` 后停止新的 handler work；`listener_socket` 返回已监听的统一 `UdpSocket`；本地 loopback packet 到达 handler，handler 接收统一 `UdpSocket`、`PacketMeta`、payload，并可用该 socket 发送响应。 | bind 失败、handler 返回错误；关闭后 `listener_socket` 返回错误；无 runtime 参数调用和 `serve_with_runtime` 不属于公开 API；`BalancedUdpSocket` 和 `DispatchPolicy` 不属于公开 API；生产 `serve` 代码不使用 `pending` 挂起。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/integration/udp_serve.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| server non-Send handler futures | server 回调 future 的单线程状态支持。 | TCP/UDP/QUIC handler future 和 socket-only callback future 可在 future 内持有 `Rc<RefCell<_>>` 并通过 API 编译契约；fallback 跨 worker 投递发送 local task factory 而不是已经创建的 `!Send` future；`ServerRuntime::spawn_task` 也验证 worker-local future 可持有 `Rc<RefCell<_>>`。 | 不支持调用线程已有 `Rc` 跨线程移动；`ServerRuntime::spawn_task` 不支持直接跨线程移动调用线程中已创建的 `!Send` future。 | unit/dv/source-review | `tests/unit/api_signatures.rs`、`tests/unit/quic_routed_udp.rs`、`tests/unit/server_runtime.rs`、`src/runtime/tokio.rs`、`src/runtime/async_std.rs`、`src/core/tcp.rs`、`src/core/udp.rs`、`src/core/quic.rs` | ready | |
| UDP unified socket API | UDP/QUIC handler、`listener_socket` 和 socket-only serve 使用统一 `UdpSocket`。 | API 编译测试确认 `UdpServer` 和 `QuicServer` handler 接收 `UdpSocket`，`serve_socket` 回调接收 `UdpSocket` 和 worker id；loopback 测试通过该 socket 发送响应或自行接收。 | 编译期确认不能从 crate 根导入 `BalancedUdpSocket`；`QuicServer::serve_socket` 不解析或丢弃非 QUIC route-key packet。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/integration/udp_serve.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| `UdpSocket` quinn helper API | 启用 `quinn` feature 后为外部 quinn adapter 提供本 crate 自有 UDP poll/readiness 接口。 | `try_send_to`、`poll_send_ready`、`poll_recv_from` 和 `poll_recv_from_vectored` 可编译调用；tokio native socket loopback 可通过 helper 收发；routed socket 内部单元测试可通过 helper 从 routed queue 接收；async-std feature 组合可编译。 | 默认 features 下 helper 受 `#[cfg(feature = "quinn")]` gate；Cargo feature 不启用 quinn 默认依赖；本 crate 不实现 quinn trait。 | unit/dv/integration | `tests/unit/api_signatures.rs`、`src/core/udp.rs` 内部 unit、`tests/integration/udp_serve.rs`、`harness/scripts/test-run.py` | ready | |
| `ServerRuntime` | 运行期 worker 所有权、随机 worker task factory 投递和混合协议服务。 | TCP/UDP/QUIC 通过 `serve` 后可接收工作，并通过各自返回对象关闭；`ServerRuntime::spawn_task` 可把调用方 factory 投递到已有 worker runtime 并在目标 worker 内创建 worker-local future；`ServerRuntime` 不公开 listener 动态管理方法或指定 worker 投递方法。 | crate 根不导出 listener id 管理类型；0 worker runtime 配置错误；内部 inactive/shutdown flag 清除后投递返回 runtime error；不能直接跨线程移动调用线程中已创建的 `!Send` future。 | unit/integration | `src/core/server_runtime.rs` 内部 unit、`tests/unit/server_runtime.rs`、`tests/unit/api_signatures.rs`、`tests/integration/dynamic_listeners.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| `QuicServer` | 使用显式 `&ServerRuntime` 的同步 QUIC-aware UDP 包分配入口、server 对象返回和监听 socket 获取。 | `serve` 同步返回 `Result<QuicServer, Error>`；`listener_socket` 返回已监听的统一 `UdpSocket`；server Initial/0-RTT 使用 DCID 前 2 字节取模；带可解析 worker index 前缀的 UDP packet 被交付到对应 worker；Linux 可用时通过 reuse-port eBPF selector 预分配到 worker socket，selector 不可用或不一致时退回用户态路由；`serve_socket` 回调交付统一 socket 和 worker id，并把读取交给应用。 | 非法 packet、空 DCID、DCID 缺失 worker index 前缀或长度越界 packet 在 handler 型 `serve` 中被丢弃；关闭后 `listener_socket` 返回错误；BPF 不可用时退回用户态路由；公开 API 不包含 TLS、connection、stream 配置、无 runtime 参数 `serve` 或 `serve_with_runtime`；生产 `serve` 代码不使用 `pending` 挂起。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/unit/quic_routed_udp.rs`、`tests/integration/quic_routed_udp.rs`、`tests/integration/dynamic_listeners.rs`、`tests/integration/udp_serve.rs` | ready | |
| `QuicCidGenerator` | 公开 QUIC CID bytes 生成器。 | `generate` 和 `generate_into` 产出符合固定 2 字节 worker index 前缀 layout 的 bytes；默认长度 8，可配置 8..=20；可按 worker index 构造。 | 不实现 quinn trait；不接受短 CID；超过 16 bit 的 worker index 被拒绝；随机源失败时返回明确错误。 | unit | `tests/unit/quic_routed_udp.rs`、`tests/unit/api_signatures.rs` | ready | |
| `examples/udp_serve_socket.rs` | 展示应用如何使用 `UdpServer::serve_socket` 接管统一 `UdpSocket` 的读取循环。 | `--addr` 指向临时 UDP 端口、`--workers 1` 时可收到 datagram 并 echo。 | 示例不调用 handler 型 `UdpServer::serve`；不改变 library API。 | dv | `harness/scripts/test-udp-serve-socket-example.py` | ready | |
| `examples/hyper_static.rs` | 展示上层 HTTP 协议如何接入 `TcpServer` 并服务静态文件。 | `--root` 指向临时目录时可返回普通文件和 `index.html`。 | 缺失文件返回 404；路径遍历和编码后的路径遍历返回 403；不改变 library API。 | dv | `harness/scripts/test-hyper-static-example.py` | ready | |
| public error API | 统一错误语义，不要求调用方按平台分支。 | unsupported、permission-denied、invalid config、invalid worker index 可区分。 | 源错误保留但不泄漏平台 API 变体。 | unit | `tests/unit/error.rs` | ready | |
| `TcpServiceConfig::with_socket_init_callback` / `UdpServiceConfig::with_socket_init_callback` | TCP/UDP 底层 socket 创建后一次性初始化。 | 回调可被配置，默认 `None`，TCP/UDP 创建路径调用回调。 | 回调返回错误时服务启动失败；回调不能替换或长期持有 socket。 | unit/dv | `tests/unit/socket_init_callback.rs`、`tests/dv/socket_init_callback.rs` | ready | |
| `TcpServiceConfig::max_concurrency_per_worker` / `UdpServiceConfig::max_concurrency_per_worker` | 配置常规 handler 型 server 的每 worker 并发上限。 | 默认 `None` 不限流；builder 写入 `Some(max)`；`Some(0)` 按不限流处理；`TcpServer`、`UdpServer`、`QuicServer` 共享该配置语义；permit helper 不依赖 `Mutex` 或多 waiter 队列；非阻塞 `try_acquire` 满载时返回 `None`。 | 不改变 worker 数量；不为 `serve_socket` 增加 crate 级应用读取限流；native 路径达到上限等待许可；用户态模拟路径达到目标 worker 上限时丢弃已收工作并继续循环。 | unit/integration/source-review | `tests/unit/api_signatures.rs`、`src/core/concurrency.rs` 内部 unit、`tests/integration/server_concurrency.rs`、`src/core/tcp.rs`/`src/core/udp.rs`/`src/core/quic.rs` source review | ready | |
| Windows-only `UdpServiceConfig::routed_packet_channel_capacity` | 配置 Windows UDP/QUIC fallback routed packet channel 的每 worker 队列容量。 | 默认值等于接口层 `DEFAULT_ROUTED_PACKET_CHANNEL_CAPACITY` 4096；Windows builder 可覆盖容量；显式 0 在 `UdpServer`/`QuicServer` 校验中被拒绝；非 Windows 不暴露公开 builder/getter；`TcpServer` 不读取、不校验该字段。 | 不改变 TCP 服务、runtime executor task channel，不为测试夹具 channel 增加公开配置；实现审查生产代码中其他无界 channel。 | unit | `tests/unit/api_signatures.rs` | ready | |

## Direct Change Coverage
| change_id | design_source | validation_id | testplan_level | testplan_step_id | gap | gap_manual_reason |
|-----------|---------------|---------------|----------------|------------------|-----|-------------------|
| CHG-runtime-features | `design.md` | VAL-runtime-features | unit | runtime-default-feature | no | |
| CHG-tokio-uring-runtime | `design.md` | VAL-tokio-uring-runtime | dv | tokio-external-io-uring-feature | no | 本 crate 不提供 `runtime-tokio-uring` feature；Tokio `io-uring` feature 要求 `tokio_unstable` cfg，canonical test runner 会为外部 `tokio/io-uring` 编译命令注入该 cfg；依赖图必须不包含 `tokio-uring` crate。 |
| CHG-server-runtime | `design.md` | VAL-explicit-runtime-serve-api | unit | explicit-runtime-serve-api | no | |
| CHG-service-config-split | `design.md` | VAL-service-config-split | unit | service-config-split-api | no | API 单元覆盖 TCP/UDP/QUIC serve entrypoint 使用不同 config 类型、`TcpServiceConfig` 不含 routed packet channel capacity、`UdpServiceConfig` 仅在 Windows 公开 capacity builder/getter 且默认 4096；integration 编译覆盖真实入口调用点。 |
| CHG-worker-thread-runtime | `design.md` | VAL-worker-thread-runtime | unit | worker-thread-runtime | no | |
| CHG-worker-model | `design.md` | VAL-worker-model | unit | worker-model | no | |
| CHG-serve-local-handlers | `design.md` | VAL-serve-local-handlers | unit | worker-local-handler-futures | no | |
| CHG-tcp-serve | `design.md` | VAL-explicit-runtime-serve-api | unit | explicit-runtime-serve-api | no | |
| CHG-udp-runtime-socket | `design.md` | VAL-udp-runtime-socket | unit | udp-runtime-socket-api | no | |
| CHG-udp-quic-listener-serve | `design.md` | VAL-udp-quic-listener-serve | integration/dv | udp-quic-socket-only-serve / udp-serve-socket-example | no | |
| CHG-linux-compatible-scheduling | `design.md` | VAL-linux-compatible-scheduling | unit | linux-compatible-scheduling | no | |
| CHG-platform-behavior | `design.md` | VAL-platform-current-target | dv | platform-current-target | no | |
| CHG-socket-options | `design.md` | VAL-socket-options-unit | unit | socket-options-unit | no | |
| CHG-socket-init-callback | `design.md` | VAL-socket-init-callback | unit | socket-init-callback | no | |
| CHG-server-concurrency-limit | `design.md` | VAL-server-concurrency-limit | unit/integration | server-concurrency-limit-api / server-concurrency-limit | no | |
| CHG-dynamic-listeners | `design.md` | VAL-listener-api-surface | unit | server-runtime-listener-api | no | |
| CHG-mixed-protocol-workers | `design.md` | VAL-mixed-protocol-workers | integration | mixed-protocol-workers | no | |
| CHG-quic-routed-udp | `design.md` | VAL-explicit-runtime-serve-api | unit | explicit-runtime-serve-api | no | 包含 QUIC Initial/0-RTT DCID 前缀取模、route key parsing、worker 稳定投递、Linux reuse-port eBPF selector best-effort、CBPF fallback 和用户态 fallback 证据；DV step `quic-reuseport-bpf` 提供平台 selector 编译覆盖。 |
| CHG-routed-packet-channel-limit | `design.md` | VAL-routed-packet-channel-limit | unit | routed-packet-channel-limit-api | no | API 单元覆盖默认 4096、Windows 显式覆盖、非 Windows 不暴露公开容量接口、UDP/QUIC 0 容量校验和 TCP 不受该字段影响；实现审查确认 `src/core/udp.rs` routed packet channel 已改为有界队列，其他生产无界 channel 不属于 routed packet channel。 |
| CHG-quic-cid-generator | `design.md` | VAL-quic-cid-generator | unit | quic-cid-generator | no | CID generator 是纯 public API 和随机 bytes 生成逻辑，unit 覆盖 layout、长度边界、worker index 边界和 crate 根导出。 |
| CHG-quinn-udp-socket-compat | `design.md` | VAL-quinn-udp-socket-compat | unit/dv/integration | quinn-udp-socket-compat / quinn-feature-check / quinn-helper-loopback | no | routed fallback 通过 `src/core/udp.rs` 内部 unit 构造 routed socket 视图覆盖；tokio native 通过 integration loopback 覆盖；async-std 通过 DV compile 覆盖 unsupported 边界；GSO/GRO/ECN fast path 不属于本 crate 承诺。 |
| CHG-hyper-static-example | `design.md` | VAL-hyper-static-example | dv | hyper-static-example | no | |

## 验证理由
| 行为或风险 | 验证信号 | 为什么足够 | 缺口/人工原因 |
|------------|----------|------------|----------------|
| runtime feature 泄漏或双 runtime 同时启用。 | 默认 feature、async-std feature、双 feature compile-fail。 | 直接验证 Cargo feature 选择和公开类型隔离。 | |
| Tokio io_uring 外部 feature/cfg 和 tokio 网络接口集成边界不一致。 | 外部 `runtime-tokio,tokio/io-uring` all-targets 编译、公开类型导入测试、runtime adapter 编译、`tokio_unstable` cfg 注入和 `cargo tree -i tokio-uring` negative check。 | 验证重点是本 crate 不提供 `runtime-tokio-uring` feature、不暴露 tokio-uring 原生 net 类型，公开 `TcpStream` 保持 tokio net 类型和 `Send`，统一 `UdpSocket` 使用 tokio UDP I/O 表面，且本 crate 不依赖 `tokio-uring` crate。 | Tokio 当前要求 `--cfg tokio_unstable` 才允许 `io-uring` feature。 |
| worker 数量必须属于 `ServerRuntime`。 | unit/API 测试断言 `ServerRuntimeConfig` 拥有 worker 配置，`TcpServiceConfig`/`UdpServiceConfig` 不支持 worker 设置；integration test 通过 `serve` 在同一 `ServerRuntime` 中注册 TCP/UDP listener。 | 公开 API 表面和运行期混合 listener 行为共同证明 worker 配置在 runtime 层共享。 | |
| 单协议 server 入口必须显式复用 `ServerRuntime` 且同步返回 server 对象。 | API 测试使用 `TcpServer::serve(&runtime, ...)`、`UdpServer::serve(&runtime, ...)`、`QuicServer::serve(&runtime, ...)` 并把返回值分别约束为 `Result<TcpServer, Error>`、`Result<UdpServer, Error>`、`Result<QuicServer, Error>`；源码检查阻止 `pub async fn serve`、`serve_with_runtime` 和生产 `pending` 重新出现。 | 公开入口签名是需求边界；编译期调用和符号搜索能直接发现隐式 runtime convenience API、异步 serve 或 pending lifecycle future 回归。 | |
| 每个 worker 必须运行在独立线程和单线程 async runtime 内。 | worker runtime unit test 和 listener integration test；代码搜索确认 worker loop 不通过 `runtime::spawn` 直接放入调用方 runtime。 | worker 启动 API 是该实现边界的集中点，integration test 覆盖该路径下 TCP/UDP listener 可工作。 | |
| worker id 只应进入 socket-only serve 回调。 | compile-time API 测试使用不含 worker id 的常规 handler，并使用包含 worker id 的 socket-only callback；单 worker integration 断言回调收到 worker id 0。 | 公开签名是该需求的契约边界，编译期验证最直接；integration 覆盖实际回调交付值。 | |
| TCP 服务必须交付 runtime 原生 stream。 | loopback integration test 建立连接并由 handler 处理。 | 覆盖 bind、accept、runtime 转换和回调交付。 | |
| UDP 服务必须交付 packet metadata 和统一 `UdpSocket`。 | API 编译测试和 loopback UDP integration test。 | 同时覆盖接收、metadata、统一 socket handler 签名、响应发送路径，以及 `BalancedUdpSocket` 不再公开导出。 | |
| UDP/QUIC socket-only serve 不应读取数据或调用 handler。 | API 编译测试和 socket-only loopback integration test。 | 测试直接通过回调收到的 `UdpSocket` 自行 `recv_from`，并断言回调携带 socket 所属 worker id，证明读取由应用负责；`QuicServer::serve_socket` 接收可路由 QUIC payload，证明应用可自行处理协议数据；fallback 后端允许为隔离 worker socket 视图执行内部收包与分流。 | |
| quinn adapter helper 必须默认关闭且不绑定 quinn 版本。 | API/source 测试确认 `quinn = []`、helper 受 `#[cfg(feature = "quinn")]` gate、无 `dep:quinn`/`dep:quinn-udp`；feature 后 cargo check/test 编译 helper 调用。 | 直接验证 feature 表面、依赖边界和公开 helper 类型不含 quinn 类型；上层 adapter 的 quinn 版本差异由调用方承担。 | |
| routed socket 视图也必须能用 quinn helper 接收。 | `src/core/udp.rs` 内部 unit 构造 routed socket，向 routed channel 投递 packet，并通过 `poll_recv_from` / `poll_recv_from_vectored` 读取。 | 该测试绕过平台 reuse-port 可用性，直接覆盖 fallback routed socket 视图的核心接收语义。 | |
| `udp_serve_socket` 示例可能偏离 socket-only serve 用法或命令行参数不可用。 | DV smoke 脚本用临时 UDP 端口启动示例，发送 datagram 并确认 echo；DV 编译矩阵覆盖三个 runtime feature。 | 该验证通过真实 `cargo run --example udp_serve_socket` 路径覆盖命令行参数、`UdpServer::serve_socket` 回调交付和应用自读 `UdpSocket` 的可观察行为。 | |
| fallback 调度必须与 Linux 兼容且不暴露配置入口。 | API signature tests 阻止 `DispatchPolicy`/`with_dispatch` 重新出现；schedule unit tests 对固定 metadata 断言稳定 worker；UDP loopback 证明无自定义 selector 也能交付。 | 公开 API 负例覆盖契约边界，内部调度是纯逻辑，unit test 可稳定覆盖 hash 行为；integration 覆盖实际 UDP 服务路径。 | |
| 平台差异不进入公开 API，且上层不依赖平台专属额外导出。 | 当前目标 DV 编译、`platform` 非 public module 源码断言、平台后端同名内部接口源码断言、OS matrix/manual 验证。 | 当前目标可自动验证公开 API 边界和后端接口形态；非当前 OS 需要矩阵或人工环境验证真实平台行为。 | 单机无法覆盖全部 OS。 |
| socket options 不能破坏 balancer 状态。 | 配置 unit tests 和 DV socket setup tests。 | 验证受控配置、错误分类和禁止 raw escape hatch 的 API 表面。 | transparent 特权路径需 manual。 |
| socket 创建后回调可能改变启动路径或吞掉错误。 | unit 测试断言默认 `None`、builder API 和错误枚举；DV 测试通过 TCP/UDP bind 入口观察回调调用与错误传播。 | 回调行为集中在 `TcpServiceConfig`/`UdpServiceConfig` 和平台 bind 路径，unit/DV 可以直接覆盖不启动完整长生命周期服务的关键边界。 | |
| handler 并发限制可能泄漏许可、错误地变成全局池、在用户态模拟路径阻塞整个 accept/recv loop，或重新引入跨线程 `Mutex`/waiter 列表。 | API 测试覆盖 `TcpServiceConfig::max_concurrency_per_worker` / `UdpServiceConfig::max_concurrency_per_worker` 默认值、builder 和显式 0；`src/core/concurrency.rs` 内部 unit 覆盖 `try_acquire` 无限流和满载返回 `None`；integration 测试用单 worker、上限 1 阻塞首个 TCP/UDP/QUIC handler，断言 native 第二个工作在释放许可前不进入、释放后进入；QUIC 用户态 fallback 测试强制关闭 BPF，断言满载 worker 的第二个 packet 被丢弃、释放许可后不会补交旧 packet、后续新 packet 仍能进入；close 路径覆盖等待许可的 loop 退出；source review 检查 `src/core/concurrency.rs` 使用原子 active 计数和单个 waker，不使用 `Mutex` 或 waiter `Vec`，并检查 `src/core/tcp.rs`、`src/core/udp.rs`、`src/core/quic.rs` 的模拟 loop 使用 `try_acquire` 而非 await。 | 单 worker 场景能稳定证明每 worker permit 的关键语义、许可释放和 native 等待行为；QUIC fallback integration 真实覆盖用户态 recv loop 不因满载 worker 阻塞；TCP/UDP 模拟路径使用同一 `try_acquire` 模式并通过源码审查覆盖；多 worker 的独立计数由每 worker 私有 permit 实现和设计审计覆盖，不引入全局共享池。 | TCP/UDP fallback 路径需要非 Linux 或不支持 reuse-port 平台才能端到端运行；当前单机自动化用 QUIC 强制 fallback 加源码审查覆盖同类 TCP/UDP 模拟 loop。 |
| listener 动态管理 API 误暴露。 | unit/API 测试断言 `ServerRuntime` 不公开 `add_tcp_listener`、`add_udp_listener`、`add_quic_listener` 和 `remove_listener`，crate 根不导出 `ListenerId`/`ListenerProtocol`；integration test 通过三个 `serve` 入口证明 listener 仍可注册并处理工作，并覆盖返回对象 `close` 与 UDP/QUIC `listener_socket`。 | 同时覆盖公开 API 收窄和保留的 TCP/UDP/QUIC-aware UDP 服务能力。 | |
| TCP 与 UDP 必须共享同一服务实例。 | integration test 在一个 `ServerRuntime` 上同时注册 TCP 与 UDP listener 并观察两个 handler。 | 直接验证混合协议入口和共享 worker 配置。 | |
| `QuicServer` 不能变成 QUIC 协议栈。 | API 编译测试只使用 UDP packet handler；测试中不存在 TLS、connection、stream 配置入口。 | 公开接口和测试输入共同证明本 crate 只负责 packet routing。 | |
| QUIC 路由字段来自不可信网络输入。 | unit 测试覆盖短包、空 DCID、Initial/0-RTT DCID 前缀取模、固定 2 字节 worker index 前缀、长度越界、eBPF selector fallback 选择；integration 测试覆盖合法 long header worker index 前缀和 server-generated CID 前缀 worker 稳定性。 | 长度检查和丢弃语义是防 panic 和防错误 handler 调用的直接边界；2 字节前缀缺少第二字节的负例强制外部遵守 layout；eBPF/CBPF 只作为内核预分配优化，自动测试允许当前内核拒绝或一致性探测失败后走 fallback。 | |
| QUIC CID 生成必须与路由 layout 一致。 | unit 测试直接检查生成 bytes 的固定 2 字节 worker index 前缀、长度边界和 worker index 边界。 | 该验证保证上层 quinn adapter 使用 generator 生成的 server CID 能被 `QuicServer` 后续前缀路由解析。 | |
| hyper 静态文件示例可能逃逸静态根目录或不使用参数 root。 | DV smoke 脚本用临时目录启动示例，分别请求普通文件、目录 index、缺失文件和路径遍历。 | 该验证覆盖示例最重要的可观察行为，且通过真实 `cargo run --example hyper_static` 路径验证命令行参数和 HTTP 响应。 | |

## Unit 测试
| 测试项 | 覆盖行为 | 测试文件 |
|--------|----------|----------|
| runtime default feature | 默认 `runtime-tokio` 可编译，公开 runtime type aliases 指向 tokio。 | `tests/unit/runtime_features.rs` |
| Tokio runtime public socket types | `runtime-tokio` 下公开 `TcpStream` 使用 tokio net 类型且保持 `Send`，统一 `UdpSocket` 可导入并保持 `Send`；外部启用 `tokio/io-uring` 不改变该公开类型契约。 | `tests/unit/runtime_features.rs` |
| runtime mutual exclusion | 同时启用 `runtime-tokio` 与 `runtime-async-std` 编译失败。 | `tests/unit/runtime_features.rs` 或 trybuild fixtures |
| server runtime API | `ServerRuntimeConfig`、listener 动态管理符号不公开，以及 server config 不含 worker 设置。 | `tests/unit/server_runtime.rs` |
| server runtime random task submission | `ServerRuntime::spawn_task` 接受 Send task factory，在随机 worker 内创建并执行 worker-local future，且不公开指定 worker 投递接口。 | `tests/unit/server_runtime.rs` |
| worker thread runtime | worker thread runtime 启动 API 和多 worker listener loop 路径。 | `tests/unit/worker_runtime.rs` |
| worker model | `WorkerCount::Default`、显式 worker 数和 0 worker runtime 配置错误。 | `tests/unit/worker_model.rs` |
| callback signatures and server entrypoints | TCP/UDP/QUIC handler 不包含 worker id；socket-only serve callback 包含 worker id；`TcpServer`、`UdpServer`、`QuicServer` 只通过显式 `&ServerRuntime` 的同步 `serve` 调用，返回各自 server 对象，生产代码不使用 `pending` 挂起。 | `tests/unit/api_signatures.rs` |
| Linux compatible scheduling | 公开 API 不导出 Dispatcher/DispatchPolicy，内部 fallback 调度对固定 metadata 稳定选择 worker。 | `tests/unit/api_signatures.rs`、`tests/unit/schedule.rs` |
| UDP unified socket API | handler 接收统一 `UdpSocket`，`serve_socket` 回调交付统一 `UdpSocket` 和 worker id，crate 根不导出 `BalancedUdpSocket`。 | `tests/unit/api_signatures.rs` |
| quinn UDP socket compatibility | 默认关闭 feature gate、无 quinn 默认依赖、feature 后 helper 可调用、routed socket helper 接收。 | `tests/unit/api_signatures.rs`、`src/core/udp.rs` 内部 unit |
| socket options | reuse-address、transparent mode 和错误映射。 | `tests/unit/socket_options.rs` |
| socket init callback | 默认 `None`、builder API、callback clone 复用和错误分类。 | `tests/unit/socket_init_callback.rs` |
| server concurrency limit API | `TcpServiceConfig::max_concurrency_per_worker` / `UdpServiceConfig::max_concurrency_per_worker` 默认值、builder 和显式 0 配置语义。 | `tests/unit/api_signatures.rs` |
| error API | 统一错误枚举和源错误保留。 | `tests/unit/error.rs` |
| QUIC route key parsing | Initial/0-RTT DCID 前缀取模、long header 固定 2 字节 worker index 前缀、short header 固定 2 字节 worker index 前缀、非法长度、2 字节前缀截断和空 DCID。 | `tests/unit/quic_routed_udp.rs` |
| QUIC CID generator | 默认 8 字节 CID、固定 2 字节 worker index 前缀 layout、随机部分 smoke test、长度边界和 worker index 边界。 | `tests/unit/quic_routed_udp.rs`、`tests/unit/api_signatures.rs` |
| Linux QUIC reuse-port BPF selector | eBPF selector 指令构造、load 属性、attach 常量、CBPF fallback、worker modulo 和平台不可用 fallback 决策。 | `src/platform/mod.rs` 内部 unit、`tests/dv/platform_cfg.rs` |

## DV 测试
| 测试项 | 覆盖行为 | 测试文件/入口 |
|--------|----------|----------------|
| cargo check default | 默认 feature 下全 crate 类型检查。 | `uv run --active python ./harness/scripts/test-run.py sfo-reuseport dv` |
| async-std feature check | `runtime-async-std` feature 下全 crate 类型检查。 | `testplan.yaml` step `runtime-async-std-feature` |
| Tokio external io_uring feature check | Cargo metadata 确认不存在 `runtime-tokio-uring` feature；`runtime-tokio,tokio/io-uring` 外部组合下 all-targets 类型检查，自动注入 `--cfg tokio_unstable`，并确认依赖图不包含 `tokio-uring` crate。 | `testplan.yaml` step `tokio-external-io-uring-feature` |
| quinn feature check | 默认 runtime 和 async-std runtime 下 `quinn` feature 编译，覆盖 tokio network helper 和 async-std unsupported 编译边界。 | `harness/scripts/test-run.py` dv |
| platform current target | 当前 OS 的 platform cfg 和 socket setup 编译。 | `tests/dv/platform_cfg.rs` |
| quic reuse-port BPF selector | Linux 当前目标编译并尝试 eBPF selector 路径，失败时尝试 CBPF，再失败时返回用户态 fallback；非 Linux 验证返回 fallback。 | `tests/dv/platform_cfg.rs` |
| socket option setup | 当前 OS 下可无特权验证的 socket option 设置路径。 | `tests/dv/socket_options.rs` |
| socket init callback setup | 当前 OS 下 TCP/UDP bind 路径调用 socket 初始化回调，并传播回调错误。 | `tests/dv/socket_init_callback.rs` |
| udp serve_socket example | 示例编译、`--addr`/`--workers` 参数和应用自读 UDP echo。 | `cargo check --example udp_serve_socket`、`harness/scripts/test-udp-serve-socket-example.py` |
| hyper static example | 示例编译、`--root` 参数、200/404/403 HTTP 响应和路径遍历拒绝。 | `cargo check --example hyper_static`、`harness/scripts/test-hyper-static-example.py` |

## Integration 测试
| 测试项 | 覆盖行为 | 测试文件/入口 |
|--------|----------|----------------|
| TCP loopback serve | 多连接 loopback accept 和 handler 交付。 | `tests/integration/tcp_serve.rs` |
| TCP error paths | bind 失败、handler 错误、invalid config。 | `tests/integration/tcp_serve.rs` |
| UDP loopback serve | packet receive、metadata、统一 `UdpSocket` response path。 | `tests/integration/udp_serve.rs` |
| UDP/QUIC socket-only serve | `UdpServer::serve_socket` 和 `QuicServer::serve_socket` 回调交付统一 socket 和 worker id，应用自行 `recv_from`。 | `tests/integration/udp_serve.rs` |
| quinn helper loopback | `UdpServer::serve_socket` 返回的 native socket 通过 `poll_recv_from`、`poll_send_ready` 和 `try_send_to` 完成 loopback 收发。 | `tests/integration/udp_serve.rs` |
| UDP error paths | bind 失败和 handler 错误；UDP 服务不依赖公开 dispatch 配置。 | `tests/integration/udp_serve.rs` |
| server object close and listener socket | `TcpServer`/`UdpServer`/`QuicServer` 返回对象可关闭对应 task；`UdpServer`/`QuicServer` 可返回已监听 socket，关闭后不可继续获取。 | `tests/integration/dynamic_listeners.rs` |
| listener API surface | TCP/UDP/QUIC `serve` listener 注册，以及 `add_tcp_listener`/`add_udp_listener`/`add_quic_listener`/`remove_listener`/`ListenerId` 不公开。 | `tests/unit/api_signatures.rs`、`tests/unit/server_runtime.rs`、`tests/integration/dynamic_listeners.rs`、`tests/integration/quic_routed_udp.rs` |
| mixed protocol runtime service | 同一 `ServerRuntime` 实例同时处理 TCP 与 UDP listener。 | `tests/integration/dynamic_listeners.rs` |
| QUIC routed UDP worker stability | `QuicServer` 将 Initial/0-RTT DCID 前缀 packet 和带 DCID worker index 前缀的 packet 投递到对应 worker。 | `tests/integration/quic_routed_udp.rs` |
| QUIC routed UDP BPF fallback | Linux BPF selector 可用且与用户态算法一致时经每 worker socket 投递；不可用、不一致或非 Linux 时经用户态 fallback 仍稳定投递。 | `tests/integration/quic_routed_udp.rs` |
| server concurrency limit | TCP/UDP/QUIC handler 型 `serve` 在 native 每 worker 上限为 1 时等待许可释放后才交付第二个工作；QUIC 用户态 fallback 在满载 worker 上丢弃已收 packet 且不阻塞后续 recv；permit helper 非阻塞满载检查由内部 unit 覆盖。 | `src/core/concurrency.rs`、`tests/integration/server_concurrency.rs` |
| platform OS matrix | Linux、macOS、FreeBSD、Windows 编译和 smoke test。 | CI matrix 或 manual |
| transparent privileged path | Linux IPv4/IPv6 transparent required/best-effort 行为。 | manual |

## 回归重点
- 同时启用多个 runtime feature 必须编译失败。
- 公开 TCP/UDP/QUIC 常规数据处理回调签名不得重新引入 worker id；`UdpServer::serve_socket` 和 `QuicServer::serve_socket` 回调必须继续携带 socket 所属 worker id。
- `TcpServer`、`UdpServer`、`QuicServer` 不得重新引入 `serve_with_runtime`、无 runtime 参数 `serve` 或其他隐式默认 runtime 启动入口。
- `TcpServer`、`UdpServer`、`QuicServer` 的 `serve` 必须继续返回各自 server 对象，且 `close` 只停止该 server 的后续 task。
- `TcpServiceConfig::max_concurrency_per_worker` / `UdpServiceConfig::max_concurrency_per_worker` 必须保持默认不限制、显式 0 不限制、每 worker 独立计数；native 路径达到上限时等待许可释放；用户态模拟 TCP/UDP/QUIC 路径达到目标 worker 上限时丢弃已收工作并继续 accept/recv；该限制不得变成跨 worker 全局共享池。
- `UdpServer`、`QuicServer` 必须继续提供 `listener_socket`，优先当前监听线程 socket，否则从该 server 的监听 socket 集合中选择一个。
- `BalancedUdpSocket` 不得重新出现在公开 API；UDP/QUIC handler、`listener_socket` 和 `serve_socket` 应继续接收或回调交付统一 `UdpSocket`，且 socket-only serve 回调应继续包含 worker id。
- `quinn` feature 必须保持默认关闭；`UdpSocket` quinn helper 不得出现在默认 API 中，不得引入默认 quinn/quinn-udp 依赖，不得让本 crate 直接实现 quinn trait。
- `DispatchPolicy`、Dispatcher 类型、`Auto`/`RoundRobin`/`SrcHash`/`Custom` 策略和 `with_dispatch` 不得重新出现在公开 API。
- fallback 用户态调度必须保持 Linux 兼容内部 hash 语义，不得引入用户自定义 selector。
- Windows 用户态模拟不得改变公开 API。
- `TransparentMode::Required` 与 `BestEffort` 的错误语义不得混淆。
- `QuicServer` 不得新增 TLS、connection 或 stream API；非法 QUIC 路由 packet、缺少完整 2 字节 DCID worker index 前缀的 packet 不得调用 handler。
- `QuicCidGenerator` 不得引用 quinn 类型或实现 quinn trait；生成 CID 的开头必须继续匹配 `QuicServer` 的 worker index 前缀 layout。

## 下游跟进
| follow_up_id | 归属阶段 | 原因 | 触发测试项 | 阻塞 |
|--------------|----------|------|------------|------|
| FU-TEST-001 | testing | 按本测试计划创建或维护对应 unit/dv/integration 测试文件，并确保它们可通过统一测试入口执行。 | 全部自动测试项 | yes |
| FU-TEST-004 | acceptance | 实现后审计 manual 验证结果和自动测试结果。 | platform OS matrix、transparent privileged path | yes |
| FU-TEST-005 | testing | 按 listener API 表面和混合协议 worker 验证项维护 unit/integration 测试文件并通过 canonical test entry。 | listener API surface、mixed protocol workers | yes |
| FU-TEST-006 | testing | 按 `ServerRuntime` 模型更新公开 API 测试和集成测试，确保 worker 配置不再位于 server/listener config。 | server runtime API | yes |
| FU-TEST-022 | acceptance | 随机 worker task 投递实现后审计 proposal、design、代码和 unit 测试是否一致，特别检查任务在 worker runtime 执行、inactive runtime 投递报错，以及不公开指定 worker 投递接口。 | server runtime random task submission | yes |
| FU-TEST-007 | testing | 增加 worker thread runtime 测试，覆盖 TCP/UDP `serve` listener loop 已迁移到 worker thread runtime 启动路径。 | worker thread runtime | yes |
| FU-TEST-008 | testing | 增加 `TcpServiceConfig`/`UdpServiceConfig` socket 创建后回调测试，覆盖默认 `None`、TCP/UDP 调用路径与错误传播。 | socket init callback | yes |
| FU-TEST-009 | testing | 增加 `QuicServer` 测试，覆盖 QUIC Initial/0-RTT DCID 前缀取模、route key parsing、稳定 worker 投递、Linux reuse-port BPF selector best-effort 路径、fallback 和非法 packet 丢弃。 | QUIC routed UDP | yes |
| FU-TEST-020 | testing | 增加 `QuicCidGenerator` 测试，覆盖 CID bytes layout、长度边界、worker index 边界、随机部分 smoke test 和 crate 根导出。 | QUIC CID generator | yes |
| FU-TEST-010 | testing | 更新公开 API 测试、loopback integration tests 和示例验证，确保 `TcpServer`、`UdpServer`、`QuicServer` 只使用 `serve(&ServerRuntime, typed service config, handler)`。 | explicit runtime serve API | yes |
| FU-TEST-011 | testing | 更新 API signature、UDP loopback 和 QUIC routed UDP 测试到统一 `UdpSocket` handler，并确认 `BalancedUdpSocket` 不再公开。 | UDP unified socket API | yes |
| FU-TEST-015 | acceptance | socket-only serve 实现后审计 proposal、design、代码、API 测试和 integration 测试是否一致，特别检查该入口通过回调交付 socket 与 socket 所属 worker id、fallback 每 worker socket 视图不暴露全量共享收包口且 Windows/fallback 机制不进入公开 API。 | UDP/QUIC socket-only serve | yes |
| FU-TEST-016 | acceptance | quinn UDP socket compatibility 实现后审计 proposal、design、代码、feature gate、API 测试和 native/routed socket 验证是否一致，特别检查本 crate 不直接依赖或实现 quinn trait。 | quinn UDP socket compatibility | yes |
| FU-TEST-012 | testing | 删除 Dispatcher/DispatchPolicy 相关策略测试和 custom dispatcher 集成测试，新增内部 schedule 单元测试与公开 API 负例。 | Linux compatible scheduling | yes |
| FU-TEST-013 | acceptance | Tokio io_uring 外部控制边界实现后审计不提供 `runtime-tokio-uring` feature、Tokio io_uring cfg、公开网络 socket 类型保持 tokio net、handler 不依赖 tokio-uring net API、依赖图不包含 `tokio-uring` crate 和测试入口证据。 | Tokio io_uring external control | yes |
| FU-TEST-014 | acceptance | hyper 静态文件服务器示例实现后审计 proposal、design、示例代码、Cargo 依赖和 smoke 验证是否一致。 | hyper static example | yes |
| FU-TEST-021 | acceptance | 并发限制实现后审计 proposal、design、代码、API 测试和 integration 测试是否一致，特别检查默认/0 不限流、每 worker permit、native 上限等待、用户态模拟满载丢弃且不阻塞循环、许可释放和关闭唤醒边界。 | server handler concurrency limit | yes |

## 完成定义
- [x] 测试文档覆盖所有直接子模块，或说明不存在直接子模块。
- [x] 当提案/设计使用直接子模块时，大模块测试文档也拆分为直接子模块包；当前未使用直接子模块。
- [x] 人工维护的 testing 文档保持在 1000 行以内。
- [x] `testplan.yaml` 与声明的测试入口一致。
- [x] 模块级测试覆盖关键边界行为和失败路径。
- [x] 外部接口拥有契约导向测试。
- [x] 每个已实现变更都有直接验证覆盖，或有明确缺口。
- [x] 每个已实现 `change_id` 都出现在 `proposal.md`、`design.md` 和生成的测试证据中；可选 `testing.md` 与 `testplan.yaml` 存在时也包含相同 `change_id`，除非验证路径明确为 `manual` 或 `disabled`。
- [x] 每个验证路径都映射到具体行为、风险或成功标准。
- [x] 任意 `manual` 或 `disabled` 层都在 `testing.md` 和 `testplan.yaml` 中使用相同原因。
- [x] 相关自动化测试通过，或执行结果和环境限制已记录；测试文件和测试入口接线已在 testing 阶段完成。
