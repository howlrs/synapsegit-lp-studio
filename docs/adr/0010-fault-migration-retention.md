# ADR-0010: Fault injection, migration recovery, and retention boundary

Status: accepted and implemented for the automated local baseline

Date: 2026-07-19

## Context

LP Studio has multiple durable systems that cannot share one transaction:

- SynapseGit Proposal and Human Decision state;
- the local Decision command journal;
- immutable site objects and manifests;
- the local Accepted pointer and materialized `site/` view; and
- retained Target, Proposal, export, and diagnostic metadata.

A successful in-process request is not sufficient evidence for these
boundaries. Disk exhaustion, permission failure, short write, failed sync,
failed rename, process termination, and a lost HTTP response can happen between
any two durable steps. Recovery must never infer adoption from browser memory
or delete an unknown artifact to make startup succeed.

The storage implementation wires these atomic-write and fail-closed primitives
to the automated C9 returned-fault, process-abort, migration, read-only recovery,
and manual-retention evidence. This does not convert the local evaluation
baseline into a release or production-readiness claim.

## Decision

### Deterministic test-only failpoints

Use the static vocabulary in
`apps/local-server/src/fault_injection.rs` at every relevant before/after
boundary. The initial vocabulary covers:

- Decision journal intent and completion records;
- immutable object writes and CAS publication;
- the Synapse Decision call, receipt persistence, and receipt query;
- Accepted pointer publication and materialization;
- HTTP Decision response publication; and
- generic storage write, filesystem sync, and rename operations.

Production builds have no environment variable, request field, configuration
file, or command-line option that enables failpoints. The production `check`
function is an unconditional no-op. Test scenarios arm an exact one-based hit
counter while holding an exclusive scenario guard. Failpoint diagnostics contain
only a static name and counter. Dynamic paths, project IDs, credentials,
rationales, provider content, and response bodies are not accepted by the
fault-injection API.

A fault test must verify durable state after constructing a new server process,
not only after catching an in-process error. Adopt, reject, and defer must each
cover the boundaries relevant to their state transition. Tests must distinguish
a simulated returned I/O error from an actual process kill after a durable
operation.

### Migration and read-only recovery

Every persisted root has an explicit schema version. Before changing bytes from
an older supported schema, LP Studio must:

1. acquire the existing single-writer lease;
2. verify the old schema and its immutable manifests without repair;
3. create an owned, versioned backup or copy-on-write recovery point;
4. record a canonical manifest and checksum for that recovery point;
5. write the new representation outside the active location;
6. validate the complete new representation; and
7. publish it atomically before removing only proven-owned staging data.

Migration never mutates the only readable old copy in place. A failed migration,
an unknown schema, or a corrupt integration database opens a read-only recovery
surface when the last Accepted manifest can still be verified. That surface may
provide privacy-safe diagnostics, backup instructions, and a last-Accepted
static export. It must not generate a Proposal, commit a Decision, advance an
Accepted pointer, publish a provenance bundle, or silently repair SynapseGit
state.

If even the last Accepted manifest cannot be verified, startup fails closed and
preserves the original bytes for explicit recovery. A successful migration test
must also prove that interruption at every publish boundary leaves either the
old schema or the fully validated new schema readable.

### Retention and deletion

Automatic garbage collection remains disabled until reachability can be proven
from all of the following roots:

- current and retained Accepted revisions;
- pending and terminal Decision records;
- unfinished or needs-reconciliation commands;
- SynapseGit bindings and receipts;
- active export/publication generation; and
- any user-visible backup or recovery point.

Manual cleanup presents the exact owned records, byte impact, immutable-history
effect, and recovery consequence before a separate Human confirmation. Cleanup
does not follow symlinks, cross a registered root, remove an unknown entry, or
describe removal of local projections as deletion of immutable SynapseGit
history. Project deletion, conversation cleanup, failed Proposal cleanup,
export cleanup, and publication-draft cleanup have separate scopes.

Quota handling may reject new work without deleting protected records. A
deterministic replacement policy is permitted only for explicitly disposable,
unreferenced metadata and must itself be crash-recoverable.

## Required evidence

C9 evidence must include at least:

- returned I/O failure and real process-kill cases at every Decision boundary;
- ENOSPC, permission, write, sync, and rename failures with the old Accepted
  pointer and bytes unchanged unless a durable adopted receipt requires forward
  reconciliation;
- restart reconciliation proving exactly one terminal disposition;
- migration fixtures for every supported source version and every interrupted
  publish step;
- corrupt and unknown-schema fixtures exercising read-only recovery;
- retention reachability and deletion-confirmation tests; and
- canary scans showing that diagnostics contain no secret or local path.

Passing unit tests for the standalone failpoint registry proves only that test
scheduling is deterministic. It is not evidence that a runtime boundary has
been wired or recovered correctly.

## Consequences

- Runtime code adds a failpoint call immediately adjacent to a durable operation,
  without passing dynamic context into the injector.
- Fault scenarios run serially because their hit counters are process-global in
  test builds.
- A migration or retention feature cannot be marked complete from a schema type
  or UI mock alone.
- The local package may remain an internal evaluation artifact. This ADR grants
  no production, external-delivery, redistribution, or brand permission.
- The automated C9 local baseline is complete only for the checked-in fixtures
  and declared Linux/WSL evaluation boundary. Product release and manual or
  external gates remain independent.
