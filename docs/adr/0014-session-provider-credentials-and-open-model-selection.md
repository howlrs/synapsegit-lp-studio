# ADR-0014: Session provider credentials and open model selection

Status: implemented (initial OpenAI slice)

Date: 2026-07-20

## Context

The implemented OpenAI adapter is intentionally conservative. A credential is
configured before server startup, the server exposes exactly one configured
model, and the browser chooses that model from a closed list. This keeps the
credential outside the browser and gives the server an exact model allow-list,
but changing either value requires operational setup and a server restart.

For local evaluation, a Creator should also be able to enter an API credential
in the Editor and request any syntactically valid model ID. The provider, not a
stale application model catalog, should decide whether that credential and
account may use the requested model. This supports newly introduced, renamed,
restricted, and retired models without an LP Studio release for every catalog
change.

GUI entry is nevertheless a deliberate relaxation of the current credential
boundary. A standard API key exists briefly in the Editor DOM and JavaScript
heap before it reaches the trusted local server. Browser extensions, developer
tools, crash capture, password managers, compromised Editor dependencies, and
same-host malware remain able to observe it. OpenAI's current guidance uses
standard API keys from a trusted backend, including in its
[Realtime WebRTC server flow](https://developers.openai.com/api/docs/guides/realtime-webrtc#creating-a-session-via-the-unified-interface),
and recommends prompt revocation of a compromised key in its
[safety guidance](https://developers.openai.com/api/docs/guides/safety-best-practices#revoke-compromised-api-keys).
The existing server-configured secret therefore remains the recommended mode
for higher-sensitivity evaluation.

This proposal changes only provider runtime configuration. It does not permit
the browser or Preview to call OpenAI directly, add a general proxy, discover
models, persist credentials, or weaken Proposal validation and Human Decision.

## Relationship to existing decisions

If accepted and implemented, this ADR supersedes only these narrow parts of
[ADR-0008](0008-ai-provider-context-and-change-set.md):

- OpenAI availability no longer requires a credential at server startup;
- the model descriptor list becomes suggestions rather than an allow-list; and
- an authenticated Editor session may supply an ephemeral credential.

It extends [ADR-0013](0013-docker-local-build-profile.md) without removing the
Docker secret volume. All other provider-context, Preview, logging, export,
ChangeSet, Proposal, Decision, and recovery boundaries remain in force.

Acceptance also requires an explicit amendment to `INT-AI-002` in the detailed
requirements. That implemented P0 requirement currently says credentials never
enter the browser. `API-001`, `API-005`, `SEC-PRIV-001`, and `SEC-PRIV-003`
remain applicable and become verification gates for the credential endpoint.

Until this ADR is implemented and its verification profile passes,
ADR-0008/0013 and the current code remain authoritative.

## Goals

- Let a Creator configure an OpenAI API key from the Editor without restarting
  the local server.
- Keep the entered key process-memory-only, session-scoped, non-exportable, and
  absent from Project and SynapseGit records.
- Accept a bounded free-text model ID and let the real provider request decide
  account/model availability.
- Bind the exact model and credential selection to the reviewed context without
  hashing or otherwise disclosing the credential.
- Preserve deterministic fake-provider CI and the existing validated Proposal
  boundary.
- Return actionable, stable, privacy-safe errors for authentication, model
  access, quota, rate limiting, transport, and malformed provider output.

## Non-goals

- Persisting a GUI-entered credential across reload, session expiry, process
  restart, backup, migration, or recovery.
- Calling OpenAI from browser JavaScript or giving Preview content network or
  credential access.
- Listing or guaranteeing currently available OpenAI models.
- Validating a key or model through a hidden, free, or side-effect-free
  preflight request. No such guarantee is assumed.
- Automatically retrying a potentially billable provider request.
- Supporting arbitrary provider URLs, compatible APIs, organization/project
  headers, OAuth, or multiple simultaneous credentials in this slice.
- Changing the strict Responses request, structured ChangeSet, static checks,
  Proposal isolation, or Human Decision rules from ADR-0008.

## Decision

### 1. Keep provider execution server-side

The Editor sends an entered key only to the exact authenticated loopback
Editor/API origin. The local Rust adapter remains the only component that sends
the reviewed context and `Authorization: Bearer` credential to OpenAI. The
Preview origin never receives the key, a credential binding ID, the Editor
session token, or a provider configuration API.

The supported topology remains direct loopback Editor/API access. A reverse
proxy, LAN bind, alternate hostname, remote Editor, TLS terminator, or hosted
deployment is outside this proposal. Loopback HTTP does not defend against a
compromised same-host process; that risk is disclosed rather than described as
transport security.

### 2. Register a GUI credential into the authenticated server session

`session credential` means memory owned by the local server's authenticated
Editor session. It does not mean `sessionStorage`.

The Editor initially holds the key only in a password input. An explicit
`このセッションで使用` action sends it once to a dedicated, bounded endpoint:

```http
PUT /api/v1/session/provider-credentials/openai
Content-Type: application/json
Authorization: Bearer <editor-session-token>
```

```json
{
  "schemaVersion": "1",
  "expectedCredentialBindingId": null,
  "apiKey": "<entered-secret>"
}
```

The route applies the same exact Host, Origin, `Sec-Fetch-Site`, content type,
body-size, and Editor-session checks as other mutations. Those checks must run
in route middleware or a credential-specific extractor **before** Axum's JSON
extractor reads or allocates the secret-bearing body; calling the current
handler-local authorization function after `Json<T>` extraction is not
sufficient for this endpoint. The credential is in
the JSON body, not a URL, query, cookie, or custom header. `Authorization` is
already reserved for the Editor session token. Moving the provider key to a
second header would not make it secret from browser network tooling and would
increase the chance that generic header capture records it.

The request type must not implement `Debug`, `Clone`, `Serialize`, or any error
conversion that includes field values. Immediately after bounded validation,
the key moves into a secret wrapper whose debug representation is always
redacted and whose buffer is zeroized on replacement, expiry, and drop. This
reduces copies but does not claim perfect zeroization of browser or allocator
memory.

The key must be 1-1,024 UTF-8 bytes, contain no control character, and have no
leading or trailing whitespace. No `sk-` prefix or other provider key format is
assumed. The whole credential request remains within the existing 64 KiB API
body limit.

The server stores at most one session credential per provider per Editor
session. It returns only non-secret state:

```json
{
  "schemaVersion": "1",
  "providerId": "openai",
  "credentialSourceId": "editor_session",
  "credentialBindingId": "pcb_<opaque-random-id>",
  "status": "configured_unverified",
  "expiresAt": "<editor-session-expiry>"
}
```

The key, its prefix/suffix, length, digest, account identity, organization, and
validation response are never returned. Registration performs syntactic
validation only and makes no OpenAI request.

Replacement sends the current binding ID as
`expectedCredentialBindingId`; initial registration requires `null`. A stale
expected value fails with `provider_credential_binding_stale` without changing
the credential. Status reads return the same non-secret fields. Clear uses an
authenticated `DELETE` on the same route with the exact body
`{"schemaVersion":"1","credentialBindingId":"pcb_<opaque-random-id>"}`.

After success, the Editor clears the input value and retains only the response
metadata. The credential disappears when any of the following occurs:

- the 30-minute Editor session expires or is swept;
- the server process exits;
- the Creator explicitly clears or replaces it; or
- the bounded session record is evicted.

A reload creates a new Editor session and requires re-entry. The previous
server-side entry is no longer reachable by the browser, but it can remain in
memory until the old session is swept, no later than its 30-minute expiry.
`pagehide` may attempt a best-effort authenticated clear, but unload delivery is
not a security guarantee and the UI must not claim immediate zeroization after
reload. The application must not use `localStorage`, `sessionStorage`, IndexedDB,
Cache API, cookies, service workers, URL state, form restoration, Project state,
or filesystem storage for the key.

### 3. Keep server-configured credentials as an explicit alternative

`OPENAI_API_KEY` and `OPENAI_API_KEY_FILE`, including the Docker secret volume,
remain supported. Bootstrap capability data reports only whether the
`server_configured` source is available; it never returns credential material
or identifying fragments.

The UI presents two distinct sources when applicable:

- `このセッションで入力したキー` (`editor_session`); and
- `起動時に設定されたキー` (`server_configured`).

The selected source is explicit and visible in the exact-context review.
Failure of one source never falls back to the other. In particular, an invalid,
rate-limited, or quota-exhausted GUI key must not consume a server-configured
credential on retry.

### 4. Bind credential selection without binding credential bytes

Each configured source receives a cryptographically random, process-local
`credentialBindingId`. The ID is not derived from the key. The following
non-secret values become part of `ContextProviderBindingV1` and the canonical
provider context:

```json
{
  "providerId": "openai",
  "adapterVersion": "openai-responses/2",
  "requestedModel": "<creator-entered-model-id>",
  "credentialSourceId": "editor_session",
  "credentialBindingId": "pcb_<opaque-random-id>",
  "external": true
}
```

The binding ID, source, provider, adapter, and model are covered by the context
digest and displayed before generation. Credential bytes, length, prefix,
suffix, and digest are not included in canonical context, Target data,
Proposal attribution, SynapseGit records, diagnostics, or export.

Replacing or clearing a credential invalidates its binding ID. A context
reviewed against that ID cannot silently execute with a replacement key; the
server returns `provider_credential_binding_stale` before claiming an attempt
or making an external request. The Creator must review a new context.

The Proposal request continues to carry only `contextId` and
`contextSha256`. The server resolves the exact credential binding from the
authenticated Editor session at execution time. The browser does not resend
the key for each Proposal request.

### 5. Define in-flight replacement, clear, cancel, and retry semantics

At attempt claim, the server takes a bounded secret snapshot for that exact
credential binding and releases the shared Store lock before network I/O. A
later UI state change cannot replace the credential used by the in-flight
request.

Credential replacement or clearing returns
`provider_credential_in_use` while any claimed attempt references the binding.
The Creator must first cancel the attempt and wait for its terminal local
status. Cancellation drops the local provider future and its secret snapshot,
but cannot guarantee that a request already received by OpenAI was not
processed or billed.

There is no automatic provider retry. A timeout, lost response, or cancellation
can have an unknown remote/billing outcome even though no Proposal was created.
Every retry is a new explicit attempt, repeats exact-context confirmation, and
uses the then-selected credential binding and model.

No credential or context is recovered after process restart. Only a completed,
validated Proposal enters the existing durable review/recovery boundary.

### 6. Make model selection open text, not an application allow-list

For providers that advertise `modelSelection.mode = "open_text"`, the Editor
renders an editable text input with optional suggestions. A `datalist` or
recent session values may aid entry, but selecting a suggestion is not
required. Suggested models are hints, not an availability claim or allow-list.

The OpenAI model ID must be 1-256 ASCII bytes using only letters, digits, `-`,
`.`, `_`, `:`, and `/`. Leading/trailing whitespace and all normalization are
rejected rather than silently changing the reviewed value. This local grammar
is an injection and resource bound, not a catalog check. LP Studio must not
reject a syntactically valid ID merely because it is absent from bootstrap
data. The implementation must reconcile the current 128-byte Rust check with
the 256-character public contract as one exact byte-based rule.

The exact model ID is bound into the reviewed context digest and retained as
caller-supplied `requestedModel` attribution. The provider-reported model is
retained separately. Alias resolution or a different reported model is shown
to the Creator and is not rewritten into the original request binding.

`LP_STUDIO_OPENAI_MODEL` changes from an exact allow-list to the initial model
suggestion/default for the server-configured mode. Removing or changing that
environment value does not disable open-text entry.

No automatic `GET /models` call is added. A model-list response is scoped to a
credential/account, can become stale, may not prove Responses/Structured
Outputs compatibility, and would add another external disclosure and failure
surface.

### 7. Separate adapter availability, credential availability, and model input

Bootstrap provider descriptors must stop overloading one `availability` field.
The exact contract update will expose these independent facts:

```json
{
  "id": "openai",
  "adapterAvailability": "available",
  "credentialSources": [
    {
      "id": "editor_session",
      "availability": "configurable"
    },
    {
      "id": "server_configured",
      "availability": "available"
    }
  ],
  "modelSelection": {
    "mode": "open_text",
    "defaultModel": "<configured-suggestion>",
    "maxUtf8Bytes": 256,
    "suggestions": []
  }
}
```

The fake provider continues to advertise `modelSelection.mode = "closed"`
with only `deterministic-v1` and no credential source. This keeps the common
provider contract provider-neutral without weakening deterministic CI.

These fields form a versioned provider-configuration sub-contract. TypeScript,
Rust DTOs, canonical JSON Schema, exact-key validators, fixtures, and parity
tests change atomically. The persisted Project schema is not bumped solely for
ephemeral provider configuration; no credential data is migrated.

### 8. Map real provider failures to stable safe errors

The OpenAI adapter reads a small bounded non-success body only to classify
allow-listed status/code/type/parameter values. It discards the provider
message and all unknown fields. Neither raw error bodies nor model IDs enter
logs. Status alone is a fallback when the body is absent or malformed.
The broad HTTP categories follow OpenAI's current
[error-code guidance](https://developers.openai.com/api/docs/guides/error-codes),
but LP Studio does not assume that every model-specific provider code is stable.

| Stable code | Provider/local signal | Local HTTP | Retryable | UI meaning |
| --- | --- | --- | --- | --- |
| `provider_authentication_failed` | provider HTTP 401 | 422 | no | Entered or configured credential was rejected. |
| `provider_permission_denied` | provider HTTP 403 | 422 | no | The credential/account lacks required access. |
| `provider_model_unavailable` | model-related provider HTTP 400/404 | 422 | no | The requested model is unknown or unavailable to this account. |
| `provider_model_incompatible` | allow-listed model/request-profile rejection | 422 | no | The model cannot satisfy this adapter's fixed Responses/Structured Outputs profile. |
| `provider_request_rejected` | other bounded provider HTTP 400/404 | 422 | no | Provider rejected this request shape or option. |
| `provider_quota_exceeded` | allow-listed provider quota code | 422 | no | Account quota/billing prevented execution. |
| `provider_rate_limited` | other provider HTTP 429 | 429 | yes | Retry later by explicit Human action. |
| `provider_service_unavailable` | provider HTTP 5xx | 503 | yes | Provider service failed temporarily. |
| `provider_timeout` | local deadline | 504 | yes | Completion is locally unknown; billing may still occur. |
| `provider_transport_error` | connection/TLS/read failure | 502 | yes | Delivery or completion is unknown. |
| `provider_response_invalid` | malformed/oversized success body | 502 | no | No valid ChangeSet envelope was accepted. |
| `provider_credential_binding_stale` | missing/mismatched binding | 409 | no | Reconfigure and review context again; no external call occurred. |

Provider authentication and permission failures intentionally do not reuse the
local API's 401/403 statuses, which remain reserved for Editor authentication
and authorization. Expected user/account/model failures are excluded from
server-fault metrics while their stable code remains observable.

The UI may combine the stable code with the already visible requested model and
credential source, but it must not display the raw provider message. Provider
request IDs are shown only from accepted, structurally valid success responses
under the existing attribution rules.

## Editor UX specification

### Credential section

- Show OpenAI as an installed external adapter even when no startup credential
  exists.
- Label the GUI path `APIキー（このセッションのみ）` and explain before input
  that the key briefly exists in browser memory and is sent to the local server.
- Use a password input with reveal/hide control, `autocomplete="off"`, disabled
  spellcheck/autocorrect/capitalization, and an accessible description. Do not
  prefill or redisplay the key after registration.
- Keep the live input ref-local/uncontrolled rather than placing secret text in
  the application reducer, action history, props, or diagnostic state. Clear it
  on successful registration, provider teardown, `pagehide`, and BFCache
  restoration. This limits intentional retention without claiming JavaScript
  heap zeroization.
- Require explicit `このセッションで使用`; do not send on blur, typing,
  provider selection, or page load.
- After registration, show only `未検証・セッション終了時に破棄` and expiry.
  Provide explicit replace and clear actions.
- Recommend the startup/Docker secret mode for sensitive credentials and link
  to the Docker guide. Do not describe GUI entry as equivalent secret storage.

### Model section

- Render a text input for OpenAI and a select for the closed fake provider.
- State `利用可否は送信時にproviderが判定します` beside open-text input.
- State that this real availability check sends the exact reviewed context to
  the provider even when the model is ultimately rejected.
- Show byte limit and grammar errors locally; do not claim that local validity
  means the model exists, supports Structured Outputs, or is affordable.
- Preserve the exact reviewed model while the context-confirmation drawer is
  open. Editing the model closes/invalidates that review and requires a new
  `送信内容を確認` action.

### Confirmation and errors

- Exact-context review shows provider, requested model, adapter, credential
  source, and `configured_unverified`, but never key metadata.
- The final generation action repeats the external-send and possible-charge
  warning. The credential-registration action itself makes no provider call.
- Authentication/model/permission/quota/rate/transport errors have distinct
  Japanese messages and recovery actions. A failed request never creates a
  Proposal or changes Accepted.
- Authentication failures return focus to the credential section, model
  unavailable/incompatible failures return focus to the model input, and
  transient failures return focus to the explicit review/retry action. Errors
  are announced through the existing accessible status boundary.
- Clearing/replacing a credential while an attempt is running is disabled in
  the UI and rejected by the server. Cancel text states that remote processing
  or billing may already have occurred.
- Reload/session expiry shows the credential as absent and invalidates any
  unexecuted context review that referenced it.

## Security and privacy invariants

- Editor CSP continues to allow provider networking only from the Rust server;
  browser `connect-src` remains limited to the Editor origin.
- Preview remains a separately scoped origin and sandboxed capability. Tests
  must prove malicious imported/AI-generated script cannot read the Editor DOM,
  session token, credential form, or binding metadata.
- Provider configuration routes exist only on the Editor router and require the
  same anti-ambient-request controls as other mutations.
- No request/body middleware, tracing span, panic formatter, debug derive,
  diagnostic package, telemetry, test snapshot, or fixture may serialize the
  credential request or secret wrapper.
- Response headers on credential routes include `Cache-Control: no-store`; the
  Editor does not register a service worker.
- The credential is excluded structurally, not by keyword redaction, from
  Project structs, persistence DTOs, canonical context, Proposal/Review,
  SynapseGit calls, retention inventory, recovery, static export, publication,
  and diagnostic types.
- A canary credential must remain absent from normal/debug logs, HTTP responses,
  state root, SQLite, CAS, exported ZIPs, publication ZIPs, diagnostics, and
  browser storage after success, failure, cancellation, replacement, expiry,
  restart, and panic-safe error paths.
- GUI entry does not protect against a malicious browser extension, compromised
  Editor bundle/dependency, developer tools, browser crash capture, clipboard
  history, password-manager capture, or same-host malware. These residual risks
  are stated in the UI and user guide.

## Implementation outline

1. Split the current OpenAI configuration into a server credential source and
   a model suggestion; remove model equality from OpenAI provider binding.
2. Extend `Session` with a bounded zeroizing credential entry and sweep it with
   the existing 30-minute session lifecycle.
3. Add configure/status/replace/clear credential DTOs and authenticated routes.
4. Extend provider descriptors and context bindings with credential source,
   binding ID, and open/closed model-selection semantics.
5. Version the changed live adapter as `openai-responses/2`; do not silently
   remove strict JSON Schema, `store: false`, no-tools, reasoning, timeout,
   redirect, or response-cap controls to accommodate a selected model.
6. Resolve and snapshot the bound credential immediately before claiming and
   executing the provider attempt; preserve no-lock network execution.
7. Add bounded non-success OpenAI error parsing and the stable error taxonomy.
8. Replace the OpenAI model select with accessible open text and add the
   credential panel, binding invalidation, and error recovery UI.
9. Update Docker/user documentation while preserving the secret-volume path as
   the recommended high-sensitivity option.

Expected implementation areas include:

- `apps/local-server/src/ai_provider.rs` for credential-independent model
  binding, secret use, and provider error classification;
- `apps/local-server/src/lib.rs` for session credential state, routes, DTOs,
  exact binding, lifecycle, and safe error responses;
- `apps/local-server/src/main.rs` for separating startup credential and default
  model suggestion;
- `packages/contracts` for provider configuration, credential status, context
  binding, JSON Schema, and strict validators;
- `apps/web/src/api/client.ts` and `apps/web/src/App.tsx` for authenticated
  configuration, open model entry, review invalidation, and recovery UX;
- web/server/unit/E2E privacy tests; and
- `README.md`, `docs/user-guide.md`, `docs/docker.md`, implementation status,
  requirements, traceability, and evidence records after implementation.

## Verification profile

### Contract and server tests

- Reject missing/extra fields, wrong content type/origin/host/fetch-site/session,
  empty/oversized/control-containing keys, invalid provider IDs, and invalid
  model grammar without echoing input.
- Prove invalid Host, Origin, fetch metadata, content type, and session requests
  are rejected before the credential JSON body is read or deserialized.
- Prove registration makes no external request and returns no credential
  fragment or digest.
- Prove any syntactically valid model reaches a mocked provider unchanged,
  whether or not it appears in suggestions.
- Prove the requested model, source, and binding ID alter the canonical context
  digest while credential bytes do not appear in canonical JSON.
- Prove replacement/clear/session expiry invalidates old binding IDs and does
  not silently select a server credential.
- Prove an in-flight attempt keeps its exact claimed binding; clear/replace is
  rejected until terminal, and a late cancelled result cannot materialize.
- Prove every failure mapping, bounded error-body parser, retryable flag, and
  absence of raw provider messages.
- Re-run existing strict ChangeSet, attribution, cancellation, redaction,
  Proposal isolation, and fake-provider suites unchanged in authority.

### Browser and accessibility tests

- Keyboard-only configure, reveal/hide, replace, clear, source choice, model
  edit, exact-context review, generation, cancellation, and error recovery.
- No API call while typing; one registration call only after explicit action.
- Password input is cleared after success and not restored after remount/reload.
- `localStorage`, `sessionStorage`, IndexedDB, Cache API, URL/history, and cookies
  contain no canary after every outcome.
- A model outside suggestions can be reviewed and sent; changing it invalidates
  the prior review.
- Invalid credentials and unavailable models produce distinct accessible
  announcements without a Proposal or Accepted change.
- Malicious Preview fixtures cannot inspect or submit the Editor credential
  form and cannot call credential routes with Editor authority.
- 200% zoom, 320 CSS px width, forced colors, focus return, and screen-reader
  labels meet the existing evidence profile.

### Privacy and operational tests

- Canary scanning covers stdout/stderr at every supported log level, state
  volume/root, SQLite, CAS, diagnostics, export/publication archives, Docker
  inspect metadata, crash/restart paths, and browser storage.
- Docker tests cover GUI credential mode without the OpenAI overlay and
  server-configured mode with it; neither performs a live provider call in
  ordinary CI.
- Live-provider evidence remains separately acknowledged and billing-aware. It
  covers one allowed model and one deliberately unavailable model without
  recording the key, provider body, prompt, or site bytes.

## Rollout and documentation

Implementation should land behind the updated exact contract and tests, not a
UI-only flag. The first release must continue to support the existing
server-configured path. There is no state migration because session credentials
and unexecuted contexts are process-local.

Documentation must clearly distinguish:

- `implemented` versus this proposed design;
- GUI session credential versus persistent Docker secret volume;
- syntactic model validation versus provider/account availability;
- local cancellation/failure versus unknown remote processing and billing; and
- local evaluation support versus production or hosted deployment.

After implementation, ADR-0008 and ADR-0013 receive narrow amendment notes
linking here; they should not be silently rewritten as if the older baseline
never existed.

## Alternatives considered

### Keep startup-only credential and closed model selection

This preserves the narrowest credential boundary but keeps restart-heavy setup
and turns a stale model list into an application release dependency. It remains
available as the recommended high-sensitivity mode, not the only UX.

### Hold the key in browser memory and resend it with every Proposal request

This avoids server session state but extends browser exposure, repeats the
secret across request lifecycles, complicates retry/TOCTOU semantics, and makes
the Proposal endpoint itself a credential-bearing DTO. It is not selected.

### Send the provider key in a custom HTTP header

Headers are commonly captured by browser/network tooling and infrastructure;
the existing `Authorization` header already carries the Editor session token.
A dedicated bounded body on a no-log configuration route gives a clearer type
and lifecycle boundary. Neither transport hides the value from the browser
that collected it.

### Validate by calling `GET /models` or a zero-output request

This adds an external call and does not prove that the selected model supports
the exact Responses/Structured Outputs request or will remain available. Any
request may have rate, privacy, or billing consequences. The actual confirmed
generation request is authoritative.

### Hash the key into the reviewed context

A credential digest would create a durable equality/verifier signal without
being necessary for ChangeSet provenance. A random process-local binding ID
provides TOCTOU protection without deriving retained data from the secret.

### Silently fall back to a server-configured credential

This obscures which account incurs cost and can turn a user-key failure into
unexpected use of a shared credential. Explicit source selection is required.

## Consequences

- Local setup becomes materially easier and model catalog churn no longer
  requires an LP Studio release.
- The exact requested model remains reviewable and attributable even when the
  provider resolves an alias to a different reported model.
- GUI entry increases credential exposure compared with startup/Docker secret
  configuration. The safer mode remains available and recommended.
- Session binding state and error classification add implementation and test
  complexity, but avoid repeated secret transport and ambiguous fallback.
- A valid local model string can still fail for access, retirement, request
  compatibility, policy, quota, or billing reasons. That failure is expected
  behavior, not evidence that the local model field is broken.
- No passing unit or fake-provider test proves live account/model availability;
  live evidence remains a separate, explicitly billed gate.
