# sfo-reuseport v0.1 Quic Reuse-Port BPF Acceptance Report

## Findings
| ID | Severity | Stage | Evidence | Problem | Fail Condition Hit |
|----|----------|-------|----------|---------|--------------------|
| QRB-A-001 | blocking | process | `stage-scope-check.py --stage design/testing/implementation` reports broad repository paths because the Git repository has no commits and all files are untracked. | The BPF selector implementation evidence is consistent, but the required stage-scope gate cannot distinguish this task's edits from the repository bootstrap baseline. | Single-stage task failed `stage-scope-check.py --stage <stage>`. |

## Object and Scope
- Module: `sfo-reuseport`
- Version: `v0.1`
- change_id values reviewed: `CHG-quic-routed-udp`
- Review date: 2026-05-25
- In scope: Linux best-effort reuse-port classic BPF selector for `QuicServer`, user-space fallback, QUIC fixed 16-bit worker shard parsing, invalid packet rejection, focused unit/DV/integration evidence.
- Out of scope: TLS, QUIC handshake, connection state, streams, congestion control, quinn integration, configurable CID layouts, hard failure on BPF attach errors.

## Evidence Coverage
| Documented Item | Source Document | Implementation Evidence | Test / Result Evidence | Status |
|-----------------|-----------------|-------------------------|------------------------|--------|
| Linux `QuicServer` first attempts a best-effort reuse-port BPF selector. | `design.md`, `testing.md`, `testplan.yaml` | `src/platform/mod.rs` adds `bind_quic_udp_reuseport_workers` and Linux CBPF attach via `SO_ATTACH_REUSEPORT_CBPF`; `src/core/udp.rs` calls that path before fallback. | `platform::tests::quic_reuseport_cbpf_reads_long_and_short_header_shards`; unit passed. | implemented |
| BPF unavailable or attach failure falls back to portable user-space routing without public API changes. | `design.md`, `testing.md` | `bind_quic_udp_reuseport_workers` returns `Ok(None)` for unsupported platforms or selector setup failure; `QuicServer` then uses existing single-socket user-space dispatch. | `quic_routed_udp_delivers_long_header_dcid_to_target_worker`; integration passed. | implemented |
| Worker socket path still rejects invalid or non-16-bit route keys before invoking handler. | `design.md`, `testing.md` | `quic_reuseport_bpf_listener_loop` re-runs `quic_worker_index` and drops invalid packets or packets delivered to the wrong worker. | route-key unit tests and integration invalid-packet tests passed. | implemented |
| Platform capability is visible to DV tests without adding a public configuration knob. | `testing.md`, `testplan.yaml` | `supports_quic_reuseport_bpf()` reports Linux target capability; no new `ServiceConfig` field or feature was added. | `quic_reuseport_bpf_capability_matches_linux_target`; DV passed. | implemented |

## Consistency Summary
- Proposal authority check: `proposal.md` includes `P-quic-routed-udp` / `CHG-quic-routed-udp` and names the Linux high-performance reuse-port path while excluding full QUIC protocol responsibilities.
- Proposal vs design: consistent after D-8; design now includes Linux best-effort BPF selector and fallback semantics.
- Design vs testing: consistent after T-8; testing and `testplan.yaml` include selector/fallback coverage.
- Design/testing vs implementation: consistent; implementation is internal, additive, and keeps the public `QuicServer` packet-handler API unchanged.
- Implementation logic review: no feature-blocking issue found in the selected paths. BPF is best-effort; user-space route parsing remains authoritative for invalid packet rejection and fallback correctness.
- Cross-module admission: not applicable; only the `sfo-reuseport` root module is affected.

## Required Command Evidence
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-quic-routed-udp`: passed.
- `python3 ./harness/scripts/stage-scope-check.py --stage design --version v0.1 --module sfo-reuseport`: failed due broad pre-existing untracked worktree baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage testing --version v0.1 --module sfo-reuseport`: failed due broad pre-existing untracked worktree baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation --version v0.1 --module sfo-reuseport`: failed due broad pre-existing untracked worktree baseline.
- `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.

## Conclusion
- Accepted / rejected / needs changes: needs changes.
- Reason: implementation, document traceability, and canonical validation satisfy the approved behavior, but the required stage-scope checks fail because the repository has no committed baseline and all files are reported as untracked.
- Return routing: process/baseline task to normalize the repository state or adjust stage-scope checking for bootstrap repositories with no commits.
