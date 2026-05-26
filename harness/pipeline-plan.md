# Pipeline Plan

## Trigger
- Approved proposal: `docs/versions/v0.1/modules/sfo-reuseport/proposal.md`
- User launch confirmed: 2026-05-25T16:46:51Z
- Per-stage user confirmation: skipped by explicit user auto-pipeline authorization
- Auto-confirm completed document stages: yes
- Version: v0.1
- Module(s): sfo-reuseport
- change_id values: CHG-server-runtime, CHG-tcp-serve, CHG-udp-runtime-socket, CHG-quic-routed-udp, CHG-tokio-uring-runtime

## Acceptance Baseline
- Final acceptance is judged against:
  - `proposal.md`

## Current Harness Rule Alignment
- Historical rows that list testing approval before implementation reflect the earlier pipeline order.
- New pipeline work uses the current Harness order: proposal approval -> design -> implementation admission -> implementation -> post-implementation testing -> acceptance.
- Implementation admission depends on approved proposal/design coverage and `schema-check` / `admission-check`; testing artifacts and `testplan.yaml` are generated or revalidated after implementation.

## Stage Graph
| Task ID | Stage | Status | Responsibility | Scope | Parent Task | Depends On | Output | Done Condition |
|---------|-------|--------|----------------|-------|-------------|------------|--------|----------------|
| D-2 | design | confirmed | convert approved dynamic listener intent into executable structure | dynamic listener and mixed protocol API | root | proposal approved | `design.md` | design complete; schema-check passed |
| T-2 | testing | confirmed | convert design into runnable verification coverage | dynamic listener and mixed protocol tests | root | proposal approved, design approved | `testing.md`, `testplan.yaml` | testing plan complete; schema-check passed |
| I-2 | implementation | complete | deliver production code inside approved boundaries | dynamic listener and mixed protocol implementation | root | proposal approved, design approved, schema-check passed, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| A-2 | acceptance | needs changes | audit the evidence chain and judge proposal satisfaction | module final | root | implementation evidence ready | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-dynamic-listeners-acceptance.md` | acceptance blocked by stage-scope baseline issue |
| P-3 | proposal | confirmed | add ServerRuntime as shared worker runtime requirement | ServerRuntime worker ownership | root | user auto-pipeline launch | `proposal.md` | proposal updated and auto-confirmed; schema-check passed |
| D-3 | design | confirmed | map ServerRuntime requirement into public API and implementation shape | ServerRuntime API and config ownership | root | P-3 | `design.md`, `docs/modules/sfo-reuseport.md` | design updated and auto-confirmed; schema-check passed |
| T-3 | testing | confirmed | add ServerRuntime validation coverage and testplan metadata | ServerRuntime API and shared worker tests | root | D-3 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed |
| I-3 | implementation | complete | migrate public API to ServerRuntime shared worker model | code, example | root | D-3, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| A-3 | acceptance | needs changes | audit ServerRuntime evidence chain and implementation | ServerRuntime final | root | I-3 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-server-runtime-acceptance.md` | acceptance blocked by stage-scope baseline issue |
| P-4 | proposal | confirmed | add one-thread-per-worker single-thread runtime requirement | worker runtime ownership | root | user auto-pipeline launch | `proposal.md` | proposal updated and auto-confirmed; schema-check passed |
| D-4 | design | confirmed | map worker thread runtime into runtime abstraction and listener loops | worker thread runtime | root | P-4 | `design.md` | design updated and auto-confirmed; schema-check passed |
| T-4 | testing | confirmed | add worker thread runtime validation coverage and testplan metadata | worker thread runtime tests | root | D-4 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed |
| I-4 | implementation | complete | run worker loops on per-worker OS threads with single-thread runtime | `src/runtime/`, TCP/UDP/dynamic loops | root | D-4, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| A-4 | acceptance | needs changes | audit worker thread runtime evidence chain and implementation | worker runtime final | root | I-4 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-worker-thread-runtime-acceptance.md` | acceptance blocked by stage-scope baseline issue |
| P-5 | proposal | confirmed | add ServiceConfig socket creation callback requirement | socket init callback API contract | root | user auto-pipeline launch | `proposal.md` | proposal updated and user-confirmed; schema-check passed |
| D-5 | design | confirmed | map socket init callback into public API, platform socket creation, and error boundaries | ServiceConfig callback type and TCP/UDP bind paths | root | P-5 | `design.md` | design updated and auto-confirmed; schema-check passed |
| T-5 | testing | confirmed | add callback validation coverage and testplan metadata | default None, TCP/UDP invocation, error propagation | root | D-5 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed |
| I-5 | implementation | complete | implement ServiceConfig socket creation callback | code for CHG-socket-init-callback | root | D-5, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| A-5 | acceptance | needs changes | audit socket init callback evidence chain and implementation | socket callback final | root | I-5 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-socket-init-callback-acceptance.md` | implementation accepted by evidence; acceptance blocked by stage-scope baseline issue |
| P-6 | proposal | confirmed | approve QuicServer as QUIC-aware UDP routing requirement | QuicServer UDP packet routing boundary | root | user auto-pipeline launch | `proposal.md` | proposal approved and mapped to `CHG-quic-routed-udp` |
| D-6 | design | confirmed | map QuicServer requirement into public API and implementation shape | QuicServer route key parsing and worker dispatch | root | P-6 | `design.md` | design updated and auto-confirmed |
| T-6 | testing | confirmed | add QuicServer validation coverage and testplan metadata | QUIC routed UDP tests | root | D-6 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed |
| I-6 | implementation | complete | implement QuicServer UDP packet routing | code for `CHG-quic-routed-udp` | root | D-6, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| A-6 | acceptance | needs changes | audit QuicServer evidence chain and implementation | QuicServer final | root | I-6 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-quic-routed-udp-acceptance.md` | implementation accepted by evidence; acceptance blocked by stage-scope baseline issue |
| D-7 | design | confirmed | tighten QuicServer CID layout to 16-bit worker shard | fixed CID layout contract | root | user auto-pipeline launch | `design.md` | design updated and auto-confirmed |
| T-7 | testing | confirmed | add 16-bit CID layout validation coverage | QuicServer route key tests | root | D-7 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed |
| I-7 | implementation | complete | update QuicServer routing to 16-bit worker shard | code for `CHG-quic-routed-udp` | root | D-7, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| D-8 | design | needs changes | bring Linux reuse-port BPF selector into QuicServer implementation design | Linux CBPF/eBPF selector, fallback, and worker loop boundary | root | proposal approved | `design.md` | design content and schema passed; stage-scope blocked by pre-existing untracked repository baseline |
| T-8 | testing | needs changes | add validation coverage for Linux reuse-port BPF selector and fallback | unit/DV coverage plus integration fallback evidence | root | D-8 | `testing.md`, `testplan.yaml` | testing content and schema passed; stage-scope blocked by pre-existing untracked repository baseline |
| I-8 | implementation | complete | implement best-effort Linux reuse-port BPF selector for QuicServer | platform socket setup, QuicServer worker sockets | root | D-8, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task |
| A-8 | acceptance | needs changes | audit QuicServer BPF selector evidence chain and implementation | eBPF/CBPF final | root | I-8 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-quic-reuseport-bpf-acceptance.md` | implementation evidence accepted; final acceptance blocked by stage-scope baseline issue |
| D-9 | design | confirmed | clarify Linux reuse-port eBPF as primary selector with CBPF and user-space fallback | Linux eBPF load/attach, verifier/permission fallback, worker loop boundary | root | user auto-pipeline launch | `design.md` | design updated and auto-confirmed; schema-check passed |
| T-9 | testing | confirmed | add validation coverage for eBPF primary selector and fallback | unit/DV coverage plus integration fallback evidence | root | D-9 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed |
| I-9 | implementation | complete | implement best-effort Linux eBPF reuse-port selector before CBPF fallback | platform eBPF syscall setup | root | D-9, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task; stage-scope blocked by pre-existing untracked repository baseline |
| A-9 | acceptance | needs changes | audit QuicServer eBPF selector evidence chain and implementation | eBPF final | root | I-9 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-quic-reuseport-ebpf-acceptance.md` | implementation evidence accepted; final acceptance blocked by stage-scope baseline issue |
| P-10 | proposal | confirmed | approve explicit ServerRuntime-only serve API requirement | TcpServer/UdpServer/QuicServer serve API surface | root | user auto-pipeline launch | `proposal.md` | proposal updated and auto-confirmed; schema-check passed |
| D-10 | design | confirmed | synchronize public API design to one `serve(&ServerRuntime, ServiceConfig, handler)` per server type | TcpServer/UdpServer/QuicServer API shape and lifecycle | root | P-10 | `design.md` | design updated and auto-confirmed; schema-check passed; stage-scope blocked by pre-existing untracked repository baseline |
| T-10 | testing | confirmed | add API contract validation for explicit runtime-only serve methods | API signature tests, integration entries, testplan metadata | root | D-10 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed; stage-scope blocked by pre-existing untracked repository baseline |
| I-10 | implementation | complete | remove legacy serve overloads and update callers | code, example for serve API convergence | root | D-10, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task; stage-scope blocked by pre-existing untracked repository baseline |
| A-10 | acceptance | needs changes | audit explicit runtime-only serve API evidence chain | serve API convergence final | root | I-10 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-serve-api-acceptance.md` | implementation evidence accepted; final acceptance blocked by stage-scope baseline issue |
| P-11 | proposal | confirmed | replace public BalancedUdpSocket UDP callback contract with runtime-native UdpSocket | UDP callback public API baseline | root | user auto-pipeline launch | `proposal.md` | proposal updated and auto-confirmed; schema-check passed |
| D-11 | design | confirmed | synchronize UDP and QUIC callback API design to runtime-native UdpSocket and remove BalancedUdpSocket public type | UDP callback design, re-export list, send/response boundary | root | P-11 | `design.md` | design updated and auto-confirmed; schema-check passed |
| T-11 | testing | confirmed | synchronize validation coverage and testplan metadata for UdpSocket callback API | API signature tests, UDP/QUIC integration expectations, no BalancedUdpSocket export | root | D-11 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed |
| I-11 | implementation | complete | remove BalancedUdpSocket API and migrate UDP/QUIC handlers to runtime-native UdpSocket | code for `CHG-udp-runtime-socket` | root | D-11, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task; scoped stage-scope check passed with untracked baseline ignored |
| A-11 | acceptance | complete | audit UdpSocket callback evidence chain and implementation | UDP callback contract final | root | I-11 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-udp-runtime-socket-acceptance.md` | accepted; no blocking issues found |
| P-12 | proposal | confirmed | replace public Dispatcher/DispatchPolicy requirement with Linux-compatible internal scheduling | scheduling public API baseline | root | user auto-pipeline launch | `proposal.md` | proposal updated; user confirmed downstream auto-pipeline |
| D-12 | design | confirmed | synchronize design to remove Dispatcher API and define Linux-compatible fallback scheduling | config/API, TCP/UDP worker selection, platform fallback | root | P-12 | `design.md` | design updated and auto-confirmed; schema-check passed; stage-scope blocked by pre-existing untracked repository baseline |
| T-12 | testing | confirmed | synchronize validation coverage and testplan metadata for Linux-compatible scheduling | API absence tests and fallback consistency tests | root | D-12 | `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; schema-check passed; stage-scope blocked by pre-existing untracked repository baseline |
| I-12 | implementation | complete | remove Dispatcher public API and migrate fallback paths to Linux-compatible internal scheduling | code for `CHG-linux-compatible-scheduling` | root | D-12, admission-check passed | production code | implementation complete; post-implementation tests recorded by testing task; stage-scope blocked by pre-existing untracked repository baseline |
| A-12 | acceptance | complete | audit Linux-compatible scheduling evidence chain and implementation | scheduling contract final | root | I-12 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-linux-compatible-scheduling-acceptance.md` | accepted; no blocking issues found; stage-scope baseline remains untracked |
| P-13 | proposal | confirmed | add tokio-uring runtime support requirement | runtime feature public API baseline | root | user auto-pipeline launch | `proposal.md` | proposal updated and auto-confirmed; schema-check passed; stage-scope blocked by pre-existing untracked repository baseline |
| D-13 | design | confirmed | define tokio-uring feature, runtime adapter, platform cfg, and public type mapping | runtime design and direct change mapping | root | P-13 | `design.md` | design updated and auto-confirmed; schema-check passed; stage-scope blocked by pre-existing untracked repository baseline |
| I-13 | implementation | complete | implement runtime-tokio-uring production support | Cargo features, runtime adapter, feature gates, examples as needed | root | D-13, admission-check passed | production code and build resources | implementation complete; default and tokio-uring cargo checks pass; stage-scope blocked by pre-existing untracked repository baseline |
| T-13 | testing | confirmed | add tokio-uring validation coverage and testplan metadata | feature matrix, API compile contract, Linux cfg behavior | root | I-13 | tests, `testing.md`, `testplan.yaml` | testing updated and auto-confirmed; unit/dv/integration passed through harness; stage-scope blocked by pre-existing untracked repository baseline |
| A-13 | acceptance | complete | audit tokio-uring evidence chain and implementation | tokio-uring runtime final | root | T-13 | `docs/versions/v0.1/reviews/sfo-reuseport-v0.1-tokio-uring-runtime-acceptance.md` | accepted; no blocking issues found; root shortcut blocked by missing uv; stage-scope blocked by pre-existing untracked repository baseline |

## Submodule Tasks
| Task ID | Stage | Status | Responsibility | Submodule | Parent Task | Depends On | Output | Done Condition |
|---------|-------|--------|----------------|-----------|-------------|------------|--------|----------------|
| | | pending / confirmed | | | | | | |

## Return Rules
- If acceptance finds proposal ambiguity:
  - return to proposal clarification task
- If acceptance finds design mismatch:
  - return to design task
- If acceptance finds testing gap:
  - return to testing task
- If acceptance finds implementation defect:
  - return to implementation task

## Exit Condition
- [x] All blocking issues closed
- [x] Required evidence exists
- [x] Document-producing stages auto-confirmed by setting front matter to `status: approved`, `approved_by: auto-pipeline`, and `approved_at`
- [x] Every implemented `change_id` has proposal/design traceability plus post-implementation testing evidence or explicit gap
- [ ] Every single-stage task passed stage-scope-check
- [x] Final acceptance passed against `proposal.md`
