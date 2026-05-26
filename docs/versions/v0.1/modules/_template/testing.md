---
module: example-module
submodule:
version: v0.1
status: draft
approved_by:
approved_at:
---

# [Module Name] Testing

## Test Document Index
| Document | Topic | Scope |
|----------|-------|-------|
| none | no split testing docs yet | full module |

<!-- If proposal/design splits a large module into direct submodules, mirror that split with submodule packets under this module directory, for example `<submodule>/testing.md` and `<submodule>/testplan.yaml`. Do not put independent submodule testing docs under `testing/<submodule>/`. Keep each human-authored testing doc under 1000 lines; split oversized docs and update this index. -->

## Unified Test Entry
- Machine-readable plan: `docs/versions/<version>/modules/<module>/testplan.yaml`
- Unit: `uv run --active python ./harness/scripts/test-run.py <module> unit`
- DV: `uv run --active python ./harness/scripts/test-run.py <module> dv`
- Integration: `uv run --active python ./harness/scripts/test-run.py <module> integration`
- Module all: `uv run --active python ./harness/scripts/test-run.py <module> all`
- Project all: `uv run --active python ./harness/scripts/test-run.py all all`
- Root shortcuts: `./test-run.sh [<module> <level>]` and `test-run.bat [<module> <level>]`

## Submodule Tests
| Submodule | Responsibility | Detailed Test Doc | Required Behaviors | Edge/Failure Cases | Test Type | Test Files | Status | Gap / Manual Reason |
|-----------|----------------|-------------------|--------------------|--------------------|-----------|------------|--------|---------------------|
| | | | | | | | ready / gap / manual / disabled | |

## Module-Level Tests
| Test Item | Covered Boundary | Entry | Expected Result | Test Type | Test File/Script | Status | Gap / Manual Reason |
|-----------|------------------|-------|-----------------|-----------|------------------|--------|---------------------|
| | | | | | | ready / gap / manual / disabled | |

## External Interface Tests
| Interface | Responsibility | Success Cases | Failure/Edge Cases | Test Type | Test Doc/File | Status | Gap / Manual Reason |
|-----------|----------------|---------------|--------------------|-----------|---------------|--------|---------------------|
| | | | | | | ready / gap / manual / disabled | |

## Direct Change Coverage
| change_id | design_source | validation_id | testplan_level | testplan_step_id | Gap? | Gap / Manual Reason |
|-----------|---------------|---------------|----------------|------------------|------|---------------------|
| CHG-example | `design.md` section / `design/...` doc | VAL-example | unit / dv / integration | example-unit | no | |

## Validation Rationale
| Behavior or Risk | Validation Signal | Why This Is Sufficient | Gap / Manual Reason |
|------------------|-------------------|------------------------|---------------------|
| | | | |

## Unit Tests
| Test Item | Covered Behavior | Test File |
|-----------|------------------|-----------|
| | | |

## DV Tests
<!-- Single-module runnable verification -->

## Integration Tests
<!-- Neighbor-module contracts and responsibilities -->

## Regression Focus
<!-- Historical bugs and high-risk boundary cases -->

## Stage and Admission Notes
- Testing is designed after implementation from `proposal.md`, `design.md`, delivered code, and any required fixtures or runners.
- Testing tasks may update test code, fixtures, unified test entrypoint wiring, `testing.md`, `testing/`, and `testplan.yaml`; they do not rewrite proposal, design, acceptance, or production implementation artifacts unless explicitly authorized.
- Every automated test added or changed here must be reachable through `harness/scripts/test-run.py`.
- Manual or disabled validation must carry the same reason in generated test evidence and optional testing metadata.

## Definition of Done
- [ ] Testing docs cover all direct submodules or explain why they do not exist
- [ ] Large-module testing docs are split into direct submodule packets when proposal/design uses direct submodules
- [ ] Human-authored testing docs stay under 1000 lines, or oversized docs are split and indexed
- [ ] `testplan.yaml` matches the declared test entrypoints
- [ ] Generated tests are registered with `harness/scripts/test-run.py`
- [ ] `uv run --active python ./harness/scripts/test-run.py <module> all` reaches this module's automated tests
- [ ] `uv run --active python ./harness/scripts/test-run.py all all` reaches all project tests registered with the harness
- [ ] Module-level tests cover key boundary behavior and failure paths
- [ ] External interfaces have contract-focused tests
- [ ] Every implemented change has direct validation coverage or an explicit gap
- [ ] Every implemented `change_id` appears in `proposal.md`, `design.md`, and generated testing evidence; optional `testing.md` and `testplan.yaml` include the same `change_id` when present unless the validation path is explicitly `manual` or `disabled`
- [ ] Every validation path maps to a concrete behavior, risk, or success criterion
- [ ] Any `manual` or `disabled` layer has the same reason in `testing.md` and `testplan.yaml`
- [ ] Relevant automated tests pass
- [ ] Single-stage testing tasks have run `uv run --active python ./harness/scripts/stage-scope-check.py --stage testing`
