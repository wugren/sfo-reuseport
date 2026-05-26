# Unified Test Entry Rules

## Goal
- Define the canonical runnable test interface for all project validation.
- Ensure every generated or hand-written test can be run through one stable command surface.

## Scope
- project-root test shortcuts: `test-run.bat` on Windows and `test-run.sh` on Unix-like systems
- `harness/scripts/test-run.py`
- generated test implementation
- optional `testing.md`
- optional `testplan.yaml`

## Canonical Commands
- Project-root shortcut:
  - Windows: `test-run.bat [<module> <level>]`
  - Unix: `./test-run.sh [<module> <level>]`
- `uv run --active python ./harness/scripts/test-run.py <module> unit`
- `uv run --active python ./harness/scripts/test-run.py <module> dv`
- `uv run --active python ./harness/scripts/test-run.py <module> integration`
- `uv run --active python ./harness/scripts/test-run.py <module> all`
- `uv run --active python ./harness/scripts/test-run.py all all`

## Consistency Rule
- `harness/scripts/test-run.py` is mandatory in generated repositories.
- A generated repository MUST include both project-root one-click test shortcuts: `test-run.bat` for Windows and `test-run.sh` for Unix-like systems.
- The root shortcut MUST check whether `uv` is installed and print an installation hint when it is missing.
- The root shortcuts MUST create a local `.venv` when it is missing, use `uv` to sync or install dependencies when project metadata exists, activate the project virtual environment, and then invoke `harness/scripts/test-run.py` through `uv run --active python`.
- The root shortcut MUST NOT bypass the unified test entrypoint.
- The unified test interface MUST be able to run every project test that is part of the harness evidence chain.
- A testing task is not complete until every new or changed test implementation is registered with, or otherwise reachable through, the unified test interface.
- Generated tests, optional `testing.md`, and optional `testplan.yaml` must reference the same validation surfaces when those artifacts exist.
- Test scripts should be non-interactive and return meaningful exit codes.
- New test execution paths should be added to the canonical entrypoint instead of creating unrelated ad hoc commands.
- Test implementation may use local framework-specific commands internally, but acceptance and pipeline tasks must call them through `harness/scripts/test-run.py`.

## Execution Contract
- Unknown modules or test levels should exit non-zero.
- The `all all` command should run all registered project tests in deterministic order.
- `<module> all` should run every registered test level for that module.
- Enabled steps should execute in declared order.
- "success without executing steps" should be reserved for `manual` or `disabled` layers.
- Each enabled step should declare stable machine-readable fields such as `id`, `name`, and `run`.
- Each enabled step should declare the `change_ids` it validates when it is used as acceptance evidence.
- `harness/scripts/schema-check.py` should reject unknown levels, duplicate step ids, enabled levels without steps, and manual or disabled levels without reasons when `testplan.yaml` exists.
