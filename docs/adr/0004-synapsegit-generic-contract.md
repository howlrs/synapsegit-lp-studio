# ADR-0004: Generic SynapseGit integration dependency

Status: accepted; upstream C1 source pinned, LP integration required

Date: 2026-07-19

## Context

SynapseGit v0.3.0 has the Core object graph and narrow AI/Human semantics, but
its implemented localhost use case and publication discovery are tied to the
three-file image Creator Pilot. Pending review authority is process-local.

SynapseGit [PR #25](https://github.com/howlrs/synapsegit/pull/25) was merged
and released as [`v0.4.0`](https://github.com/howlrs/synapsegit/releases/tag/v0.4.0),
pinned here at the tagged source commit
[`5352aa9`](https://github.com/howlrs/synapsegit/commit/5352aa9412dfdd2ad6cfcf3746770d015af11b49).
It supplies the C1 generic artifact primitives, but remains a source-available
evaluation release and does not grant LP Studio production/distribution permission.

LP Studio cannot truthfully use raw `update-ref`, CAS/SQLite writes, or
browser-selected Ref/OID/authority as a generic integration.

## Decision

- Treat [SynapseGit #22](https://github.com/howlrs/synapsegit/issues/22) as the
  C1 contract tracker. Continue durable orchestration in
  [#23](https://github.com/howlrs/synapsegit/issues/23) and mandatory browser
  Human approval in [#24](https://github.com/howlrs/synapsegit/issues/24).
- Define and implement a versioned generic regular-file Tree Proposal/Human
  Decision use-case plus durable receipt/outcome query before the real-boundary
  M0 slice.
- Keep LP Studio's adapter server-side and capability-negotiated.
- Bind file bytes/path Tree, Accepted base, redacted ProvenanceTarget,
  attribution, Proposal, and one-disposition Human Decision.
- Support `adopted_unchanged`, `rejected`, and `deferred`; do not emulate
  partial/modified adoption.
- Reconstruct trusted authority after restart from server configuration and
  full Core revalidation; never serialize a permit or accept authority from
  the browser.
- Pin the contract schema/version, exact reviewed upstream commit, and artifact
  hashes in [the contract lock](../synapsegit-contract.lock.json). Do not vendor
  SynapseGit source/spec material into this non-fork repository. Tag/release
  remains a separate explicit operation.
- Continue LP publication under the projection-first scope of
  [Issue #17](https://github.com/howlrs/synapsegit/issues/17#issuecomment-5013850363).
- If upstream is unavailable, independent UI work may run visibly local-only,
  but C1, the 80% gate, and M1 cannot pass.

## Claim boundary

If the model executes outside a trusted SynapseGit executor, record
`AI-attributed` / `caller-supplied`. Do not claim verified generation.
Byte/graph verification does not prove authorship, truth, rights, permission,
semantic correctness, visual correctness, or physical change.

## Consequences

- Upstream SynapseGit work uses its own Issue/branch/PR and tests.
- The v1 contract accepts caller-supplied AI attribution only and always marks
  execution unverified.
- Upstream `decide_artifact_proposal` is trusted-process authority, not real
  browser-user authentication; LP routes require a separate one-shot approval.
- Checked re-registration and the separate journal are not end-to-end restart
  orchestration or cryptographic admission evidence.
- Development checkpoints before Creator review must not fabricate a Human
  Decision.
- Low-level compatibility evidence can be stored only as trusted-operator
  observation and is not M1 completion.
