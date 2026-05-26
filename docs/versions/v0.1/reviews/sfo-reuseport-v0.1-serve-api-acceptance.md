# sfo-reuseport v0.1 serve API acceptance

## Findings
| Severity | Finding | Evidence | Return Route |
|----------|---------|----------|--------------|
| High | Final process acceptance remains blocked by `stage-scope-check.py` because the working tree contains aggregate cross-stage and pre-existing pipeline changes. The reviewed serve API behavior itself is implemented and validated. | `stage-scope-check.py --stage implementation` and `stage-scope-check.py --stage acceptance` report proposal/design/testing docs, tests, examples, and pre-existing hyper static files outside the single-stage scope. | harness/process cleanup or a clean per-stage baseline before a strict final gate can pass. |
| None | No document-to-implementation mismatch found for synchronous `serve` registration. | Proposal, design, testing docs, production code, API tests, examples, and integration tests all converge on synchronous `TcpServer::serve`, `UdpServer::serve`, and `QuicServer::serve`. | none |

## Conclusion
needs changes

The serve API behavior is accepted by implementation evidence: the three public `serve` methods are synchronous, return `Result<(), Error>`, register listener work with `ServerRuntime`, and no longer use `pending` internally. Strict final process acceptance remains blocked only by the repository-wide stage-scope baseline described above.

## Document Coverage
- `proposal.md` now explicitly requires synchronous `TcpServer::serve`, `UdpServer::serve`, and `QuicServer::serve` methods that accept `&ServerRuntime`, return after listener registration, and do not use `pending` or an equivalent lifecycle future internally.
- `design.md` maps `CHG-server-runtime`, `CHG-tcp-serve`, `CHG-udp-runtime-socket`, and `CHG-quic-routed-udp` to synchronous listener registration methods and documents that service lifetime is held by `ServerRuntime` and listener registry state.
- `testing.md` and `testplan.yaml` cover the same API contract through `explicit-runtime-serve-api`, integration loopback tests, and source checks that reject `pub async fn serve` and production `pending`.

## Implementation Evidence
- `src/core/tcp.rs`: `TcpServer::serve` is `pub fn`, registers TCP listener work, and returns `Ok(())`.
- `src/core/udp.rs`: `UdpServer::serve` and `QuicServer::serve` are `pub fn`, register UDP/QUIC listener work, and return `Ok(())`.
- `src/core/tcp.rs` and `src/core/udp.rs` no longer import or call `std::future::pending`.
- `examples/tcp_echo.rs` and `examples/hyper_static.rs` now keep example process lifetime outside `serve`, after successful synchronous registration.
- Tests using long-lived servers keep the test task alive outside `serve`, which preserves the API contract while allowing runtime workers to process loopback traffic.

## Harness Results
- `uv run --active python ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `uv run --active python ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-server-runtime`: passed.
- `uv run --active python ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-tcp-serve`: passed.
- `uv run --active python ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-udp-runtime-socket`: passed.
- `uv run --active python ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-quic-routed-udp`: passed.
- `uv run --active python ./harness/scripts/test-run.py sfo-reuseport all`: passed.
- `uv run --active python ./harness/scripts/test-run.py all all`: passed.
- `./test-run.sh`: passed; shell printed `.venv/bin/activate: OSTYPE: parameter not set` before delegating successfully.
- `git diff --check`: passed.
- `stage-scope-check.py --stage proposal/design/testing/implementation/acceptance`: failed due aggregate working-tree scope, not a serve API logic mismatch.

## Test Evidence
- Unit: `api_signatures::server_entrypoints_are_public` constrains each `serve` return value to `Result<(), Error>`.
- Unit: `api_signatures::serve_entrypoints_are_synchronous_and_do_not_pending` checks production source for `pub fn serve`, absence of `pub async fn serve`, and absence of `pending` in TCP/UDP server implementation.
- DV: socket init callback and socket option tests call synchronous `serve` directly.
- Integration: TCP, UDP, and QUIC loopback tests pass with lifecycle waiting outside `serve`.
- DV smoke: `hyper_static` example starts successfully and serves expected 200/404/403 responses after moving lifecycle wait into the example.

## Mismatches
- No behavior mismatch for the synchronous serve API.
- Process mismatch remains: current stage-scope tooling evaluates the full dirty working tree, so strict single-stage checks fail during this multi-stage auto-pipeline continuation.

## Return Routing
Return category: harness/process cleanup.

Expected fix output: establish a clean baseline or make stage-scope validation compare only the active child-task delta. No serve API design, implementation, or testing return is required.
