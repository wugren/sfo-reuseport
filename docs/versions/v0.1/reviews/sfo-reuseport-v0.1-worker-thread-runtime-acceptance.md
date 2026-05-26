# sfo-reuseport v0.1 Worker Thread Runtime Acceptance Report

## Findings
| ID | Severity | Stage | Evidence | Problem | Fail Condition Hit |
|----|----------|-------|----------|---------|--------------------|
| WTR-A1 | High | process / repository baseline | `python3 ./harness/scripts/stage-scope-check.py --stage implementation --version v0.1 --module sfo-reuseport` | stage-scope-check fails because the repository baseline is fully untracked, so unrelated harness/docs files are reported as scope violations. This blocks formal pipeline acceptance even though the reviewed worker-thread runtime evidence is internally consistent. | single-stage task fails `stage-scope-check.py --stage <stage>` |

## Object and Scope
- Module: `sfo-reuseport`
- Version: `v0.1`
- change_id values reviewed: `CHG-worker-thread-runtime`, `CHG-server-runtime`, `CHG-worker-model`
- Review date: 2026-05-22T04:13:32Z
- In scope: one OS thread per worker, single-thread async runtime per worker, TCP/UDP/dynamic listener loop startup paths, validation tests.
- Out of scope: graceful shutdown/join handle public API and OS matrix manual validation.

## Evidence Coverage
| Documented Item | Source Document | Implementation Evidence | Test / Result Evidence | Status |
|-----------------|-----------------|-------------------------|------------------------|--------|
| Each worker starts through a worker-thread runtime API. | `proposal.md`, `design.md` | `runtime::spawn_worker` in `src/runtime/tokio.rs` and `src/runtime/async_std.rs`. | `tests/unit/worker_runtime.rs` passed. | implemented |
| Tokio worker runtime is single-threaded. | `design.md` | `tokio::runtime::Builder::new_current_thread().enable_all()` in `src/runtime/tokio.rs`. | unit/DV/integration passed. | implemented |
| TCP worker loops do not directly spawn onto caller runtime. | `design.md`, `testing.md` | `src/core/tcp.rs` uses `runtime::spawn_worker` for each worker listener. | TCP integration tests passed. | implemented |
| UDP worker loops do not directly spawn onto caller runtime. | `design.md`, `testing.md` | `src/core/udp.rs` uses `runtime::spawn_worker` for each worker socket. | UDP integration tests passed. | implemented |
| Dynamic TCP/UDP listeners use the same worker-thread runtime boundary. | `design.md`, `testing.md` | `src/core/dynamic.rs` uses `runtime::spawn_worker` for TCP and UDP listener loops. | dynamic listener and mixed protocol integration tests passed. | implemented |

## Consistency Summary
- Proposal authority check: `proposal.md` directly includes `CHG-worker-thread-runtime`.
- Proposal vs design: consistent; design maps worker runtime requirement to `runtime::spawn_worker` and listener loop startup.
- Design vs testing: consistent; testing has `VAL-worker-thread-runtime` and `testplan.yaml` step `worker-thread-runtime`.
- Design/testing vs implementation: consistent for reviewed behavior.
- Testing docs vs testplan vs test code vs results: consistent for automatic unit/DV/integration paths.
- change_id traceability: schema and admission checks passed for reviewed change IDs.
- Targeted migration search: `rg "runtime::spawn\\(|spawn_worker|CHG-worker-thread-runtime|DynamicServer|DynamicServerConfig" ...` found worker loops using `spawn_worker` and no live `runtime::spawn` / old `DynamicServer` use in `src` or `tests`.

## Required Command Evidence
- `python3 ./harness/scripts/schema-check.py --version v0.1 --module sfo-reuseport`: passed.
- `python3 ./harness/scripts/admission-check.py --version v0.1 --module sfo-reuseport --change-id CHG-worker-thread-runtime --change-id CHG-server-runtime --change-id CHG-worker-model`: passed.
- `python3 ./harness/scripts/stage-scope-check.py --stage implementation --version v0.1 --module sfo-reuseport`: failed because the repository baseline is untracked and reports unrelated docs/harness files.
- Unit test command: `python3 ./harness/scripts/test-run.py sfo-reuseport unit`: passed.
- DV test command: `python3 ./harness/scripts/test-run.py sfo-reuseport dv`: passed.
- Integration test command: `python3 ./harness/scripts/test-run.py sfo-reuseport integration`: passed.

## Conclusion
- Accepted / rejected / needs changes: needs changes.
- Reason: implementation and tests satisfy the reviewed worker-thread runtime behavior, but formal auto-pipeline acceptance remains blocked by the repository baseline issue that makes stage-scope-check fail.
- Supporting test evidence: unit, DV, and integration canonical entries passed.
- Residual risk: `serve` APIs now spawn detached worker threads and await indefinitely; graceful stop/join is not yet a public v0.1 contract.

## Follow-Up Tasks
- Requirement task: none for worker-thread runtime.
- Design task: future lifecycle/shutdown API if public stop/join behavior becomes required.
- Testing task: none for current automatic coverage.
- Implementation task: none for reviewed behavior.
- Process task: establish a tracked repository baseline or adjust stage-scope tooling to ignore pre-existing untracked bootstrap state, then rerun stage-scope-check and final acceptance.
