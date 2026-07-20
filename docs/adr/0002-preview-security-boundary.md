# ADR-0002: Separate-origin Preview security boundary

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

Preview content is untrusted active content. A same-origin iframe could read
Editor state or invoke local privileged APIs. A single Preview origin reused
across projects could also leak localStorage, IndexedDB, Cache Storage,
BroadcastChannel, service worker, or `window.name` state.

At the same time, imported sites may require JavaScript, modules, CSSOM access,
and relative local assets.

## Decision

- The Editor/API listener and Preview listener use different origins.
- A process-secret-derived 128-bit opaque host label creates an ephemeral
  origin for every active session/project/snapshot on the Preview listener.
  Closing/switching the session revokes the route; restart rotates the process
  secret and therefore the entire origin namespace.
- Preview access requires an exact scoped Host plus the bound
  project/snapshot route. Invalid, expired, foreign, stale, and terminal
  Proposal requests use the same not-found response.
- Preview origin exposes only bounded static revision files and the ephemeral
  bridge. It has no privileged API routes.
- Editor session credentials remain in Editor memory and are sent only to the
  exact Editor/API origin in an explicit authorization header.
- The iframe disables top navigation, popup, download, and form submission.
- Start with the narrowest sandbox compatible with the fixture corpus.
  `allow-scripts allow-same-origin` may be enabled only on a distinct
  ephemeral Preview origin with no privileged routes and only if module/site
  compatibility tests require it. It is never used on the Editor origin.
- Default CSP blocks connect/form/external resources. Project-level external
  hosts require explicit capability review and are reported in export.
- Preview/editor messages use a versioned MessageChannel, expected origin, and
  source checks. Bridge data remains untrusted because page scripts may forge it.
- Preview instrumentation is response-only and never enters site revisions or
  exports.
- The response-first bridge clears `window.name`, blocks direct document
  replacement, and uses the Navigation API with captured native intrinsics to
  reject destinations outside the exact project prefix. Same-project relative
  navigation remains available.
- Privacy-safe diagnostics contain only a bounded code, severity, scope
  binding, and `sourceUnavailable`; raw URLs, stack traces, prompts, source
  paths, and credentials are excluded.
- The native launcher allocates available ports dynamically and prints the
  exact Editor URL; fixed port availability is not assumed. The Docker profile
  uses fixed, identical host/container ports under
  [ADR-0013](0013-docker-local-build-profile.md) because the scoped Preview
  origin and CSP bind that external port.

## Consequences

- Browser security tests must cover cross-project storage canaries, service
  worker registration, forged messages, fetch/CSRF, popup/navigation, and
  Editor token access.
- Some imported sites will be incompatible with the default CSP/sandbox.
  The UI reports blocked capabilities instead of silently weakening isolation.
- Active Preview fails closed when the Navigation API is unavailable. M1
  browser evidence is Chromium-only; broader compatibility belongs to C10.
- CSP `webrtc 'block'` and a same-realm constructor guard provide defense in
  depth. They do not claim complete containment of arbitrary hostile code in
  every browser realm; additional runtime and fault hardening belongs to C9.
- Response-first injection before a non-canonical/legacy doctype may change
  quirks-mode behavior. Explicit Preview port 80 is unsupported because URL
  canonicalization omits the port; dynamically allocated high ports are the
  tested configuration.
- Geometry conversion records iframe offset, scroll, CSS pixels, preview
  scale, visual viewport scale, and DPR.

## C4 verification

Rust and Web contract tests cover origin derivation, Host/route binding,
expiry/revocation, uniform errors, security headers, source/export purity, and
strict message validation. Three Chromium E2E flows cover cross-project and
restart storage canaries, service workers, Editor API reachability,
popup/navigation/document/WebRTC denial, forged messages, same-project
navigation, diagnostics, and normal LP script execution.
