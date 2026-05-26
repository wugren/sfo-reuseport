---
module: example-module
submodule:
version: v0.1
status: draft
approved_by:
approved_at:
---

# [Module Name] Acceptance

> This optional file provides acceptance guidance. Generated acceptance rules, expected results, and run conclusions belong in standalone review reports.

## Acceptance Baseline
- Primary baseline: `proposal.md`
- Supporting evidence:
  - `design.md` and `design/`
  - `testing.md` and `testing/`
  - `testplan.yaml`
  - long-lived module docs
  - implementation
  - test code
  - test results
  - optional git diff/status evidence when useful for locating implementation evidence

## Required Outcomes
| Outcome | Proposal Source | Acceptance Evidence | Pass Condition |
|---------|-----------------|---------------------|----------------|
| | proposal section / table row | doc, code, test, or result reference | |

## Consistency Checks
- [ ] Proposal, design, optional testing artifacts, generated acceptance rules, expected results, and implementation describe the same intended result
- [ ] Any document or code inconsistency is resolved against approved `proposal.md`
- [ ] If proposal is satisfied, non-requirement fixes route through design -> implementation/code -> testing implementation
- [ ] Optional testing documents conform to proposal, design, and delivered code behavior
- [ ] Code conforms to approved proposal and design; tests verify proposal/design/code behavior
- [ ] Design and long-lived module docs agree on stable boundaries and contracts
- [ ] Implementation matches approved design items
- [ ] Test code and test results match `testing.md` and `testplan.yaml`
- [ ] Every implemented change maps back to direct proposal and design items, plus generated test coverage or an explicit testing gap
- [ ] No downstream document contradicts, narrows, or silently expands approved proposal intent
- [ ] Every document-described behavior, constraint, non-goal, and acceptance boundary has implementation or explicit non-implementation evidence
- [ ] Every evidence-bearing module has approved direct `change_id` coverage in proposal/design and post-implementation test evidence
- [ ] Document and implementation logic contains no blocking contradiction, invalid assumption, impossible state, or correctness defect

## Required Evidence
| Evidence | Source | Required? | Notes |
|----------|--------|-----------|-------|
| Requirement coverage | `proposal.md` | yes | |
| Design coverage | `design.md` / `design/` | yes | |
| Test plan coverage | test implementation, optional `testing.md` / `testplan.yaml` | yes / manual / disabled | |
| Implementation evidence | production code and runtime/build resources | yes | |
| Testing evidence | test code and test results | yes | |
| Test results | latest accepted run output | yes / manual / disabled | |
| Diff/status evidence | `git status --short`, `git diff --stat`, `git diff --name-status`, `git diff --check` | optional | Discovery aid only; not a pass/fail standard |

## Failure Conditions
- Proposal mismatch
- Design mismatch
- Testing implementation gap or missing required evidence
- Generated acceptance rules or expected results not traceable to proposal intent
- Implementation defect or unimplemented approved behavior
- Documentation and implementation describe different behavior
- Optional testing artifacts or test implementation contradict design while proposal intent is satisfied
- Code contradicts design while proposal intent is satisfied
- Document contains a contradiction, unsupported assumption, or impossible requirement
- Implementation contains a logical correctness, compatibility, lifecycle, state, or error-handling defect
- Public API, codec, wire format, or runtime semantics changed without direct design coverage and generated test coverage or an explicit gap

## Return Routing
| Failure Type | Owning Stage | Notes |
|--------------|--------------|-------|
| proposal ambiguity or contradiction | proposal | |
| proposal-to-design mismatch | design | proposal is authoritative unless ambiguous or contradictory |
| design-to-testing mismatch | testing | testing must follow design |
| testing gap or invalid test metadata | testing | |
| document-to-code mismatch | implementation | code must follow proposal and design |
| implementation defect | implementation | |
| document logic defect | owning document stage | route by the document that contains the contradiction or invalid assumption |
| implementation logic defect | implementation | route by whether docs or code are defective |

## Acceptance Guardrails
- Do not record run-specific conclusions in this file.
- Do not use acceptance to repair proposal, design, testing, or implementation artifacts.
- A separate acceptance report must put findings first and state scope, coverage evidence, consistency evidence, optional diff/status evidence, conclusion, and follow-up tasks.
- Passing tests are supporting evidence only; they do not automatically satisfy acceptance.
- Acceptance must generate or finalize acceptance rules and expected results from proposal, design, implementation, and testing implementation before judging pass/fail.
- Every evidence-bearing module or direct submodule packet must have approved direct `change_id` coverage and post-implementation test evidence or an explicit gap.
- Single-stage acceptance tasks must finish by running `uv run --active python ./harness/scripts/stage-scope-check.py --stage acceptance`.
