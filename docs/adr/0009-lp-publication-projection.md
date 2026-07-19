# ADR-0009: LP publication projection and local GitHub-ready review

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

LP Studio needs a privacy-filtered account of an AI-attributed Proposal and
Human Decision that a Creator can inspect before sharing. That record is not
the deployable LP, a SynapseGit Core archive, or a GitHub remote operation.
Treating any of those artifacts as interchangeable would either disclose site
or authority data or imply a publication that never happened.

SynapseGit v0.3.0 provides a deterministic provider-neutral publication layer,
but its discovery and semantic roles are tied to the three-image CreatorReport
workflow. The pinned generic-artifact v1 contract supplies the LP Proposal and
Decision receipt boundary; it does not make the image-oriented renderer a
valid generic LP history projection.

The LP-specific projection must therefore consume only reviewed public-safe
facts from the local Decision/reconciliation boundary. It must not accept a
full local Project, Target, provider request, Synapse repository handle, or
filesystem path and attempt to redact those after the fact.

## Decision

- Define `org.synapsegit-lp-studio.publication` version 1 as the
  provider-neutral semantic source. Generate target-facing views only after
  that projection has been validated and serialized.
- Limit generator input to a public project label/title/summary, Accepted
  revision and manifest binding, pinned generic-artifact binding, bounded
  caller-supplied attribution labels, completeness, supported Human
  disposition, receipt checksums, and a separately authored public Decision
  note.
- Make raw LP bytes, full prompt, raw provider response, private rationale,
  credential, internal Actor/authority value, repository path, and full
  Target/DOM quote absent from the input type. Do not depend on keyword
  filtering to remove them after ingestion.
- Mark public title, summary, label, and public Decision note as
  `author_supplied`. Keep those fields distinct from the source-private
  rationale; no default or convenience action copies private rationale into a
  public note.
- Distinguish `verified_from_synapse`, `observed_from_synapse`,
  `derived_summary`, and `author_supplied` values. The current pinned contract
  reports `caller_supplied_ai_attributed` and `execution_verified=false`; the
  publication must not strengthen that claim.
- Represent complete and incomplete histories explicitly. An incomplete or
  outcome-unknown history has no Human disposition and carries a visible
  limitation; absence must never be rendered as rejection or as no change to
  Accepted.
- Always state that identifiers/checksums can correlate copies, checksums are
  not signatures or authorship proof, and byte/graph identity does not prove
  truth, rights, semantic or visual correctness, or physical change.
- Omit raw assets and thumbnails in v1. Publishing an asset derivative is a
  separate opt-in pipeline requiring decode isolation, metadata removal,
  pixel/byte limits, and rights confirmation.
- Generate exactly these provider-neutral local review files:
  `projection.json`, `story.md`, script-free `index.html`, `manifest.json`,
  and `checksums.json`. Store them in lexical `BTreeMap` order.
- Treat compact UTF-8 `projection.json` as the semantic source. Struct field
  order is fixed, all maps use lexical order, and output contains no current
  time, random identifier, local path, or environment-dependent presentation
  data. Identical input must produce identical bytes.
- Escape all author-supplied text independently for Markdown and HTML. The HTML
  renderer has no JavaScript, form, external resource, or remote action.
- Let `manifest.json` bind schema/generator/renderer versions, fixed inventory,
  target profile, and the exact `projection.json` identity. Let
  `checksums.json` cover every other bundle file. It intentionally does not
  hash itself, avoiding a circular identity claim.
- Label the target `github_ready_local` and visibility `private_review`.
  `GitHub-ready filesを生成` means local deterministic file generation only.
  It does not run Git, `gh`, a GitHub API, upload, deploy, issue creation,
  commit, push, pull request, release, or any network operation.
- Keep any future `GitHubへ公開` operation outside this generator. Such an
  adapter requires a separate contract, exact destination and diff review,
  explicit Human confirmation, idempotency/partial-outcome handling, receipt,
  and privacy revalidation.

## Verification profile

- Golden tests compare all output bytes from repeated identical input.
- Complete and incomplete fixtures assert that disposition and completeness
  claims cannot be confused.
- Privacy canaries cover every structurally omitted private field category.
- Manifest/checksum tests recompute every covered file identity and verify the
  fixed inventory and projection binding.
- Renderer tests use Markdown/HTML injection text and verify that no active
  script or external-resource element is produced.
- Production integration tests additionally prove that publication generation
  leaves Git state, Synapse state, Accepted files, and external network traffic
  unchanged.

## Consequences

- LP Studio can offer an exact-byte local publication review without claiming
  remote publication or using the incompatible image CreatorReport shape.
- A Creator may export the Accepted LP without generating or sharing its
  history. Static export and publication review remain independent actions.
- The v1 record explains provenance and Decision scope but cannot display the
  LP itself because raw assets are deliberately absent.
- A future upstream generic LP renderer may replace the local renderer only
  through a new reviewed contract/version and byte-compatibility migration; it
  is not inferred from the existing `synapse-present` capability.
