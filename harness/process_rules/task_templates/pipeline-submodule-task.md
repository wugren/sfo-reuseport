# Pipeline Submodule Task

## Task Identity
- Task ID:
- Stage:
- Responsibility:
- Version:
- Module:
- Submodule:
- change_id:
- Parent Task:
- Depends On:
- Owner:

## Goal
- Complete the named stage for this direct submodule only.

## Scope Boundary
- In scope:
- Out of scope:
- Shared topics handled elsewhere:

## Inputs
- Proposal excerpts:
- Design references:
- Testing references:
- Upstream outputs:

## Admission Checks
- [ ] Required upstream artifacts exist
- [ ] Required upstream approvals exist
- [ ] If per-stage user confirmation is skipped, the pipeline plan records explicit user auto-pipeline authorization
- [ ] The pipeline plan task status was updated to `confirmed` or `complete` before dependent tasks continue
- [ ] If this task produces a stage document and auto-confirmation is enabled, the document front matter was updated to `status: approved`, `approved_by: auto-pipeline`, and `approved_at`
- [ ] Scope stays inside this direct submodule
- [ ] Scope stays inside the named stage unless the user explicitly requested cross-stage synchronization
- [ ] For single-stage tasks, `stage-scope-check.py --stage <stage>` passed for the current diff
- [ ] For implementation: proposal and design inputs for this submodule are approved
- [ ] For implementation: active `version`, `module`, submodule, and `change_id` are explicit
- [ ] For implementation: `schema-check.py --submodule <submodule>` passed for the submodule packet
- [ ] For implementation: `admission-check.py --submodule <submodule>` passed for every admitted `change_id`
- [ ] For implementation: the approved submodule docs were inspected and they explicitly cover this submodule change

## Required Outputs
- Output file(s):
- Evidence:

## Allowed Changes
- Can modify:
- Must not modify:

Stage-task defaults:
- Proposal can modify: `<submodule>/proposal.md` only
- Design can modify: `<submodule>/design.md` and required long-lived boundary sync only
- Testing can modify: submodule test code, fixtures, test runners, unified test entrypoint wiring, `<submodule>/testing.md`, and `<submodule>/testplan.yaml` only
- Implementation can modify: production code and required non-test runtime/build resources only
- Acceptance can modify: review reports only
- Cross-stage edits require explicit user instruction naming the extra stage(s) or asking for cross-stage synchronization

## Done Condition
- [ ] Submodule output is complete
- [ ] No out-of-scope files were changed
- [ ] Stage scope check passed when applicable
- [ ] Handover data for the next dependent task exists

## Failure Handling
- If the issue is shared or upstream, return it instead of solving it inside this submodule task.
- Record:
  - issue id
  - return stage
  - return target task
  - expected upstream fix
