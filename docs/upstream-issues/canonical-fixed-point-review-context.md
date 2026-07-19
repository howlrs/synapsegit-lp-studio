# generic-artifact: expose a canonical-safe fixed-point review-context helper

## Context

A sibling local LP Studio calls `synapse_artifact::review_context_sha256` and
`begin_artifact_proposal` with a reviewed `TargetV1` context containing CSS
pixel geometry and resolver confidence.

`serde_json` serializes those values as JSON fractions such as `1.0` and
`0.92`. `synapse-canonical` intentionally rejects fraction, exponent, and
negative-zero tokens with `NumberTokenForbidden`. The generic-artifact v1
contract correctly requires the application-owned review context to cross
that strict canonical boundary.

SynapseGit documents the `ScaledInteger` representation, but its validator
representation is private and the generic-artifact API exposes no constructor
or projector for arbitrary application measurements.

## Minimal reproduction

```rust
let context = br#"{"geometry":{"cssWidth":1440.0},"confidence":0.92}"#;
synapse_artifact::review_context_sha256(context)?;
```

The call returns `NumberTokenForbidden` for the first fractional token. The
same failure occurs before `begin_artifact_proposal` can admit a proposal.

## Observed impact

- Geometry-bearing application workflows fail late at the canonical boundary.
- Each consumer must invent decimal-string or `mantissa`/`scale`/`unit`
  conversion and normalization.
- Equivalent values can therefore acquire incompatible reviewed-context
  digests across sibling applications.

This is an ergonomics and interoperability request. It is not a request to
allow floating-point tokens in the canonical profile.

## Requested improvement

1. Expose a small public deterministic helper or type for converting a finite
   application decimal to a canonical-safe normalized `ScaledInteger`.
2. Document the recommended generic-artifact review-context representation,
   with Rust and JSON examples for CSS pixels, normalized ratios, and
   confidence scores.
3. Reject non-finite, excess-precision, non-normalized, and out-of-range values
   during preflight, before Proposal repository mutation.
4. Add golden vectors demonstrating stable `review_context_sha256` results for
   equivalent values.
5. Preserve the current strict rejection of raw fractional, exponent, and
   negative-zero number tokens.

## Acceptance criteria

- A consumer can construct and hash a geometry-bearing reviewed context
  without hand-written fixed-point normalization.
- Invalid measurement values fail with a specific preflight error before
  repository mutation.
- The public representation and normalization rules are covered by frozen
  cross-consumer digest vectors.

## Consumer evidence

Discovered while implementing C5 Target v1 and resolver support in
`howlrs/synapsegit-lp-studio`. Its Rust integration tests reproduce the error
with both resolver scores and CSS-pixel geometry.
