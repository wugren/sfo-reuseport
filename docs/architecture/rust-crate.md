# Rust Crate 约束

## 范围
- 本仓库当前包含一个 Rust package：`sfo-reuseport`。
- 当前源码入口是 `src/lib.rs`；示例或 smoke binary 只作为使用示例，不定义 crate 的公开边界。

## 构建与测试入口
- Canonical harness 命令：`uv run --active python ./harness/scripts/test-run.py sfo-reuseport <level>`。
- `unit` 覆盖核心逻辑、公开 API 编译契约和纯业务规则测试。
- `dv` 覆盖单模块可运行验证、feature 组合编译和当前平台 cfg 检查。
- `integration` 覆盖 loopback TCP/UDP 行为、公开接口错误路径和平台验证。
- `all all` 必须通过统一入口触达所有已注册项目测试；根目录 `test-run.sh` 和 `test-run.bat` 只委托给统一入口。

## 格式化
- Agent 不自动运行 `cargo fmt`。
- 只有在用户明确要求，或未来 repo-local 规则要求时，才运行格式化。

## 变更纪律
- 实现类变更必须先定位 `version`、`module` 和具体 `change_id`，读取已批准的 `proposal.md` 与 `design.md`，并通过 schema/admission 检查后才能修改生产代码。
- 宽泛模块描述、聊天上下文、旧实现和历史 review report 都不能替代直接 `change_id` 映射。
- 单阶段文档任务结束前必须通过对应的 stage-scope 检查；跨阶段同步必须由用户明确要求。
- 添加依赖前，优先使用 Rust 标准库设施。
- 任何新依赖都必须先在活跃 design 文档中说明理由，然后才能实现。
