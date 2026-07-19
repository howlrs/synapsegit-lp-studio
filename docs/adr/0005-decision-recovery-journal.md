# ADR-0005: Crash-safe Decision and Accepted-pointer journal

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

SynapseGit Decision publication and the LP Studio Accepted pointer cannot share
one database transaction. A crash may happen after either durable boundary or
after a response is lost.

## Decision

Use an idempotent SQLite command journal rather than a general saga framework.

For adopt:

1. verify and freeze the immutable Proposal manifest;
2. commit local Decision intent, expected Accepted revision, Proposal receipt,
   disposition, and idempotency key;
3. publish/query the SynapseGit Decision and persist its durable receipt;
4. only after an adopted receipt, atomically advance the local Accepted pointer;
5. materialize `site/`, verify its manifest, and mark the command complete.

Reject/defer use the same receipt/reconciliation flow without changing the
Accepted pointer.

No timeout or process failure is blindly retried. Startup reconciliation
queries the durable upstream outcome. A stale base, changed Proposal head,
Decision Ref conflict, or second disposition fails closed.

## Consequences

- Fault injection is required before/after every journal commit, Synapse call,
  receipt persistence, Accepted pointer change, materialization, and response.
- Once Synapse records adopted, LP Studio recovery completes the deterministic
  Accepted pointer change rather than attempting to roll back immutable history.
- Browser requests contain only opaque IDs, disposition, expected revision,
  bounded rationale, and idempotency key.
