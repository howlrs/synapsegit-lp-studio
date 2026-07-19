# Host-authenticated Human approval boundary

Posted as [SynapseGit Issue #24](https://github.com/howlrs/synapsegit/issues/24).

## Context

The generic artifact convenience workflow in SynapseGit draft PR #25 is a
trusted, same-process primitive. Its pending authority is non-serializable and
the final Decision still goes through `synapse-application` and
`HumanDecisionRuntime`.

Calling the Rust Decision helper is nevertheless treated as trusted-process
authority. Core validates immutable lineage, Policy, disposition, Proposal
preconditions, and Decision CAS; it does not authenticate the LP Studio browser
user. Directly connecting a browser route would collapse the intended Human
gate into server-side self-authentication.

## Requested boundary

- Browser fields, review IDs, dispositions, and idempotency keys are never
  authority by themselves.
- LP Studio authenticates and authorizes the user before project/review lookup.
- A server-issued process-local approval is bound to project, Proposal/head,
  expected Decision/head, canonical intent, expiry, and one use.
- Foreign, expired, replayed, mismatched, and stale approval fails without Ref,
  reflog, or Accepted-state mutation.
- The final Decision continues through the ordinary SynapseGit one-shot Human
  permit and `HumanDecisionRuntime` path.
- Credentials, approval material, internal OIDs/Refs, repository paths, and
  private rationale are absent from Debug/public payloads.

This Issue does not request an HTTP framework, identity provider, tag, release,
or production claim. Until it is resolved upstream, LP Studio must implement
and test the host approval wrapper at its own trusted server boundary.
