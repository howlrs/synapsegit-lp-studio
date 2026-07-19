# ADR-0007: Target v1 capture and fail-closed resolution

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

LP requests must identify a page, semantic block, element, text range, point,
or region without treating transient DOM identity or raw coordinates as durable
authority. Accepted and Proposed previews can render different bytes, and a
Target selected from a terminal or stale Proposal must not silently bind to the
current Accepted revision.

The Preview executes untrusted site code on a separate scoped origin. Its DOM
observations therefore require exact message binding and server validation
before persistence or AI-context use.

## Decision

- Use a strict `TargetV1` discriminated union with six kinds. Every Target has
  an application-generated opaque ID, page, capture revision/source, bounded
  label, viewport, document epoch, and kind-specific evidence.
- Bind a Proposed Target to both the Proposal ID and its current Accepted base.
  The Proposal must still be pending when the Target is resolved or used.
- Keep `runtimeNodeHandle` only in the current iframe's bounded structure
  message. It is excluded from Target schema, disk, AI context, SynapseGit, and
  export.
- Persist each validated Target as immutable canonical JSON under the managed
  project `targets/` directory. Startup accepts only canonical, bounded,
  unique records and fails closed on drift.
- Treat coordinates as CSS-pixel evidence and explanatory hints. Record
  document and viewport rectangles, clipped normalized rectangles, scroll,
  DPR, visual viewport scale, preview scale, and layout epoch. Point and region
  capture is clamped; a region is at least eight CSS pixels.
- Extract bounded block candidates from semantic elements, landmarks, and
  visible body/main children. Canvas and tree captures produce the same
  persistent Target contract; source files are never annotated with Studio
  identity.
- Version the resolver independently. A resolution is
  `resolved`, `ambiguous`, or `detached`, includes bounded candidate reasons,
  and receives a server-derived `resolutionId`. Context creation recomputes
  the resolution and requires the exact current receipt.

## Resolver v1 baseline

The current static-site fixture resolver uses conservative evidence:

- page existence resolves a page Target at `1.0`;
- one matching source ID plus matching tag/semantic fingerprint resolves at
  `0.92`;
- the same evidence plus an exact text quote resolves at `0.98`;
- duplicate source IDs remain ambiguous at `0.72` per bounded candidate;
- text or accessible-name evidence alone remains ambiguous at `0.65`;
- no usable non-coordinate evidence is detached.

Thus an automatic result requires at least the specified `0.85` baseline and
two independent signals; geometry, class, or text alone never resolves across
a revision. Candidate scores remain diagnostic data. The UI presents status
and reason categories without percentage precision and blocks AI context until
the result is resolved.

## Canonical SynapseGit context

SynapseGit's canonical boundary intentionally rejects JSON fraction tokens.
Target responses and private Target storage retain numeric JSON, while the
reviewed SynapseGit context recursively projects fractions to deterministic
decimal strings and declares
`numericEncoding: serde-json-shortest-decimal-string-v1`. Integer tokens remain
integers and negative zero becomes `"0"`.

This application-local bridge is tracked upstream in
[SynapseGit Issue #29](https://github.com/howlrs/synapsegit/issues/29). A future
public normalized fixed-point helper may replace it only through a versioned
context migration.

## Consequences and deferred work

- Ambiguous and detached Targets remain immutable evidence but cannot generate
  a context or Proposal.
- Switching Preview source invalidates the active Target. Delayed messages and
  superseded Target responses cannot replace the current selection.
- Text offsets are UTF-16 code-unit offsets only for one unchanged DOM text
  node. When extraction/redaction changes the quote, offsets are omitted.
- C5 includes a dynamic bounded tree, keyboard node capture, pointer point and
  region capture, persistence/restart validation, and six-kind browser flow.
- Reference Targets, user-defined block labels, breadcrumb polish, restored
  Target history UI, candidate overlays, and the general multi-signal resolver
  corpus remain later M1 acceptance/hardening work. They do not weaken the
  current fail-closed result.
