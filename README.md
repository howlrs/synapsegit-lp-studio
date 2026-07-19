# SynapseGit LP Studio

SynapseGitを活用し、AIと対話しながらランディングページを制作する、
ローカルファーストのWebアプリケーションです。

## Product goals

- ブランクページまたは既存LPから制作を開始できる
- プレビュー上のDOM要素や空白領域を指定してAIへ修正を依頼できる
- AIの変更案とHuman DecisionをSynapseGitへ記録できる
- 完成したLPを単独で動作するHTML、CSS、JavaScript、画像等の静的ファイルとして出力できる
- 人が確認したprovenance情報をGitHubへ記録し、SynapseGitの実活用事例として検証・共有できる

## Repository boundary

このリポジトリはLP制作アプリケーションを管理します。SynapseGit Coreは
別リポジトリの [`howlrs/synapsegit`](https://github.com/howlrs/synapsegit)
で管理し、バージョン化されたAPIまたはCLI adapterを介して連携します。

この開発リポジトリは、SynapseGitの実利用検証と進捗共有のためPublicです。
Public visibilityはLP StudioやSynapseGitのproduction利用・再配布許諾を
意味しません。LP Studio自身のライセンスと、SynapseGitのlicense/brand条件は
リリース前にそれぞれ確定します。

制作中の会話、注釈、revision、provenance metadataは静的LPの出力物へ混入させません。

## Workspace

```text
apps/
  web/                  React editor and review UI
  local-server/         Rust loopback API, preview, SynapseGit boundary
packages/
  contracts/            strict API and preview-bridge contracts
templates/
  blank/                exportable blank LP fixture
tests/
  e2e/                  real browser vertical slice
```

The SynapseGit application boundary is pinned by full commit and contract hashes in
[`docs/synapsegit-contract.lock.json`](docs/synapsegit-contract.lock.json).

## Local development

Prerequisites are pinned to Node.js 24.14.1, pnpm 10.33.0, and Rust 1.95.0.

```bash
pnpm install --frozen-lockfile
pnpm build
LP_STUDIO_STATE_ROOT=.studio-data pnpm dev:server
```

Open the `editorOrigin` printed in `LP_STUDIO_READY`. The server binds random
IPv4 loopback ports for the Editor/API and the isolated Preview. Each live
Preview uses an opaque session/project/snapshot-scoped `localhost` subdomain;
expired, foreign, stale, and terminal Proposal URLs are revoked. Omitting
`LP_STUDIO_STATE_ROOT` uses a private process-owned temporary directory and
removes it at shutdown.

The local deterministic fake provider is always available and is used by the
ordinary test suite. To enable the optional OpenAI Responses adapter, provide
the credential only to the local server process:

```bash
OPENAI_API_KEY=... \
LP_STUDIO_OPENAI_MODEL=gpt-5.4-mini \
LP_STUDIO_STATE_ROOT=.studio-data \
pnpm dev:server
```

`LP_STUDIO_OPENAI_MODEL` is optional and defaults to `gpt-5.4-mini`. The key is
not returned to the browser or stored in project/export data. Before a live
request, the UI discloses that the expanded, redacted context shown in the
review dialog will leave the machine and may incur provider charges. The
adapter sends no tools, uses low reasoning effort, and stores only validated
ChangeSet plus provider attribution. A manually acknowledged live contract
test is available but is never part of ordinary CI:

```bash
LP_STUDIO_LIVE_PROVIDER_TEST=1 \
OPENAI_API_KEY=... \
cargo test -p synapsegit-lp-local-server \
  openai_live_adapter_returns_an_app_valid_changeset -- --ignored
```

To review and copy-import a built static LP from a server-owned directory, set
an optional, real directory that is disjoint from the state root:

```bash
LP_STUDIO_STATE_ROOT=.studio-data \
LP_STUDIO_IMPORT_ROOT=/absolute/path/to/built-lp \
pnpm dev:server
```

The current C6 slice persists blank/imported projects in the explicit state
root, previews the exact included/excluded import set before confirmation,
keeps the source directory unchanged, and blocks Accepted-manifest drift before
Proposal, Decision, and export. Preview execution is isolated by scoped origin,
CSP/sandbox, bounded navigation, storage rotation, revocable URLs, and a
privacy-safe diagnostic bridge. All six Target kinds feed an exact Creator
reviewed AI context. The deterministic fake and optional live adapter return a
strict, preconditioned ChangeSet that is atomically validated in an isolated
Proposal workspace before SynapseGit registration. It deliberately exposes
caller-supplied attribution and `execution未検証`; SynapseGit did not execute or
verify the model. If an included site file requires redaction, its exact local
context remains reviewable but C6 blocks full-file ChangeSet generation before
calling the provider; safe protected-range editing is deferred to a later
protocol. Active Preview currently requires Chromium's Navigation API and
fails closed when the boundary cannot be installed.

## Status

AI context and ChangeSet checkpoint complete: 65%. Not production-ready.
SynapseGit draft PR #25 is source-level evaluation work, not a released
dependency or permission for production/distribution.

Current product and architecture requirements are documented in
[`docs/current-specification.md`](docs/current-specification.md). The
implementation-ready baseline and weighted delivery plan are in
[`docs/detailed-requirements.md`](docs/detailed-requirements.md) and
[`docs/implementation-plan.md`](docs/implementation-plan.md).
