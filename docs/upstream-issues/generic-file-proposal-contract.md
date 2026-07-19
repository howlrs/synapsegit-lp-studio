## Context

SynapseGit LP Studio needs to record an AI-attributed static-site file-tree
Proposal and a Human `adopted_unchanged`, `rejected`, or `deferred`
Decision without exposing Core authority to the browser.

Audited baseline:

- released baseline: `v0.3.0`
- current main audited: `524c2f7354a880f002eb7f8c60d558f8d12de6e0`
- Core already has Blob, ManifestTree, Commit, Ref CAS/reflog,
  Creative AI admission, and narrow Human Decision semantics
- the implemented localhost Creator Pilot accepts original/current/
  caller-supplied AI-output files for its image-oriented workflow

## Current limitation

There is no supported application/HTTP/CLI use-case contract for:

- deterministically mapping a bounded regular-file directory to a site Tree;
- bootstrapping a generic project and canonical Decision lineage;
- publishing a generic file-tree Proposal from an exact accepted base;
- binding an application-owned target/context record to that Proposal; or
- recording the existing one-disposition Human Decision semantics for that
  generic Proposal.

Using raw `update-ref`, direct CAS writes, or browser-selected OIDs/Refs would
bypass the authenticated application boundary and is not an acceptable
integration.

## Requested contract

Add a versioned, provider-neutral generic artifact/file-tree use-case boundary
that:

1. accepts only a trusted server-selected project plus a bounded normalized
   regular-file manifest;
2. maps file bytes to Blob and normalized relative paths to nested
   ManifestTree deterministically;
3. bootstraps the trusted Actor/Policy/Grant/ContextPack and existing canonical
   Decision Ref needed by the narrow Human route;
4. admits a Proposal from an exact current Decision base and fails closed on
   `stale_base` or `ref_conflict`;
5. permits an application-owned canonical JSON context/target Blob to be bound
   without claiming that Core validates its DOM semantics;
6. supports only `adopted_unchanged`, `rejected`, and `deferred` in this
   initial profile;
7. returns versioned capability and receipt data without exposing permit,
   authority, raw repository path, Actor, Policy, Grant, or caller-selected
   Ref/head values; and
8. distinguishes trusted-executor verification from caller-supplied/
   AI-attributed output.

Restart-durable pending review and outcome query are tracked separately so this
Issue can stay focused on the generic semantic contract.

## Acceptance criteria

- A frozen regular-file fixture maps to the same Blob/Tree/Commit identities
  across repeated runs.
- Symlink, device, absolute/traversal path, Unicode/case collision, file-count,
  and byte-limit violations fail before Ref mutation.
- From one bootstrapped project, a generic Proposal can be admitted and each
  supported disposition can be exercised and validated in isolated integration
  test fixtures.
- `adopted_unchanged` selects exactly the Proposal Tree; reject/defer retain
  exactly the base Tree.
- A second disposition for the same Proposal is rejected.
- Stale base and concurrent Decision CAS tests fail closed without implicit
  rebase.
- Untrusted request fields cannot select repository, Ref, head, OID, Actor,
  Policy, Grant, candidate Commit, or permit.
- Caller-supplied output is reported as attribution, not verified model
  execution.
- The contract has an explicit name/version/capability response and frozen
  JSON/golden fixtures consumable by a sibling application.

## Non-goals

- browser/editor UI
- model-provider invocation
- partial or modified adoption
- remote GitHub/Synapse publication
- multi-user or distributed authorization
