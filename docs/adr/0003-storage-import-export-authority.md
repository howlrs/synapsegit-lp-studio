# ADR-0003: Storage, import, and export authority

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

The application needs immutable Accepted/Proposal revisions, fast autosave,
crash recovery, bounded import, deterministic export, and external-change
detection without editing a Creator's source directory in place.

## Decision

- Copy every imported static site into an application-managed project root.
- Keep the source directory unchanged and never use it as the Accepted view.
- Use SQLite for project registry, schema version, conversation metadata,
  Proposal/Decision status, idempotency keys, and the command journal.
- Use application-owned content-addressed files for immutable revision bytes.
- Materialize `site/` from the Accepted manifest; its pointer changes only
  through a committed command-journal transition.
- Compute a canonical manifest from normalized relative path, media type,
  byte length, and SHA-256. Do not include mtime in identity.
- Reject symlink, device, FIFO, socket, traversal, case-fold collision,
  reserved name, and root-escape input.
- Treat archive entries as untrusted and bound expanded count/bytes,
  compression ratio, depth, duplicates, and path normalization.
- Detect external changes to materialized `site/` before Proposal, Decision,
  and export. Import them only through an explicit human-authored revision.
- Export only an immutable Accepted manifest.
- Limit directory export to a startup-registered root, managed export root, or
  native/user-selected bounded handle. Otherwise return a downloadable archive.
- Stage and validate export before no-replace/confirmed publication; never
  delete unknown destination files.

## Initial limits

The detailed requirements' 1,000-file, 200 MiB total, 20 MiB single-file,
20-operation, and 5 MiB changed-text values are initial configurable defaults.
They remain server-owned and are finalized against fixtures before C3.

## Consequences

- SQLite backup/migration and filesystem CAS reconciliation are explicit tests.
- Studio DB, SynapseGit Ref DB, Git repository, and static export are distinct.
- Direct `file://` execution is not guaranteed; ordinary unprivileged static
  HTTP serving is the M1 profile.
