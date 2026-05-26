# Acceptance Review Gate

## Goal
- Define acceptance as an evidence-chain and consistency review.
- Acceptance confirms that the behavior described by the approved documents is implemented, that the documents agree with each other and with the implementation, and that no document or implementation logic defect invalidates the result.
- Git diff output is optional scoping evidence. It is not the acceptance standard, and diff-only cleanliness problems do not block acceptance unless they reveal a document inconsistency, document-to-implementation mismatch, missing approved behavior, or logical defect.

## Required Audits
Acceptance MUST perform:
- document coverage audit for every behavior, non-goal, constraint, and acceptance boundary described by the approved documents
- document consistency audit across proposal, design, optional testing artifacts, generated acceptance rules, expected results, and long-lived module docs
- implementation consistency audit between the approved proposal/design documents and the delivered implementation/test evidence
- logic audit for contradictions, impossible states, missing cases, invalid assumptions, or other reasoning defects in documents or implementation

## Optional Diff Commands
When useful for locating changed files or implementation evidence, run and record:
- `git status --short`
- `git diff --stat`
- `git diff --name-status`
- `git diff --check`

These commands are evidence discovery tools only. Their output is not a pass/fail criterion by itself.

For public symbol, API, codec, or wire-format migrations, also run a targeted search such as:
- `rg "old_symbol|old_encoding|old_method"`

## Required Harness Commands
For every module needed as evidence for accepted behavior, run and record:
- `uv run --active python ./harness/scripts/schema-check.py --version <version> --module <module>`
- `uv run --active python ./harness/scripts/admission-check.py --version <version> --module <module> --change-id <change_id>`
- the relevant generated test commands through `uv run --active python ./harness/scripts/test-run.py <module> <level-or-all>`

For every direct submodule packet needed as evidence for accepted behavior, run the same commands with `--submodule <submodule>`.

For whole-project evidence or final pipeline acceptance, run and record:
- `uv run --active python ./harness/scripts/test-run.py all all`
- the project-root shortcut with no arguments, for example `test-run.bat` or `./test-run.sh`, unless the current platform cannot execute that format

For single-stage tasks, run and record:
- `uv run --active python ./harness/scripts/stage-scope-check.py --stage <stage>`

If a test layer is manual, disabled, or deferred, the acceptance report MUST cite the generated test evidence and any optional testing document or `testplan.yaml` reason.

## Evidence Scope Audit
Acceptance SHOULD identify the documents, code paths, tests, and results used as evidence for each approved behavior.

Changed-file classification is optional and only supports evidence discovery. Do not reject acceptance merely because the working tree contains unrelated churn, formatting changes, generated files, or other diff noise. Reject only when the reviewed evidence shows:
- an approved document behavior is not implemented
- documents disagree with each other
- documents and implementation disagree
- a document contains a logical contradiction, unsupported assumption, or impossible requirement
- implementation contains a logical correctness, compatibility, lifecycle, state, or error-handling defect relevant to the accepted behavior

## Cross-Module Admission Audit
- Acceptance MUST NOT check only the current module or declared `change_id`.
- Every evidence-bearing module MUST have approved proposal/design coverage and post-implementation test evidence.
- Every evidence-bearing module MUST have a direct `change_id` mapping across proposal and design, plus generated test coverage or an explicit testing gap.
- Every automated test used as acceptance evidence MUST be reachable through the unified test entrypoint.
- The project-root test shortcut MUST delegate to the unified test entrypoint and must not maintain a separate test list.
- Evidence spanning multiple modules MUST pass schema and admission checks independently for each affected module packet.
- Evidence spanning multiple submodule packets MUST pass schema and admission checks independently for each affected submodule packet.
- If the implementation evidence for an accepted behavior depends on a module with draft, missing, ambiguous, or non-covering proposal/design documents or missing test evidence, acceptance MUST fail even when workspace tests pass.

## Implementation Logic Checklist
Acceptance MUST review the implementation evidence for correctness risks beyond test pass/fail:
- public API, enum, codec, or wire-format changes
- downstream semantic changes, compatibility shims, or migration behavior that is missing from design coverage or post-implementation test evidence
- language-level invariant violations, such as inconsistent `Eq` and `Hash` behavior in Rust
- concurrency, state-machine, resource-release, lifecycle, retry, or cancellation defects
- error-path gaps and fallback behavior
- risks that tests do not cover but that are logically inferable from the code

## Document Timing Consistency
- Downstream `design.md` approval metadata MUST NOT predate new proposal coverage it claims to implement.
- Optional testing artifacts created before the final implementation MUST be regenerated or explicitly revalidated against the delivered code.
- If approved documents receive new substantive content, acceptance MUST require re-approval or a clear recorded re-approval note.
- Acceptance MUST fail when approval state exists but the approved content does not directly cover the reviewed evidence.

## Acceptance Must Fail If
- any approved behavior, constraint, non-goal, or acceptance boundary described by the documents is not implemented or cannot be verified
- any required evidence module lacks approved proposal/design coverage or post-implementation test evidence
- a single-stage task fails `stage-scope-check.py --stage <stage>`
- public API, codec, wire format, or runtime semantics changed without direct design coverage and generated test coverage or an explicit gap
- stage documents contradict each other, or downstream documents silently narrow or expand approved proposal intent
- documents and implementation describe different behavior
- any document or implementation contains a plausible correctness, compatibility, governance, or logical defect
- the same non-requirement issue remains unresolved after more than 5 design -> implementation -> testing iterations

## Report Format
- Findings MUST appear first in the acceptance report and be sorted by severity.
- Test success is supporting evidence only; it does not automatically mean accepted.
- Any High finding MUST produce a `rejected` or `needs changes` conclusion.
- The report MUST include generated acceptance rules, expected results, document coverage, consistency findings, implementation evidence, harness command results, test evidence, optional diff summaries if used, iteration count, and unresolved risks.
