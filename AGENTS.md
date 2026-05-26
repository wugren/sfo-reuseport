# sfo-reuseport Agent Instructions

## Overview
- `sfo-reuseport` is a small Rust crate managed with a Harness Engineering workflow.
- Repository process, approval state, validation metadata, and acceptance reports live in versioned files.

## Loading Order
1. Read `AGENTS.md`.
2. Read `harness/rules/task-entry-gate-rules.md`.
3. For module work, read the active packet under `docs/versions/<version>/modules/<module>/`.
4. Read `docs/modules/<module>.md` for long-lived module boundaries.
5. Read project-wide constraints under `docs/architecture/`.
6. Read the task-specific rule file under `harness/rules/`.

## Hard Task Entry Gate
- Implementation-like work must classify the stage before editing files.
- If code, tests, build behavior, runtime behavior, refactoring, optimization, or bugfix work is requested, locate the active `version`, `module`, and `change_id` first.
- Implementation may begin only after `proposal.md` and `design.md` are present, approved, read, and directly cover the current `change_id`.
- Run `uv run --active python ./harness/scripts/schema-check.py --version <version> --module <module>` before implementation admission.
- Run `uv run --active python ./harness/scripts/admission-check.py --version <version> --module <module> --change-id <change_id>` before implementation begins.
- Output `实现准入通过` (`Implementation admission passed`) only after admission passes.
- Single-stage document tasks must run `uv run --active python ./harness/scripts/stage-scope-check.py --stage <stage>` before completion.

## Stage Responsibilities
- Proposal: define goals, scope, non-goals, constraints, assumptions, success criteria, and stable `change_id` values in `proposal.md`.
- Design: turn the approved proposal into the simplest sufficient implementation shape in `design.md` and any needed `design/` files.
- Testing: after implementation, define validation strategy, add or update test code, wire tests through the unified entrypoint, and maintain `testing.md`, `testing/`, and `testplan.yaml` when persistent metadata is needed.
- Implementation: change only production code and required non-test runtime/build resources needed by the admitted `change_id`.
- Acceptance: audit documents, implementation, tests, and results for consistency; write standalone reports under `docs/versions/<version>/reviews/` or `docs/reviews/`.

## Stage Boundaries
- One stage task owns one artifact group by default.
- Cross-stage edits require an explicit user request naming the extra stages or requesting synchronization.
- Upstream document edits do not automatically authorize downstream edits; record follow-up instead.
- Acceptance uses git diff/status only to discover evidence. It is not the pass/fail standard.
- If documents or code disagree, the approved `proposal.md` is authoritative.

## Auto Pipeline
- `harness/rules/auto-pipeline-rules.md` exists by default but is inactive.
- Enter auto-pipeline mode only when the user explicitly asks to enable, launch, run, or enter it.
- Proposal approval is required for downstream pipeline execution but is not itself an auto-pipeline trigger.

## Rust Defaults
- Do not run `cargo fmt` automatically. Run it only when the user explicitly asks or repo-local rules require it.
- Use `uv run --active python ./harness/scripts/test-run.py sfo-reuseport <level>` as the canonical test entry for `unit`, `dv`, `integration`, and `all`.
- Root shortcuts are `test-run.sh` and `test-run.bat`; they prepare `.venv` with `uv`, activate it, and delegate to the canonical test entrypoint.
