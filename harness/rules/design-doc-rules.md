# Design Document Rules

## Goal
- Define the minimum structure and approval requirements for `design.md` and `design/`.

## Scope
- `docs/versions/<version>/modules/<module>/design.md`
- `docs/versions/<version>/modules/<module>/design/`
- `docs/versions/<version>/modules/<module>/<submodule>/design.md` when a large module is split into direct submodule packets
- required long-lived boundary sync in `docs/modules/<module>.md`

## Required Metadata
- `module`
- `version`
- `status`
- `approved_by`
- `approved_at`

## Required Content
- submodule list and responsibilities
- dependencies between submodules and affected external modules
- key call flows
- implementation order
- exported interfaces
- acyclic module and submodule dependency graph
- business-logic submodule boundaries and shared implementation submodules when they affect external behavior or ownership
- document index
- direct change mapping for implementation-ready work
- stable `change_id` values matching proposal items
- simplicity check covering reused components and any new abstractions
- large-module submodule documentation decision when adding new features
- major risks and rollback notes

## Guardrails
- Design must implement approved proposal intent without silently changing scope.
- Design tasks are single-stage by default and MUST NOT edit `proposal.md`, `testing.md`, `testing/`, `testplan.yaml`, `acceptance.md`, code, or test code unless the user explicitly requested those additional stages.
- Design tasks must not modify testing strategy, acceptance criteria, or implementation code.
- Design should stay at module shape level: submodules, dependencies, key call flows, exported interfaces, and external module dependencies. Avoid low-level implementation detail unless it affects a public contract, cross-module dependency, or important control flow.
- Design should list direct submodules or explicitly say that none exist.
- Design MUST keep module and submodule dependencies acyclic. Circular dependencies between modules, between submodules, or between a module and one of its submodules are design failures and MUST be resolved before implementation.
- Design MUST split modules and submodules by business logic first. Different business responsibilities belong in different business submodules.
- If multiple business submodules share implementation logic, that common logic MUST be modeled as its own shared submodule instead of being duplicated or hidden inside one business submodule.
- Technically distinct implementation areas inside a business module, such as HTTP interfaces, persistence/database access, external adapters, codecs, schedulers, or storage, SHOULD be modeled as dedicated submodules when they have clear responsibility boundaries.
- A small implementation submodule MAY be represented by a single file. A larger implementation submodule that contains internal sub-responsibilities SHOULD describe only its visible responsibilities and external dependencies here; keep detailed internal layout out of `design.md` unless required for the planned change.
- For existing code, describe current structure before describing the change.
- When the target module is a large subproject package, crate, service, or similar module root that already contains several logically independent submodules, a new logically independent feature MUST be modeled as its own direct submodule unless the design explains why it belongs inside an existing submodule.
- For large-module changes, keep the large module's `design.md` as the module overview and document index. Put detailed submodule design in the submodule packet under the large module directory, such as `docs/versions/<version>/modules/<module>/<submodule>/design.md`, not under `design/<submodule>/`.
- Human-authored design docs SHOULD stay under 1000 lines each. Any design document that would exceed 1000 lines MUST be split by submodule, responsibility, or interface boundary and the document index MUST be updated.
- Do not introduce idealized architecture unless the proposal approved that shift.
- Prefer the simplest sufficient approach that satisfies the approved proposal and constraints.
- Do not add speculative features, extension points, configuration, or abstractions.
- New abstractions should either match an established local pattern or remove real duplicated complexity.
- Design must identify every affected module for cross-module work.
- Design must not use broad change buckets as implementation admission evidence.
- If a design task discovers proposal ambiguity, return work to proposal instead of repairing it in place.
- If a design change implies testing or acceptance updates, record the downstream follow-up unless the user explicitly requested cross-stage synchronization.
