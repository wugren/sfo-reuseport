# Trigger Rules

## Goal
- Define which classes of changes require extra checks.
- Make escalation logic public, stable, and reviewable.
- Fail closed when the trigger decision is ambiguous.

## Scope
These rules apply to proposal, design, testing, implementation, and acceptance tasks that may affect:
- public or internal contracts
- persisted data, schemas, migrations, or compatibility
- security, privacy, authentication, authorization, or permissions
- runtime behavior, integration behavior, background jobs, or distributed flows
- build, dependency, configuration, packaging, deployment, or environment behavior
- UI data models, presentation contracts, accessibility, or user-visible workflows
- test infrastructure, harness rules, admission checks, or release gates

## Trigger Decision Rule
- Evaluate triggers before implementation admission and again before acceptance.
- If a change matches any trigger below, the owning proposal/design/testing documents MUST record the trigger and required extra checks.
- If it is unclear whether a trigger applies, treat it as triggered until the owning document stage records a concrete reason why it does not apply.
- A trigger may be marked not applicable only with evidence from versioned docs or inspected code, not from chat-only assumptions.
- Triggered checks may be deferred only when the testing document records the reason, owner, risk, and acceptance impact.

## Trigger Types

### Contract or Protocol Changes
- Trigger when a change adds, removes, renames, retypes, reorders, or changes semantics for an API, CLI, RPC, event, message, file format, extension point, public function, module boundary, or cross-module interface.
- Required document coverage: design MUST list the affected contract and compatibility impact; testing MUST define positive and negative contract checks.
- Additional checks: compatibility test, caller/callee impact review, versioning or migration note, and at least one boundary-focused validation path.
- Reviewer focus: backward compatibility, undocumented behavior relied on by callers, error semantics, idempotency, and cross-module admission coverage.

### Durable Data, Schema, or Migration Changes
- Trigger when a change affects persisted data, database schemas, serialized state, cache keys, indexes, migrations, default values, reset behavior, import/export shape, or data retention.
- Required document coverage: proposal or design MUST state migration and rollback expectations; testing MUST cover old data, new data, and mixed-version or reset paths where relevant.
- Additional checks: migration dry-run or documented manual verification, rollback assessment, data compatibility review, and backup/recovery impact note.
- Reviewer focus: irreversible writes, partial migration failure, stale readers, downgrade behavior, and data loss risk.

### Security, Privacy, or Permission Changes
- Trigger when a change affects authentication, authorization, identity, secrets, tokens, encryption, transport security, audit logs, input trust boundaries, PII, tenant isolation, sandboxing, or privilege checks.
- Required document coverage: proposal or design MUST name the trust boundary and denied cases; testing MUST include at least one negative or abuse-case validation path.
- Additional checks: permission matrix review, secret handling review, input validation review, audit/logging review, and regression checks for denied access.
- Reviewer focus: fail-open paths, confused deputy behavior, information leaks, unsafe defaults, and logs that expose sensitive data.

### Runtime or Integration Changes
- Trigger when a change affects startup/shutdown, scheduling, retries, timeout behavior, concurrency, ordering, network calls, background work, external services, resource limits, or observability.
- Required document coverage: design MUST describe lifecycle and failure behavior; testing MUST identify unit, DV, or integration coverage for the changed runtime path.
- Additional checks: failure-mode test, timeout/retry review, dependency availability review, log/metric review, and integration or DV run unless explicitly documented as manual/disabled.
- Reviewer focus: race conditions, stuck work, duplicate side effects, resource leaks, and unclear operational recovery.

### Build, Dependency, Config, or Deployment Changes
- Trigger when a change affects build scripts, package metadata, lockfiles, dependency versions, feature flags, config keys/defaults, environment variables, release packaging, deployment scripts, or generated resources.
- Required document coverage: design MUST name changed build/config surfaces; testing MUST include reproducibility or configuration validation.
- Additional checks: clean build or equivalent documented validation, config compatibility review, dependency risk review, and deployment rollback note.
- Reviewer focus: environment-specific behavior, hidden dependency upgrades, generated-file drift, and defaults that change production behavior.

### UI DataModel, Presentation Contract, or Workflow Changes
- Trigger when a change affects UI-visible state, navigation, form validation, accessibility semantics, localization keys, user-facing copy with behavioral meaning, or frontend/backend data contracts.
- Required document coverage: design MUST identify affected UI states and data contracts; testing MUST cover the changed workflow or record a manual validation path.
- Additional checks: state coverage review, accessibility or keyboard-path review where relevant, contract validation, and screenshot/manual evidence for visual workflow changes when automation is not available.
- Reviewer focus: broken empty/error/loading states, data mismatch, inaccessible controls, layout overlap, and workflow regressions.

### Test Harness, Admission, or Process Rule Changes
- Trigger when a change affects `harness/rules/`, `harness/process_rules/`, `harness/scripts/`, module templates, `AGENTS.md`, `testplan.yaml` schema, CI entrypoints, or acceptance report formats.
- Required document coverage: design MUST state the process behavior being changed; testing MUST include at least one generated-scaffold or checker validation path.
- Additional checks: run the affected checker or document why it cannot run, inspect generated path references, and verify that new rules fail closed.
- Reviewer focus: contradictions between templates and rules, missing generated files, bypassable wording, and checks that pass without validating the intended condition.

## Stage Gates
- Before proposal approval: list triggered categories, affected surfaces, explicit non-goals, and unresolved trigger questions.
- Before design approval: map each triggered category to affected files, interfaces, compatibility expectations, and rollback or mitigation notes.
- Before testing approval: define required extra checks, mark each as automated, manual, or disabled, and record reasons for every manual or disabled path.
- Before implementation admission: every triggered category MUST have direct proposal and design coverage for the admitted `change_id`.
- Before testing completion: every triggered category MUST have direct testing and `testplan.yaml` coverage, or an approved manual/disabled deferral, for the implemented `change_id`.
- Before acceptance: report which triggers applied, which checks ran, which checks were deferred, and whether any deferral blocks acceptance.

## Output Requirements
Every proposal/design/testing/acceptance artifact that handles a triggered change MUST record:

| Trigger Category | Applies? | Evidence | Required Checks | Completed Checks | Deferred Checks and Reason | Residual Risk |
|------------------|----------|----------|-----------------|------------------|----------------------------|---------------|
| contract/protocol | yes / no | | | | | |
| data/schema | yes / no | | | | | |
| security/privacy/permission | yes / no | | | | | |
| runtime/integration | yes / no | | | | | |
| build/dependency/config/deployment | yes / no | | | | | |
| ui/datamodel/workflow | yes / no | | | | | |
| harness/process | yes / no | | | | | |

Rules:
- `Applies?` may be `no` only when `Evidence` explains why the trigger does not apply.
- `Deferred Checks and Reason` must include owner, reason, and acceptance impact.
- Acceptance MUST NOT pass if a required triggered check is missing and no approved deferral exists.
