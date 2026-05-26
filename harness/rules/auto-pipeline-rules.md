# Auto Pipeline Rules

## Goal
- Define the repository's fully automatic downstream workflow after explicit user launch and proposal approval.
- Make stage planning, child-task execution, return-routing, and exit conditions explicit.

## Trigger
- This rule is inactive unless the user explicitly asks to enter it.
- Entry signal: user explicitly asks to enable, launch, run, or enter the automatic pipeline
- Required prerequisite: `proposal.md` exists and `status: approved`
- `proposal.md` approval alone does not enter auto-pipeline mode
- Optional launch command or workflow entry:

## User Authorization Precedence
- Explicit user instructions have highest priority for entering auto-pipeline mode, requested pipeline scope, and whether per-stage user confirmation is required.
- If the user explicitly asks auto-pipeline mode to handle all subsequent stages or the whole downstream workflow, the pipeline MUST NOT stop after each stage to ask for separate user confirmation before continuing.
- That launch instruction authorizes downstream design, implementation, testing, and acceptance execution according to the pipeline plan.
- When a document-producing stage completes, the pipeline MUST auto-confirm that stage by updating the produced stage document front matter to:
  - `status: approved`
  - `approved_by: auto-pipeline`
  - `approved_at: <current timestamp>`
- Auto-confirmation happens only after that stage's declared done criteria and required checks pass.
- After each child task completes, the pipeline MUST update the pipeline plan task status to `confirmed` or `complete` before continuing to dependent tasks.
- Implementation completion MUST be recorded in the pipeline plan and implementation evidence, and final acceptance MUST be recorded in the pipeline plan and acceptance report.
- This authorization does not waive proposal authority, stage write scopes, `stage-scope-check.py`, implementation admission, schema checks, admission checks, required validation, or final acceptance.

## Acceptance Baseline
- Final acceptance baseline: approved `proposal.md`
- Downstream documents (`design.md`, `testing.md`, `acceptance.md`) may refine execution detail but MUST NOT contradict, narrow, or silently expand the proposal.
- When downstream documents or code disagree, fixes MUST preserve the approved proposal and route by priority: design first, implementation/code second, testing implementation third.
- Code MUST conform to proposal and design. Testing implementation and optional testing metadata MUST verify proposal, design, and delivered code behavior.

## Stage Responsibilities
- Proposal responsibility:
  - define the approved baseline of goals, scope, non-goals, and constraints
- Pipeline planning responsibility:
  - plan stage tasks, dependencies, outputs, and done conditions before execution starts
- Design responsibility:
  - convert the approved proposal into executable structure, interfaces, and implementation order
- Testing responsibility:
  - after implementation, convert proposal, design, and delivered code into runnable verification coverage and entrypoints
- Implementation responsibility:
  - deliver the smallest production code and required non-test runtime/build resource changes that satisfy approved proposal and design
- Acceptance responsibility:
  - independently evaluate document coverage, document consistency, document-to-implementation consistency, and logic, then return failures to the correct earlier stage

## Pipeline Planning Rule
- Before execution starts, the pipeline MUST create a plan for:
  - design tasks
  - implementation tasks
  - testing tasks
  - acceptance tasks
- The planner MUST declare:
  - task ids
  - stage
  - responsibility
  - scope
  - dependencies
  - outputs
  - done conditions

## Implementation Admission Rule
- The task entry gate still applies inside the pipeline: implementation tasks MUST classify scope and run admission before editing production code, build files, or resources.
- No implementation task may start unless:
  - `proposal.md` exists and `status: approved`
  - `design.md` exists and `status: approved`
- Implementation tasks MUST read those approved docs and confirm they cover the current task before coding.
- Implementation tasks MUST identify explicit `version`, `module`, and `change_id` values before coding.
- Implementation tasks for direct submodule packets MUST also identify explicit `submodule`.
- Implementation tasks MUST pass `schema-check.py` and `admission-check.py` for each affected module packet.
- Implementation tasks for direct submodule packets MUST pass those checks with `--submodule <submodule>`.
- Cross-module implementation tasks MUST pass admission independently for every affected module.
- Cross-submodule implementation tasks MUST pass admission independently for every affected submodule packet.
- If approved docs are incomplete for the current task, the pipeline MUST return to the owning doc stage to supplement them before implementation resumes.
- Bugfix tasks follow the same rule unless the repository publishes a narrower exception path.
- If any prerequisite is missing or not approved, the task MUST return to the owning upstream stage.

