---
module: sfo-reuseport
submodule:
version: v0.1
change_id: CHG-tokio-uring-runtime
status: accepted
reviewed_by: auto-pipeline
reviewed_at: 2026-06-03T18:20:14+08:00
---

# Tokio-Uring Runtime Acceptance

## Findings
- No blocking findings.
- Non-blocking scope note: this was an auto-pipeline update spanning proposal, design, implementation, testing, validation, and acceptance artifacts. Single-stage scope checks are not applicable as pass/fail evidence for the combined pipeline; schema, admission, canonical test entrypoints, root shortcut, and diff whitespace validation passed.
- Non-blocking dependency note: `runtime-tokio-uring` retains the `tokio-uring` feature/dependency boundary while network socket conversion, public network types, handler execution, and examples use the tokio compatibility backend. Proposal and design now describe this boundary explicitly.

## Conclusion
accepted

## Scope Reviewed
- Proposal item: `P-tokio-uring-runtime`
- Change id: `CHG-tokio-uring-runtime`
- Code evidence:
  - `src/runtime/tokio_uring.rs`
  - `src/core/mod.rs`
  - `src/core/server_runtime.rs`
  - `src/core/tcp.rs`
  - `src/core/udp.rs`
  - `src/core/config.rs`
  - `examples/tcp_echo.rs`
  - `examples/udp_server.rs`
  - `examples/udp_serve_socket.rs`
  - `examples/hyper_static.rs`
- Test and harness evidence:
  - `tests/unit/runtime_features.rs`
  - `harness/scripts/test-run.py`
  - `docs/versions/v0.1/modules/sfo-reuseport/testing.md`
  - `docs/versions/v0.1/modules/sfo-reuseport/testplan.yaml`

## Acceptance Rules And Expected Results
- `runtime-tokio-uring` remains mutually exclusive with other runtime features.
- The feature remains Linux-only through cfg/compile boundary.
- Network-related public interfaces under `runtime-tokio-uring` use tokio TCP/UDP socket types or the crate unified `UdpSocket` tokio I/O surface.
- No tokio-uring native TCP/UDP socket type is exposed as the public network interface.
- Handler futures under `runtime-tokio-uring` use the same `Send + 'static` boundary as the tokio compatibility path.
- TCP, UDP, UDP socket-only, quinn helper compilation, and examples remain reachable through the unified test entrypoint.

Expected result: all reviewed rules are satisfied.

## Evidence Summary
- `src/runtime/tokio_uring.rs` keeps the Linux-only boundary and includes the tokio runtime adapter, so `TcpStream`, `UdpSocket`, spawn, sleep, and socket conversion use tokio-compatible behavior under the `runtime-tokio-uring` feature.
- `src/core/mod.rs`, `src/core/server_runtime.rs`, `src/core/tcp.rs`, and `src/core/udp.rs` no longer special-case tokio-uring network handlers as non-`Send` or local-only; they submit through the same executor path as tokio where network public types are tokio-backed.
- `src/core/udp.rs` uses tokio queue/socket handling for `runtime-tokio-uring`, including socket-only simulated listener support and quinn helper compilation with tokio readiness semantics.
- Examples that previously used or avoided tokio-uring runtime setup now use tokio entrypoints and tokio network operations under `runtime-tokio-uring`.
- `tests/unit/runtime_features.rs` asserts that `runtime-tokio-uring` exposes a tokio `TcpStream` type and that both `TcpStream` and the unified `UdpSocket` are `Send`.
- `harness/scripts/test-run.py` wires the `runtime-tokio-uring` unit test into the canonical `unit` entrypoint.

## Command Results
- `uv run --active python ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `uv run --active python ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-tokio-uring-runtime`: passed.
- `cargo check`: passed.
- `cargo check --no-default-features --features runtime-tokio-uring`: passed.
- `cargo check --examples --no-default-features --features runtime-tokio-uring`: passed.
- `cargo check --no-default-features --features runtime-tokio-uring,quinn`: passed.
- `cargo test --no-default-features --features runtime-tokio-uring --test unit runtime_features -- --nocapture`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.
- `uv run --active python ./harness/scripts/test-run.py all all`: passed.
- `./test-run.sh`: passed.
- `git diff --check`: passed.
- Targeted search for `tokio_uring::net`, `tokio_uring::start`, and `tokio_uring::Runtime` in `src`, `examples`, and `tests`: no production or example usage of tokio-uring native network/runtime APIs remains for this change.

## Consistency Audit
- Proposal, design, testing metadata, and testplan all map `P-tokio-uring-runtime` to `CHG-tokio-uring-runtime`.
- Proposal and design both state that `runtime-tokio-uring` preserves the feature/Linux/dependency boundary while using tokio network interfaces and tokio-compatible handler execution.
- Testing metadata requires the same evidence: Linux feature compilation, tokio network public type assertion, `Send` assertions, quinn feature compilation, and example smoke coverage.
- Implementation matches the documented compatibility backend and does not expose tokio-uring native TCP/UDP socket types.
- No contradiction found between approved documents, implementation, test metadata, and validation output.

## Unresolved Risks
- Non-Linux behavior is intentionally limited to cfg/compile boundary evidence and is not runtime-tested on this host.
- The retained optional `tokio-uring` dependency is a feature/dependency boundary, not evidence of native tokio-uring network I/O usage.

## Return Routing
- No return required.
