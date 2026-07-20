# ADR-0001: Local application architecture and toolchains

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

LP Studio executes untrusted imported/AI-generated HTML and JavaScript while
holding local filesystem, AI credential, revision, export, and SynapseGit
authority. The UI needs rapid TypeScript iteration, while filesystem and
SynapseGit operations benefit from the same audited Rust boundary.

The initial repository has no implementation scaffold. The available
environment has Node, pnpm, Bun, and Rust; SynapseGit v0.3.0 is a Rust
workspace and documents Rust 1.88 as its source-build baseline.

## Decision

- Use a pnpm workspace for Web/TypeScript packages and a Cargo workspace for
  the local authority.
- Implement `apps/web` as a strict TypeScript React/Vite SPA.
- Implement `apps/local-server` as a Rust Axum application.
- Bind separate random IPv4-loopback listeners for native Editor/API and active
  Preview sessions. The explicit Docker profile is governed separately by
  [ADR-0013](0013-docker-local-build-profile.md).
- Store mutable Studio metadata/command state in SQLite and immutable revision
  bytes in an application-owned filesystem CAS.
- Keep SynapseGit integration behind a trusted Rust use-case/companion
  contract; never expose raw Core or SQLite primitives to the browser.
- Define browser/server contracts as versioned canonical JSON Schema and
  generate or parity-test both TypeScript and Rust representations.
- Use deterministic fake AI in ordinary CI and add the first live provider
  behind the same server-side adapter.
- Use Playwright with pinned Chromium for geometry/security E2E and native
  Rust/TypeScript unit/integration tests.
- Do not add Electron, Tauri, a reverse proxy, or a hosted server to M1.

## Alternatives considered

### TypeScript/Bun filesystem server

This can be made secure with the same separate-origin and path controls and is
not rejected as inherently unsafe. It was not selected because it would still
need a separate trusted Rust boundary for SynapseGit, splitting journal,
filesystem mapping, and Decision recovery across two authorities.

### Rust-rendered UI

This would simplify the language boundary but makes iframe overlay,
conversation, responsive Preview, and component testing more costly than a
TypeScript SPA.

### Desktop shell

Electron/Tauri could provide native pickers and packaging, but expands the
privilege and distribution surface before the browser/local-server contract is
proven.

## Consequences

- Cross-language schema generation/parity is a C1 gate, not optional cleanup.
- Rust build time is accepted in exchange for one filesystem/Synapse authority.
- Web assets may be served by Axum in local builds; Vite remains the developer
  server with an exact configured Editor origin.
- Toolchain and dependency versions are pinned in lock/toolchain files during
  scaffold.
