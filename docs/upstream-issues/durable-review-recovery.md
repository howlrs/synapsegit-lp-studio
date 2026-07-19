## Context

A local creative session may remain under review for minutes or hours, and the
local process may restart between Proposal publication and Human Decision.

Audited baseline:

- released baseline: `v0.3.0`
- current main audited: `524c2f7354a880f002eb7f8c60d558f8d12de6e0`
- `AdmittedProposalHandle`, registrations, permits, ACL/profile state, and
  project fences are process-local and non-serializable
- the v0.3.0 release notes correctly state that restart leaves an incomplete
  Proposal that diagnostics can explain but cannot resume

## Current limitation

An embedding application cannot safely complete a long-running review after a
restart or determine a Decision outcome after a timeout/process failure.
Blind retry is unsafe because permits burn, Ref CAS may already have committed,
and one canonical disposition is allowed per Proposal.

LP Studio also has an atomicity boundary between the immutable proposed site,
the SynapseGit Decision, and its local Accepted pointer. It needs a durable,
queryable receipt rather than a serialized authority/permit.

## Requested contract

Add a versioned durable review/outcome boundary that:

1. persists a public-safe opaque operation/receipt identifier only after
   Proposal publication succeeds;
2. reconstructs trusted review authority after restart from server-owned
   project configuration plus full Core revalidation, never from
   caller-supplied OIDs/Refs/heads;
3. reports bounded states such as pending review, Decision committed,
   terminal denial, retryable failure, and outcome unknown;
4. accepts an idempotency key for Human Decision and preserves one canonical
   disposition per Proposal;
5. lets a caller reconcile timeout/process-failure outcomes without blind
   replay;
6. retains exact Proposal-head and canonical Decision-lineage CAS checks; and
7. exposes no credential, permit, Actor/Policy/Grant authority, repository
   path, or oracle for unauthorized projects.

## Acceptance criteria

- A Proposal published before process shutdown can be reviewed after a new
  process starts with the same trusted project configuration.
- Restart does not serialize or restore the old permit or
  `AdmittedProposalHandle`; authority is reconstructed and revalidated.
- Fault injection tests before/after Proposal CAS, Decision CAS, receipt
  persistence, and response delivery yield a queryable, unambiguous state or
  an explicit bounded `outcome_unknown`.
- Retrying the same idempotency key never creates two DecisionFeedback records,
  Decision Commits, or reflog events.
- A different disposition for an already decided Proposal remains rejected.
- Stale proposal head, changed canonical Decision head, ACL/profile change,
  expiry, and unauthorized project tests fail closed.
- Authentication and project anti-oracle behavior remain consistent with the
  current application boundary.
- An embedding application can implement the sequence
  `durable intent -> Decision receipt -> atomic local Accepted pointer` and
  complete it safely after restart.

## Non-goals

- portable bearer authority or serialized permits
- weakening full Core revalidation
- distributed/multi-region coordination
- organization/quorum/partial-adoption workflows
