# Implementation status

Status date: 2026-07-19

Branch: `agent/lp-studio-m1`

Draft PR: [#1](https://github.com/howlrs/synapsegit-lp-studio/pull/1)

Completed progress: 30%

## Checkpoints

| Checkpoint | Weight | Status | Evidence |
| --- | ---: | --- | --- |
| C0 Requirements and early ADR baseline | 5% | complete | commit `e256075`; detailed requirements, plan, ADR-0001–0006, traceability generator, docs QA; draft PR #1 |
| C1 Upstream Synapse generic contract | 15% | complete | SynapseGit commit [`7ddb58b`](https://github.com/howlrs/synapsegit/commit/7ddb58b2ad585db3823431135ae33222d4704f9f), [draft PR #25](https://github.com/howlrs/synapsegit/pull/25), [passing CI](https://github.com/howlrs/synapsegit/actions/runs/29672697156/job/88154428767), contract lock/parity check, workspace quality gates |
| C2 Real-boundary M0 vertical slice | 10% | complete | real pinned `synapse-artifact` Proposal/Decision; strict API/bridge schema; separate random loopback origins; deterministic blank/fake-AI/adopt/export E2E; Rust, TypeScript, CSP, quota, and ZIP tests |
| C3 Project/revision/import | 8% | planned | — |
| C4 Separate-origin preview | 8% | planned | — |
| C5 Target v1 and resolution | 10% | planned | — |
| C6 AI context and ChangeSet | 9% | planned | — |
| C7 Review, Decision, recovery | 8% | planned | — |
| C8 Export, publication, integrated E2E | 7% | planned | [#17 follow-up](https://github.com/howlrs/synapsegit/issues/17#issuecomment-5013850363) |
| **80% local verification gate** | — | blocked until C0–C8 complete | Creator check required before C9 |
| C9 Security and fault hardening | 10% | planned | — |
| C10 Acceptance evidence | 7% | planned | — |
| C11 M1 completion gate | 3% | planned | — |

## Current facts

- SynapseGit released baseline remains v0.3.0; the generic contract is pinned to
  unreleased source commit `7ddb58b2ad585db3823431135ae33222d4704f9f`.
- Generic regular-file mapping, caller-supplied/unverified Proposal/Decision,
  checked recovery registration, and a separate review journal are under
  review in upstream draft PR #25. They are not v0.3.0 release features.
- The frozen public v1 contract excludes verified executor attribution. The LP
  adapter must not claim that SynapseGit ran or verified a model.
- The upstream workflow is trusted-process authority, not browser-user
  authentication. Host-authenticated one-shot approval is tracked in
  [#24](https://github.com/howlrs/synapsegit/issues/24) and remains mandatory
  in the LP server integration.
- Checked re-registration and the journal are separate primitives. They are not
  a journal-integrated restart-resumable orchestrator or cryptographic durable
  admission evidence; full reconciliation remains C7 work and #23 scope.
- Source integration also confirmed that the convenience workflow permits one
  Proposal in one Ref-empty repository and has no selected-site checkout API.
  Iterative admission is tracked in
  [#26](https://github.com/howlrs/synapsegit/issues/26), and bounded verified
  checkout in [#27](https://github.com/howlrs/synapsegit/issues/27). C2 is one
  isolated evaluation Proposal; host-retained Accepted bytes are not described
  as a SynapseGit checkout.
- GitHub App Issue writes returned 403; authenticated GitHub CLI successfully
  created the reviewed upstream feedback.
- C0 through C2 are implemented and locally verified. C2 evidence is bound to
  this checkpoint commit and its GitHub checks. No tag, merge,
  release, product publication, or distribution permission exists yet.
- The development repository is Public by explicit Creator direction. Public
  visibility is not a product-publication action or a license grant.
- Development checkpoint pushes are explicitly authorized; product publication
  remains a separate Human action.

## C2 evidence and limits

- The production bundle is exercised through Chromium against two independently
  allocated loopback origins. Editor framing is denied; Preview cannot reach
  privileged API routes.
- `hero-heading`, `hero-copy`, and `hero-cta` each produce a target-bound,
  deterministic single-file mutation. Proposed bridge messages bind both the
  Proposal snapshot and its Accepted base revision.
- Human adoption requires a hashed, expiring, intent-bound, atomically consumed
  host approval. Accepted bytes change only after the real Synapse Decision
  receipt succeeds and are then fetched again by GET.
- Repeated export produces byte-identical ZIPs with lexical entries, fixed
  timestamp/mode, and no prompt, token, bridge, Studio, or attribution metadata.
- Runtime state collections are capped and return stable 429 responses. A
  process-owned temporary state root is private and removed on shutdown.
- The upstream convenience workflow still allows one Proposal per isolated
  repository and retains same-process Decision authority. C2 therefore permits
  one Proposal per project; iterative/restart behavior remains #26/#23 and C6/C7.

## Next checkpoint

C3 adds retained managed project storage, immutable manifests, bounded import,
Accepted pointer materialization, and external-drift detection. Completion is
reported only after restart and malicious-path tests pass and the checkpoint is
committed/pushed.
