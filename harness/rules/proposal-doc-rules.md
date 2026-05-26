# Proposal Document Rules

## Goal
- Define the minimum structure, ownership, and scope boundaries for `proposal.md`.
- Make requirement, scope, non-goal, and acceptance-boundary changes land in proposal first.

## Scope
- `docs/versions/<version>/modules/<module>/proposal.md`
- `docs/versions/<version>/modules/<module>/<submodule>/proposal.md` when a large module is split into direct submodule packets

## Required Metadata
- `module`
- `version`
- `status`
- `approved_by`
- `approved_at`

## Required Content
- goals
- scope
- non-goals
- assumptions and ambiguities
- constraints
- large-module submodule split decision when the target module already contains several logically independent submodules
- proposal items with stable `change_id` values for implementation-ready work
- success evidence
- downstream follow-up or return routes when design, testing, implementation, or acceptance may need updates

## Guardrails
- Proposal is the requirement baseline and answers why and what.
- Proposal tasks are single-stage by default and MUST NOT edit `design.md`, `design/`, `testing.md`, `testing/`, `testplan.yaml`, `acceptance.md`, code, or test code unless the user explicitly requested those additional stages.
- Requests that add, remove, narrow, widen, or reclassify goals, scope, non-goals, obligations, supported behavior, unsupported behavior, acceptance boundaries, or success evidence MUST default to proposal stage.
- Requests phrased as "does not need", "no longer needs", "should not provide", "must provide", "support", "do not support", or equivalent requirement language MUST default to proposal stage unless the user explicitly asks to synchronize downstream stages in the same task.
- When the target module is a large subproject package, crate, service, or similar module root that already contains several logically independent submodules, proposal MUST decide whether the requested feature is a new direct submodule or belongs inside an existing submodule.
- A new logically independent feature in such a large module MUST get its own direct submodule packet under the large module directory, such as `docs/versions/<version>/modules/<module>/<submodule>/proposal.md` and `design.md`; optional post-implementation testing artifacts may live there when generated.
- Do not store an independent submodule's proposal, design, or testing details under the large module's `design/<submodule>/` or `testing/<submodule>/` directories. Those docs belong in the submodule packet.
- Human-authored proposal docs SHOULD stay under 1000 lines each. Any proposal document that would exceed 1000 lines MUST be split by submodule, responsibility, or requirement boundary and the document index MUST be updated.
- A proposal task may record downstream follow-up in `proposal.md`, but it must not repair downstream documents in place by default.
- Do not convert a requirement/scope change into a cross-stage consistency task unless the user explicitly names the downstream stages or asks for cross-stage synchronization.
- If proposal changes make design, testing, implementation, or acceptance stale, record the return route or follow-up inside the proposal task.
- Do not use downstream documents, existing code behavior, oral context, or historical notes to silently override proposal intent.
