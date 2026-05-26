# sfo-reuseport v0.1 Quic Reuse-Port eBPF Acceptance Report

## Findings
| id | severity | category | finding | evidence | route |
|----|----------|----------|---------|----------|-------|
| QRE-A-001 | blocking | process | `stage-scope-check.py --stage design/testing/implementation` still reports broad repository paths because the repository has no committed baseline and all files are untracked. | Design/testing/implementation stage-scope checks list unrelated repository files, matching prior acceptance reports. | Establish a git baseline or adjust the checker workflow before final process acceptance. |

## Conclusion
Needs changes for final process acceptance because the stage-scope baseline issue remains open.

The eBPF implementation evidence is accepted: documents, code, and tests consistently describe and implement Linux eBPF as the first reuse-port selector attempt, CBPF as fallback, and user-space routing as final fallback.

## Scope
- In scope: Linux best-effort `SO_ATTACH_REUSEPORT_EBPF` selector for `QuicServer`, `BPF_PROG_TYPE_SK_REUSEPORT` load path, CBPF fallback, user-space fallback, fixed 16-bit QUIC worker shard parsing, invalid packet rejection, focused unit/DV/integration evidence.
- Out of scope: TLS, QUIC handshake, connection state, streams, congestion control, quinn integration, configurable CID layouts, hard failure on BPF/CBPF attach errors.

## Evidence Matrix
| Requirement | Document Evidence | Implementation Evidence | Validation Evidence | Status |
|-------------|-------------------|--------------------------|---------------------|--------|
| Linux `QuicServer` first attempts eBPF reuse-port selection. | `design.md`, `testing.md`, `testplan.yaml` | `src/platform/mod.rs` loads `BPF_PROG_TYPE_SK_REUSEPORT` and attaches with `SO_ATTACH_REUSEPORT_EBPF` before CBPF. | `platform::tests::quic_reuseport_ebpf_reads_long_and_short_header_shards`; unit passed. | implemented |
| eBPF failure falls back to CBPF, then user-space routing without public API change. | `design.md`, `testing.md` | `bind_quic_udp_reuseport_workers_impl` accepts either eBPF or CBPF success, otherwise returns `Ok(None)` so `QuicServer` uses user-space routing. | unit/DV/integration passed. | implemented |
| QUIC route key remains fixed 16-bit worker shard. | `proposal.md`, `design.md`, `testing.md` | eBPF instruction generation reads long-header DCID bytes 6/7 and short-header bytes 1/2, then modulo worker count; user-space loop still validates route key. | unit route parsing tests and integration `quic_routed_udp_*` tests passed. | implemented |
| BPF is not public API and attach/load failure is not startup-fatal. | `design.md` | Platform function returns `Option<Vec<UdpSocket>>`; public `QuicServer` API unchanged. | DV and integration passed on current target. | implemented |

## Checks
| Check | Result |
|-------|--------|
| `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport` | passed |
| `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-quic-routed-udp --change-id CHG-platform-behavior` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport unit` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport dv` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport integration` | passed |
| `python3 ./harness/scripts/stage-scope-check.py --stage design` | failed due repository baseline |
| `python3 ./harness/scripts/stage-scope-check.py --stage testing` | failed due repository baseline |
| `python3 ./harness/scripts/stage-scope-check.py --stage implementation` | failed due repository baseline |

## Consistency Review
- Proposal vs design: consistent. Proposal allows Linux high-performance path to depend on eBPF/CBPF and requires permission/fallback behavior to be covered.
- Design vs testing: consistent after T-9. Testing now names eBPF primary selector, CBPF fallback, and user-space fallback.
- Testing vs implementation: consistent. Unit tests cover generated eBPF/CBPF instruction shape; integration validates stable QUIC routing when selector succeeds or fallback occurs.
- Implementation logic: no feature-blocking issue found in the reviewed paths. eBPF is best-effort and does not remove user-space invalid packet validation.

## Return Routing
- Process issue remains: stage-scope baseline must be fixed outside the eBPF implementation task.
- No design, testing, or implementation return is required for the eBPF behavior itself.
