# Schema Validation Rules

## Goal
- Define the machine-checkable structure for module packets, rule files, and validation metadata.
- Make implementation admission fail closed when required fields, approval state, or change-level traceability are missing.

## Scope
- `docs/versions/<version>/modules/<module>/proposal.md`
- `docs/versions/<version>/modules/<module>/design.md`
- optional `docs/versions/<version>/modules/<module>/testing.md`
- optional `docs/versions/<version>/modules/<module>/testplan.yaml`
- `docs/versions/<version>/modules/<module>/<submodule>/proposal.md` and sibling stage files when a large module uses direct submodule packets
- `harness/scripts/schema-check.py`
- `harness/scripts/admission-check.py`
- `harness/scripts/stage-scope-check.py`

## Required Front Matter
`proposal.md` and `design.md` MUST contain YAML-style front matter. Optional `testing.md` SHOULD use the same metadata when generated:

```yaml
module: <module>
version: <version>
status: draft | approved | rejected | superseded
approved_by: <person-or-process>
approved_at: <iso-8601-date-or-datetime>
```

Direct submodule packet docs MAY also include:

```yaml
submodule: <submodule>
```

Implementation admission accepts only `status: approved`.

## Change Traceability Schema
- Every implementation-ready change MUST have one stable `change_id`.
- `change_id` values MUST be specific enough to name one behavior, contract, or implementation unit. Do not use broad IDs such as `misc`, `cleanup`, `all`, `module`, or `bugfix`.
- The same `change_id` MUST appear in these exact locations:
  - `proposal.md` section `## Proposal Items`, column `change_id`; the same row MUST include non-empty `proposal_id`, `Outcome`, and `Success Evidence`.
  - `design.md` section `## Directly Mapped Change Items`, column `change_id`; the same row MUST include non-empty `proposal_id`, `Design Coverage`, and `Scope Paths`.
- Post-implementation testing evidence SHOULD also reference the same `change_id`; optional `testing.md` / `testplan.yaml` SHOULD include the `change_id` when generated, but those files are not implementation-admission prerequisites.
- `change_id` text in comments, prose, unrelated tables, historical notes, or module overviews MUST NOT satisfy admission.
- A broad module overview, historical note, or oral explanation MUST NOT satisfy this schema.

## Active Module Resolution
- A task MUST name the active `version`, `module`, and one or more `change_id` values before implementation admission can pass.
- If the active packet is a direct submodule under a large module, the task MUST also name the active `submodule`.
- If the request affects multiple modules, admission MUST be evaluated independently for each affected module packet.
- If the request affects multiple direct submodules, admission MUST be evaluated independently for each affected submodule packet with `--submodule <submodule>`.
- If the active module cannot be determined from repository paths, module docs, or the user's explicit request, route to proposal or design instead of selecting a convenient module.

## Optional Testplan Schema
When generated, `testplan.yaml` MUST include:

```yaml
schema_version: 1
version: <version>
module: <module>
submodule: <submodule> # optional; required only when the repository chooses explicit submodule metadata
levels:
  unit|dv|integration:
    mode: enabled | manual | disabled
    summary: <text>
    test_targets: []
    preconditions:
      tools: []
      env: []
      services: []
      notes: []
    steps:
      - id: <stable-id>
        name: <text>
        change_ids: [<change-id>]
        run: [<command>, <arg>]
```

Rules:
- `enabled` levels MUST have at least one step.
- Each enabled step MUST define `id`, `name`, `change_ids`, and `run`.
- Step ids MUST be unique within the module packet.
- `manual` and `disabled` levels MUST include `change_ids` and a reason in generated test evidence and optional `testing.md` / `testplan.yaml` when present.
- Unknown test levels MUST fail validation.

## Checker Contract
- `harness/scripts/schema-check.py` validates mandatory proposal/design packet structure and optional testplan shape for module packets and, with `--submodule <submodule>`, direct submodule packets.
- `harness/scripts/admission-check.py` validates implementation admission for explicit `version`, `module`, optional `submodule`, and `change_id` values, including the mandatory proposal/design traceability positions above.
- `harness/scripts/stage-scope-check.py` validates that the current diff stays inside one declared stage artifact group.
- These scripts MUST exit non-zero on missing mandatory files, missing approval metadata, missing direct proposal/design traceability, ambiguous active module, malformed optional test metadata, or out-of-stage diffs.
- Stage scope checks MUST exit non-zero when a proposal task changes anything except `proposal.md`, when a design task changes non-design artifacts, when a testing task changes non-testing artifacts, when an acceptance task changes anything except review reports, or when an implementation task changes stage documents or harness governance files.
- Passing checker output is necessary but not sufficient for implementation: agents must still read the approved docs and keep edits inside the admitted scope.
