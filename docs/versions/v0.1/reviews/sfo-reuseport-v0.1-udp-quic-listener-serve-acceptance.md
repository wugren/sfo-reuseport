---
module: sfo-reuseport
submodule:
version: v0.1
status: accepted
reviewed_by: auto-pipeline
reviewed_at: 2026-05-28T07:21:24Z
change_ids:
  - CHG-udp-quic-listener-serve
---

# UDP/QUIC Listener Serve Acceptance

## Findings
| Severity | Finding | Owner | Status |
|----------|---------|-------|--------|
| Low | Stage-scope checks cannot pass in the cumulative auto-pipeline worktree because proposal, design, implementation, testing, and pre-existing untracked review reports are present together. Evidence below was reviewed by stage responsibility instead. | process | accepted caveat |
| Low | Direct fallback worker-isolation validation is recorded as a testing gap because this Linux run used native reuse-port/BPF-capable paths; the implementation includes the fallback routed socket backend and the test plan records the missing forced-fallback coverage. | testing | accepted caveat |

## Conclusion
Accepted. `UdpServer::serve_socket` and `QuicServer::serve_socket` now use a socket callback and return the server lifecycle object. Native reuse-port paths deliver each worker socket on its worker thread. Fallback paths create routed `UdpSocket` views so each worker can only receive packets selected for that worker by internal scheduling.

## Acceptance Rules And Expected Results
| Rule | Expected Result | Evidence |
|------|-----------------|----------|
| Socket-only serve uses callback delivery rather than returning a single socket. | API requires `&ServerRuntime`, `ServiceConfig`, and a socket callback; return type is `UdpServer` or `QuicServer`. | `src/core/udp.rs`, `tests/unit/api_signatures.rs` |
| Native reuse-port path returns worker-local sockets through callbacks. | Worker sockets are created with `bind_udp_workers` or QUIC reuse-port BPF bind, submitted to matching worker ids, registered, converted to unified `UdpSocket`, and passed to the callback. | `src/core/udp.rs` |
| Fallback path does not expose one full shared receive socket to every worker. | Fallback socket-only serve creates per-worker routed `UdpSocket` views backed by queues; a dispatcher reads the real socket and sends each packet only to the selected worker queue. | `src/core/udp.rs` |
| UDP fallback selection follows internal Linux-compatible scheduling. | Plain UDP fallback uses `linux_reuseport_select(meta, worker_count)` with peer/local metadata. | `src/core/udp.rs`, `src/core/schedule.rs` |
| QUIC fallback selection follows the fixed worker shard rule. | QUIC fallback uses `quic_worker_index` and drops unroutable packets. | `src/core/udp.rs`, `tests/unit` |
| Application-owned reads still work. | Integration tests obtain the callback-delivered socket and call `recv_from`/`recv_from_vec` from the application side. | `tests/integration/udp_serve.rs` |

## Document Coverage
- Proposal directly covers `CHG-udp-quic-listener-serve`, including callback delivery and fallback per-worker receive isolation.
- Design maps `CHG-udp-quic-listener-serve` and defines callback signatures, server lifecycle return types, native worker socket delivery, and fallback routed socket views.
- Testing docs and `testplan.yaml` cover callback API and loopback behavior, and record the forced-fallback worker isolation gap.

## Implementation Evidence
- `UdpSocket` now supports runtime-backed sockets and routed fallback socket views.
- `UdpServer::serve_socket` returns `Result<UdpServer, Error>` and calls the supplied callback with worker socket views.
- `QuicServer::serve_socket` returns `Result<QuicServer, Error>` and uses QUIC reuse-port BPF worker sockets when available, otherwise fallback routed socket views.
- Fallback routed views share send capability but receive only packets dispatched to their worker queue.
- tokio-uring keeps native reuse-port-only behavior and returns `UnsupportedPlatformOption` for simulated fallback, preserving its non-`Send` socket boundary.

## Test Evidence
| Command | Result |
|---------|--------|
| `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport` | passed |
| `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-udp-quic-listener-serve` | passed |
| `cargo check` | passed |
| `cargo check --no-default-features --features runtime-tokio-uring --all-targets` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport unit` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport integration` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport dv` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport all` | passed |

## Residual Risk
- Forced fallback worker-isolation tests should be added when the harness can force `supports_reuse_port_balancing() == false` on a platform or test backend. The implementation path exists, but this run did not execute it directly on Linux.
