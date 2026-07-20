# SynapseGit LP Studio

SynapseGit LP Studioは、AIへLP（ランディングページ）の変更を依頼し、
結果を人が確認してから採用する、ローカルファーストのWebアプリです。

> **現在地:** 65% baseline以降のC7–C10と、C11の自動化可能なlocal
> packaging/evidenceを統合した開発・評価baselineです。Creator UX、manual
> screen-reader、live provider、license/brand、merge/releaseの各Human・外部gateは
> 未完了です。ローカル評価用であり、production-readyな製品、配布物、または
> リリース版ではありません。現在の実装事実は
> [実装ステータス](docs/implementation-status.md)を正本として確認してください。

## まず知ってほしい3つの言葉

| 言葉 | 意味 |
| --- | --- |
| **Accepted** | 現在、人が採用済みのLPです。Previewとexportの基準になります。 |
| **Proposal** | AIが作った変更案です。Acceptedを自動では変更しません。 |
| **Human Decision** | Proposalを人が確認し、採用などを決める操作です。 |

基本ルールは単純です。

~~~text
Acceptedを選ぶ
  → AIへ要望を出す
  → Proposalを比較する
  → 人が採用・却下・保留を決める
  → 採用時だけ新しいAcceptedになる
  → AcceptedだけをZIPへexportする
~~~

SynapseGitはProposalとHuman Decisionの来歴を扱います。ただし、現在の表示は
`caller-supplied`かつ`execution未検証`です。SynapseGitがAIを実行または
検証した、という意味ではありません。

## 5分でローカル起動する

### Docker（Windows + WSL2推奨）

WSLをNode/Rust dependencyで汚したくない場合は、Docker Engine 28以降とDocker Desktopの
WSL integrationを使います。配布済みimageではなく、このsourceからlocal buildします。

~~~bash
docker compose up --build
~~~

起動後にWindows側のChromiumで`http://127.0.0.1:4173`を開きます。Project stateは
Docker named volumeへ残ります。現在のlicenseはprebuilt container imageの公開を許可して
いないため、GHCR/Docker Hub imageは提供しません。詳しい起動、import、credential、停止、
security boundaryは[Docker利用ガイド](docs/docker.md)を参照してください。

### Native toolchain

#### 必要なもの

- Node.js `24.14.1`
- pnpm `10.33.0`
- Rust `1.95.0`（`rustfmt`と`clippy`を含む）
- ChromeまたはChromium

バージョンは
[`.node-version`](.node-version)、
[`package.json`](package.json)、
[`rust-toolchain.toml`](rust-toolchain.toml)に固定されています。

#### 起動

~~~bash
pnpm install --frozen-lockfile
pnpm build
LP_STUDIO_STATE_ROOT=.studio-data pnpm dev:server
~~~

1. terminalに`LP_STUDIO_READY`が表示されるまで待ちます。
2. `editorOrigin`のURLをChromeまたはChromiumで開きます。
3. `previewOrigin`はPreview専用です。直接開く必要はありません。

`.studio-data`を指定すると、Accepted projectはserver再起動後も残ります。
`LP_STUDIO_STATE_ROOT`を省略するとprivateな一時directoryを使い、server終了時に
削除します。

## 最短の動作確認

外部AIを使わない確認手順です。

1. 「空のLPを作成」を押します。
2. Previewの「まだ、白紙です。」という見出しを選びます。
3. 「AIへの要望」へ、たとえば
   `公開向けの明確な見出しにしてください`と入力します。
4. providerが`fake`であることを確認します。
5. 「送信内容を確認」を押し、実際に渡すcontextを確認します。
6. 「変更案を作成」を押します。
7. Accepted / Proposed、diff、Validationを確認します。
8. 問題がなければ「変更を採用」を押します。
9. 「Acceptedをエクスポート」を押します。

fake providerは動作確認専用です。要望文を解釈せず、対応するblank templateの
見出し・説明文・CTAを固定文へ置換します。自由なLP生成や品質評価には使えません。

より詳しい画面説明とトラブル対応は
[利用ガイド](docs/user-guide.md)を参照してください。

## 現在できること

