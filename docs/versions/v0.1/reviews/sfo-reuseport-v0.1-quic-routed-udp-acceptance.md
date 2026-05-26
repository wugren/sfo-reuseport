# sfo-reuseport v0.1 Quic Routed UDP Acceptance Report

## Findings
| ID | Severity | Stage | Evidence | Problem | Fail Condition Hit |
|----|----------|-------|----------|---------|--------------------|
| QRU-A-001 | blocking | process | `stage-scope-check.py --stage proposal/design/testing/implementation` all report broad pre-existing untracked repository paths outside the active stage. | Stage-scope validation cannot pass in the current worktree baseline even though the implemented evidence chain for `CHG-quic-routed-udp` is consistent. | Single-stage task failed `stage-scope-check.py --stage <stage>`. |

## Object and Scope
- Module: `sfo-reuseport`
- Version: `v0.1`
- change_id values reviewed: `CHG-quic-routed-udp`
- Review date: 2026-05-25
- In scope: `QuicServer` as QUIC-aware UDP packet routing, route-key parsing, stable worker delivery, invalid route-key handling, public re-export, focused unit/integration tests.
- Out of scope: TLS, QUIC handshake, connection state, streams, congestion control, quinn integration, Linux eBPF/CBPF selector implementation.

## Optional Diff / Status Evidence
- `git status --short` summary: current repository baseline reports many paths as untracked, including source, docs, harness, and tests.
- `git diff --stat` summary: not useful for this worktree because tracked baseline is absent for the touched paths.
- `git diff --name-status` summary: not useful for this worktree because tracked baseline is absent for the touched paths.
- `git diff --check` result: not used as acceptance standard.
- Note: diff/status output is a discovery aid only, not the acceptance standard.

## Evidence Coverage
| Documented Item | Source Document | Implementation Evidence | Test / Result Evidence | Status |
|-----------------|-----------------|-------------------------|------------------------|--------|
| `QuicServer` is a dedicated QUIC-aware UDP routing entrypoint, not a QUIC protocol stack. | `proposal.md`, `design.md` | `src/core/udp.rs` defines `QuicServer`; `src/core/mod.rs` and `src/lib.rs` re-export it; no TLS/connection/stream API was added. | `tests/unit/quic_routed_udp.rs`; `tests/unit/api_signatures.rs`; unit passed. | implemented |
| Long-header QUIC DCID route key selects a stable worker. | `design.md`, `testing.md`, `testplan.yaml` | `quic_worker_index` reads long-header DCID first two bytes as big-endian `u16` worker shard and maps it modulo worker count. | `quic_routed_udp_delivers_long_header_dcid_to_target_worker`; integration passed. | implemented |
| Invalid, missing, or non-16-bit QUIC route key is rejected without handler delivery. | `design.md`, `testing.md`, `testplan.yaml` | `quic_worker_index` returns `None` for empty packet, zero DCID length, 1-byte DCID, and truncated DCID; listener loop continues without handler call. | internal unit tests, `quic_routed_udp_drops_invalid_route_key`, and `quic_routed_udp_requires_sixteen_bit_worker_shard`; integration passed. | implemented |
| User handler remains packet-level and can be consumed by upper QUIC modules. | `proposal.md`, `design.md` | `QuicServer` handler signature matches UDP packet handler: `BalancedUdpSocket`, `PacketMeta`, `Vec<u8>`. | API signature tests passed. | implemented |

## Inputs
- `proposal.md`
- `design.md`
- `testing.md`
- `testplan.yaml`
- implementation under `src/core/udp.rs`, `src/core/mod.rs`, `src/lib.rs`
- test code under `tests/unit/quic_routed_udp.rs`, `tests/integration/quic_routed_udp.rs`, `tests/unit/api_signatures.rs`
- test results from canonical unit, DV, and integration commands
- `harness/rules/acceptance-review-rules.md`

## Consistency Summary
- Proposal authority check: `proposal.md` directly includes `P-quic-routed-udp` / `CHG-quic-routed-udp` and excludes TLS, QUIC connection, and stream responsibilities.
- Proposal vs design: consistent; design maps `CHG-quic-routed-udp` to `QuicServer`, route-key parsing, invalid packet handling, and portable user-space worker dispatch.
- Design vs testing: consistent; testing maps `CHG-quic-routed-udp` to unit and integration validation plus testplan entries.
- Design vs long-lived boundary doc: no long-lived module boundary update was required for this root-module API addition.
- Design/testing vs implementation: consistent.
- Testing docs vs testplan vs test code vs results: consistent.
- change_id traceability: present across proposal, design, testing, and `testplan.yaml`.
- Acceptance criteria traceability: packet routing and non-QUIC-stack boundary are covered.
- Cross-module admission: not applicable; only `sfo-reuseport` root module is affected.
- Public API / codec / runtime semantics review: public API is additive; QUIC parsing is limited to length-checked 16-bit route-key extraction.
- Document logic review: coherent; eBPF/CBPF selector is documented as out of current implementation scope.
- Implementation logic review: no correctness blocker found in the implemented route-key and worker-dispatch path.
- Document approval timing: proposal/design/testing were auto-confirmed during explicit auto-pipeline launch before implementation admission.

## Required Command Evidence
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-quic-routed-udp`: passed.
- `python3 ./harness/scripts/stage-scope-check.py --stage proposal`: failed due broad pre-existing untracked worktree baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage design`: failed due broad pre-existing untracked worktree baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage testing`: failed due broad pre-existing untracked worktree baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation`: failed due broad pre-existing untracked worktree baseline.
- Unit test command from `testplan.yaml`: `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- DV test command from `testplan.yaml`: `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- Integration test command from `testplan.yaml`: `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.

## Conclusion
- Accepted / rejected / needs changes: needs changes.
- Reason: implementation and validation evidence satisfy `CHG-quic-routed-udp`, but stage-scope remains blocking because the repository baseline causes checker violations unrelated to the feature files.
- Supporting test evidence: canonical unit, DV, and integration commands passed.
- Residual risk: `QuicServer` currently uses portable user-space routing, not Linux eBPF/CBPF kernel socket selection; that is documented as outside this implementation and requires a new design/testing pass before implementation.

## Follow-Up Tasks
- Requirement task: none for the accepted feature semantics.
- Design task: add a future Linux eBPF/CBPF selector design if kernel-level reuse-port routing is required.
- Testing task: add eBPF/CBPF permission, kernel version, and fallback tests if that design is approved.
- Implementation task: resolve or normalize the repository baseline so `stage-scope-check.py` can distinguish task diffs from pre-existing untracked files.
