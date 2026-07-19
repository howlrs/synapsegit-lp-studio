# M1 evidence records

This directory contains fail-closed templates, deterministic smoke profiles,
and result schemas. A template or schema is not passing evidence.

## Files

- `m1-evidence-manifest.template.v1.json`: C0–C11 evidence manifest template.
- `m1-support-matrix.template.v1.json`: exact local support candidate template.
- `fixtures/m1-package-browser-smoke-profile.v1.json`: small packaged-Editor
  performance, responsive, and accessibility smoke profile.
- `schemas/m1-package-browser-smoke-result.schema.v1.json`: strict result shape
  for that smoke profile.
- `fixtures/m1-performance-geometry-profile.v1.json`: runtime-generated 500-file,
  50 MiB, 10,000-node synthetic corpus and 72-case geometry matrix.
- `schemas/m1-performance-geometry-result.schema.v1.json`: strict result shape
  for the bounded corpus scan, geometry hard gate, and advisory timing reference.
- `fixtures/m1-production-integration-performance-profile.v1.json`: packaged
  App, scoped Preview, production bridge/overlay, Project metadata autosave, and
  exact 10-file/2 MiB ChangeSet production-path profile.
- `schemas/m1-production-integration-performance-result.schema.v1.json`:
  strict privacy-safe result for all 72 raw production geometry cases, overlay
  feedback, autosave, and ChangeSet samples with recomputable p95 values.
- `fixtures/m1-safe-log-profile.v1.json`: packaged fake-provider workflow,
  maximum supported log level, required safe fields, adapter phases, privacy
  canary classes, and still-pending external gates.
- `schemas/m1-safe-log-result.schema.v1.json`: strict digest-only result for the
  packaged stdout/stderr privacy scan and LP Studio/SynapseGit adapter
  correlation check.
- `schemas/m1-automated-evidence-record.schema.v1.json`: extensible strict
  record binding actual automated results to the evaluated source, normalized
  command argv, profile/result digests, requirement IDs, and pending gates.

The package profile deliberately keeps Creator, keyboard/zoom/contrast,
screen-reader, and live-provider evidence pending. The synthetic corpus measures
the exact file/byte/node bounds and overlay projection across the pinned
viewport, DPR, content-zoom, preview-scale, transform, and nested-scroll matrix.
Its result binds the OS release digest, kernel, architecture, Node, pnpm, Rust,
Playwright, and Chromium versions and retains all 72 raw case identities, errors,
and feedback durations so the maximum and p95 can be recomputed. It does not
measure LP Studio application integration, autosave, or ChangeSet latency.
Those production paths are measured separately by the packaged production
integration profile: the actual App imports a fixture, renders it in the scoped
Preview iframe, traverses `preview_target_runtime` and the bound bridge into the
Target API, and renders the actual runtime overlay across a separate 72-case
production matrix of three configured and distinct rendered Preview widths, two
DPRs, three content zooms, two Preview scales, and two scroll states. Runtime
iframe width and height are retained as measured values. Each rendered outer
frame must equal its configured width, each iframe must be within the outer
frame's two-pixel border allowance, and each bridge-reported runtime width must
equal the iframe `clientWidth`. The production profile makes no unsupported
height-control claim. The same run measures final
display-name input through durable API/UI completion and invokes the packaged
production ChangeSet parser for exactly ten text files totaling 2 MiB.
Cancellation, manual accessibility, and cross-browser behavior remain outside
these profiles. None of these profiles claims WCAG conformance, full
application performance, production readiness, a release, distribution
permission, or platform support. Performance reference thresholds are advisory
on an uncharacterized shared CI runner; bounded scans, geometry accuracy,
functional timeouts, and basic accessibility/responsive invariants remain hard
failures.

## Validation

~~~bash
pnpm check:evidence
~~~

This checks the template boundaries, C0–C11 weights, all generated requirement
mappings, all P0 planned-evidence cells, manual pending states, all pinned
profiles and result schemas, and contradictory-result rejection. It does not
turn planned evidence into a passing result.

The full synthetic corpus can also be measured directly. It creates and removes
its 50 MiB fixture under the system temporary directory and emits one
schema-validated JSON result.

~~~bash
pnpm measure:performance-geometry
~~~

The package verifier emits a source-bound evidence JSON and runs the browser
profiles from an immutable source snapshot. It also starts the packaged binary
at the maximum supported `trace` level, copy-imports private canary-bearing site
bytes, runs the deterministic fake-provider Proposal and adopted Decision path,
and emits one intentional safe error. The scan requires operation/correlation
ID, error code, duration, version, and same-operation adapter phases. Prompt,
session and approval tokens, file body, private rationale, absolute path, raw
provider-response marker, and the authorization scheme must be absent from
captured stdout/stderr. Raw logs and raw canary values are discarded; only
counts and SHA-256 digests enter retained evidence.

The retained `LOCAL-EVALUATION-EVIDENCE.json` embeds an actual automated record
with individual result digests and requirement mappings. The production
performance result remains `partial_automated_evidence`, and that status does
not mean a mapped requirement is complete. Application autosave and ChangeSet
application latency are recorded as measured; bounded application
cancellation remains `pending_unmeasured`; Creator, live-provider, manual
accessibility, license, brand, and release gates also remain pending. A clean
tree is the default; a dirty candidate requires the explicit `--allow-dirty`
flag and is recorded as `explicit_dirty_tree`. To retain the package and
SHA-256 records, choose a non-existing directory outside the repository whose
parent already exists.

~~~bash
mkdir -p /tmp/lp-studio-evidence
pnpm verify:package -- \
  --allow-dirty \
  --output-dir /tmp/lp-studio-evidence/candidate
~~~

Do not commit generated evidence containing private paths or input data. The
automated result is designed to contain only versions, counters, timings,
digests, static status codes, and explicit claim boundaries.

CI retains the package, evidence JSON, and checksum file only after the clean
package verifier and checksum verification both succeed. The retained artifact
remains scoped to local internal evaluation and does not imply distribution,
license, brand, production, release, or tag permission.
