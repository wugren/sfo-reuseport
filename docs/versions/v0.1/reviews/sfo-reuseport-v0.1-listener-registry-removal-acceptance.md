# sfo-reuseport v0.1 Listener Registry Removal Acceptance

## Findings
| Severity | Finding | Evidence | Return Route |
|----------|---------|----------|--------------|
| process-blocking | Strict single-stage `stage-scope-check` cannot pass in the current aggregate working tree because proposal, design, implementation, testing, pipeline-plan, review, and pre-existing unrelated changes are present together. | `stage-scope-check.py --stage implementation` and `--stage testing` both failed with cross-stage path lists. | process baseline / split-stage cleanup |
| none | No behavior-blocking mismatch found for `CHG-dynamic-listeners`. | Approved proposal/design, implementation diff, unit/integration/all harness results. | n/a |

## Conclusion
- Behavioral acceptance: accepted.
- Strict final process acceptance: needs changes because stage-scope remains blocked by the aggregate working-tree baseline.

## Scope
- Version: `v0.1`
- Module: `sfo-reuseport`
- change_id reviewed: `CHG-dynamic-listeners`
- User request: remove `ServerRuntimeInner.listeners` and automatically process downstream workflow.

## Acceptance Rules
- `ServerRuntime` must not retain a listener registry or per-listener id management surface.
- Public API must not expose `ListenerId`, `ListenerProtocol`, `add_tcp_listener`, `add_udp_listener`, `add_quic_listener`, or `remove_listener`.
- TCP, UDP, and QUIC-aware UDP listeners must still register through their `serve(&ServerRuntime, ServiceConfig, handler)` entrypoints.
- Runtime lifecycle must avoid retaining `ServerRuntime` clones from long-running simulated listener loops.

## Evidence
- `proposal.md`: `P-dynamic-listeners` now requires no public listener dynamic management API and removal of listener registry/id management.
- `design.md`: `CHG-dynamic-listeners` maps removal of registry, listener id API, and per-listener deletion to `server_runtime`, TCP, UDP, exports, and tests.
- `src/core/server_runtime.rs`: `ServerRuntimeInner` now holds workers plus a shared active flag, with no `listeners` map, `next_id`, `ListenerId`, `ListenerProtocol`, or `remove_listener`.
- `src/core/tcp.rs`: simulated TCP accept loop uses worker executor handles plus the shared active flag instead of holding a `ServerRuntime` clone.
- `src/core/udp.rs`: QUIC listener registration uses `QuicServer::serve`; user-space dispatch uses worker executor handles instead of a `ServerRuntime` clone.
- `src/lib.rs` and `src/core/mod.rs`: listener id/protocol/config exports removed.
- `tests/unit/api_signatures.rs` and `tests/unit/server_runtime.rs`: assert listener dynamic management symbols are absent.
- `tests/integration/quic_routed_udp.rs`: uses `QuicServer::serve` instead of `add_quic_listener/remove_listener`.

## Command Results
- `schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-dynamic-listeners`: passed.
- `cargo check`: passed.
- `test-run.py sfo-reuseport unit`: passed, 8 lib tests and 22 unit tests.
- `test-run.py sfo-reuseport integration`: passed, 10 integration tests.
- `test-run.py sfo-reuseport all`: passed, including unit, DV, examples, async-std check, tokio-uring all-targets check, and integration.
- `stage-scope-check.py --stage implementation --version v0.1 --module sfo-reuseport`: failed on aggregate cross-stage diffs.
- `stage-scope-check.py --stage testing --version v0.1 --module sfo-reuseport`: failed on aggregate cross-stage diffs.

## Consistency
- Proposal, design, implementation, and tests agree on serve-only listener registration.
- No downstream document expands the proposal back to listener id deletion.
- The implementation removes the registry rather than leaving an unused field or inert public API.

## Residual Risk
- The current working tree still contains earlier unrelated or aggregate pipeline changes, so strict single-stage scope cannot be proven from the raw diff.
- Runtime drop behavior is covered by compilation and serve-path integration tests, but there is no dedicated stress test for runtime drop racing a simulated accept loop.
