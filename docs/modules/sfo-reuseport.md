# sfo-reuseport 模块边界

## 职责
- 负责 `src/` 下实现的 Rust crate 行为，以及 `Cargo.toml` 中声明的包边界。

## 当前结构
- `Cargo.toml`：package 元数据和依赖。
- `src/lib.rs`：library crate 公开入口，re-export `ServerRuntime`、TCP/UDP server、配置、错误和 runtime socket 类型。
- `src/core/`：worker/runtime 配置、listener registry、TCP/UDP 服务、分发和错误。
- `src/runtime/`：tokio/async-std feature-gated runtime 适配。
- `src/platform/`：平台 socket 行为适配。

## 版本化文档包
- 活跃模块包：`docs/versions/v0.1/modules/sfo-reuseport/`。
- 当前没有 Harness 直接子模块；`runtime`、`core`、`platform` 是 crate 内部 Rust 模块，不是独立文档包。

## 契约
- 公开 runtime 行为、外部可见接口、worker 生命周期、socket 选项、平台 fallback、QUIC-aware UDP routing 等行为，必须先在活跃 `proposal.md` 和 `design.md` 中拥有直接 `change_id` 映射。
- testing 是后置阶段：测试实现、测试夹具、统一入口接线、`testing.md` 和 `testplan.yaml` 在实现后根据 proposal、design 和交付代码生成或更新。
- acceptance 只审计证据链并写独立 review report；git diff/status 只能作为证据发现线索，不能替代 proposal/design/code/test 一致性判断。
- 当前未定义跨模块工作；如果新增模块或直接子模块，每个证据承载模块都需要自己的文档包、`change_id` 映射、测试证据和准入检查。

## 准入与阶段边界
- Implementation admission：`uv run --active python ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`。
- Change admission：`uv run --active python ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id <change_id>`。
- 单阶段文档任务：`uv run --active python ./harness/scripts/stage-scope-check.py --stage <stage> --version v0.1 --module sfo-reuseport`。
- Proposal、design、testing、implementation 和 acceptance 默认各自只写本阶段产物；跨阶段同步需要明确任务授权。

## 验证
- 使用 `uv run --active python ./harness/scripts/test-run.py sfo-reuseport unit`。
- 使用 `uv run --active python ./harness/scripts/test-run.py sfo-reuseport dv`。
- 使用 `uv run --active python ./harness/scripts/test-run.py sfo-reuseport integration`。
- 使用 `uv run --active python ./harness/scripts/test-run.py sfo-reuseport all` 运行本模块所有注册测试。
- 使用 `uv run --active python ./harness/scripts/test-run.py all all` 运行项目级统一测试入口。