## Stage Execution Rule
- Each stage MUST execute as an independent child task.
- Each stage child task MUST keep writes inside that stage's artifact group unless the user explicitly requested cross-stage synchronization for that task.
- Each single-stage child task MUST run `stage-scope-check.py --stage <stage>` before completion and fail on out-of-stage diffs.
- Upstream-stage changes MUST NOT automatically edit downstream-stage artifacts. If a downstream artifact becomes stale, the pipeline MUST create or reopen the downstream stage task instead of silently bundling the edit into the upstream task.
- If a stage contains direct submodules, the pipeline SHOULD create independent child tasks for those submodules.
- Each child task MUST have:
  - one owner
  - one clear output
  - explicit file or scope boundary
  - explicit dependencies
  - observable done criteria

## Recommended Stage Order
1. Design planning and design tasks
2. Implementation tasks
3. Testing planning and testing tasks
4. Acceptance task

## Recursive Submodule Rule
- If `proposal.md` and `design.md` define direct submodules, the pipeline SHOULD create submodule packets under the large module directory and mirror them in:
  - design child tasks
  - testing child tasks
  - implementation child tasks where ownership can be separated safely
- Independent submodule proposal and design artifacts MUST live in the submodule packet, such as `docs/versions/<version>/modules/<module>/<submodule>/proposal.md`; optional post-implementation testing artifacts also live in that packet when generated, not under `design/<submodule>/` or `testing/<submodule>/`.
- Shared cross-cutting topics may be separate child tasks if they have clear boundaries.

## Acceptance Task Rule
- Final acceptance MUST compare delivered results back to the approved `proposal.md`.
- Final acceptance MUST check consistency between stage documents and between documents and code.
- Final acceptance MUST apply `harness/rules/acceptance-review-rules.md`.
- Git diff/status output may be used to locate evidence, but it is not the final acceptance standard.
- Final acceptance MUST audit admission for every module needed as evidence for the accepted behavior.
- If consistency problems exist, final acceptance MUST use the approved `proposal.md` as the authority and route fixes by priority: design first, implementation/code second, testing implementation third.
- Acceptance MUST inspect supporting evidence from:
  - `design.md` and `design/`
  - `testing.md` and `testing/`
  - `testplan.yaml`
  - direct submodule packets when the accepted work is split by submodule
  - implementation
  - test code
  - test results
- Acceptance MUST output:
  - findings first, sorted by severity
  - accepted, rejected, or needs changes conclusion
  - evidence summary
  - mismatch list
  - document and implementation logic findings
  - return-routing decision

## Return Routing Rule
- If acceptance fails, the pipeline MUST return work to the correct earlier stage instead of exiting.

Minimum return categories:
- proposal issue
- design issue
- testing issue
- implementation issue

For each failed acceptance run, record:
- blocking issue id
- owning stage
- target task to reopen or recreate
- reason for return
- expected fix output

## Exit Condition
- The pipeline MUST continue until:
  - proposal-defined outcomes are satisfied
  - blocking issues are closed
  - required tests and evidence exist
  - final acceptance passes

## Guardrails
- The pipeline MUST NOT skip planning and jump straight into implementation.
- The pipeline MUST NOT treat draft or missing design artifacts as implementation-ready.
- The pipeline MUST NOT treat missing post-implementation testing evidence as acceptance-ready.
- The pipeline MUST NOT treat one failed acceptance as terminal completion.
- The pipeline MUST NOT let downstream documents override proposal intent.
- The pipeline SHOULD avoid unnecessary task depth; split by real ownership or validation boundaries.

## Suggested Companion Files
- `harness/pipeline-plan.md` or equivalent generated plan artifact
- child task template
- acceptance report template
- trigger rules
