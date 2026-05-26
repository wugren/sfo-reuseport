---
module: sfo-reuseport
submodule:
version: v0.1
status: approved
approved_by: auto-pipeline
approved_at: 2026-05-26T15:09:01Z
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
| tokio-uring runtime feature | `runtime-tokio-uring` 互斥 feature、Linux cfg、公开 socket 类型和 handler API。 | unit/dv | tokio-uring feature 可在 Linux 下编译；与其他 runtime feature 互斥；公开 `TcpStream`/`UdpSocket` 类型可导入；tokio-uring all-targets 编译覆盖 handler 中使用 tokio-uring 风格 UDP send/recv 接口的类型边界。 | automated | `tests/unit/runtime_features.rs`、`cargo check --no-default-features --features runtime-tokio-uring --all-targets` via `test-run.py` dv | ready | 非 Linux 由 compile_error 边界覆盖，需要目标平台编译验证。 |
| server runtime API | `ServerRuntime` 命名、共享 worker 配置、server/listener config 不含 worker 设置，以及单协议 server 入口只接受显式 runtime 并同步返回。 | unit/integration | `ServerRuntimeConfig` 可设置 worker；`ServiceConfig`/`ListenerConfig` 不暴露 worker 字段或 `with_workers`；`TcpServer`、`UdpServer`、`QuicServer` 不暴露 `serve_with_runtime` 或无 runtime 参数的 `serve`；`serve` 返回 `Result<(), Error>` 而不是 future，且生产代码不使用 `pending` 挂起；多个 TCP/UDP listener 注册到同一 runtime。 | automated | `tests/unit/server_runtime.rs`、`tests/unit/api_signatures.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| worker thread runtime | 每个 worker 对应独立 OS 线程，线程内运行单线程 async runtime。 | unit/integration | worker 启动路径使用 runtime worker-thread API；TCP/UDP listener loop 不直接使用调用方 runtime spawn 代表 worker。 | automated | `tests/unit/worker_runtime.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| worker model | 默认 CPU 数、显式 worker 数、0 worker 配置错误、回调签名不含 worker id。 | unit | worker 数解析符合设计；公开 handler 类型不接收 worker id。 | automated | `tests/unit/worker_model.rs`、compile-time API tests | ready | |
| Linux compatible scheduling | Dispatcher/DispatchPolicy 不公开，fallback 用户态路径使用 Linux 兼容内部调度。 | unit/integration | `ServerRuntimeConfig` 不暴露 dispatch 配置；crate 根不能导入 `DispatchPolicy`；内部调度对固定 TCP/UDP metadata 稳定选择 worker；fallback 路径不会调用用户自定义 selector。 | automated | `tests/unit/api_signatures.rs`、`tests/unit/schedule.rs`、`tests/integration/udp_serve.rs` | ready | |
| socket options | `reuse_address`、IPv4 transparent mode、unsupported/permission-denied 错误分类。 | unit/dv | 配置转换和错误语义稳定，不允许覆盖内部 reuse-port/bind 状态。 | automated | `tests/unit/socket_options.rs`、`tests/dv/socket_options.rs` | ready | |
| socket init callback | `ServiceConfig` 创建后回调的默认值、调用时机和错误传播。 | unit/dv | 默认 `None` 不改变配置；配置回调后 TCP/UDP bind 路径都会调用；回调错误阻止服务启动并保留错误信息。 | automated | `tests/unit/socket_init_callback.rs`、`tests/dv/socket_init_callback.rs` | ready | |
| platform backend selection | Linux/BSD/Windows 后端 cfg 选择和统一错误类型。 | dv | 当前目标平台可编译；平台能力通过统一 `PlatformCapabilities` 表达。 | automated/manual | `tests/dv/platform_cfg.rs`；非当前 OS 由 CI matrix 或人工记录验证。 | manual | 单机无法覆盖所有 OS，需要 CI matrix 或人工在目标 OS 运行。 |
| dynamic listeners | 服务运行期新增和删除 TCP/UDP listener。 | integration | 新增 listener 后可接收工作；删除 listener 后不再接收新工作；未知 listener id 返回明确错误。 | automated | `tests/integration/dynamic_listeners.rs` | ready | |
| mixed protocol workers | 一个 `ServerRuntime` 实例内 TCP 与 UDP listener 同时工作，并共享同一 worker 配置。 | integration | 同一 runtime 实例同时处理 TCP connection 与 UDP packet。 | automated | `tests/integration/dynamic_listeners.rs` | ready | |
| QUIC routed UDP | `QuicServer` 按 QUIC DCID 前 2 字节 big-endian `u16` worker shard 稳定分配 UDP packet，不提供 QUIC 协议栈 API。 | unit/integration | long header 16-bit DCID worker shard 被解析；packet 投递到对应 worker；DCID 短于 2 字节、非法或缺失路由键不调用 handler。 | automated | `tests/unit/quic_routed_udp.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| QUIC Linux reuse-port BPF selector | Linux 上 best-effort 优先附加 reuse-port eBPF selector，失败时退回 CBPF，再失败时保持用户态 QuicServer fallback。 | unit/dv/integration | eBPF 程序指令生成、`BPF_PROG_TYPE_SK_REUSEPORT` load 属性、`SO_ATTACH_REUSEPORT_EBPF` attach 路径、CBPF fallback 和 worker modulo 覆盖；当前平台编译覆盖平台 cfg；loopback integration 在 selector 可用或 fallback 时均保持稳定投递。 | automated/manual | `src/platform/mod.rs` 内部 unit、`tests/dv/platform_cfg.rs`、`tests/integration/quic_routed_udp.rs` | ready | 非 Linux 只能验证 fallback/cfg；内核拒绝 eBPF/CBPF 加载或附加时以 fallback 行为作为自动验证证据。 |
| hyper static example | `examples/hyper_static.rs` 示例编译、参数设置静态根目录、基础 HTTP 静态文件响应和路径逃逸拒绝。 | dv | `cargo check --example hyper_static` 通过；smoke 脚本用临时静态根目录启动示例，验证 `/hello.txt` 返回 200、`/` 返回 index、缺失文件返回 404、`..` 和 `%2e%2e` 路径返回 403。 | automated | `harness/scripts/test-run.py`、`harness/scripts/test-hyper-static-example.py` | ready | |

## 外部接口测试
| 接口 | 职责 | 成功用例 | 失败/边界用例 | 测试类型 | 测试文档/文件 | 状态 | 缺口/人工原因 |
|------|------|----------|----------------|----------|----------------|------|----------------|
| `TcpServer::serve` | 使用显式 `&ServerRuntime` 的同步 TCP listener 注册和 async 回调交付。 | `serve` 同步返回 `Result<(), Error>`；本地 loopback 多连接被 accept，handler 接收 runtime 原生 `TcpStream`。 | bind 失败、handler 返回错误；无 runtime 参数调用和 `serve_with_runtime` 不属于公开 API；生产 `serve` 代码不使用 `pending` 挂起。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/integration/tcp_serve.rs` | ready | |
| `UdpServer::serve` | 使用显式 `&ServerRuntime` 的同步 UDP listener 注册、packet 接收、metadata 和 handler 交付。 | `serve` 同步返回 `Result<(), Error>`；本地 loopback packet 到达 handler，handler 接收 runtime 原生 `UdpSocket`、`PacketMeta`、payload，并可用该 socket 发送响应。 | bind 失败、handler 返回错误；无 runtime 参数调用和 `serve_with_runtime` 不属于公开 API；`BalancedUdpSocket` 和 `DispatchPolicy` 不属于公开 API；生产 `serve` 代码不使用 `pending` 挂起。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/integration/udp_serve.rs` | ready | |
| UDP runtime socket API | UDP/QUIC handler 使用 runtime 原生 `UdpSocket`。 | API 编译测试确认 `UdpServer`、`QuicServer` 和动态 UDP listener handler 接收 `UdpSocket`；loopback 测试通过该 socket 发送响应。 | 编译期确认不能从 crate 根导入 `BalancedUdpSocket`。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/integration/udp_serve.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| `ServerRuntime` | 运行期 listener 管理和混合协议服务。 | `add_tcp_listener`、`add_udp_listener` 后可接收工作，`remove_listener` 后停止新工作。 | 删除未知 listener、删除后已交付工作不被强制中断、0 worker runtime 配置错误。 | unit/integration | `tests/unit/server_runtime.rs`、`tests/integration/dynamic_listeners.rs` | ready | |
| `QuicServer` | 使用显式 `&ServerRuntime` 的同步 QUIC-aware UDP 包分配入口。 | `serve` 同步返回 `Result<(), Error>`；带可解析 16-bit DCID worker shard 的 UDP packet 被交付到对应 worker；Linux 可用时通过 reuse-port eBPF selector 预分配到 worker socket，eBPF 不可用时退回 CBPF 或用户态路由。 | 非法 packet、空 DCID、DCID 短于 2 字节或长度越界 packet 被丢弃；BPF 不可用时退回用户态路由；公开 API 不包含 TLS、connection、stream 配置、无 runtime 参数 `serve` 或 `serve_with_runtime`；生产 `serve` 代码不使用 `pending` 挂起。 | unit/integration | `tests/unit/api_signatures.rs`、`tests/unit/quic_routed_udp.rs`、`tests/integration/quic_routed_udp.rs` | ready | |
| `examples/hyper_static.rs` | 展示上层 HTTP 协议如何接入 `TcpServer` 并服务静态文件。 | `--root` 指向临时目录时可返回普通文件和 `index.html`。 | 缺失文件返回 404；路径遍历和编码后的路径遍历返回 403；不改变 library API。 | dv | `harness/scripts/test-hyper-static-example.py` | ready | |
| public error API | 统一错误语义，不要求调用方按平台分支。 | unsupported、permission-denied、invalid config、invalid worker index 可区分。 | 源错误保留但不泄漏平台 API 变体。 | unit | `tests/unit/error.rs` | ready | |
| `ServiceConfig::with_socket_init_callback` | TCP/UDP 底层 socket 创建后一次性初始化。 | 回调可被配置，默认 `None`，TCP/UDP 创建路径调用回调。 | 回调返回错误时服务启动失败；回调不能替换或长期持有 socket。 | unit/dv | `tests/unit/socket_init_callback.rs`、`tests/dv/socket_init_callback.rs` | ready | |

## Direct Change Coverage
| change_id | design_source | validation_id | testplan_level | testplan_step_id | gap | gap_manual_reason |
|-----------|---------------|---------------|----------------|------------------|-----|-------------------|
| CHG-runtime-features | `design.md` | VAL-runtime-features | unit | runtime-default-feature | no | |
| CHG-tokio-uring-runtime | `design.md` | VAL-tokio-uring-runtime | dv | runtime-tokio-uring-feature | no | Linux-only feature；非 Linux 以 compile_error/cfg 边界作为验证信号。 |
| CHG-server-runtime | `design.md` | VAL-explicit-runtime-serve-api | unit | explicit-runtime-serve-api | no | |
| CHG-worker-thread-runtime | `design.md` | VAL-worker-thread-runtime | unit | worker-thread-runtime | no | |
| CHG-worker-model | `design.md` | VAL-worker-model | unit | worker-model | no | |
| CHG-tcp-serve | `design.md` | VAL-explicit-runtime-serve-api | unit | explicit-runtime-serve-api | no | |
| CHG-udp-runtime-socket | `design.md` | VAL-udp-runtime-socket | unit | udp-runtime-socket-api | no | |
| CHG-linux-compatible-scheduling | `design.md` | VAL-linux-compatible-scheduling | unit | linux-compatible-scheduling | no | |
| CHG-platform-behavior | `design.md` | VAL-platform-current-target | dv | platform-current-target | no | |
| CHG-socket-options | `design.md` | VAL-socket-options-unit | unit | socket-options-unit | no | |
| CHG-socket-init-callback | `design.md` | VAL-socket-init-callback | unit | socket-init-callback | no | |
| CHG-dynamic-listeners | `design.md` | VAL-dynamic-listeners | integration | dynamic-listeners | no | |
| CHG-mixed-protocol-workers | `design.md` | VAL-mixed-protocol-workers | integration | mixed-protocol-workers | no | |
| CHG-quic-routed-udp | `design.md` | VAL-explicit-runtime-serve-api | unit | explicit-runtime-serve-api | no | 包含 QUIC route key parsing、worker 稳定投递、Linux reuse-port eBPF selector best-effort、CBPF fallback 和用户态 fallback 证据；DV step `quic-reuseport-bpf` 提供平台 selector 编译覆盖。 |
| CHG-hyper-static-example | `design.md` | VAL-hyper-static-example | dv | hyper-static-example | no | |

## 验证理由
| 行为或风险 | 验证信号 | 为什么足够 | 缺口/人工原因 |
|------------|----------|------------|----------------|
| runtime feature 泄漏或双 runtime 同时启用。 | 默认 feature、async-std feature、双 feature compile-fail。 | 直接验证 Cargo feature 选择和公开类型隔离。 | |
| tokio-uring runtime API 与现有 Send 模型不一致。 | Linux tokio-uring feature all-targets 编译、公开类型导入测试、tokio-uring feature 下 integration tests cfg 分离和 runtime adapter 编译。 | tokio-uring 原生 net 类型为 `!Send`/`!Sync`，因此验证重点是 adapter 将 socket future 创建和 poll 保留在 worker thread runtime 内，并且公开类型在该 feature 下可用。 | 非 Linux 运行行为不承诺，使用 compile_error 边界。 |
| worker 数量必须属于 `ServerRuntime`。 | unit/API 测试断言 `ServerRuntimeConfig` 拥有 worker 配置，`ServiceConfig`/`ListenerConfig` 不支持 worker 设置；integration test 在同一 `ServerRuntime` 中注册 TCP/UDP listener。 | 公开 API 表面和运行期混合 listener 行为共同证明 worker 配置在 runtime 层共享。 | |
| 单协议 server 入口必须显式复用 `ServerRuntime` 且同步返回。 | API 测试使用 `TcpServer::serve(&runtime, ...)`、`UdpServer::serve(&runtime, ...)`、`QuicServer::serve(&runtime, ...)` 并把返回值约束为 `Result<(), Error>`；源码检查阻止 `pub async fn serve`、`serve_with_runtime` 和生产 `pending` 重新出现。 | 公开入口签名是需求边界；编译期调用和符号搜索能直接发现隐式 runtime convenience API、异步 serve 或 pending lifecycle future 回归。 | |
| 每个 worker 必须运行在独立线程和单线程 async runtime 内。 | worker runtime unit test 和 listener integration test；代码搜索确认 worker loop 不通过 `runtime::spawn` 直接放入调用方 runtime。 | worker 启动 API 是该实现边界的集中点，integration test 覆盖该路径下 TCP/UDP listener 可工作。 | |
| worker id 不应暴露给用户回调。 | compile-time API 测试使用不含 worker id 的 handler，并用负例阻止含 worker id 的签名。 | 公开签名是该需求的契约边界，编译期验证最直接。 | |
| TCP 服务必须交付 runtime 原生 stream。 | loopback integration test 建立连接并由 handler 处理。 | 覆盖 bind、accept、runtime 转换和回调交付。 | |
| UDP 服务必须交付 packet metadata 和 runtime 原生 `UdpSocket`。 | API 编译测试和 loopback UDP integration test。 | 同时覆盖接收、metadata、runtime socket handler 签名、响应发送路径，以及 `BalancedUdpSocket` 不再公开导出。 | |
| fallback 调度必须与 Linux 兼容且不暴露配置入口。 | API signature tests 阻止 `DispatchPolicy`/`with_dispatch` 重新出现；schedule unit tests 对固定 metadata 断言稳定 worker；UDP loopback 证明无自定义 selector 也能交付。 | 公开 API 负例覆盖契约边界，内部调度是纯逻辑，unit test 可稳定覆盖 hash 行为；integration 覆盖实际 UDP 服务路径。 | |
| 平台差异不进入公开 API。 | 当前目标 DV 编译、平台能力 unit tests、OS matrix/manual 验证。 | 当前目标可自动验证；非当前 OS 需要矩阵或人工环境。 | 单机无法覆盖全部 OS。 |
| socket options 不能破坏 balancer 状态。 | 配置 unit tests 和 DV socket setup tests。 | 验证受控配置、错误分类和禁止 raw escape hatch 的 API 表面。 | transparent 特权路径需 manual。 |
| socket 创建后回调可能改变启动路径或吞掉错误。 | unit 测试断言默认 `None`、builder API 和错误枚举；DV 测试通过 TCP/UDP bind 入口观察回调调用与错误传播。 | 回调行为集中在 `ServiceConfig` 和平台 bind 路径，unit/DV 可以直接覆盖不启动完整长生命周期服务的关键边界。 | |
| 动态 listener 删除语义不清。 | integration test 先新增 listener 并验证工作交付，再删除 listener 并验证后续连接或数据包不再交付。 | 覆盖 add/remove registry、停止信号和本地 wake-up 的可观察行为。 | |
| TCP 与 UDP 必须共享同一服务实例。 | integration test 在一个 `ServerRuntime` 上同时注册 TCP 与 UDP listener 并观察两个 handler。 | 直接验证混合协议入口和共享 worker 配置。 | |
| `QuicServer` 不能变成 QUIC 协议栈。 | API 编译测试只使用 UDP packet handler；测试中不存在 TLS、connection、stream 配置入口。 | 公开接口和测试输入共同证明本 crate 只负责 packet routing。 | |
| QUIC 路由字段来自不可信网络输入。 | unit 测试覆盖短包、空 DCID、1 字节 DCID、长度越界、eBPF selector 指令生成和 fallback 选择；integration 测试覆盖合法 long header 16-bit DCID worker shard。 | 长度检查和丢弃语义是防 panic 和防错误 handler 调用的直接边界；1 字节 DCID 负例强制外部遵守 16-bit layout；eBPF/CBPF 只作为内核预分配优化，自动测试允许当前内核拒绝后走 fallback。 | |
| hyper 静态文件示例可能逃逸静态根目录或不使用参数 root。 | DV smoke 脚本用临时目录启动示例，分别请求普通文件、目录 index、缺失文件和路径遍历。 | 该验证覆盖示例最重要的可观察行为，且通过真实 `cargo run --example hyper_static` 路径验证命令行参数和 HTTP 响应。 | |

## Unit 测试
| 测试项 | 覆盖行为 | 测试文件 |
|--------|----------|----------|
| runtime default feature | 默认 `runtime-tokio` 可编译，公开 runtime type aliases 指向 tokio。 | `tests/unit/runtime_features.rs` |
| tokio-uring runtime feature | `runtime-tokio-uring` 下公开 runtime socket 类型可导入，Send 断言按 tokio-uring `!Send` 原生边界关闭。 | `tests/unit/runtime_features.rs` |
| runtime mutual exclusion | 同时启用 `runtime-tokio` 与 `runtime-async-std` 编译失败。 | `tests/unit/runtime_features.rs` 或 trybuild fixtures |
| server runtime API | `ServerRuntimeConfig`、`ListenerConfig`、`ListenerId`、未知 listener 错误，以及 server/listener config 不含 worker 设置。 | `tests/unit/server_runtime.rs` |
| worker thread runtime | worker thread runtime 启动 API 和多 worker listener loop 路径。 | `tests/unit/worker_runtime.rs` |
| worker model | `WorkerCount::Default`、显式 worker 数和 0 worker runtime 配置错误。 | `tests/unit/worker_model.rs` |
| callback signatures and server entrypoints | TCP/UDP/QUIC handler 不包含 worker id；`TcpServer`、`UdpServer`、`QuicServer` 只通过显式 `&ServerRuntime` 的同步 `serve` 调用，返回 `Result<(), Error>`，生产代码不使用 `pending` 挂起。 | `tests/unit/api_signatures.rs` |
| Linux compatible scheduling | 公开 API 不导出 Dispatcher/DispatchPolicy，内部 fallback 调度对固定 metadata 稳定选择 worker。 | `tests/unit/api_signatures.rs`、`tests/unit/schedule.rs` |
| UDP runtime socket API | handler 接收 runtime 原生 `UdpSocket`，crate 根不导出 `BalancedUdpSocket`。 | `tests/unit/api_signatures.rs` |
| socket options | reuse-address、transparent mode 和错误映射。 | `tests/unit/socket_options.rs` |
| socket init callback | 默认 `None`、builder API、callback clone 复用和错误分类。 | `tests/unit/socket_init_callback.rs` |
| error API | 统一错误枚举和源错误保留。 | `tests/unit/error.rs` |
| QUIC route key parsing | long header 16-bit DCID worker shard、short header 16-bit shard、非法长度、1 字节 DCID 和空 DCID。 | `tests/unit/quic_routed_udp.rs` |
| Linux QUIC reuse-port BPF selector | eBPF selector 指令构造、load 属性、attach 常量、CBPF fallback、worker modulo 和平台不可用 fallback 决策。 | `src/platform/mod.rs` 内部 unit、`tests/dv/platform_cfg.rs` |

## DV 测试
| 测试项 | 覆盖行为 | 测试文件/入口 |
|--------|----------|----------------|
| cargo check default | 默认 feature 下全 crate 类型检查。 | `uv run --active python ./harness/scripts/test-run.py sfo-reuseport dv` |
| async-std feature check | `runtime-async-std` feature 下全 crate 类型检查。 | `testplan.yaml` step `runtime-async-std-feature` |
| tokio-uring feature check | `runtime-tokio-uring` feature 下 Linux all-targets 类型检查。 | `testplan.yaml` step `runtime-tokio-uring-feature` |
| platform current target | 当前 OS 的 platform cfg 和 socket setup 编译。 | `tests/dv/platform_cfg.rs` |
| quic reuse-port BPF selector | Linux 当前目标编译并尝试 eBPF selector 路径，失败时尝试 CBPF，再失败时返回用户态 fallback；非 Linux 验证返回 fallback。 | `tests/dv/platform_cfg.rs` |
| socket option setup | 当前 OS 下可无特权验证的 socket option 设置路径。 | `tests/dv/socket_options.rs` |
| socket init callback setup | 当前 OS 下 TCP/UDP bind 路径调用 socket 初始化回调，并传播回调错误。 | `tests/dv/socket_init_callback.rs` |
| hyper static example | 示例编译、`--root` 参数、200/404/403 HTTP 响应和路径遍历拒绝。 | `cargo check --example hyper_static`、`harness/scripts/test-hyper-static-example.py` |

## Integration 测试
| 测试项 | 覆盖行为 | 测试文件/入口 |
|--------|----------|----------------|
| TCP loopback serve | 多连接 loopback accept 和 handler 交付。 | `tests/integration/tcp_serve.rs` |
| TCP error paths | bind 失败、handler 错误、invalid config。 | `tests/integration/tcp_serve.rs` |
| UDP loopback serve | packet receive、metadata、runtime 原生 `UdpSocket` response path。 | `tests/integration/udp_serve.rs` |
| UDP error paths | bind 失败和 handler 错误；UDP 服务不依赖公开 dispatch 配置。 | `tests/integration/udp_serve.rs` |
| dynamic listener lifecycle | 运行中新增 TCP/UDP listener、删除 listener、删除未知 listener。 | `tests/integration/dynamic_listeners.rs` |
| mixed protocol runtime service | 同一 `ServerRuntime` 实例同时处理 TCP 与 UDP listener。 | `tests/integration/dynamic_listeners.rs` |
| QUIC routed UDP worker stability | `QuicServer` 将带 DCID worker shard 的 packet 投递到对应 worker。 | `tests/integration/quic_routed_udp.rs` |
| QUIC routed UDP BPF fallback | Linux BPF selector 可用时经每 worker socket 投递；不可用或非 Linux 时经用户态 fallback 仍稳定投递。 | `tests/integration/quic_routed_udp.rs` |
| platform OS matrix | Linux、macOS、FreeBSD、Windows 编译和 smoke test。 | CI matrix 或 manual |
| transparent privileged path | Linux IPv4 transparent required/best-effort 行为。 | manual |

## 回归重点
- 同时启用多个 runtime feature 必须编译失败。
- 公开 TCP/UDP 回调签名不得重新引入 worker id。
- `TcpServer`、`UdpServer`、`QuicServer` 不得重新引入 `serve_with_runtime`、无 runtime 参数 `serve` 或其他隐式默认 runtime 启动入口。
- `BalancedUdpSocket` 不得重新出现在公开 API；UDP/QUIC handler 应继续接收 runtime 原生 `UdpSocket`。
- `DispatchPolicy`、Dispatcher 类型、`Auto`/`RoundRobin`/`SrcHash`/`Custom` 策略和 `with_dispatch` 不得重新出现在公开 API。
- fallback 用户态调度必须保持 Linux 兼容内部 hash 语义，不得引入用户自定义 selector。
- Windows 用户态模拟不得改变公开 API。
- `TransparentMode::Required` 与 `BestEffort` 的错误语义不得混淆。
- `QuicServer` 不得新增 TLS、connection 或 stream API；非法 QUIC 路由 packet 或不满足 16-bit worker shard layout 的 packet 不得调用 handler。

## 下游跟进
| follow_up_id | 归属阶段 | 原因 | 触发测试项 | 阻塞 |
|--------------|----------|------|------------|------|
| FU-TEST-001 | testing | 按本测试计划创建或维护对应 unit/dv/integration 测试文件，并确保它们可通过统一测试入口执行。 | 全部自动测试项 | yes |
| FU-TEST-004 | acceptance | 实现后审计 manual 验证结果和自动测试结果。 | platform OS matrix、transparent privileged path | yes |
| FU-TEST-005 | testing | 按新增动态 listener 和混合协议 worker 验证项创建 unit/integration 测试文件并通过 canonical test entry。 | dynamic listeners、mixed protocol workers | yes |
| FU-TEST-006 | testing | 按 `ServerRuntime` 模型更新公开 API 测试和集成测试，确保 worker 配置不再位于 server/listener config。 | server runtime API | yes |
| FU-TEST-007 | testing | 增加 worker thread runtime 测试，覆盖 TCP/UDP/dynamic listener loop 已迁移到 worker thread runtime 启动路径。 | worker thread runtime | yes |
| FU-TEST-008 | testing | 增加 `ServiceConfig` socket 创建后回调测试，覆盖默认 `None`、TCP/UDP 调用路径与错误传播。 | socket init callback | yes |
| FU-TEST-009 | testing | 增加 `QuicServer` 测试，覆盖 QUIC route key parsing、稳定 worker 投递、Linux reuse-port BPF selector best-effort 路径、fallback 和非法 packet 丢弃。 | QUIC routed UDP | yes |
| FU-TEST-010 | testing | 更新公开 API 测试、loopback integration tests 和示例验证，确保 `TcpServer`、`UdpServer`、`QuicServer` 只使用 `serve(&ServerRuntime, ServiceConfig, handler)`。 | explicit runtime serve API | yes |
| FU-TEST-011 | testing | 更新 API signature、UDP loopback、QUIC routed UDP、dynamic listener 测试到 runtime 原生 `UdpSocket` handler，并确认 `BalancedUdpSocket` 不再公开。 | UDP runtime socket API | yes |
| FU-TEST-012 | testing | 删除 Dispatcher/DispatchPolicy 相关策略测试和 custom dispatcher 集成测试，新增内部 schedule 单元测试与公开 API 负例。 | Linux compatible scheduling | yes |
| FU-TEST-013 | acceptance | tokio-uring runtime feature 实现后审计 feature 互斥、Linux cfg、公开 socket 类型和测试入口证据。 | tokio-uring runtime feature | yes |
| FU-TEST-014 | acceptance | hyper 静态文件服务器示例实现后审计 proposal、design、示例代码、Cargo 依赖和 smoke 验证是否一致。 | hyper static example | yes |

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
