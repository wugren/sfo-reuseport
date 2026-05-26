# sfo-reuseport v0.1 tokio-uring runtime acceptance

## Findings
- None blocking.

## Conclusion
- Accepted.
- `CHG-tokio-uring-runtime` is covered by approved proposal/design, implemented in production code, registered in post-implementation testing evidence, and verified through the unified test entrypoint.

## Scope
- Version: `v0.1`
- Module: `sfo-reuseport`
- Change ID: `CHG-tokio-uring-runtime`
- Acceptance time: `2026-05-26T08:26:22Z`

## Acceptance Rules And Expected Results
| Rule | Expected result | Outcome |
|------|-----------------|---------|
| Proposal and design directly map `CHG-tokio-uring-runtime`. | `proposal.md` and `design.md` contain matching table rows. | pass |
| Runtime features remain mutually exclusive. | `runtime-tokio-uring` cannot be combined with tokio or async-std features. | pass |
| tokio-uring is Linux bounded. | Non-Linux targets get an explicit compile-time boundary. | pass |
| Users get tokio-uring-related TCP/UDP interfaces under the feature. | `TcpStream` maps to tokio-uring TCP stream; `UdpSocket` wraps tokio-uring UDP socket and exposes tokio-uring buffer-owned send/recv methods. | pass |
| Existing tokio and async-std behavior remains reachable. | Default tokio unit/integration tests pass; async-std compile check passes. | pass |
| Tests are reachable through the unified entrypoint. | `python3 ./harness/scripts/test-run.py all all` reaches unit, dv, and integration layers. | pass |

## Evidence
- Proposal coverage:
  - `docs/versions/v0.1/modules/sfo-reuseport/proposal.md` has `P-tokio-uring-runtime` / `CHG-tokio-uring-runtime`.
- Design coverage:
  - `docs/versions/v0.1/modules/sfo-reuseport/design.md` has direct mapping for `CHG-tokio-uring-runtime`, including Cargo feature, adapter, Linux cfg, type mapping, and worker runtime behavior.
- Implementation evidence:
  - `Cargo.toml` adds `runtime-tokio-uring` and optional `tokio-uring`.
  - `src/lib.rs` enforces three-way runtime feature exclusivity and Linux-only tokio-uring.
  - `src/runtime/tokio_uring.rs` implements the adapter and tokio-uring socket wrapper, with an internal Linux compile guard.
  - `CurrentThreadExecutor` for tokio-uring holds one `tokio_uring::Runtime` created on the worker thread, reuses it for local `block_on` execution, uses direct tokio-uring local spawn on the owner thread, and uses a task channel only when submissions come from another thread.
  - `src/core/tcp.rs`, `src/core/udp.rs`, and `src/core/server_runtime.rs` use local worker submission for tokio-uring socket futures.
- Testing evidence:
  - `docs/versions/v0.1/modules/sfo-reuseport/testing.md` and `testplan.yaml` include `CHG-tokio-uring-runtime`.
  - `tests/unit/runtime_features.rs` covers public runtime socket type visibility.
  - `harness/scripts/test-run.py` runs default, async-std, and tokio-uring feature checks.

## Command Results
| Command | Result |
|---------|--------|
| `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport` | passed |
| `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-tokio-uring-runtime` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport unit` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport dv` | passed |
| `python3 ./harness/scripts/test-run.py sfo-reuseport integration` | passed |
| `python3 ./harness/scripts/test-run.py all all` | passed |
| `./test-run.sh sfo-reuseport unit` | blocked: `uv` is not installed in this environment |

## Harness Notes
- Required `uv run --active ...` commands could not be executed because `uv` is unavailable. The same scripts were run with `python3` and passed.
- `stage-scope-check.py` is blocked by the repository baseline: all repository files are currently untracked, so the checker reports unrelated stage violations. This is consistent with earlier pipeline records and did not expose a tokio-uring evidence mismatch.
- `git diff --stat` and `git diff --check` produced no output because the repository has no tracked baseline; `git status --short` shows the whole working tree as untracked.

## Residual Risk
- Automated tokio-uring validation is compile-level on this Linux target. Runtime io_uring behavior should be exercised in a privileged/compatible Linux CI environment if the project requires kernel-level coverage beyond type and integration compile evidence.

## Return Routing
- No return required.
