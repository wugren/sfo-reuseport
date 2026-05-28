---
module: sfo-reuseport
version: v0.1
status: accepted
reviewed_by: auto-pipeline
reviewed_at: 2026-05-26T15:42:38Z
change_ids:
  - CHG-dynamic-listeners
---

# Acceptance: remove public TCP/UDP add listener APIs

## Findings
- No blocking findings.

## Conclusion
Accepted.

## Evidence
- Proposal, design, testing metadata, and testplan now state that `ServerRuntime::add_tcp_listener` and `ServerRuntime::add_udp_listener` are not public API.
- Production code removes the public `impl ServerRuntime` methods from `src/core/tcp.rs` and `src/core/udp.rs`.
- Unit tests assert the public TCP/UDP add-listener methods are absent.
- Integration tests verify TCP/UDP listeners still register and receive work through `TcpServer::serve` and `UdpServer::serve` on the same `ServerRuntime`.

## Validation
- `schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-dynamic-listeners`: passed.
- `test-run.py sfo-reuseport unit`: passed.
- `test-run.py sfo-reuseport integration`: passed.
- `test-run.py sfo-reuseport all`: passed.
- `test-run.py all all`: passed.

## Residual Risk
- Existing `Cargo.lock` is untracked in the worktree and predates this acceptance task; this review did not classify it as evidence for the API removal.
