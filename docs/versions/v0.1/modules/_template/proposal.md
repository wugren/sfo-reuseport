---
module: example-module
submodule:
version: v0.1
status: draft
approved_by:
approved_at:
---

# [Module Name] Proposal

## Background and Goal
<!-- What problem is being solved and why now -->

## Scope
### In scope
### Out of scope
### Boundary with neighboring modules

## Assumptions and Ambiguities
- Assumptions:
- Open ambiguities:
- Decision needed before approval:

## Constraints
- Allowed libraries/components:
- Disallowed approaches:
- System constraints:

## Large Module Submodule Decision
<!-- If this module is a large package/crate/service with several independent submodules, decide whether this proposal creates a new direct submodule or belongs inside an existing submodule. New independent features should use a submodule packet under this module directory, for example `<submodule>/proposal.md`, `<submodule>/design.md`, `<submodule>/testing.md`, and `<submodule>/testplan.yaml`. Do not put independent submodule docs under `design/<submodule>/` or `testing/<submodule>/`. Keep each human-authored proposal doc under 1000 lines; split oversized docs by submodule, responsibility, or requirement boundary. -->

| Submodule | New or Existing | Responsibility | Proposal Packet | Reason |
|-----------|-----------------|----------------|-----------------|--------|
| example | new / existing / none | | `<submodule>/proposal.md` | |

## High-Level Outcomes
<!-- Business outcomes only; detailed acceptance rules and expected results are finalized in review reports. Optional acceptance.md may provide guidance but is not required. -->

## Proposal Items
| proposal_id | change_id | Outcome | Scope Boundary | Success Evidence | Explicit Non-Goal |
|-------------|-----------|---------|----------------|------------------|-------------------|
| P-001 | CHG-example | | | | |

## Success Criteria
- Concrete user-visible or system-visible result:
- Required evidence:
- Explicit non-goals:

## Risks
<!-- High-risk changes, shared contracts, security or migration impact -->

## Downstream Follow-Up
| follow_up_id | Owning Stage | Reason | Triggering Proposal Item | Blocking |
|--------------|--------------|--------|--------------------------|----------|
| FU-001 | design/implementation/testing/acceptance | | P-001 | yes/no |

## Stage and Admission Notes
- Proposal approval is required before downstream design, implementation, testing, or acceptance can rely on this scope.
- Implementation-ready behavior must map to concrete `change_id` values in both proposal and design; chat-only context and historical implementation are not admission evidence.
- Changing this proposal does not authorize downstream document or code edits unless the task explicitly requests cross-stage synchronization.

## Proposal Guardrails
- Proposal-stage tasks modify only `proposal.md` unless the user explicitly requests a multi-stage update.
- If this proposal change requires design, testing, implementation, or acceptance updates, record the needed follow-up instead of editing downstream artifacts by default.
- If this is a large module with many independent submodules, classify whether the requested feature is a new direct submodule before design or testing starts.
- Put the split submodule's proposal and design files in a submodule directory under this module packet; if post-implementation testing artifacts are generated, put `testing.md` and `testplan.yaml` in that same submodule packet. Do not use `design/<submodule>/` or `testing/<submodule>/` for independent submodule docs.
- Keep human-authored proposal docs under 1000 lines where practical; if a doc would exceed 1000 lines, split it and update the relevant document index.
- If the request has multiple plausible meanings, record the ambiguity instead of silently choosing one.
- Proposal approval should not depend on chat-only context; task-critical assumptions belong in this file.
- Keep implementation strategy out of proposal except where a constraint is part of the requirement.
- Every implementation-ready requirement must have a stable `change_id`.
- A broad module-level statement is not enough for implementation admission; the relevant `change_id` must name the concrete behavior, contract, or implementation unit being admitted.
- Single-stage proposal tasks must finish by running `uv run --active python ./harness/scripts/stage-scope-check.py --stage proposal`.
