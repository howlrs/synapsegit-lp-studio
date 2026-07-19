# Sequential generic artifact Proposals

Posted as [SynapseGit Issue #26](https://github.com/howlrs/synapsegit/issues/26).

## Integration finding

The source-pinned generic artifact convenience workflow bootstraps one
Ref-empty repository, admits one Proposal, and records one Decision. A second
`begin_artifact_proposal` against that repository returns
`artifact_project_exists`.

C2 can therefore prove one real Proposal/Decision in an isolated evaluation
repository, but C6 and later need a supported way to admit the next Proposal
against the current exact Decision head without fragmenting one LP's history.

## Requested boundary

- Reconstruct project/Actor/Policy/Grant/Ref authority from trusted host
  configuration and verified repository state.
- Verify the supplied Accepted manifest against the selected current site Tree.
- Preserve all earlier Proposal/Decision history.
- Fail closed on stale base, active review, Proposal conflict, or Decision CAS.
- Compose with durable recovery (#23) and host approval (#24).
- Keep Ref, OID, repository path, actor, credential, and permit out of public
  request/response payloads.

Until #26 is resolved, the UI/API model may support multiple opaque Proposal
IDs, but the real adapter must visibly reject a second Proposal for one project
rather than presenting separate repositories as one canonical history.
