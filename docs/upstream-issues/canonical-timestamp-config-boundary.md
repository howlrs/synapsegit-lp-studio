# Canonical timestamp construction at the artifact config boundary

Posted as [SynapseGit Issue #28](https://github.com/howlrs/synapsegit/issues/28).

## Integration finding

SynapseGit intentionally requires canonical protocol timestamps in the exact
30-byte UTC form `YYYY-MM-DDTHH:mm:ss.nnnnnnnnnZ`. The generic artifact
`TrustedArtifactProjectConfig` accepts `recorded_at` and `grant_expires_at` as
arbitrary strings, but its local validation does not validate that lexical
form and the public artifact API exposes no canonical formatter or typed-time
constructor.

Rust `time`'s ordinary RFC 3339 formatter can trim trailing fractional zeroes.
LP Studio therefore intermittently reached a downstream `timestamp_invalid`
until it added an exact-width local formatter.

## Requested boundary

- Preserve SynapseGit's strict canonical protocol bytes.
- Expose a public canonical timestamp formatter, validated timestamp type, or
  typed-time config constructor so embedders do not duplicate the format.
- Reject a non-canonical raw string with a field-specific `InvalidArgument` at
  config validation rather than later in Proposal admission.
- Test exact-second and trailing-zero values and document the exact lexical
  requirement beside the public config API.

This is an ergonomics and early-validation request, not a request to accept
arbitrary RFC 3339 encodings or weaken canonical identity.
