---
module: example-module
submodule:
version: v0.1
status: draft
approved_by:
approved_at:
---

# [Module Name] Design

> This file explains implementation design only. Keep full test strategy in `testing.md`.

## Design Scope
### Goals
### Non-goals

## Overall Approach
<!-- Main implementation path, layers, data flow, key interactions -->

## Simplicity Check
- Smallest sufficient approach:
- Existing components or patterns reused:
- New abstractions introduced:
- Why each new abstraction is necessary:

## Current Structure
<!-- For existing code, describe the current structure and constraints before describing the planned change -->

## Module Breakdown
| Submodule | Type | Responsibility | Input | Output | Dependencies | Separate Doc |
|-----------|------|----------------|-------|--------|--------------|--------------|
| example | feature / platform / shared / assembly | | | | | no |

## Large Module Submodule Decision
<!-- If this module is a large package/crate/service with several independent submodules, confirm whether the approved proposal creates a new direct submodule. Independent new features should use a submodule packet under this module directory, for example `<submodule>/proposal.md`, `<submodule>/design.md`, `<submodule>/testing.md`, and `<submodule>/testplan.yaml`. Do not put independent submodule design under `design/<submodule>/`. Keep each human-authored doc under 1000 lines; if a doc would exceed 1000 lines, split it by submodule, responsibility, or interface boundary. -->

## Directly Mapped Change Items
| change_id | proposal_id | Design Coverage | Scope Paths | Interface / Boundary Impact | Notes |
|-----------|-------------|-----------------|-------------|-----------------------------|-------|
| CHG-example | P-001 | this file section or `design/...` doc | `path/or/component` | none / describe impact | |

## Implementation Order
| Phase | Goal | Preconditions | Outputs | Depends On | Parallel |
|-------|------|---------------|---------|------------|----------|
| 1 | | | | | no |

## Key Decisions
<!-- Why this design was chosen and what alternatives were rejected -->

## Data and State
<!-- Data model, state transitions, consistency constraints -->

## Interfaces and Dependencies
### Public interface summary
### Public HTTP interface details
### Public code interface details
### Dependency interfaces and external constraints

## Implementation Layout
```text
[module-root]
├── [dir-or-file]
└── ...
```

| Path | Type | Responsibility | Notes |
|------|------|----------------|-------|
| | | | |

## Document Index
| Document | Topic | Scope |
|----------|-------|-------|
| `design.md` | module overview | full module |

## Risks and Rollback
<!-- Implementation, migration, compatibility, rollback -->

## Stage and Admission Notes
- Design may begin only from an approved proposal that directly covers the same `change_id` values.
- Implementation may begin only after this design is approved and `schema-check.py` plus `admission-check.py --change-id <change_id>` pass for every affected module or direct submodule packet.
- Testing remains a post-implementation stage and should be recorded as follow-up unless the current task explicitly includes testing synchronization.
- If the design affects multiple modules or direct submodule packets, list each packet and require independent admission for each one.

## Design Guardrails
- Do not rewrite approved proposal intent in `design.md`.
- If this module has no direct submodules, say so explicitly.
- If this is a large module with many independent submodules, model each new independent feature as its own direct submodule unless this design explains why it belongs inside an existing submodule.
- Keep detailed independent submodule documentation in a submodule packet under this module directory, such as `<submodule>/design.md`; do not accumulate all module documentation in one file or place independent submodule docs under `design/<submodule>/`.
- Keep human-authored docs under 1000 lines where practical; if a doc would exceed 1000 lines, split it and update the document index.
- For existing code, describe current structure first, then the change.
- Do not introduce idealized architecture that the proposal did not approve.
- Prefer the simplest design that satisfies the approved proposal and documented constraints.
- Do not add speculative extension points, configuration, or abstractions for single-use code.
- Every implementation-ready design item must carry the same `change_id` used in `proposal.md`.
- For multi-module or cross-boundary work, list each affected module and explain whether it needs separate implementation admission.
- Single-stage design tasks must finish by running `uv run --active python ./harness/scripts/stage-scope-check.py --stage design`.
