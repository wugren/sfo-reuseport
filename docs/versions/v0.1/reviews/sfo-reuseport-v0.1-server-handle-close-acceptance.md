# sfo-reuseport v0.1 server handle close acceptance

## Findings

No blocking findings remain.

## Conclusion

Accepted with environment notes. The approved proposal, design, implementation, tests, and testplan are consistent for returning typed server objects from `serve`, explicitly closing TCP/UDP/QUIC server tasks, keeping listener management out of `ServerRuntime`, and exposing UDP/QUIC listener socket lookup from the returned server object.

`uv` is not installed in this environment, so commands documented with `uv run --active` were executed through the equivalent `python3 ./harness/scripts/...` entrypoint. The root shortcut `./test-run.sh` was executed and failed only because it requires `uv`.

## Generated Acceptance Rules

- `TcpServer::serve`, `UdpServer::serve`, and `QuicServer::serve` must synchronously accept an explicit `&ServerRuntime` and return `Result<TcpServer, Error>`, `Result<UdpServer, Error>`, and `Result<QuicServer, Error>` respectively.
- Returned server objects must expose `close` and closing one server must not close the shared `ServerRuntime` or unrelated server tasks.
- `UdpServer` and `QuicServer` must expose `listener_socket`; lookup must prefer the current listener thread socket and otherwise select from that server's listener socket set.
- `ServerRuntime` must not expose public listener add/remove APIs or listener id/protocol registry types.
- Already delivered handler futures are not force-cancelled by server close.
- UDP/QUIC handlers receive the crate-level unified `UdpSocket`; `BalancedUdpSocket` and dispatch policy APIs remain non-public.

## Expected Results

- API signature tests compile against typed server return values and can call `close`.
- TCP close wakes and exits listener work without invoking new handlers after close.
- UDP/QUIC close marks the server closed; UDP close wakes `recv_from`; `listener_socket` returns an active listener socket before close and errors after close.
- QUIC routed UDP behavior, invalid packet drops, platform fallback, socket options, and examples remain valid.

## Evidence

- Proposal coverage: `proposal.md` maps the behavior to `CHG-server-runtime`, `CHG-tcp-serve`, `CHG-udp-runtime-socket`, `CHG-dynamic-listeners`, `CHG-mixed-protocol-workers`, and `CHG-quic-routed-udp`.
- Design coverage: `design.md` defines typed server return objects, explicit `close`, UDP/QUIC listener socket lookup, no `ServerRuntime` listener registry, and no independent stop handle type.
- Implementation evidence:
  - `src/core/tcp.rs`: `TcpServer` holds per-server task state, returns itself from `serve`, and `close` cancels the server-owned listener tasks.
  - `src/core/udp.rs`: `UdpServer` and `QuicServer` share per-server state, register listener sockets per worker thread, return typed objects, expose `close` and `listener_socket`, shut down recorded UDP sockets where available, and cancel server-owned tasks on close.
  - `src/platform/linux.rs`: uses `socket2::Socket::set_ip_transparent_v4`, matching the current socket2 API.
- Test evidence:
  - `tests/unit/api_signatures.rs` verifies typed `serve` return values and close methods.
  - `tests/unit/server_runtime.rs` verifies server objects attach through `serve` without public runtime listener management.
  - `tests/integration/dynamic_listeners.rs` covers TCP close, UDP close, UDP listener socket lookup, QUIC listener socket lookup, and mixed TCP/UDP runtime service.
  - `tests/dv/socket_init_callback.rs` was updated for typed server return values without requiring `Debug`.

## Harness Results

- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-dynamic-listeners`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-server-runtime`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-tcp-serve`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-udp-runtime-socket`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-quic-routed-udp`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-mixed-protocol-workers`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-socket-options`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- `python3 ./harness/scripts/test-run.py all all`: passed.
- `./test-run.sh`: failed before running tests because `uv` is not installed.
- `cargo check`: passed.
- `cargo test --test unit`: passed.
- `cargo test --test integration`: passed.

## Scope And Diff Evidence

- `git diff --check`: passed.
- `git diff --name-status`: proposal, design, testing docs, testplan, TCP/UDP core code, Linux socket option code, and relevant unit/integration/DV tests changed.
- `stage-scope-check.py --stage proposal --version v0.1 --module sfo-reuseport --ignore-untracked`: passed before downstream stages.
- `stage-scope-check.py --stage design --version v0.1 --module sfo-reuseport --ignore-untracked`: failed because the working tree also contained the prior proposal edit from the same auto pipeline.
- `stage-scope-check.py --stage testing --version v0.1 --module sfo-reuseport --ignore-untracked`: failed because the working tree also contained proposal, design, and implementation edits from the same auto pipeline.

## Iterations

Two acceptance iterations:

1. Initial acceptance review found that `close` only flipped an active flag and listener tasks could remain blocked until external IO.
2. Implementation was updated to actively wake TCP accept and UDP/QUIC recv loops; full module and project test evidence was rerun and passed.
3. Follow-up review found the first TCP wake approach used loopback connects and therefore could not guarantee waking every `SO_REUSEPORT` listener. The current implementation closes by cancelling server-owned listener tasks; unit and integration evidence was rerun and passed.

## Residual Risks

- Root shortcut validation depends on `uv`; this environment does not have `uv` installed.
- Manual platform matrix and privileged transparent socket checks remain governed by the existing manual testplan entries.
