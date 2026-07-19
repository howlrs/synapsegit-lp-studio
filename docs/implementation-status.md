# Implementation status

Status date: 2026-07-19

Branch: `agent/lp-studio-m1`

Draft PR: [#1](https://github.com/howlrs/synapsegit-lp-studio/pull/1)

Completed progress: 65%

## Checkpoints

| Checkpoint | Weight | Status | Evidence |
| --- | ---: | --- | --- |
| C0 Requirements and early ADR baseline | 5% | complete | commit `e256075`; detailed requirements, plan, ADR-0001–0006, traceability generator, docs QA; draft PR #1 |
| C1 Upstream Synapse generic contract | 15% | complete | SynapseGit commit [`7ddb58b`](https://github.com/howlrs/synapsegit/commit/7ddb58b2ad585db3823431135ae33222d4704f9f), [draft PR #25](https://github.com/howlrs/synapsegit/pull/25), [passing CI](https://github.com/howlrs/synapsegit/actions/runs/29672697156/job/88154428767), contract lock/parity check, workspace quality gates |
| C2 Real-boundary M0 vertical slice | 10% | complete | real pinned `synapse-artifact` Proposal/Decision; strict API/bridge schema; separate random loopback origins; deterministic blank/fake-AI/adopt/export E2E; Rust, TypeScript, CSP, quota, and ZIP tests |
| C3 Project/revision/import | 8% | complete | private retained state root; canonical immutable revision/CAS and Accepted pointer; reviewed copy import; restart, drift, malicious-path, root-overlap, source-preservation, and browser E2E evidence |
| C4 Separate-origin preview | 8% | complete | session/project/snapshot-scoped opaque Preview origins; exact Host/route binding and revocation; CSP/sandbox/storage/navigation isolation; privacy-safe diagnostics; Rust, Web, and Chromium evidence |
| C5 Target v1 and resolution | 10% | complete | strict six-kind TargetV1/schema; immutable target persistence/restart; scoped bridge capture and dynamic tree; fail-closed resolution receipt; Web/Rust/Chromium evidence; [SynapseGit #29](https://github.com/howlrs/synapsegit/issues/29) |
| C6 AI context and ChangeSet | 9% | complete | exact reviewed provider context/manifest; deterministic fake and optional OpenAI Responses adapters; strict four-operation ChangeSet; atomic isolated workspace, Target re-resolution, blocking active-behavior checks; Web/Rust/Chromium evidence |
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
- C0 through C6 are implemented and locally verified. C6 evidence is bound to
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
  be accepted by the copy boundary but rejected by Preview. Aligned import-time
  entry diagnostics remain deferred rather than weakening Preview validation.
- An adversarial same-user process that swaps and restores the same directory
  inode during both complete scans remains a theoretical live-filesystem race.
  The registered-root, no-follow, identity, rescan, digest, and private-state
  boundaries substantially narrow it but do not claim transactional filesystem
  snapshots.

## C4 evidence and limits

- The Editor/API listener and Preview listener remain distinct loopback
  origins. Every live Preview snapshot receives a process-secret-derived,
  128-bit opaque `pv-….localhost` host label bound to its session, project,
  snapshot, route, and listener port. A restart rotates the process secret.
- Preview requests require the exact scoped Host and bounded static route.
  Unknown, expired, foreign-project, stale Accepted, and terminal Proposal
  routes return the same not-found response, without an enumeration oracle.
- Preview responses expose only revision files plus a response-only bridge.
  They carry no Editor credential, OS path, source manifest, Synapse handle,
  prompt, or provider response, and the bridge is absent from source and
  deterministic export bytes.
- The iframe, Editor CSP, Preview CSP, and common security headers deny Editor
  framing, external connections, forms, popups, top navigation, downloads,
  workers, nested frames, WebRTC, referrers, MIME sniffing, and cache reuse.
  The bridge blocks document replacement and navigation outside the exact
  scoped project prefix while retaining same-project relative navigation.
- The bridge captures native navigation intrinsics before untrusted site
  scripts run, clears `window.name`, and reports only bounded diagnostic codes
  for missing resources, CSP violations, site errors, and unhandled
  rejections. It never invents a source location when one is unavailable.
- Chromium isolation tests exercise localStorage, IndexedDB, Cache Storage,
  BroadcastChannel, cookies, `window.name`, service-worker registration,
  Editor API access, popup/top/external navigation, document replacement,
  WebRTC construction, forged bridge messages, same-project navigation, URL
  expiry, project switch, and process restart.
- Local gates pass with 118 Web tests, 24 Rust library tests, 10 launcher
  tests, production build, strict contract/schema checks, and 3 Chromium E2E
  flows.
- Navigation containment depends on the browser Navigation API. The Editor
  fails closed and does not mount active Preview content when that API is not
  available. Current browser evidence is Chromium-only.
- `webrtc 'block'` CSP plus the same-realm constructor guard is defense in
  depth, not a claim that arbitrary hostile JavaScript across every browser
  realm is fully contained. Broader browser/runtime and fault hardening remains
  C9/C10 work.
- A non-canonical or legacy doctype receives the bridge at byte zero for
  response-first execution; this can change quirks-mode behavior. Port 80 is
  also outside the supported Preview configuration because URL
  canonicalization removes its explicit port. Normal random high ports are
  covered.

## C5 evidence and limits

- `TargetV1` is a strict six-kind discriminated union shared by TypeScript,
  Draft 2020-12 JSON Schema, the Preview bridge, and the Rust API. Kind-specific
  guards reject missing or extra evidence, malformed normalized geometry,
  partial or non-UTF-16 text offsets, runtime handles, stale capture binding,
  and decorated/sparse arrays.
- The response-first bridge captures page, semantic/heuristic block, arbitrary
  visible element, text, clamped point, and normalized region Targets. It emits
  a bounded dynamic tree with render-local handles, draws a response-only
  overlay, and never annotates source/export files with persistent Studio IDs.
- Target records are validated server-side, stored as immutable canonical JSON,
  sorted and bounded on load, and rehydrated across restart. A Proposed Target
  resolves only while that exact Proposal remains pending on the current
  Accepted base.
- Resolver v1 returns resolved, ambiguous, or detached candidates and a
  deterministic receipt. Context creation recomputes source/revision evidence
  and rejects stale receipts, ambiguity, detachment, or a terminal Proposal.
  Geometry, class, or text alone does not acquire authority.
- Browser UI exposes all six kinds, the dynamic tree, pointer point/region
  capture, keyboard node alternatives, capture source/revision, status, and
  candidate reasons. Preview source changes clear the Target; superseded API
  responses cannot replace a later selection.
- Local gates pass with 164 Web tests, 30 Rust library tests, 10 launcher
  tests, strict schema/guard parity, Clippy warnings denied, production build,
  and 3 Chromium E2E flows. The primary flow creates all six kinds through the
  tree and also exercises pointer point/region capture before a real
  fake-AI/SynapseGit Proposal and Human adoption.
- SynapseGit correctly rejects fractional JSON tokens at its canonical
  boundary, but exposes no public normalized fixed-point helper for generic
  application contexts. The local versioned decimal-string projection is
  covered by tests; the integration ergonomics request is tracked in
  [Issue #29](https://github.com/howlrs/synapsegit/issues/29).
- Reference Targets, user-defined labels, breadcrumb polish, restored Target
  history UI, candidate overlays, a general weighted HTML resolver corpus, and
  cross-browser visual accuracy remain deferred to later M1 acceptance and
  hardening. The current resolver fails closed instead of guessing.

## C6 evidence and limits

- Context creation binds the exact attempt, instruction, Target, current
  resolution receipt, Accepted revision, provider/model/adapter, system
  instruction, output contract, and redacted site content into one canonical
  digest. The Creator can expand those exact provider bytes before generation.
- The manifest is limited to ten UTF-8 text files, 512 KiB per file, and 2 MiB
  total. It reports path, media type, purpose, line range, source/included byte
  count, digest, token estimate, redactions, truncation, and screenshot-off
  state. Credential-like values and common local absolute paths are redacted;
  site content is explicitly quoted as untrusted data.
- Original Accepted and included-redacted content digests are distinct. Because
  ChangeSet v1 uses full-file replacement, a context containing any redacted
  site file remains locally reviewable but generation is disabled in the UI
  and rejected server-side before provider execution. C6 never rehydrates a
  model-authored placeholder or pays for a provider call that cannot be safely
  applied. Redacted instructions and Target labels may still generate when the
  site-file manifest itself is clean.
- Provider capabilities expose the local deterministic fake and, only when a
  server-side key is configured, an OpenAI Responses adapter. Provider/model
  selection is allow-listed. The live request uses no tools, low reasoning
  effort, strict Structured Outputs, no provider-side storage request, no
  redirects, bounded timeouts, and a 4 MiB streaming response cap.
- The Responses adapter accepts exactly one completed assistant message with
  one non-empty output-text part. Missing control fields, unknown/extra output
  items, multiple text parts, incomplete messages, and malformed envelopes
  fail closed; any explicit refusal rejects the entire response even when text
  is also present.
- The browser receives no provider credential. Safe errors omit provider response
  bodies, and failed or invalid raw results are neither logged nor materialized.
  Valid attribution binds attempt, provider request ID, provider, requested and
  reported model, adapter, external/local status, and only provider-reported
  usage.
- ChangeSet v1 rejects unknown fields/operations, stale bases, unsafe or
  case-colliding paths, media mismatches, binary generation, excessive counts
  or bytes, conflicting operations, rename cycles, and exact hash-precondition
  failures. All four operations apply to a clone before entry point, syntax,
  local-reference, export deny-list, and Target checks.
- A valid result is materialized into an immutable isolated Proposal workspace,
  then registered through the pinned real SynapseGit generic artifact path.
  SynapseGit receives a privacy-filtered digest projection rather than the user
  prompt, full Target, site snippets, credential, or raw model result. The UI
  continues to state caller-supplied attribution and `execution未検証`.
- Newly introduced external origins (including protocol-relative URLs), form
  actions, script/iframe/download behavior, inline event handlers, changed
  local or inline script bodies, analytics, and cookie behavior are visible
  blocking warnings with exact destinations. Both the UI and Decision server
  reject adoption while one is present; reject and defer remain available
  Human dispositions. Accepted bytes are unchanged on every failed
  generation/validation.
- Local gates pass with 205 Web tests, 63 Rust library tests, 10 launcher
  tests, Clippy warnings denied, strict schema/guard parity, production build,
  and 3 Chromium E2E flows. One external-billing live-provider test remains
  explicitly ignored unless manually acknowledged.
- Ordinary CI uses only the fake adapter. The opt-in ignored OpenAI live contract
  test requires an explicit environment acknowledgement and credential and was
  not executed in the credential-free checkpoint gates. Its external network,
  billing, and account availability are therefore not claimed as automated CI
  evidence.
- Consultation mode, message persistence/branching, streaming and cancellation,
  screenshot opt-in, target-optimized snippets, pending Proposal restart
  recovery, multi-Proposal history, and protected-range editing for redacted
  site files remain deliberately deferred. Shared stream/cancel contract types
  are not presented as an available runtime capability. Safe same-file secret
  preservation requires a future digest-bound range-edit protocol rather than
  ChangeSet v1 placeholder rehydration. Recovery and sequential admission
  continue in C7 and SynapseGit Issues #23/#26.

## Next checkpoint

C7 makes review decisions explicit and restart-safe: adopt, reject, and defer
remain Human-only terminal actions; a durable local journal reconciles the
Synapse receipt with the application Accepted pointer without inventing
restart authority that the pinned SynapseGit contract does not expose.
