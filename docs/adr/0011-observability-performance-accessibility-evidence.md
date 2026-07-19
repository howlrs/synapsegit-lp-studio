# ADR-0011: Safe observability and acceptance-evidence profile

Status: automated local measurements implemented; manual/external gates pending

Date: 2026-07-19

## Context

Feature tests alone do not establish the C10 acceptance criteria. LP Studio must
show that its supported local environment remains diagnosable, responsive,
geometrically accurate, keyboard accessible, and privacy preserving. Evidence
must be reproducible without turning prompts, site bytes, credentials, private
rationales, provider responses, permits, or local paths into logs or CI
artifacts.

The M1 support scope is deliberately narrow. A passing Chromium test does not
prove support for Firefox, Safari, another operating system, production
deployment, binary redistribution, or a hosted service.

## Decision

### Structured observability

Server operations use a generated operation/correlation ID that is independent
from browser authority. Safe structured events may contain:

- application, schema, adapter, and contract version;
- operation kind and phase;
- privacy-safe error code and retry/outcome classification;
- monotonic duration and bounded counters;
- opaque local record IDs only when required for same-root diagnosis; and
- a one-way correlation value for the corresponding Synapse adapter operation.

Default logs never contain a session or approval token, Actor/Policy/Grant or
permit, repository or filesystem path, environment value, prompt, rationale,
file body, Target quote, provider request body, raw provider response, or
publication note. Debug formatting follows the same exclusion. Log-level
selection cannot weaken this structural omission. The readiness protocol is
the only stdout record; structured diagnostic logs use stderr. The formatter
disables ANSI control sequences so retained fields are machine-parseable
without a display normalization step; evidence rejects any raw capture
containing a VT sequence.

UI diagnostics show local Proposal, Decision, export, publication, and recovery
status plus a safe last-error code. A diagnostic package is generated only by a
separate explicit action after exact file/field and redaction preview. Telemetry
is absent and disabled by default; adding it requires a new destination, field,
retention, opt-in, and disablement decision.

### Pinned performance and geometry profile

The versioned baseline fixture contains:

- 500 regular static-site files and 50 MiB total bytes;
- 10,000 visible and hidden DOM nodes;
- three responsive breakpoints;
- transformed elements and nested scroll containers; and
- deterministic targets for page, block, element, text, point, and region.

The evidence runner records the exact OS, architecture, browser build,
Playwright version, Node/pnpm/Rust versions, fixture digest, warm-up count,
sample count, and raw privacy-safe measurements. It measures rather than
inferring:

- hover/selection feedback p95 at or below 100 ms;
- overlay-to-element error at or below 2 CSS px across the supported viewport,
  zoom, DPR, transform, and nested-scroll matrix;
- metadata autosave completion p95 at or below 1 second after the final input;
- non-provider ChangeSet validation p95 at or below 2 seconds for ten changed
  text files and 2 MiB total; and
- bounded cancellation and failure behavior for import, Preview, diff, and
  operation queues.

Pointer-driven overlay work is coalesced at most once per animation frame. A
performance threshold is not made a hard gate on an uncharacterized shared CI
runner. The repository retains the measurement profile and a result from the
declared support environment; ordinary CI keeps deterministic functional and
regression checks.

### Accessibility evidence

Automated checks cover the home, Editor, import review, exact-context review,
Proposal review, recovery, and error states. Manual or browser-driven evidence
also covers:

- keyboard-only Target selection, generation, review, all dispositions, and
  export;
- focus entry, containment where appropriate, return, visible focus, landmarks,
  names, and iframe escape;
- polite/assertive announcements for Target, Proposal, validation, stale state,
  Decision, and recovery;
- non-color warning and status distinctions;
- 200% zoom, 320 CSS px width, reduced motion, and forced/high contrast; and
- point/region keyboard alternatives using semantic context.

The generated LP receives an automated accessibility report in the review
surface. The report names the tool and limitations and is never described as
proof of complete WCAG conformance. A screen-reader manual flow remains a P1
release gate unless the completion scope is explicitly expanded beyond the
local M1 evaluation.

### Evidence records and support claims

Versioned templates under `docs/evidence/` define the minimum support matrix and
M1 evidence manifest. A template, empty field, planned test, or successful build
is not passing evidence. A completed record binds:

- source revision and clean-worktree state;
- requirement and checkpoint IDs;
- exact commands, fixture digests, result digests, and CI run links;
- manual evidence actor/date/scope without secret input;
- unresolved P0 findings and claim mismatches; and
- license, brand, distribution, and support status without inferring permission.

The only support claim allowed by the initial profile is an exactly evidenced
Linux x86-64 GNU/WSL and pinned Chromium local evaluation combination. Other
platforms are explicitly unverified. Navigation API feature detection must fail
closed when the Preview boundary cannot be established.

## Required evidence

The automated local C10/C11 evidence record must reference:

- the pinned performance/geometry result and baseline fixture digest;
- automated accessibility and keyboard/zoom/contrast results;
- the supported browser security matrix;
- safe-log and privacy-canary scans;
- deterministic fake-provider integrated E2E;
- a clean temporary-package build, checksum, start, health, and stop smoke test.

Creator UX, screen-reader, and separately acknowledged live-provider evidence
remain explicit pending fields. They are not synthesized from browser
automation and must pass before a corresponding product/release claim.

Every P0 requirement needs an automated result or a recorded manual result.
P1 deferrals remain visible and are not silently converted to completed P0
evidence.

## Consequences

- Observability fields are allow-listed; redaction is defense in depth rather
  than permission to log arbitrary input.
- Performance evidence is comparable only when environment and fixture bindings
  match.
- Browser support is evidence-driven and may initially contain one exact
  Chromium/Linux combination.
- License and brand fields may truthfully remain unresolved for local M1, but
  their owner and external-release gate must be explicit before final evidence.
- The checked-in runner and templates complete the automated local evidence
  contract only. They do not complete the pending Human/external gates, a
  release, production readiness, distribution, or legal permission.
