# sfo-reuseport v0.1 ServerRuntime Acceptance Report

## Findings
| ID | Severity | Stage | Evidence | Problem | Fail Condition Hit |
|----|----------|-------|----------|---------|--------------------|
| SR-A1 | High | process / repository baseline | `python3 ./harness/scripts/stage-scope-check.py --stage implementation --version v0.1 --module sfo-reuseport` | stage-scope-check fails because the repository baseline is fully untracked, so unrelated harness/docs/source files are reported as scope violations. This blocks formal pipeline acceptance even though the reviewed ServerRuntime evidence is internally consistent. | single-stage task fails `stage-scope-check.py --stage <stage>` |

## Object and Scope
- Module: `sfo-reuseport`
- Version: `v0.1`
- change_id values reviewed: `CHG-server-runtime`, `CHG-worker-model`, `CHG-dynamic-listeners`, `CHG-mixed-protocol-workers`
- Review date: 2026-05-22T03:26:42Z
- In scope: `ServerRuntime` naming, runtime-owned worker configuration, server/listener config removal of worker ownership, dynamic TCP/UDP listener behavior, tests and example update.
- Out of scope: OS matrix manual validation and privileged IPv4 transparent manual validation.

## Optional Diff / Status Evidence
- `git status --short` summary: repository is currently untracked at top-level (`?? docs/`, `?? src/`, `?? tests/`, etc.), so status cannot isolate this task's delta.
- `git diff --stat` summary: empty because files are untracked.
- `git diff --name-status` summary: not useful for the same reason.
- `git diff --check` result: not used.
- Targeted migration search: `rg "DynamicServer|DynamicServerConfig|ServiceConfig::new\\([^\\n]+\\)\\.with_workers|with_dispatch\\(" . -S` found no live old `DynamicServer` API use outside a historical review report; `with_dispatch` remains only on `ServerRuntimeConfig`.
- Note: diff/status output is a discovery aid only, not the acceptance standard.

## Evidence Coverage
| Documented Item | Source Document | Implementation Evidence | Test / Result Evidence | Status |
|-----------------|-----------------|-------------------------|------------------------|--------|
| `ServerRuntime` is the shared worker runtime abstraction. | `proposal.md`, `design.md` | `src/core/dynamic.rs`, `src/lib.rs` export `ServerRuntime` / `ServerRuntimeConfig`. | unit tests and integration tests passed. | implemented |
| Worker count belongs to runtime config, not server/listener config. | `proposal.md`, `design.md`, `testing.md` | `ServiceConfig` has `bind_addr` and `socket_options`; `ServerRuntimeConfig` has `workers` and `dispatch`. | `tests/unit/server_runtime.rs`, `tests/unit/worker_model.rs` passed. | implemented |
| Single protocol entries can still run while explicit worker config uses runtime config. | `design.md`, `testing.md` | `TcpServer::serve_with_runtime`, `UdpServer::serve_with_runtime`; default `serve` uses default runtime config. | `tests/integration/tcp_serve.rs`, `tests/integration/udp_serve.rs` passed. | implemented |
| TCP and UDP listeners share one runtime instance. | `proposal.md`, `design.md`, `testing.md` | `ServerRuntime::start`, `add_tcp_listener`, `add_udp_listener` share `ServerRuntimeInner.workers`. | `one_server_runtime_handles_tcp_and_udp_listeners` passed. | implemented |
| Example uses runtime-owned worker config. | `design.md` | `examples/tcp_echo.rs` uses `ServerRuntimeConfig::new().with_workers(4)`. | `cargo test --all-targets` through integration entry passed. | implemented |

## Consistency Summary
- Proposal authority check: `proposal.md` now directly includes `CHG-server-runtime` and requires runtime-owned worker configuration.
- Proposal vs design: consistent; design maps `CHG-server-runtime` to `ServerRuntimeConfig`, `ServerRuntime`, and server/listener config boundaries.
- Design vs testing: consistent; testing has `VAL-server-runtime` and `testplan.yaml` step `server-runtime-api`.
- Design vs long-lived boundary doc: consistent; `docs/modules/sfo-reuseport.md` now describes the library and `ServerRuntime` export.
- Design/testing vs implementation: consistent for reviewed behavior.
- Testing docs vs testplan vs test code vs results: consistent for automatic unit/DV/integration paths.
- change_id traceability: schema and admission checks passed for all reviewed change IDs.
- Public API / runtime semantics review: no remaining live `DynamicServer`/`DynamicServerConfig` references; worker configuration is no longer exposed on `ServiceConfig` or `ListenerConfig`.
- Document approval timing: proposal/design/testing front matter auto-confirmed at 2026-05-22T03:26:42Z.

## Required Command Evidence
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-server-runtime --change-id CHG-worker-model --change-id CHG-dynamic-listeners --change-id CHG-mixed-protocol-workers`: passed.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation --version v0.1 --module sfo-reuseport`: failed because the repository baseline is untracked and reports unrelated docs/harness files.
- Unit test command: `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- DV test command: `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- Integration test command: `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.

## Conclusion
- Accepted / rejected / needs changes: needs changes.
- Reason: implementation and tests satisfy the reviewed `ServerRuntime` behavior, but formal auto-pipeline acceptance is blocked by the repository baseline issue that makes stage-scope-check fail.
- Supporting test evidence: unit, DV, and integration canonical entries passed.
- Residual risk: API is a breaking rename/removal from `DynamicServer` and `ServiceConfig::with_workers`; no compatibility aliases were kept because the approved proposal/design make `ServerRuntime` the public abstraction.

## Follow-Up Tasks
- Requirement task: none for `ServerRuntime`.
- Design task: none for `ServerRuntime`.
- Testing task: none for automatic coverage; OS matrix and privileged transparent paths remain manual from existing plan.
- Implementation task: none for reviewed behavior.
- Process task: establish a tracked repository baseline or adjust stage-scope tooling to ignore pre-existing untracked bootstrap state, then rerun stage-scope-check and final acceptance.
