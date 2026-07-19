# Implementation status

Status date: 2026-07-19

Baseline branch: `main`

Baseline integration PR: [#1](https://github.com/howlrs/synapsegit-lp-studio/pull/1)

C0–C6 baseline integration commit: `c9a22b15369c25f1e83a39d2cf9834962bb0db5a`

Current branch: `agent/lp-studio-m1-completion`

Automated local baseline: C0–C10 implemented; C11 clean-package handoff integrated

M1 completion claim: blocked by the explicit gates below

この文書は「現在実装され、検証済みの範囲」のdocumentation正本です。
要件・計画・contract typeだけからruntime capabilityを推測しないでください。

## Checkpoints

| Checkpoint | Weight | Status | Evidence |
| --- | ---: | --- | --- |
| C0 Requirements and early ADR baseline | 5% | complete | commit `e256075`; detailed requirements, plan, ADR-0001–0006, traceability generator, docs QA; opened draft PR #1, now merged as the C0–C6 baseline |
| C1 Upstream Synapse generic contract | 15% | complete | SynapseGit commit [`7ddb58b`](https://github.com/howlrs/synapsegit/commit/7ddb58b2ad585db3823431135ae33222d4704f9f), [draft PR #25](https://github.com/howlrs/synapsegit/pull/25), [passing CI](https://github.com/howlrs/synapsegit/actions/runs/29672697156/job/88154428767), contract lock/parity check, workspace quality gates |
| C2 Real-boundary M0 vertical slice | 10% | complete | real pinned `synapse-artifact` Proposal/Decision; strict API/bridge schema; separate random loopback origins; deterministic blank/fake-AI/adopt/export E2E; Rust, TypeScript, CSP, quota, and ZIP tests |
| C3 Project/revision/import | 8% | complete | private retained state root; canonical immutable revision/CAS and Accepted pointer; reviewed copy import; restart, drift, malicious-path, root-overlap, source-preservation, and browser E2E evidence |
| C4 Separate-origin preview | 8% | complete | session/project/snapshot-scoped opaque Preview origins; exact Host/route binding and revocation; CSP/sandbox/storage/navigation isolation; privacy-safe diagnostics; Rust, Web, and Chromium evidence |
| C5 Target v1 and resolution | 10% | complete | strict six-kind TargetV1/schema; immutable target persistence/restart; scoped bridge capture and dynamic tree; fail-closed resolution receipt; Web/Rust/Chromium evidence; [SynapseGit #29](https://github.com/howlrs/synapsegit/issues/29) |
| C6 AI context and ChangeSet | 9% | complete | exact reviewed provider context/manifest; deterministic fake and optional OpenAI Responses adapters; strict four-operation ChangeSet; atomic isolated workspace, Target re-resolution, blocking active-behavior checks; Web/Rust/Chromium evidence |
| C7 Review, Decision, recovery | 8% | automated local baseline complete | exact-pinned trusted Rust sidecar; durable Proposal/Decision binding; adopt/reject/defer UI; restart rehydration, reconciliation, and sequential lineage tests |
| C8 Export, publication, integrated E2E | 7% | automated local baseline complete | deterministic Accepted static export/receipt; local privacy-filtered publication exact-byte UI; restart/recovery browser flow; [#17 follow-up](https://github.com/howlrs/synapsegit/issues/17#issuecomment-5013850363) |
| **80% local verification gate** | — | automated criteria complete; Creator result pending | C0–C8 automated gates are integrated; the continuation instruction is not a recorded Creator UX/keyboard result |
| C9 Security and fault hardening | 10% | automated local baseline complete | 26 test-only durable failpoints; returned-fault and real process-abort matrix; v1→v2 migration/backups; byte-preserving unknown-schema read-only recovery; exact reachability-based manual retention |
| C10 Acceptance evidence | 7% | automated local evidence complete; manual/external gates pending | automated keyboard/focus/320px/contrast/reduced-motion flows; safe observability/privacy canaries; synthetic corpus and packaged production App 72-case geometry/autosave/ChangeSet measurements |
| C11 M1 completion gate | 3% | automated package/handoff complete; Human/release gates pending | clean-source package verifier, checksums, launcher/restart/browser/safe-log evidence, traceability and claim-boundary schemas; Creator, screen-reader, live-provider, license/brand, merge/post-merge, and release decisions remain pending |

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
- Checked re-registration and the upstream journal remain separate primitives.
  The C7 integration therefore uses an exact-pinned trusted Rust sidecar plus a
  Studio-owned durable command intent/receipt binding; the local journal is not
  presented as SynapseGit authority.
- The upstream convenience workflow still permits one Proposal in one Ref-empty
  repository and has no selected-site checkout API. The C7 sidecar
  maps each durable operation through lower-level exact-pinned crates while the
  host retains and revalidates Accepted bytes; those bytes are not described as
  a SynapseGit checkout. Iterative admission remains tracked in
  [#26](https://github.com/howlrs/synapsegit/issues/26), and bounded verified
  checkout in [#27](https://github.com/howlrs/synapsegit/issues/27).
- GitHub App Issue writes returned 403; authenticated GitHub CLI successfully
  created the reviewed upstream feedback.
- C0 through C6 are implemented and locally verified. The Creator explicitly
  approved integrating that historical C0–C6 development baseline through PR #1. `main`
  contains merge commit `c9a22b15369c25f1e83a39d2cf9834962bb0db5a`, and the
  post-merge [GitHub Actions run](https://github.com/howlrs/synapsegit-lp-studio/actions/runs/29683014738)
  passed the documentation/lock, formatting, Web, Rust, build, and Chromium
  gates.
- A fresh local audit of that merge commit also passed frozen install,
  `pnpm check`, `pnpm test`, `pnpm build`, `pnpm test:e2e`,
  `pnpm format:check`, `pnpm check:docs`, and `git diff --check`: 205 Web
  tests, 63 Rust library tests, 10 launcher tests, and 3 Chromium flows passed.
  The separately acknowledged, externally billed OpenAI live test remained
  intentionally ignored.
- The post-65% baseline-completion audit passes 265 Web tests, 202 Rust library
  tests, 11 launcher tests, one production-recovery integration test, and 4
  Chromium flows. Two direct Rust test entries remain intentionally ignored:
  the externally billed OpenAI live call and a subprocess-only abort helper
  that is exercised by the parent crash-recovery matrix.
- That C0–C6 merge was an explicit exception to the default pre-80% draft-PR
  policy. The post-65% branch completes the automatable local C7–C11 baseline,
  but it does not convert the manual 80% result or final Human/external/release
  gates to passed. No tag, release, product publication, production-readiness
  claim, or distribution permission exists.
- The development repository is Public by explicit Creator direction. Public
  visibility is not a product-publication action or a license grant.
- Development checkpoint pushes are explicitly authorized; product publication
  remains a separate Human action.
- On 2026-07-19 the Creator explicitly instructed development to continue from
  the 65% baseline through completion. That instruction authorizes work beyond
  the historical 80% stop; it is not recorded as a completed Creator UX,
  keyboard, screen-reader, live-provider, license, brand, merge, or release
  verification.

## C1 evidence and limits

- The Rust workspace pins the upstream SynapseGit source contract to exact
  commit `7ddb58b2ad585db3823431135ae33222d4704f9f`. The committed lock records
  source hashes and capabilities, and the parity check rejects drift from the
  reviewed adjacent source checkout.
- Only the local Rust server imports and calls the versioned trusted use-case
  API. Browser contracts expose Studio-owned opaque identifiers and bounded
  receipts, not raw Ref, OID, CAS, repository path, Actor, Policy, Grant,
  permit, or Decision authority.
- The generic artifact mapping accepts regular files only. The application
  boundary also rejects symlinks, hardlink aliases, and non-regular entries
  before materializing a reviewed file tree.
- Proposal attribution is fixed to caller-supplied with execution verification
  false. Rust and TypeScript guards reject a forged verified-execution claim,
  and byte or graph identity is not presented as authorship, truth, rights, or
  semantic correctness.
- A Proposal becomes locally reviewable only after real Synapse registration
  succeeds. The Decision path requires that registered Proposal plus a valid
  host approval, so an unregistered Proposal cannot be adopted.
- Startup-wide capability negotiation, coordinated external-writer locking,
  durable pending recovery, outcome-unknown reconciliation, and publication
  claims remain deliberately outside this C1 baseline and are still planned.

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
- UI errors distinguish failure before Decision from a lost Decision response
  and from failure to refresh Accepted after a committed response. Once a
  Decision may have run, the UI reports the outcome as unknown, warns against
  blind retry, and requires reload plus Accepted-revision reconciliation
  instead of claiming that Accepted remained unchanged.
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
- At C3, deliberately deferred work included archive upload, destructive UI,
  DOM-node/runtime limits, restart-durable pending Proposal authority, outcome
  reconciliation, fault injection, and retention. C7 later completed Proposal
  and Decision recovery, and C9 completed the automated fault/retention baseline.
  Archive upload, terminal-history-only deletion, and some per-limit diagnostics
  remain outside scope. Imported non-UTF-8 or oversized entry HTML can be
  accepted by the copy boundary but rejected by Preview; aligned import-time
  diagnostics remain deferred rather than weakening Preview validation.
- An adversarial same-user process that swaps and restores the same directory
  inode during both complete scans remains a theoretical live-filesystem race.
  The registered-root, no-follow, identity, rescan, digest, and private-state
  boundaries substantially narrow it but do not claim transactional filesystem
  snapshots.
- The later C6 ChangeSet boundary also rejects AI operations that would create
  Studio metadata or credential-pattern paths; this closes `FR-FILE-007`
  without claiming archive import or the deferred C3 limits.

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
- A safe same-project directory route such as `docs/` resolves to the existing
  `docs/index.html` file. The server injects that canonical file path into the
  response-only Target runtime, so nested navigation, capture, restart, and
  later resolution do not disagree about Target authority.
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
- Target `pagePath` policy is aligned across TypeScript, JSON Schema, the
  Preview runtime, and Rust: a safe relative HTML file path at most 512 UTF-8
  bytes, with ASCII case-insensitive `.htm` or `.html`. TypeScript and Rust
  runtime guards additionally enforce NFC because JSON Schema has no Unicode
  normalization assertion. Root aliases, directory aliases, queries,
  traversal, managed-storage-invalid names, and non-HTML paths fail closed at
  Target admission. A `#` remains valid inside a canonical managed filename;
  browser URL fragments never enter the server-injected `pagePath`.
- When the 32-record persistence bound is reached, the server deterministically
  recycles only Target metadata that no reviewed context references, verifies
  its exact persisted bytes before deletion, and syncs the directory. A
  referenced Target is never removed merely to admit a new capture.
- Recycling relies on the process-held private state root and its writer lease;
  it is not claimed as an atomic defense against a hostile same-user process
  that ignores that boundary and swaps internal directory entries. That fault
  model, plus crash-atomic replacement across remove/persist I/O, remains part
  of C9 hardening. Shape and metadata-size validation complete before any
  existing unreferenced Target is removed.
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
- A dormant C2 `synapsegit-lp.selection` producer and private v1 contract remain
  alongside the C5 `target-draft` runtime. Production Web code has no consumer,
  the message carries no authority, and current capture does not depend on it;
  remove the legacy hook and contract together before any external v1 freeze.

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
- The syntax gate reparses the complete resulting site, not only changed files.
  JSON remains strict; `.css`, `.js`, `.mjs`, and `.cjs` use language parsers
  (with JavaScript semantic early-error checks); `.xml` and `.svg` use a strict,
  namespace-aware XML parser with bounded nodes and no DTD/external-entity
  processing. HTML/HTM uses HTML5 document parsing: tokenizer/tree-builder
  recovery reports a bounded, redacted `static-syntax` advisory count because
  browser HTML5 recovery is not a hard parse failure. UTF-8/NUL failures and
  CSS/JavaScript/JSON/XML/SVG parser diagnostics remain hard failures. This
  gate also parses executable inline classic/module `<script>` bodies,
  `<style>` bodies, and `style` attributes in HTML and in SVG/XHTML XML
  namespaces (including those embedded in `.xml`); unknown XML namespaces are
  not activated by local element name alone. Inert data-block script types such
  as `application/json`, `application/ld+json`, and `importmap` are not
  misclassified as JavaScript. Inline event-handler attributes are handled by
  the separate blocking active-behavior review gate rather than executed or
  syntax-normalized here.
- A valid result is materialized into an immutable isolated Proposal workspace,
  then registered through the pinned real SynapseGit generic artifact path.
  SynapseGit receives a privacy-filtered digest projection rather than the user
  prompt, full Target, site snippets, credential, or raw model result. The UI
  continues to state caller-supplied attribution and `execution未検証`.
- Newly introduced external origins (including protocol-relative URLs), form
  actions, script/iframe/download behavior, inline event handlers, analytics,
  and cookie behavior are visible blocking warnings with exact destinations.
  Script identity includes each occurrence, active attributes, and body bytes;
  every new or changed JavaScript asset is conservatively blocking regardless
  of statically observed reachability. Both the UI and Decision server reject
  adoption while one is present. New or changed CSS/srcdoc reference syntax
  that the bounded scanner cannot exhaustively classify is also a blocking
  warning; removing or renaming any path while such an opaque source remains is
  blocked as well. Quote-aware raw/RCDATA/script scanning, character-reference
  checks on URL-sensitive attributes, and conservative handling of ambiguous
  declarations, processing instructions, `image-set()`, and legacy fetch/base
  attributes prevent tokenizer or escaping ambiguity from hiding a reference
  or active behavior. Accepted bytes are unchanged on every failed
  generation/validation. The C6 baseline UI exposed only adoption; the C7 adds
  explicit Reject and Defer without rewriting this historical C6 evidence.
- Local gates pass with 205 Web tests, 63 Rust library tests, 10 launcher
  tests, Clippy warnings denied, strict schema/guard parity, production build,
  and 3 Chromium E2E flows. One external-billing live-provider test remains
  explicitly ignored unless manually acknowledged.
- Ordinary CI uses only the fake adapter. The opt-in ignored OpenAI live contract
  test requires an explicit environment acknowledgement and credential and was
  not executed in the credential-free checkpoint gates. Its external network,
  billing, and account availability are therefore not claimed as automated CI
  evidence.
- Consultation mode, message persistence/branching, streaming text, screenshot
  opt-in, target-optimized snippets, concurrent ready Proposal comparison,
  non-AI long-operation cancellation, and protected-range editing for redacted
  site files remain deliberately deferred. The post-C6 baseline implements
  explicit server-owned AI generation status/cancel with a late-response
  tombstone; it does not imply conversation streaming. Safe same-file secret
  preservation requires a future digest-bound range-edit protocol rather than
  ChangeSet v1 placeholder rehydration. Restart recovery and sequential
  admission are implemented by C7 and are not retroactively counted as C6
  evidence.

## C7 evidence and limits

- The integration embeds only exact-pinned SynapseGit Rust crates behind a trusted
  local sidecar. Proposal workspace/metadata and a canonical review binding are
  durable before browser review; the browser never receives raw repository,
  Actor, Policy, Grant, permit, Ref, or Decision authority.
- Adopt, Reject, and Defer share one Human approval/Decision path. Reject and
  Defer preserve Accepted, Defer is terminal, and continuation creates a new
  Proposal on the latest base with a predecessor link.
- Startup validates persisted Project, Proposal, review context, Synapse receipt,
  Decision receipt, immutable workspace, Accepted lineage, and current
  materialized bytes before rehydrating an active Review or history. An active
  Proposal takes precedence over older terminal history. A committed Synapse
  Decision with a missing local receipt/pointer is reconciled from durable state;
  browser memory, raw private rationale, or approval tokens are not reconstructed.
- The private rationale is accepted only as bounded transient Decision input.
  Synapse/Core, Studio state, logs, export, and publication retain neither its
  raw bytes nor a correlation digest; only a fixed disposition classification
  is durable.
- Tests cover pending and all three terminal dispositions across restart, a
  Synapse Decision committed before local receipt/pointer completion, sequential
  Reject/Defer/Adopt lineage, idempotent reconciliation, and the C9 returned-fault
  and process-abort matrix.
- Only one active Proposal per Project is exposed. Concurrent ready Proposal
  comparison, partial/file/hunk adoption, and editing Proposed bytes remain out
  of scope.

## C8 evidence and limits

- Static export takes an immutable Accepted snapshot and generates a fixed-profile
  ZIP plus source manifest, option, file-manifest, validation, archive identity,
  and checksum receipt. Proposal, prompt, Target, credential, raw provider data,
  Studio metadata, and Synapse internal data are excluded by selecting only the
  Accepted site manifest.
- The publication generator produces versioned, provider-neutral, privacy-filtered
  GitHub-ready files locally. The UI previews the exact generated bytes, field
  provenance, checksum, redactions, and limitations before download. Its receipt
  states `networkWrites: false` and `remotePublication: separate_human_action`.
- Public title, summary, and Decision note remain explicitly author-supplied.
  The bundle is not a signature, authorship proof, rights proof, GitHub action,
  deployment, or release.
- The static validator is quote-aware for HTML/XML attributes, treats malformed
  markup fail closed, and refuses an offline-self-contained claim when CSS
  escaping/comment ambiguity prevents exact reference classification. Active
  SVG/XML/XHTML documents are never mounted as navigable Preview documents.
- Generator, contract, Rust integration, and Chromium tests cover deterministic
  output, purity, tamper/drift failure, exact-byte publication review, and the
  create/import through restart/recovery/export/publication flow.

## C9 evidence and limits

- Twenty-six static test-only failpoints sit immediately around Decision intent,
  Synapse call/receipt/query, CAS/object publication, Accepted pointer,
  materialization, completion, response, write/sync, and rename boundaries.
  Production has no request, environment, CLI, or configuration switch for
  arming them. Exact thread ownership prevents unrelated parallel tests from
  consuming an armed hit.
- Returned ENOSPC/permission/write/sync/rename faults and 80 real child-process
  abort executions reconstruct the server from disk and verify one terminal
  disposition, unchanged Accepted bytes before authority, or deterministic
  forward reconciliation after an adopted receipt.
- v1→v2 migration verifies the old representation before mutation, creates a
  versioned private backup and canonical checksum, stages and validates the new
  representation, and publishes atomically. Interruption fixtures leave either
  the verified old schema or complete new schema readable.
- Read-only startup inventories and validates the complete source root before any
  recovery mutation path. Corrupt/unknown-schema subprocess tests compare an
  exact before/after tree fingerprint and prove that diagnostics and verified
  last-Accepted export do not alter source bytes.
- Manual retention shows exact scope/binding/byte impact and requires a separate
  confirmation. Cleanup revalidates CAS reachability across retained Accepted,
  Proposal/Decision/reconciliation, Synapse bindings, generated artifacts, and
  recovery backups; unknown or shared objects are not removed. Automatic GC and
  telemetry remain absent.
- Safe-log, persisted-state, archive, and publication canary tests cover prompt,
  file body, credentials, tokens, raw provider response, absolute paths, and raw
  private rationale. The READY protocol is isolated on stdout, diagnostics use
  stderr, server formatting disables ANSI, and the package verifier rejects VT
  control sequences in raw captured logs before field correlation.

## C10 evidence and limits

- Every API error has a stable code, safe static message, request ID, operation
  ID, retryability, Accepted-state classification, and recovery action. Body and
  response-header IDs must match. Once a Decision can have crossed the Synapse
  authority boundary, all subsequent failures require reconciliation and never
  claim that Accepted is unchanged or that a blind retry is safe.
- AI generation exposes phase, elapsed time, server-owned status, and explicit
  cancel. Queued cancel and active cancel converge on the same tombstone; HTTP
  disconnect is not semantic cancel; double cancel is idempotent; cancelled or
  timed-out late provider results cannot become a Proposal; retry requires a new
  attempt.
- Every claimed run also receives a process-local monotonic generation. After
  provider return, the handler rechecks cancellation and exact
  project/attempt/generation ownership under the Store lock before any
  ChangeSet, storage, or sidecar side effect; stale guards can neither complete
  nor release a later reuse of the same public attempt ID after terminal-status
  eviction.
- Keyboard/browser tests cover Target selection, context review, fake Proposal,
  Adopt/Reject/Defer, export, focus containment/return, iframe escape, visible
  focus, 320 CSS px, reduced motion, forced colors, and point/region semantic
  alternatives. Generated-LP accessibility checks remain advisory and do not
  establish WCAG conformance.
- The browser smoke fixture records navigation/response p95, DOM size, basic
  names/labels/landmarks/IDs, initial Tab focus, 320 CSS px overflow,
  reduced-motion rendering, and forced-colors rendering. Its result schema
  always preserves manual evidence as pending and explicitly denies WCAG,
  full-performance-profile, production, and release claims. Performance reference
  thresholds are advisory on an uncharacterized shared CI runner, as required by
  ADR-0011; deterministic functional timeouts remain failures.
- A separate deterministic runner generates exactly 500 regular files totaling
  50 MiB and an exact 10,000-element DOM at runtime. Its bounded scan and
  72-case overlay matrix cover three viewports, two DPRs, three content-zoom
  factors, two preview scales, two scroll cases, transforms, and nested scrolling;
  the 2 CSS px geometry limit is a hard gate. The result records exact
  OS/kernel/toolchain bindings and all 72 raw case identities, errors, and
  feedback durations; the verifier independently recomputes the maximum and
  p95. The 100 ms feedback reference is advisory on shared CI.
- The packaged production runner imports its fixture through the actual App,
  verifies the scoped Preview iframe, traverses `preview_target_runtime` and the
  bound bridge into the Target API, and measures the actual runtime overlay over
  72 cases: three configured widths × two DPRs × three content zoom factors ×
  two Preview scales × two scroll conditions. Height is recorded from runtime,
  not claimed as a configured viewport dimension. Maximum overlay error is a
  hard 2 CSS px gate; timing references remain environment-bound.
- The same production run measures display-name final input through durable
  PATCH/UI completion and invokes the packaged production ChangeSet parser and
  full bounded validator for exactly ten changed text files totaling 2 MiB.
  Raw cases/samples and recomputable p95 values are retained without project IDs,
  paths, prompt, file body, tokens, or provider content.
- Creator UX, manual screen-reader, cross-browser security/geometry, and the
  acknowledged external live-provider run remain pending. Cancellation for
  import/Preview/diff queues is also outside the implemented AI-attempt scope.

## C11 evidence and limits

- Versioned evidence/support templates remain fail closed: manual and external
  fields stay pending, license/brand ownership stays unresolved, and no merge,
  release, tag, product publication, production, distribution, or broad support
  claim is inferred from automated success.
- `pnpm check:evidence` validates all 348 requirement mappings, P0 evidence
  cells, checkpoint weights, manual pending gates, pinned profiles/result
  schemas, and contradictory-result rejection.
- The package verifier rejects a dirty source unless `--allow-dirty` is explicit,
  snapshots the exact tracked/nonignored source, requires Linux x86-64 GNU, and
  builds from that snapshot. It verifies launcher/start/health/Editor, separate
  Preview listener, graceful stop, same-root restart, browser/accessibility
  smoke, synthetic corpus, production integration performance, maximum-level
  safe logs, evidence JSON, dependency inventory, unresolved-license notice,
  per-file SHA-256 manifest, and retained `SHA256SUMS` outside the repository.
- This is a local evaluation package, not a redistribution or release artifact.

## Remaining Human and external gates

- Record the Creator 80% UX/keyboard scenario and a manual screen-reader flow.
- Run the separately acknowledged live-provider test only with explicit
  credential, network, billing, and data-transfer consent.
- Assign and resolve license/brand/support owners and distribution terms.
- Review the draft completion PR, choose merge or rejection, run post-merge
  verification if merged, and make a separate explicit release/tag decision.
