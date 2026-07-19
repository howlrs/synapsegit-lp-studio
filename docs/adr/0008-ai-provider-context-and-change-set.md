# ADR-0008: Reviewed AI context, provider boundary, and ChangeSet v1

Status: accepted for M1 implementation

Date: 2026-07-19

## Context

An LP edit request crosses three different trust boundaries: Creator-reviewed
local content, an optional external model provider, and SynapseGit's
provenance record. Treating those as one payload would either disclose the full
prompt and Target to SynapseGit or let an unreviewed model response become site
bytes. Provider credentials and raw failures must also stay outside the
browser, project, logs, and export.

The deterministic fake provider remains the ordinary development and CI
baseline. M1 additionally needs one optional live adapter without granting the
model filesystem, shell, network-fetch, or Synapse authority.

## Decision

- Version a provider-neutral contract around an exact `attemptId`, Accepted
  base revision, provider/model selection, adapter version, reviewed context
  digest, structured result, attribution, usage when reported, and safe error.
- Assemble canonical AI context only on the local server. Bind the instruction,
  Target, current resolution receipt, provider/model, system instruction,
  output contract, and bounded UTF-8 site content into one reviewed digest.
- Limit context to ten text files, 512 KiB per file, and 2 MiB total. Always
  include the selected entry point; record exact path, media type, line and byte
  range, content digest, token estimate, redactions, truncation, and screenshot
  state in the visible manifest. Screenshots remain disabled in C6.
- Mark imported site content as quoted untrusted data and state that boundary
  independently in the provider system instruction. Redact credential-like
  assignments, bearer/key tokens, and common local absolute paths before the
  review and provider call. A changed context digest requires a new review.
- Record the original Accepted digest separately from the digest of each
  included redacted file. If any site-file entry was redacted, keep the exact
  context locally reviewable but reject change generation before provider
  execution. ChangeSet v1 full-file replacement has no safe rule for restoring
  a secret placeholder without allowing a model to move or duplicate that
  secret. Instruction/Target-only redactions do not block a change when every
  included site file is clean.
- Keep provider context separate from SynapseGit review context. SynapseGit
  receives only the Accepted base, provider-context digest, ChangeSet digest,
  coarse Target provenance plus its digest, caller-supplied attribution, and
  provider receipt fields. It does not receive the user instruction, complete
  Target, site snippets, credential, or raw provider output.
- Expose the deterministic local fake adapter unconditionally. Expose the
  OpenAI Responses adapter only when `OPENAI_API_KEY` exists on the server.
  Credentials never enter bootstrap capabilities or response DTOs. The model
  ID is an exact server allow-list entry; the default is `gpt-5.4-mini`, with a
  bounded `LP_STUDIO_OPENAI_MODEL` override.
- The OpenAI adapter uses the Responses API with `store: false`, no tools,
  low reasoning effort, a strict JSON Schema response format, no redirects, a
  90-second request timeout, and a streaming 4 MiB response cap. The adapter
  contract is `openai-responses/1`; an API/model behavior change requires a
  new adapter version or compatibility evidence.
- Accept only one completed assistant message containing one non-empty
  `output_text` part. Reject missing control fields, unknown or additional
  output items, multiple text parts, and incomplete messages. An explicit
  refusal rejects the entire response even when output text is also present.
- Parse every result again as strict application-owned ChangeSet v1. Unknown
  fields and operations fail. Allow only UTF-8 `create_text`, `replace_text`,
  `rename`, and `delete` with bounded paths, media types, counts, bytes, graph,
  and exact hash preconditions.
- Apply the complete ChangeSet to an in-memory clone, then validate syntax,
  the `index.html` entry point, local references, export deny-list, active
  behavior, and Target re-resolution. Only a fully valid result is
  materialized as an immutable isolated Proposal workspace and registered with
  SynapseGit. Accepted bytes remain unchanged on every failure.
- Treat newly introduced external origins (including protocol-relative URLs),
  form actions, script, iframe, download, inline event handlers, changed local
  or inline script bodies, analytics, or cookie behavior as blocking review
  warnings with destinations. The server rejects adoption while a blocking
  warning remains; reject and defer remain available, and UI disabling is not
  the authority boundary.

The live adapter follows the official OpenAI
[Responses create contract](https://developers.openai.com/api/reference/resources/responses/methods/create)
and [Structured Outputs guidance](https://developers.openai.com/api/docs/guides/structured-outputs).
The model default follows the official [model catalog](https://developers.openai.com/api/docs/models)
and remains configurable because availability can differ by account.

## Failure and retention semantics

- Only provider attribution and validated canonical ChangeSet are retained in
  the pending Proposal. HTTP bodies for errors and invalid raw model output are
  discarded without logging or materialization.
- One active attempt is allowed per project. Provider execution occurs without
  holding the project mutex; the Accepted revision, context digest, attempt,
  provider attribution, and Proposal slot are revalidated afterward.
- A timeout, transport failure, refusal/incomplete response, attribution
  mismatch, stale base, malformed ChangeSet, failed precondition, or static
  validation failure returns a stable redacted error and never creates a ready
  Proposal.
- A site-file redaction returns a stable non-retryable error before an active
  attempt is claimed or an external provider can receive the candidate
  context. This also keeps the original-file digest from becoming an external
  known-plaintext verifier for a low-entropy redacted value.
- Provider-reported usage is displayed when present. Missing usage or cost is
  left unknown rather than estimated as an actual charge.

## Consequences and deferred work

- CI and browser tests remain network- and credential-independent through the
  exact same fake-provider boundary and ChangeSet parser.
- A live call intentionally sends the expanded reviewed context to OpenAI; the
  UI labels this before the Creator confirms generation.
- Conversation consultation mode, streaming progress, cancellation,
  autosaved/branched messages, screenshot opt-in, provider cost estimation,
  and live secret-gated network evidence remain C10 or later. Their shared
  contract types do not imply that the runtime capability is complete.
- Editing a redacted site file requires a future protocol with digest-bound
  protected ranges. C6 does not retrofit placeholder rehydration into
  ChangeSet v1.
- C7 adds durable Proposal/Decision recovery. C6's immutable Proposal workspace
  is not yet a restart-resumable pending Synapse authority record.
