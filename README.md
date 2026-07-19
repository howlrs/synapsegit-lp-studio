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
IPv4 loopback ports for the Editor/API and the isolated Preview. Omitting
`LP_STUDIO_STATE_ROOT` uses a private process-owned temporary directory and
removes it at shutdown.

To review and copy-import a built static LP from a server-owned directory, set
an optional, real directory that is disjoint from the state root:

```bash
LP_STUDIO_STATE_ROOT=.studio-data \
LP_STUDIO_IMPORT_ROOT=/absolute/path/to/built-lp \
pnpm dev:server
```

The current C3 slice persists blank/imported projects in the explicit state
root, previews the exact included/excluded import set before confirmation,
keeps the source directory unchanged, and blocks Accepted-manifest drift before
Proposal, Decision, and export. It also supports element selection, exact
context review, deterministic fake-AI Proposal, explicit adopt, and
deterministic Accepted ZIP export. It deliberately exposes caller-supplied
attribution and `execution未検証`; SynapseGit did not execute or verify the
model.

## Status

Managed project/revision/import checkpoint complete: 38%. Not production-ready. SynapseGit
draft PR #25 is source-level evaluation work, not a released dependency or
permission for production/distribution.

Current product and architecture requirements are documented in
[`docs/current-specification.md`](docs/current-specification.md). The
implementation-ready baseline and weighted delivery plan are in
[`docs/detailed-requirements.md`](docs/detailed-requirements.md) and
[`docs/implementation-plan.md`](docs/implementation-plan.md).
