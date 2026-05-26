# Pipeline Stage Task

## Task Identity
- Task ID:
- Stage: design / testing / implementation / acceptance
- Responsibility:
- Scope:
- Version:
- Module:
- Submodule:
- change_id:
- Parent Task:
- Depends On:
- Owner:

## Goal
- Describe the single stage outcome this task must complete.

## Inputs
- Proposal inputs:
- Upstream task outputs:
- Relevant docs:
- Relevant code:
- Constraints:

## Admission Checks
- [ ] For implementation-like scopes: `harness/rules/task-entry-gate-rules.md` was applied before any production code, build, or resource edit
- [ ] Required upstream artifacts exist
- [ ] Required upstream approvals exist
- [ ] If per-stage user confirmation is skipped, the pipeline plan records explicit user auto-pipeline authorization
- [ ] The pipeline plan task status was updated to `confirmed` or `complete` before dependent tasks continue
- [ ] If this task produces a stage document and auto-confirmation is enabled, the document front matter was updated to `status: approved`, `approved_by: auto-pipeline`, and `approved_at`
- [ ] Scope does not cross into another stage
- [ ] If scope crosses into another stage, the user explicitly requested those stages or cross-stage synchronization
- [ ] For single-stage tasks, `stage-scope-check.py --stage <stage>` passed for the current diff
- [ ] For implementation: `proposal.md` and `design.md` are both `approved`
- [ ] For implementation: active `version`, `module`, and `change_id` are explicit
- [ ] For implementation in a direct submodule packet: active `submodule` is explicit
- [ ] For implementation: `schema-check.py` passed for the active module packet
- [ ] For implementation: `admission-check.py` passed for every admitted `change_id`
- [ ] For implementation in a direct submodule packet: both checks passed with `--submodule <submodule>`
- [ ] For implementation: those approved docs were inspected and they explicitly cover this task's change scope
- [ ] For cross-module implementation: each affected module passed admission independently
- [ ] For cross-submodule implementation: each affected submodule packet passed admission independently
- [ ] For implementation: `实现准入通过` (`Implementation admission passed`) was explicitly output before code edits

## Allowed Changes
- Can modify:
- Must not modify:

Stage-task defaults:
- Proposal can modify: `proposal.md` in the active module or submodule packet only
- Design can modify: `design.md`, `design/`, direct submodule packet design files, and required long-lived boundary sync only
- Testing can modify: test code, test fixtures, test runners, unified test entrypoint wiring, optional `testing.md`, `testing/`, `testplan.yaml`, and direct submodule packet testing files only
- Implementation can modify: production code and required non-test runtime/build resources only
- Acceptance can modify: review reports only
- Downstream follow-up from an upstream change is recorded as a return route unless cross-stage synchronization was explicitly requested

## Required Outputs
- Output 1:
- Output 2:

## Done Condition
- [ ] Required output exists
- [ ] Scope boundary respected
- [ ] Stage scope check passed when applicable
- [ ] Dependencies satisfied
- [ ] Evidence attached

## Failure Handling
- If blocked by an upstream issue, do not patch outside scope.
- Record:
  - blocking issue
  - suspected owning stage
  - return target
  - evidence