| 項目 | 現在の状態 |
| --- | --- |
| blank project | 作成、保存、再オープンができます。 |
| 既存LP | 起動時に登録したbuild済みstatic directoryを確認後にcopy importできます。 |
| project metadata | opaque project IDを維持したまま表示名を自動保存できます。 |
| Preview | Editorとは別のscoped originで実行します。現在のbrowser evidenceはChromiumのみです。 |
| Target | page / block / element / text / point / regionの6種類を選べます。 |
| AI context | Target、Accepted revision、provider、対象fileを含むexact contextを送信前に確認できます。 |
| provider | deterministic fakeを常に利用できます。serverにkeyを設定した場合だけOpenAIを選べます。 |
| AI attempt | phaseと経過を表示し、明示的にcancelできます。cancel後のlate responseはProposalになりません。 |
| ChangeSet | create / replace / rename / deleteを厳格に検証し、isolated Proposalへ適用します。 |
| review | Accepted / Proposed、変更file、text diff、Validationを確認できます。 |
| Human Decision | UIからProposal全体を採用、却下、または終端の保留にできます。 |
| recovery/history | pending ReviewとDecision reconciliationを再起動後に復旧し、terminal historyを表示します。 |
| export | Acceptedだけを決定的なZIPとして出力し、file manifest・validation・archive identityを表示します。 |
| publication draft | privacy-filteredなGitHub-ready filesをexact-byte reviewしてローカルZIPへ出力します。network writeは行いません。 |
| retention/recovery | exact bindingを確認したmanual cleanupと、normal起動不能時のread-only diagnostics/verified Accepted exportを提供します。 |

## まだできないこと

次の項目は型、要件、またはserver contractに存在しても、現在のUIで完成しているとは
限りません。

- 同じprojectで複数のready Proposalを同時に保持・比較すること
- terminal historyだけをProjectから独立して削除すること
- conversationの保存、分岐、streaming（cancelはAI生成attemptだけに対応）
- 複数Target、screenshot付きcontext、部分採用、Proposalの直接編集
- archive upload、GitHubへのremote publish/deploy
- 完了済みCreator/keyboard/screen-reader/live-provider evidence
- Chromium以外を含むrelease support matrix

bootstrap contractの`singleProposalPerProject: true`は、1 projectで同時に扱う
active Proposalを1件に制限する現在のcontractです。terminal Decision後は同じ
server processで次のProposalを作れます。deferから続ける場合も旧Proposalを再利用せず、
最新Acceptedをbaseに新しいProposalとして関連を記録します。

要件文書に書かれた機能を「実装済み」と推測しないでください。
[要件traceability](docs/requirements-traceability.md)と
[実装ステータス](docs/implementation-status.md)を確認してください。

## OpenAI providerを有効にする

OpenAIは任意です。keyをlocal server processだけへ渡します。

~~~bash
OPENAI_API_KEY=... \
LP_STUDIO_OPENAI_MODEL=gpt-5.4-mini \
LP_STUDIO_STATE_ROOT=.studio-data \
pnpm dev:server
~~~

`LP_STUDIO_OPENAI_MODEL`は省略でき、現在のdefaultは
`gpt-5.4-mini`です。

OpenAIを選ぶと、review dialogに表示されたredacted contextが外部providerへ
送信され、provider契約に応じた料金が発生する場合があります。keyはbrowser、
project、exportへ返しません。requestはtoolsを持たず、validated ChangeSetと
provider attributionだけをProposalへ保持します。

included site fileにredactionがある場合、full-file ChangeSet v1では元のsecretを
安全に戻せません。この場合はcontextの確認まではできますが、providerを呼ぶ前に
generationを停止します。

外部networkと課金を明示的に受け入れるmanual testだけを実行する場合は、次を使います。
通常のCIでは実行しません。

~~~bash
LP_STUDIO_LIVE_PROVIDER_TEST=1 \
OPENAI_API_KEY=... \
cargo test -p synapsegit-lp-local-server \
  openai_live_adapter_returns_an_app_valid_changeset -- --ignored
~~~

## 既存のstatic LPを取り込む

serverが読むdirectoryを起動時に登録します。state rootとimport rootには別の
実directoryを指定してください。

~~~bash
LP_STUDIO_STATE_ROOT=.studio-data \
LP_STUDIO_IMPORT_ROOT=/absolute/path/to/built-lp \
pnpm dev:server
~~~

UIでincluded / excluded file、byte数、warning、entry pointを確認してから
copy importします。元directoryは変更しません。browserから任意のfilesystem
pathを送るAPIは提供しません。

対象は`index.html`を持つbuild済みstatic siteです。frameworkのsource project、
package install、database、またはruntime serverを必要とするsiteは対象外です。

## 安全性の要点

