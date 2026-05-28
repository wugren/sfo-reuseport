# sfo-reuseport v0.1 IPv6 Transparent Acceptance Report

## Findings
| ID | Severity | Stage | Evidence | Problem | Status |
|----|----------|-------|----------|---------|--------|
| A-IPV6-TRANS-001 | Medium | testing | `python3 ./harness/scripts/test-run.py sfo-reuseport dv` fails while compiling `--features runtime-tokio-uring --example tcp_echo`; error points to `src/core/tcp.rs:209` moving `tokio_uring::net::TcpStream` through a `Send` submit closure. | Full DV entrypoint is not green in this workspace. The failure is in the tokio-uring TCP path and is not caused by the IPv6 transparent socket option changes. | follow-up required outside `CHG-socket-options` |

## Object and Scope
- Module: `sfo-reuseport`
- Version: `v0.1`
- Reviewed change_id: `CHG-socket-options`
- Review date: 2026-05-27
- In scope: `SocketOptions` IPv6 transparent configuration, Linux platform application, unsupported platform behavior, focused unit/DV coverage, proposal/design/testing traceability.
- Out of scope: tokio-uring TCP stream cross-thread submission, QUIC routing, server handle lifecycle.

## Evidence Coverage
| Documented Item | Source Document | Implementation Evidence | Test / Result Evidence | Status |
|-----------------|-----------------|-------------------------|------------------------|--------|
| Socket options cover IPv4 and IPv6 transparent modes. | `proposal.md`, `design.md` | `src/core/config.rs` adds `ipv6_transparent: TransparentMode` next to existing `ipv4_transparent`. | `tests/unit/socket_options.rs`; unit harness passed. | implemented |
| Linux applies transparent mode by address family. | `design.md` | `src/platform/linux.rs` applies `set_ip_transparent_v4` for IPv4 bind addresses and `set_ip_transparent_v6` for IPv6 bind addresses. | `cargo check` passed; focused DV socket option tests passed. | implemented |
| `Required` on the wrong address family returns explicit unsupported error; `BestEffort` does not block startup for mismatched address family or failed privileged set. | `proposal.md`, `design.md`, `testing.md` | `src/platform/linux.rs` separates v4/v6 helpers and preserves `BestEffort` swallowing behavior. | Focused DV tests cover required IPv4 and required IPv6 paths. | implemented |
| Non-Linux platforms do not expose transparent support as successful `Required`. | `proposal.md`, `design.md` | `src/platform/bsd.rs` and `src/platform/windows.rs` reject required transparent mode when either v4 or v6 is required. | Platform matrix remains manual/not covered in this workspace. | manual gap retained |

## Command Evidence
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-socket-options`: passed.
- `cargo check`: passed.
- `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- `cargo test --test dv socket_options -- --nocapture`: passed, 2 tests.
- `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: failed in `runtime-tokio-uring` example compile path, unrelated to socket options.
- Canonical `uv run --active ...` commands could not be used because `uv` is not installed in this environment; equivalent `python3` checker/test entrypoints were used where possible.

## Consistency Summary
- Proposal and design are approved by auto-pipeline and directly map `CHG-socket-options`.
- Proposal, design, testing, and testplan now all describe IPv4/IPv6 transparent coverage.
- Implementation changes are limited to socket option configuration and platform socket option application.
- Focused validation for the changed behavior passed.
- Full DV remains blocked by an unrelated tokio-uring `Send` boundary issue in `src/core/tcp.rs`.

## Conclusion
- Current change status: accepted with unrelated DV blocker recorded.
- Follow-up: route `A-IPV6-TRANS-001` through the tokio-uring/runtime design and implementation path, not through `CHG-socket-options`.
