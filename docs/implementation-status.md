# Implementation status

Status date: 2026-07-19

Branch: `agent/lp-studio-m1`

Draft PR: [#1](https://github.com/howlrs/synapsegit-lp-studio/pull/1)

Completed progress: 38%

## Checkpoints

| Checkpoint | Weight | Status | Evidence |
| --- | ---: | --- | --- |
| C0 Requirements and early ADR baseline | 5% | complete | commit `e256075`; detailed requirements, plan, ADR-0001–0006, traceability generator, docs QA; draft PR #1 |
| C1 Upstream Synapse generic contract | 15% | complete | SynapseGit commit [`7ddb58b`](https://github.com/howlrs/synapsegit/commit/7ddb58b2ad585db3823431135ae33222d4704f9f), [draft PR #25](https://github.com/howlrs/synapsegit/pull/25), [passing CI](https://github.com/howlrs/synapsegit/actions/runs/29672697156/job/88154428767), contract lock/parity check, workspace quality gates |
| C2 Real-boundary M0 vertical slice | 10% | complete | real pinned `synapse-artifact` Proposal/Decision; strict API/bridge schema; separate random loopback origins; deterministic blank/fake-AI/adopt/export E2E; Rust, TypeScript, CSP, quota, and ZIP tests |
| C3 Project/revision/import | 8% | complete | private retained state root; canonical immutable revision/CAS and Accepted pointer; reviewed copy import; restart, drift, malicious-path, root-overlap, source-preservation, and browser E2E evidence |
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
- C0 through C3 are implemented and locally verified. C3 evidence is bound to
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

## C3 evidence and limits

- Explicit `LP_STUDIO_STATE_ROOT` storage retains blank and imported projects
  across server recreation. A process-wide lease rejects a second writer to the
  same root; ephemeral state remains the safe default when the variable is
  omitted.
- Accepted bytes are stored in an application-owned CAS with canonical NFC,
  lexical file manifests. Revision manifest, artifact digest, current pointer,
  materialized `site/`, and in-memory Accepted state are cross-checked.
- Project creation uses owned staging and atomic publication. Immutable files
  use same-directory temporary writes, file sync, atomic rename, and parent
  directory sync. Startup recovers known transaction states and fails closed on
  unknown or inconsistent state instead of deleting it.
- Registered-root import has no browser-supplied path. It presents the exact
  included/excluded files, byte counts, entry point, limits, warnings, and
  review digest, then rescans under the same session before copying. Source and
  state roots must be real, disjoint directories.
- Import rejects traversal, separator ambiguity, NFC/full-casefold collision,
  Windows reserved names, symlink, hardlink alias, non-regular entries,
  credential/Studio metadata, excessive entries, bytes, path depth, and path
  length. Directory/file identities are checked before and after bounded reads.
- Proposal, approval/Decision, and export stop with
  `external_changes_detected` when the materialized Accepted view drifts. A
  failed drift preflight does not consume Human approval.
- Local gates pass with 109 Web tests, 21 Rust library tests, 9 launcher tests,
  production build, strict schema/guard parity, and 2 Chromium E2E flows. The
  import E2E hashes the complete recursive source tree before and after copy and
  re-open.
- SynapseGit's strict timestamp bytes remain intact; integration ergonomics and
  earlier config validation are tracked in
  [Issue #28](https://github.com/howlrs/synapsegit/issues/28).
- Deliberately deferred work includes archive upload, rename/delete/history
  destruction UI, DOM-node/runtime limits, per-limit failure details,
  restart-durable pending Proposal authority, and Synapse/local Accepted
  outcome reconciliation. The last two remain C7/#23 work; fault-injection and
  retention hardening remain C9. Imported non-UTF-8 or oversized entry HTML can
  be accepted by the copy boundary but rejected by Preview; C4 adds aligned
  preflight diagnostics rather than weakening Preview validation.
- An adversarial same-user process that swaps and restores the same directory
  inode during both complete scans remains a theoretical live-filesystem race.
  The registered-root, no-follow, identity, rescan, digest, and private-state
  boundaries substantially narrow it but do not claim transactional filesystem
  snapshots.

## Next checkpoint

C4 replaces the process-shared Preview storage scope with ephemeral
project/session origins, binds every static request to an opaque grant, and adds
browser evidence for CSP, service-worker, storage, navigation, and bridge
isolation before the checkpoint is committed/pushed.