- Editor/APIとuntrusted Previewは別originです。
- filesystem、AI credential、export、SynapseGit authorityはRust local serverが所有します。
- AI outputはProposalであり、人の採用前にAcceptedを変更しません。
- path、size、hash precondition、local reference、active behaviorを検査します。
- 外部origin、form、script、iframe、downloadなどの新しいactive behaviorは採用をblockします。
- exportにはAccepted site fileだけを含めます。
- error時はfail closedし、未確認のbytesをAcceptedへ入れません。

詳しい境界は
[ADR一覧](docs/adr/README.md)と
[checkpoint evidence](docs/implementation-status.md#checkpoints)にあります。

## 開発用command

| Command | 内容 |
| --- | --- |
| `pnpm build` | Web production bundleとRust workspaceをbuild |
| `pnpm check` | lock、TypeScript/Vitest、rustfmt、Clippy、docsを検査 |
| `pnpm test` | WebとRustのtestを実行 |
| `pnpm test:e2e` | production buildをChromiumでE2E検証 |
| `pnpm format:check` | Prettierとrustfmtを検査 |
| `pnpm check:docs` | Markdown、link、要件ID、traceabilityを検査 |
| `pnpm check:docker` | Docker local-build profileとimage非公開境界を検査 |
| `pnpm check:evidence` | C0–C11 evidence template、P0 traceability、manual gate、result schemaを検査 |
| `pnpm measure:performance-geometry` | 500 files / 50 MiB / 10,000 DOM nodesのsynthetic fixtureと72-case geometry matrixを測定 |
| `pnpm measure:production-integration-performance` | packaged production App、実Preview bridge/overlay、表示名autosave、ChangeSetを実測 |
| `pnpm verify:package` | source snapshotからlocal evaluation packageをbuildし、launcher/restart、browser smoke、synthetic/production performance、safe logs、checksumsを検証 |

通常の変更後は少なくとも`pnpm check`と関連testを実行してください。

`verify:package`はdefaultでdirty worktreeを拒否します。未commitの開発候補を明示的に
測定するときだけ`--allow-dirty`を付けます。packageとSHA-256 evidenceを保持する場合、
repository外に存在しないoutput directoryを指定してください。

~~~bash
mkdir -p /tmp/lp-studio-evidence
pnpm verify:package -- \
  --allow-dirty \
  --output-dir /tmp/lp-studio-evidence/candidate
~~~

保持された`LICENSE`は一般的なlicense grantではなく、repositoryにlicense termsが
記録されていない事実を明示するnoticeです。`THIRD-PARTY-NOTICES.md`もdependency
metadataのinventoryであり、upstream license textの代替ではありません。

## Repository map

~~~text
apps/
  web/                  React editor and review UI
  local-server/         Rust loopback API, storage, provider, SynapseGit boundary
packages/
  contracts/            strict TypeScript and JSON Schema contracts
templates/
  blank/                exportable blank LP
tests/
  e2e/                  real Chromium flows
docs/
  docker.md             Docker local-build evaluation guide
  user-guide.md         first-time user instructions
  ai-agent-guide.md     deterministic instructions for AI agents
  implementation-status.md
~~~

SynapseGit integrationは別repository
[`howlrs/synapsegit`](https://github.com/howlrs/synapsegit)のsource contractを
使用します。exact commitとcontract hashは
[`docs/synapsegit-contract.lock.json`](docs/synapsegit-contract.lock.json)に
固定されています。

## Documentation

読者ごとの入口は[docs/README.md](docs/README.md)にあります。

- 初めて使う人: [利用ガイド](docs/user-guide.md)
- 現在の完成範囲を知りたい人: [実装ステータス](docs/implementation-status.md)
- 開発に参加する人: [詳細要件](docs/detailed-requirements.md)と
  [実装計画](docs/implementation-plan.md)
- AI agent: [AI agent guide](docs/ai-agent-guide.md)
- 設計理由を確認する人: [ADR一覧](docs/adr/README.md)

## Repository、license、publicationの境界

このrepositoryはSynapseGitの実利用検証と進捗共有のためPublicです。
Public visibilityはLP StudioまたはSynapseGitのproduction利用、再配布、
brand利用の許諾を意味しません。licenseとbrand条件はrelease前に確定します。

制作中のprompt、annotation、revision metadata、credential、provider raw response、
SynapseGit internal dataはstatic LP exportへ混入させません。GitHubへのpublicationは
local exportとは別の、明示的なHuman actionです。
