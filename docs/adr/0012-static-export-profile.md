# ADR-0012: Accepted static export and external-asset profile

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

The M1 export must produce an ordinary static site from one immutable Accepted
revision without mixing in a pending Proposal or Studio/SynapseGit metadata.
Static LPs can contain nested local references, external URLs, and JavaScript
whose dynamic references cannot be exhaustively inspected. The product must
state that validation boundary instead of presenting a deterministic ZIP as
proof that a site is offline-complete or production-safe.

Directory export also carries overwrite and path-authority risk. The existing
browser download path can avoid that risk entirely: the local server creates a
new downloadable archive and never accepts a browser-supplied server
destination path.

## Decision

- Fix each export operation to the exact immutable Accepted revision and its
  canonical allow-listed site manifest at operation start. Revalidate the
  materialized Accepted view before generation and never read Proposal,
  conversation, Target, credential, provider, Preview bridge, session, or
  Synapse repository data as export input.
- Use a downloadable ZIP as the M1 default and only implemented destination
  profile. Browser download placement is controlled by the user agent; LP
  Studio does not overwrite or delete an existing server-side directory.
  Requirements for existing-destination diff and overwrite confirmation are
  therefore not applicable to this profile.
- Do not add an arbitrary server-path field to the browser API. If directory
  export is added later, limit it to a startup-registered export root,
  application-managed export root, or native bounded directory handle. It must
  use no-follow inspection, show the exact destination/existing diff/overwrite
  scope, bind that review into one confirmation, stage and validate all files,
  publish atomically or recoverably, and never delete an unknown file.
- Version and record these options in every receipt:
  - entry point: exact Accepted project entry point;
  - base-path profile: `relative_static_http`;
  - external-asset policy: `report`;
  - artifact format: `zip_stored_v1`.
- Copy only canonical manifest entries. Reject symlinks, non-regular entries,
  root escape, traversal, ambiguous path encodings, and files not present in
  the immutable Accepted manifest.
- Validate bounded local references in HTML/SVG/XML `src`, `href`, `poster`,
  `srcset`/`imagesrcset`, `ping`, `background`, `manifest`, object `data`,
  refresh destinations, SVG presentation attributes, style attributes, and raw
  `style` elements, plus CSS `url()` and `@import`, including percent-decoding
  and nested relative resolution. Missing or unsafe local references are
  errors. Embedded `srcdoc`, character references in URL-sensitive markup
  attributes, raw/RCDATA/script tokenizer ambiguity, processing instructions,
  legacy fetch/base attributes, `image-set()`, and CSS syntax that the bounded
  scanner cannot classify force an incomplete-analysis warning and prevent the
  stronger offline-self-contained classification.
- Report every detected external HTTP(S) or protocol-relative destination.
  Classify the result as `standalone_static` when external dependencies remain
  and as `offline_self_contained` only when the bounded validator finds none.
- Report that JavaScript-built URLs, runtime fetches, and other dynamic
  references are not exhaustively inspected. This limitation remains visible
  even when all statically discoverable references resolve.
- Require ordinary unprivileged static HTTP hosting for the MVP support
  profile. Direct `file://` use is not guaranteed for modules, fetch, routing,
  or browser security behavior.
- Keep ZIP identity deterministic: lexical entry order, fixed timestamp, fixed
  regular-file permission, fixed stored-compression profile, and exact source
  bytes. Filesystem enumeration order and mtime are not identity inputs.
- Persist the receipt outside the site. It includes source revision, all export
  options, generated lexical file manifest, archive checksum/byte length,
  warnings, server-generated UTC time, and application version. Repeated
  receipts let the UI compare checksums for the same source/options; time and
  receipt ID are not archive identity inputs.
- Provide or document an unprivileged local static-server command for opening
  the extracted export. The helper serves only the selected export directory
  and has no LP Studio API, credential, Synapse, import, or write authority.
- Export generation and download do not deploy, host publicly, commit, push,
  create a GitHub record, or grant distribution/production permission.

## Verification profile

- Unit/property tests cover nested paths, encoded traversal, query/fragment
  handling, `srcset`, CSS URLs/imports, external URL reporting, missing files,
  dynamic-reference limitations, and canonical ordering.
- ZIP tests compare repeated complete bytes and assert fixed timestamps,
  permissions, compression, entry order, and source bytes.
- Privacy fixtures seed Studio metadata, prompt, Target, session, absolute path,
  Preview bridge, provider and Synapse canaries outside the site manifest and
  prove their structural absence from both archive and generated manifest.
- Browser E2E downloads and verifies the archive, extracts it into a temporary
  bounded directory, serves it with an unprivileged static HTTP server, and
  confirms the entry point and local assets load without missing-file errors.
- Drift, stale revision, unsafe path, validation failure, and capacity failure
  leave Accepted bytes and prior persisted receipts unchanged.

## Consequences

- M1 has one narrow, deterministic export profile with no server destination
  overwrite surface.
- Sites with external dependencies remain exportable only with an explicit
  `standalone_static` report; they are not described as offline-complete.
- Framework source projects, build execution, deploy adapters, file-URL
  compatibility, and confirmed directory overwrite remain separate future
  profiles rather than implicit behavior of the ZIP endpoint.
