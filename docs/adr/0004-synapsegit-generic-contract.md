# ADR-0004: Generic SynapseGit integration dependency

Status: accepted direction; upstream implementation required

Date: 2026-07-19

## Context

SynapseGit v0.3.0 has the Core object graph and narrow AI/Human semantics, but
its implemented localhost use case and publication discovery are tied to the
three-file image Creator Pilot. Pending review authority is process-local.

LP Studio cannot truthfully use raw `update-ref`, CAS/SQLite writes, or
browser-selected Ref/OID/authority as a generic integration.

## Decision

- Treat [SynapseGit #22](https://github.com/howlrs/synapsegit/issues/22) and
  [#23](https://github.com/howlrs/synapsegit/issues/23) as C1 blockers.
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
- Pin the contract schema/version and exact reviewed upstream commit during
  development. Tag/release remains a separate explicit operation.
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
- Development checkpoints before Creator review must not fabricate a Human
  Decision.
- Low-level compatibility evidence can be stored only as trusted-operator
  observation and is not M1 completion.
