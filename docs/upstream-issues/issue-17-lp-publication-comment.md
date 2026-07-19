LP Studio adds a concrete downstream case for the projection-first work in this
Issue.

The current v0.3.0 publication path correctly creates a local-only,
privacy-filtered, provider-neutral projection and Synapse/GitHub layouts with
no network write. Its discovery and public artifact roles are still tied to
the image-oriented CreatorReport workflow, so a generic static-site Proposal/
Decision history cannot yet produce the same reviewed bundle.

Proposed LP-specific follow-up acceptance:

- consume the versioned generic file-tree Proposal/Decision contract rather
  than `proposal/creator-agent/*` / `decision/creator/*` assumptions;
- include LP Studio/contract/schema versions, accepted site Tree/Commit
  binding, Proposal attribution, Human disposition, completeness, and
  limitations in one provider-neutral projection;
- accept only a separately reviewed/redacted public Target projection, never
  the full DOM text quote, prompt, raw provider response, private rationale,
  raw site assets, repository path, or internal Actor/authority values;
- retain the existing origin distinctions such as verified/observed/derived/
  author-supplied and the limits of byte/graph identity;
- generate deterministic JSON, escaped Markdown, script-free HTML, manifest,
  and checksums locally with zero Git/GitHub/network operations;
- keep GitHub-ready generation separate from any later commit/push adapter;
  and
- cover complete and incomplete generic LP sessions with frozen privacy
  canaries and exact verification tests.

I suggest keeping this under #17 unless the generic LP renderer/projection
becomes large enough for a separately acceptance-bounded implementation Issue.
