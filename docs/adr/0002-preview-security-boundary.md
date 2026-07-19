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
- Each active project/Preview session receives an ephemeral random loopback
  port or an equivalently isolated origin. Closing/switching the session
  destroys the listener and storage scope.
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
- The launcher allocates available ports dynamically and prints the exact
  Editor URL; fixed port availability is not assumed.

## Consequences

- Browser security tests must cover cross-project storage canaries, service
  worker registration, forged messages, fetch/CSRF, popup/navigation, and
  Editor token access.
- Some imported sites will be incompatible with the default CSP/sandbox.
  The UI reports blocked capabilities instead of silently weakening isolation.
- Geometry conversion records iframe offset, scroll, CSS pixels, preview
  scale, visual viewport scale, and DPR.
