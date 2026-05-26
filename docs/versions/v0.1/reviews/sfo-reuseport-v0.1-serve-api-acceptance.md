# sfo-reuseport v0.1 serve API acceptance

## Findings
| Severity | Finding | Evidence | Return Route |
|----------|---------|----------|--------------|
| High | Final acceptance is blocked because required `stage-scope-check.py` still reports repository-wide pre-existing untracked baseline paths outside the active stage. The reviewed implementation evidence is consistent, but the auto-pipeline acceptance gate cannot mark final pass while this checker fails. | `stage-scope-check.py --stage implementation` and `stage-scope-check.py --stage acceptance` report unrelated docs/rules/reviews/source paths as violations in addition to this task's files. This matches prior pipeline baseline blockers. | harness/process cleanup or baseline normalization before final acceptance can pass. |
| None | No document-to-implementation mismatch found for the explicit runtime-only serve API. | Proposal, design, testing, `testplan.yaml`, source, tests, and examples all converge on explicit `&ServerRuntime` serve calls. | none |

## Conclusion
needs changes

The implementation satisfies the approved serve API behavior, and required automated validation passed. Final acceptance remains blocked only by the repository baseline stage-scope failure.

## Document Coverage
- `proposal.md` approves exactly one `serve(runtime: &ServerRuntime, config: ServiceConfig, handler: F)` entry for each of `TcpServer`, `UdpServer`, and `QuicServer`, and forbids `serve_with_runtime` or implicit default runtime entrypoints.
- `design.md` maps `CHG-server-runtime`, `CHG-tcp-serve`, `CHG-udp-balanced-socket`, and `CHG-quic-routed-udp` to the explicit runtime API and documents `ServerRuntime::add_*_listener` as the dynamic listener path.
- `testing.md` and `testplan.yaml` add `explicit-runtime-serve-api` coverage for the same change IDs.

## Implementation Evidence
- `src/core/tcp.rs` exposes only `TcpServer::serve(&ServerRuntime, ServiceConfig, handler)`.
- `src/core/udp.rs` exposes only `UdpServer::serve(&ServerRuntime, ServiceConfig, handler)` and `QuicServer::serve(&ServerRuntime, ServiceConfig, handler)`.
- `src/core/server_runtime.rs` owns dynamic listener registration through `add_tcp_listener`, `add_udp_listener`, and `add_quic_listener`.
- Tests and `examples/tcp_echo.rs` use explicit `ServerRuntime` construction and pass `&runtime` into server `serve` methods.
- Targeted search found no remaining `serve_with_runtime` or `serve_on` symbols outside the regression test assertions that check they are absent.

## Harness Results
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-server-runtime --change-id CHG-tcp-serve --change-id CHG-udp-balanced-socket --change-id CHG-quic-routed-udp`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation`: failed due to existing repository-wide untracked baseline.
- `python3 ./harness/scripts/stage-scope-check.py --stage acceptance`: failed due to existing repository-wide untracked baseline.

## Test Evidence
- Unit tests include `api_signatures::server_entrypoints_are_public` and `api_signatures::legacy_server_entrypoints_are_not_public`.
- Integration tests cover TCP loopback, UDP loopback, dynamic listener registration/removal, mixed TCP/UDP runtime behavior, and QUIC routed UDP behavior after the API migration.
- DV remains a clean `cargo check`.

## Mismatches
- No behavioral or document mismatch for this serve API change.
- Governance mismatch remains: stage-scope cannot distinguish this task from the repository's pre-existing all-untracked baseline.

## Return Routing
Return category: harness/process cleanup.

Expected fix output: establish a clean tracked baseline or adjust the stage-scope workflow so single-stage checks can evaluate only the active task delta. No serve API code or test return is required.
