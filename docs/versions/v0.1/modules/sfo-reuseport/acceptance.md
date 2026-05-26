---
module: sfo-reuseport
submodule:
version: v0.1
status: draft
approved_by:
approved_at:
---

# sfo-reuseport 验收

> 本文件是可选的验收指导。具体验收规则、预期结果和运行结论以独立 review report 为准。

## 验收基线
- 主要基线：`proposal.md`
- 支撑证据：
  - `design.md` 和 `design/`
  - `testing.md` 和 `testing/`
  - `testplan.yaml`
  - 长期模块文档
  - 实现代码
  - 测试代码
  - 测试结果
  - 在有助于定位实现证据时，可选使用 git diff/status 证据

## 必需结果
| 结果 | Proposal 来源 | 验收证据 | 通过条件 |
|------|---------------|----------|----------|
| | proposal 章节/表格行 | 文档、代码、测试或结果引用 | |

## 一致性检查
- [ ] Proposal、design、可选 testing artifacts、生成的验收规则、预期结果和实现描述的是同一个预期结果。
- [ ] 任意文档或代码不一致时，以已批准的 `proposal.md` 为准解决。
- [ ] 如果 proposal 已满足，非需求问题通过 design -> implementation/code -> testing implementation 路径修复。
- [ ] 可选测试文档符合设计文档。
- [ ] 代码符合 approved design，测试实现验证 proposal/design/code 行为。
- [ ] 设计文档和长期模块文档在稳定边界与契约上保持一致。
- [ ] 实现匹配已批准的 design items。
- [ ] 测试代码和测试结果匹配 `testing.md` 与 `testplan.yaml`。
- [ ] 每个已实现变更都能追溯到直接 proposal 和 design 项，并拥有后置测试实现覆盖或明确测试缺口。
- [ ] 下游文档不得与已批准 proposal 意图矛盾、缩窄或静默扩展。
- [ ] 每个文档描述的行为、约束、非目标和验收边界都有实现证据或明确的非实现证据。
- [ ] 每个承载证据的模块都在 proposal/design 中拥有已批准的直接 `change_id` 覆盖，并拥有后置测试证据或明确测试缺口。
- [ ] 文档和实现逻辑中不存在阻塞性矛盾、无效假设、不可能状态或正确性缺陷。

## 必需证据
| 证据 | 来源 | 是否必需 | 备注 |
|------|------|----------|------|
| 需求覆盖 | `proposal.md` | yes | |
| 设计覆盖 | `design.md` / `design/` | yes | |
| 测试计划覆盖 | 测试实现，可选 `testing.md` / `testplan.yaml` | yes / manual / disabled | |
| 实现证据 | 代码和测试代码 | yes | |
| 测试结果 | 最近一次被接受的运行输出 | yes / manual / disabled | |
| Diff/status 证据 | `git status --short`、`git diff --stat`、`git diff --name-status`、`git diff --check` | optional | 只作为发现线索，不作为通过/失败标准 |

## 失败条件
- Proposal 不匹配。
- 设计不匹配。
- 测试实现存在缺口或缺失必需证据。
- 生成的验收规则或预期结果无法追溯到 proposal 意图。
- 实现缺陷或已批准行为未实现。
- 文档和实现描述了不同的行为。
- 当 proposal 意图已满足时，可选 testing artifacts 或测试实现与 design 矛盾。
- 当 proposal 意图已满足时，代码与 design 矛盾，或测试实现没有验证 proposal/design/code 行为。
- 文档包含矛盾、不受支持的假设或不可能的要求。
- 实现包含逻辑正确性、兼容性、生命周期、状态或错误处理缺陷。
- 公开 API、codec、wire format 或 runtime 语义发生变更，但没有直接 design 覆盖和后置测试证据或明确缺口。

## 返回路由
| 失败类型 | 归属阶段 | 备注 |
|----------|----------|------|
| proposal 歧义或矛盾 | proposal | |
| proposal 到 design 不匹配 | design | proposal 是权威来源，除非 proposal 本身有歧义或矛盾 |
| design 到 testing 不匹配 | testing | testing 必须遵循 design |
| testing 缺口或无效测试元数据 | testing | |
| 文档到代码不匹配 | implementation | 代码必须遵循 approved proposal 和 design |
| 实现缺陷 | implementation | |
| 文档逻辑缺陷 | 所属文档阶段 | 按包含矛盾或无效假设的文档路由 |
| 实现逻辑缺陷 | implementation | 按文档或代码哪一方有缺陷来路由 |

## 验收护栏
- 不要在本文件中记录某次运行的具体结论。
- 不要用 acceptance 来修复 proposal、design、testing 或 implementation 产物。
- 独立 acceptance report 必须先列发现，并说明范围、覆盖证据、一致性证据、可选 diff/status 证据、结论和后续任务。
- 测试通过只是支撑证据，不会自动满足验收。
- 验收必须先从 proposal、design、实现和测试实现生成或最终确定验收规则与预期结果，再判断通过或失败。
- 每个承载证据的模块或直接子模块包都必须有 approved 的直接 `change_id` 覆盖，并拥有后置测试证据或明确缺口。
- 单阶段 acceptance 任务结束前必须运行 `uv run --active python ./harness/scripts/stage-scope-check.py --stage acceptance`。
