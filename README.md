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

制作中の会話、注釈、revision、provenance metadataは静的LPの出力物へ混入させません。

## Initial architecture

```text
apps/
  web/                  LP editor and preview UI
  local-server/         local AI and filesystem boundary
packages/
  editor-protocol/      selected-target and annotation types
  site-model/           editable LP project model
  exporter/             static-site export pipeline
  synapsegit-adapter/   SynapseGit integration boundary
```

Implementation scaffolding and technology selection will be added in the first
development change.

## Status

Initial project setup. Not production-ready.

Current product and architecture requirements are documented in
[`docs/current-specification.md`](docs/current-specification.md). The
implementation-ready baseline and weighted delivery plan are in
[`docs/detailed-requirements.md`](docs/detailed-requirements.md) and
[`docs/implementation-plan.md`](docs/implementation-plan.md).
