# sfo-reuseport v0.1 Linux Compatible Scheduling Acceptance

## Findings
- None.

## Conclusion
Accepted.

## Scope
- Version: `v0.1`
- Module: `sfo-reuseport`
- change_id: `CHG-linux-compatible-scheduling`
- Baseline: approved `proposal.md`

## Evidence Summary
- Proposal now requires removal of public Dispatcher/DispatchPolicy logic and Linux `SO_REUSEPORT` compatible internal scheduling for systems without usable `SO_REUSEPORT` worker allocation.
- Design maps `CHG-linux-compatible-scheduling` to config/API removal, private `src/core/schedule.rs`, TCP/UDP fallback worker selection, and public API absence tests.
- Testing maps the same change_id to `VAL-linux-compatible-scheduling` and testplan step `linux-compatible-scheduling`.
- Implementation removes public `DispatchPolicy`, removes `with_dispatch`, replaces `dispatch.rs` with private `schedule.rs`, updates TCP/UDP fallback paths, and removes custom dispatcher tests.

## Harness Results
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-linux-compatible-scheduling`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation`: failed because this repository baseline currently has all files untracked.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation --ignore-untracked`: no changed files.
- `python3 ./harness/scripts/stage-scope-check.py --stage acceptance`: failed for the same untracked repository baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage acceptance --ignore-untracked`: no changed files.

## Consistency Audit
- Proposal, design, testing, and `testplan.yaml` all contain direct `CHG-linux-compatible-scheduling` coverage.
- Public API no longer re-exports `DispatchPolicy`.
- `ServerRuntimeConfig` now contains only worker configuration and no dispatch strategy field.
- TCP and UDP fallback paths use the private Linux-compatible scheduling helper; QUIC routed UDP keeps its approved 16-bit shard routing.

## Residual Risk
- The stage-scope checker cannot provide useful working-tree scope evidence while the repository is entirely untracked. This is an environment/baseline issue, not a finding against the scheduling implementation.

## Return Routing
- No return required.
