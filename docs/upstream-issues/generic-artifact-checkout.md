# Verified generic artifact site checkout

Posted as [SynapseGit Issue #27](https://github.com/howlrs/synapsegit/issues/27).

## Integration finding

The generic contract records a manifest digest and selected snapshot, but does
not expose a trusted bounded read/checkout API for only the selected `site`
Tree. The publication API intentionally omits raw assets, while repository
archive export includes the whole Synapse repository.

Keeping host input bytes is sufficient for the same-process C2 evaluation
slice. It is not sufficient after restart to prove that Preview/export bytes
are the exact verified Tree selected by the Decision.

## Requested boundary

- Resolve a server-owned project and exact Decision outcome from one coherent
  read-only snapshot.
- Validate generic lineage, disposition, and selected snapshot.
- Traverse only the `site` Tree using verified Tree/Blob reads and portable NFC
  regular-file paths.
- Enforce file, byte, depth, and path limits while streaming.
- Return a canonical manifest digest matching the reviewed Decision digest.
- Exclude control Tree data and redact paths, Refs, OIDs, credentials, private
  rationale, and file contents from public errors.

LP Studio C8 export must remain based on its immutable Accepted manifest until
#27 supplies Synapse-backed checkout truth; it must state that limitation and
must not claim a host-retained copy was checked out from SynapseGit.
