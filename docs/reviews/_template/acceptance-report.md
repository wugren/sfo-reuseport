# [Module Name] Acceptance Report

## Findings
| ID | Severity | Stage | Evidence | Problem | Fail Condition Hit |
|----|----------|-------|----------|---------|--------------------|
| | | | | | |

## Object and Scope
- Module:
- Version:
- change_id values reviewed:
- Review date:
- In scope:
- Out of scope:

## Optional Diff / Status Evidence
- `git status --short` summary:
- `git diff --stat` summary:
- `git diff --name-status` summary:
- `git diff --check` result:
- Note: diff/status output is a discovery aid only, not the acceptance standard.

## Evidence Coverage
| Documented Item | Source Document | Implementation Evidence | Test / Result Evidence | Status |
|-----------------|-----------------|-------------------------|------------------------|--------|
| | | | | implemented / missing / inconsistent / logically invalid |

## Inputs
- `proposal.md`
- `design.md`
- `testing.md`
- `testplan.yaml`
- optional `acceptance.md`
- long-lived module doc
- implementation
- test code
- test results
- optional git diff/status evidence
- `harness/rules/acceptance-review-rules.md`

## Review Order
1. Review approved requirements and acceptance boundaries
2. Review design against proposal
3. Generate or finalize acceptance rules and expected results from proposal, design, implementation, and testing implementation
4. Review optional testing artifacts against design and delivered code
5. Review implementation against proposal and design
6. Review tests and results against testing implementation and optional `testplan.yaml`
7. Review document and implementation logic for contradictions, invalid assumptions, impossible states, and correctness defects
8. Use diff/status output only when helpful to locate evidence
9. Produce conclusion

## Consistency Summary
- Proposal authority check:
- Proposal vs design:
- Design vs optional testing artifacts:
- Design vs long-lived boundary doc:
- Proposal/design vs implementation:
- Testing docs vs testplan vs test code vs results:
- change_id traceability:
- Generated acceptance rules / expected results traceability:
- Cross-module admission:
- Public API / codec / runtime semantics review:
- Document logic review:
- Implementation logic review:
- Document approval timing:

## Required Command Evidence
- `python3 ./harness/scripts/schema-check.py --version <version> --module <module>`:
- `python3 ./harness/scripts/admission-check.py --version <version> --module <module> --change-id <change_id>`:
- `python3 ./harness/scripts/stage-scope-check.py --stage <stage>`:
- Unit test command from `testplan.yaml`:
- DV test command from `testplan.yaml`:
- Integration test command from `testplan.yaml`:
- Targeted migration search, when applicable:

## Conclusion
- Accepted / rejected / needs changes:
- Reason:
- Supporting test evidence:
- Residual risk:

## Follow-Up Tasks
- Requirement task:
- Design task:
- Testing task:
- Implementation task:
