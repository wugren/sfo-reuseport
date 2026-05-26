# Task Entry Gate Rules

## Goal
- Prevent implementation, bugfix, optimization, and refactor requests from bypassing versioned document admission and moving directly into code changes.

## Priority
- This is the highest-priority entry rule for task classification and implementation admission.
- Apply this rule before reading task-type process rules or editing any repository artifact.

## Scope
- requests involving code
- requests involving tests
- requests involving runtime behavior
- requests involving UI behavior
- requests involving build behavior
- bugfix, optimization, refactor, and implementation requests

## Stage Write-Scope Rule
- When the user explicitly enters one stage, that stage is the only write scope by default.
- Proposal-stage tasks MUST modify only `proposal.md` files in the active module or submodule packet.
- Design-stage tasks MUST modify only `design.md`, `design/`, direct submodule packet design files, and required long-lived boundary sync.
- Testing-stage tasks MUST modify only test code, test fixtures, test runners, unified test entrypoint wiring, and optional testing artifacts such as `testing.md`, `testing/`, `testplan.yaml`, and direct submodule packet testing files.
- Implementation-stage tasks MUST modify only production code and required non-test runtime/build resources after implementation admission passes.
- Acceptance-stage tasks MUST audit evidence, generate or finalize acceptance rules and expected results when needed, and write review reports only.
- A task MUST NOT edit multiple stage artifact groups unless the user explicitly names the stages or explicitly asks for cross-stage synchronization.
- Changing an upstream document does not automatically authorize downstream document edits. If downstream artifacts need updates, record the return route or follow-up unless the user explicitly requested those downstream edits.
- Before finishing a single-stage task, run `uv run --active python ./harness/scripts/stage-scope-check.py --stage <stage>` and treat any out-of-stage diff as a task failure.

## Requirement And Scope Classification Rule
- A request that adds, removes, narrows, widens, or reclassifies goals, scope, non-goals, obligations, supported behavior, unsupported behavior, acceptance boundaries, or success evidence MUST default to proposal stage.
- Requirement language such as "does not need", "no longer needs", "should not provide", "must provide", "support", "do not support", or equivalent terms MUST be treated as proposal-stage language by default.
- Do not reinterpret a requirement/scope request as "make the whole packet consistent" unless the user explicitly asks to update downstream documents or asks for cross-stage synchronization.
- In a proposal-stage task, update `proposal.md` and record downstream design/implementation/testing/acceptance follow-up there when needed. Do not edit `design.md`, `testing.md`, `testplan.yaml`, `acceptance.md`, code, or test code unless the user explicitly requested those stages in the same task.

## Task Entry Gate Rule
- When the user request touches code, tests, runtime behavior, UI behavior, build behavior, bugfixes, optimization, or refactoring, the default path is not immediate code modification.
- The first task step MUST locate the current versioned module packet.
- The task MUST identify the active `version`, `module`, and one or more concrete `change_id` values.
- If the active packet is a direct submodule under a large module, the task MUST also identify the active `submodule`.
- If the request affects multiple modules, repeat the gate and admission check for each affected module packet.
- If the request affects multiple direct submodules, repeat the gate and admission check for each affected submodule packet.
- Before any implementation path starts, the task MUST read:
  - `proposal.md`
  - `design.md`
- Before implementation admission passes, the task MUST run:
  - `uv run --active python ./harness/scripts/schema-check.py --version <version> --module <module>`
  - `uv run --active python ./harness/scripts/admission-check.py --version <version> --module <module> --change-id <change_id>`
- For direct submodule packets, pass `--submodule <submodule>` to both checks.
- Before document reading and direct mapping judgment are complete, the task MUST NOT edit:
  - code files
  - build files
  - resource files
- If any required proposal/design document is missing, is not `approved`, or does not directly cover the current user request, the task MUST return to the corresponding document stage.
- Code modification may begin only after the task explicitly outputs: `实现准入通过` (`Implementation admission passed`).

## Default Stage Classification Rule
- When the user has not explicitly said to enter the proposal, design, testing, or acceptance stage, classify the task by its likely artifact changes.
- If the request is a requirement, scope, non-goal, supported/unsupported behavior, or acceptance-boundary change, classify it as proposal stage before considering downstream consistency work.
- If the request would lead to code changes, the task MUST run the implementation admission check first.
- If the admission check fails, the current task is automatically classified as the earliest missing document stage.
- If the active module cannot be determined without guessing, return to proposal or design.
- If the active direct submodule cannot be determined without guessing, return to proposal or design.
- If no concrete `change_id` maps to the request, return to proposal or design based on the missing coverage.
- A concrete `change_id` maps to the request only when it appears in the required `change_id` column of the proposal and design mapping tables.
- User oral requirements, chat context, old implementation, module overviews, and historical notes MUST NOT be treated as admission evidence.
- If the request has multiple plausible meanings and the ambiguity affects scope, behavior, risk, or validation, route to proposal or the owning upstream document stage instead of silently choosing an interpretation.
- "Read code first and then decide" permits code inspection only; it does not permit code edits.
- If code inspection reveals a document gap, the task MUST stop the code path and return to the owning document stage.

## Return Routing
- Missing or unapproved `proposal.md`: return to proposal stage.
- Missing direct proposal coverage: return to proposal stage.
- Missing or unapproved `design.md`: return to design stage.
- Missing direct design coverage, boundary coverage, or interface coverage: return to design stage.
