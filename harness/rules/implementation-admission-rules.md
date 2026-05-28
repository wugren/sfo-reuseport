# Implementation Admission Rules

## Goal
- Define the hard prerequisites for starting implementation or bugfix work.
- This rule is reached only after `task-entry-gate-rules.md` classifies the task and requires implementation admission.

## Scope
- implementation tasks
- bugfix tasks
- production code changes for a versioned module

## Required Inputs
- `docs/versions/<version>/modules/<module>/proposal.md`
- `docs/versions/<version>/modules/<module>/design.md`
- Or, for a direct submodule packet under a large module:
  - `docs/versions/<version>/modules/<module>/<submodule>/proposal.md`
  - `docs/versions/<version>/modules/<module>/<submodule>/design.md`

## Admission Rule
- Implementation admission MUST be evaluated before editing production code, build files, or resources.
- Implementation MUST NOT start unless all required inputs exist.
- Implementation MUST NOT start unless proposal and design documents have `status: approved`.
- Approved status alone does NOT satisfy implementation admission; implementation and bugfix tasks MUST read the approved proposal/design docs and confirm they contain task-relevant coverage for the current change.
- Bugfix tasks follow the same rule unless the repository publishes an explicit exception path.
- Implementation MUST NOT start unless the active `version`, `module`, and concrete `change_id` values are known.
- If implementation targets a direct submodule packet, implementation MUST NOT start unless the active `submodule` is also known.
- Implementation MUST NOT start unless each `change_id` maps through the exact required traceability locations:
  - `proposal.md` `## Proposal Items` table, `change_id` column
  - `design.md` `## Directly Mapped Change Items` table, `change_id` column
- If the request affects multiple modules, each affected module MUST pass admission separately.
- If the request affects multiple direct submodules, each affected submodule packet MUST pass admission separately.
- Implementation MUST run `schema-check.py` and `admission-check.py` successfully before code edits.
- For direct submodule packets, implementation MUST run those checks with `--submodule <submodule>`.
- If approved docs do not yet contain the current change's required content, implementation MUST stop and return work to the owning upstream doc stage before coding starts.
- Code modification may begin only after the task explicitly outputs: `实现准入通过` (`Implementation admission passed`).
- Module-level baseline docs, package overviews, historical notes, or oral explanation do not count as sufficient implementation admission.
- If direct mapping is missing, the default path is to return work upstream, not to implement first and document later.

## Allowed Changes
- production code
- required non-test runtime/build resources

## Execution Guardrails
- Implement the minimum production code needed to satisfy the approved proposal, design, and current request.
- Leave test implementation for the post-implementation testing stage unless the user explicitly requested a combined implementation/testing task.
- Touch only files and lines required by the admitted task.
- Match surrounding style, naming, and structure.
- Runtime work MUST stay inside `ServerRuntime` owned worker threads: listener loops, socket I/O, handler dispatch, server lifecycle callbacks, and runtime-owned background work MUST be submitted to the `ServerRuntime` worker executor instead of spawning external threads, external thread pools, caller-runtime tasks, or global-runtime tasks.
- New production uses of `std::thread::spawn`, `thread::Builder::spawn`, runtime-specific worker creation, or equivalent external execution are allowed only when implementing `ServerRuntime`'s own worker-thread ownership, and only when the approved design and admitted `change_id` directly require that boundary.
- Do not refactor, reformat, rewrite comments, rename symbols, or clean adjacent code unless the admitted task requires it.
- Do not add unrequested features, options, extension points, configuration, or defensive handling for scenarios ruled out by approved docs or reachable code paths.
- Remove only unused imports, variables, functions, or files made unused by the current change.
- If unrelated dead code or defects are noticed, record them as residual risk or follow-up instead of repairing them in the implementation task.
- Every changed line should trace to the current task's approved docs, requested behavior, or required verification.
- Every changed line should trace to at least one admitted `change_id`.

## Forbidden Changes
- `proposal.md`
- `design.md`
- `design/`
- `testing.md`
- `testing/`
- `testplan.yaml`
- `acceptance.md`
- test code and test fixtures, unless the user explicitly requested a combined implementation/testing task

## Verification Default
- Repositories may choose a strict default where implementation does not proactively run validation commands.
- In that mode, tests run only when:
  - the user explicitly requests validation
  - debugging needs fresh evidence
  - task docs or repo-local rules explicitly require validation
- "quick sanity check", "minimal self-test", and similar habitual reasons are not valid exceptions.

## Rust Formatting Default
- For Rust repositories, agents MUST NOT automatically run `cargo fmt`.
- `cargo fmt` may run only when the user explicitly requests formatting or repo-local rules explicitly require it for the current task.

## Return Routing
- Missing or draft proposal: return to proposal task
- Missing or draft design: return to design task
- Missing direct proposal mapping: return to proposal task
- Missing direct design mapping or changed boundaries/interfaces: return to design task
- Missing active module or `change_id`: return to proposal or design task
- Missing active submodule for work that belongs to a direct submodule packet: return to proposal or design task
- Failed schema or admission checker: return to the owning document stage named by the checker output
- Approved proposal/design docs exist but do not yet cover the current task in enough detail: return to the owning proposal/design task to supplement docs first
- Upstream contradiction discovered during implementation: return to the owning upstream stage instead of patching docs in place
