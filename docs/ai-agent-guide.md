# AI agent implementation guide

この文書は、能力や文脈長が限られたAI agentが、このrepositoryを安全かつ
再現可能に変更するための手順書である。製品仕様そのものではない。

迷った場合は実装済みと推測しない。変更を止め、矛盾と不足情報を報告する。

## 1. 最初に固定する事実

1. 現在のcheckpointはC6完了、M1進捗65%である。
2. C7以降と80% local verification gateは未完了である。
3. この製品は単一user、単一端末、loopback-onlyのlocal applicationである。
4. AI出力は常にProposalであり、Accepted revisionではない。
5. 通常CIはdeterministic fake providerだけを使う。
6. current runtimeは`singleProposalPerProject: true`である。
7. Public repositoryであることは、製品の公開、配布許諾、license grantを
   意味しない。

## 2. 必須の読書順

作業開始前に、次を上から順に読む。途中の文書だけで実装を決めない。

1. [本書](ai-agent-guide.md)
2. [実装状況](implementation-status.md)
3. [現在の製品仕様](current-specification.md)
4. [詳細要件](detailed-requirements.md)の対象section
5. [ADR一覧](adr/README.md)と対象に関係するADR
6. [TypeScript contract](../packages/contracts/src/index.ts)
7. [JSON Schema](../packages/contracts/schemas/api-v1.schema.json)
8. 対象の実装fileと、その実装を直接検証するtest
9. [実装計画](implementation-plan.md)
10. [要件traceability](requirements-traceability.md)

外部SynapseGit境界を変更する場合は、さらに
[contract lock](synapsegit-contract.lock.json)と
[ADR-0004](adr/0004-synapsegit-generic-contract.md)を必ず読む。

## 3. Source of truthの優先順位

同じ内容に見える記述でも、用途ごとにsource of truthが異なる。

| 判断対象 | 優先するsource |
| --- | --- |
| 現在動くruntime behavior | source code、strict contract、test |
| APIまたはPreview bridgeのwire shape | TypeScript contract、JSON Schema、Rust validation |
| 現在の完了率、implemented、planned、既知制限 | `implementation-status.md` |
| 製品境界と弱めてはいけない原則 | `current-specification.md` |
| 採用済みのarchitecture decision | 対象ADR |
| M1の実装・試験可能な到達要件 | `detailed-requirements.md` |
| checkpointの順番とgate | `implementation-plan.md` |
| requirementごとの派生matrix | generatorと生成済み`requirements-traceability.md` |
| 初見user向け要約 | repository rootの`README.md` |

README、計画、型名だけを根拠に「実装済み」と判断してはいけない。
契約に型があっても、UIや永続化が存在するとは限らない。

二つの上位sourceが矛盾する場合は、都合のよい方を採用しない。該当箇所、
source codeの観察結果、必要な決定を短く示して停止する。

`requirements-traceability.md`は生成物である。matrixを直接編集してはいけない。
statusを変えるときはgeneratorの入力と実装証拠を更新し、再生成する。

## 4. 現在のimplementedとplanned

「一部implemented」は、表に書いた範囲だけを意味する。

| Area | 現在の状態 | 次の未実装範囲 |
| --- | --- | --- |
| blank Project | implemented | rename、delete、history等の製品UI |
| static site import | startup登録rootからのreviewed copy importをimplemented | archive upload等 |
| retained state | immutable revision、CAS、Accepted pointer、materialized `site/`をimplemented | pending ProposalとDecisionのrestart-safe recovery |
| Preview | separate scoped origin、CSP、sandbox、revocable URLをimplemented | cross-browser evidenceと追加hardening |
| TargetV1 | `page`、`block`、`element`、`text`、`point`、`region`をimplemented | reference Target、履歴UI、一般化したresolver corpus |
| resolution | `resolved`、`ambiguous`、`detached`のfail-closed resolutionをimplemented | C9/C10の精度・browser matrix |
| AI context | exact review、manifest、digest、redactionをimplemented | conversation履歴、streaming、cancel、screenshot、target最適化snippet |
| provider | `fake`を常時提供。server key設定時だけ`openai`を提供 | live-provider acceptance evidence |
| ChangeSetV1 | `create_text`、`replace_text`、`rename`、`delete`をimplemented | binary生成はv1 non-goal。将来のbounded asset adapterとprotected-range protocolは未実装 |
| Proposal validation | cloneへのatomic apply、path/hash/syntax/reference/Target検証をimplemented | C7の完成したdiff/review workflow |
| Human Decision | contract/serverは`adopted_unchanged`、`rejected`、`deferred`に対応 | 現在のReviewDrawer UIはAdoptだけ。Reject/Defer UIとrestart reconciliationはC7 |
| active-behavior warning | blocking warningがあるProposalのAdoptをserverでも拒否 | Reject/Deferへ到達するUI |
| sequential work | 一Project、一Proposalのbaselineだけ | multi-Proposal historyとiterative admission |
| export | Acceptedだけのdeterministic ZIP baselineをimplemented | C8のpublication record、history/settings、統合handoff |
| SynapseGit | pinned source contractを使うreal Proposal/Decision baseline | released generic contract、durable end-to-end orchestration |
| GitHub publication | development checkpointのGit push/PRは別運用 | 製品からのpublicationは未実装で、常に別のHuman action |
| M1 release | 未完了、production-readyではない | C7からC11、80% gate、license/brand/release判断 |

OpenAI live testは外部network、credential、billing、account availabilityを必要とし、
通常testでは意図的にignoredである。実行していないlive testを証拠にしない。

## 5. 用語集

| 用語 | このrepositoryでの意味 |
| --- | --- |
| Creator | LPを操作し、Human Decisionを行うlocal user |
| local-first | Editor、server、stateの基本authorityがlocal/loopbackにあること。OpenAI選択時までofflineという意味ではない |
| Project | applicationが管理する一つのstatic site |
| revision | LP Studioのsite snapshot identity。Git commitではない |
| Accepted revision | Human Decision後に採用済みとなったimmutable file manifest |
| Accepted pointer | 現在のAccepted revisionを指すapplication-owned pointer |
| Proposal | Accepted baseに束縛された、未採用の変更候補 |
| Proposed Preview | Proposal bytesの表示。Acceptedではない |
| TargetV1 | 変更対象を表すstrict six-kind contract |
| resolution receipt | Targetを現在のrevisionへ再解決したserver由来の結果とID |
| AI context | Creatorが送信前に確認できる、digest-bound provider input |
| ChangeSetV1 | AIが返すstrictな四操作のtext-file変更protocol |
| Human Decision | `adopted_unchanged`、`rejected`、`deferred`の終端判断 |
| one-shot approval | project、review、intent、期限に束縛された一回限りのhost approval |
| manifest | path、media type、byte length、SHA-256等のcanonical file一覧 |
| CAS | immutable file bytesをdigestで管理するcontent-addressed storage |
| materialized `site/` | Accepted manifestから作る実体view。import元directoryではない |
| Editor | privileged local UI。Previewとは別origin |
| Preview | untrusted LPを実行するunprivileged、scoped origin |
| bridge | response時だけ注入されるPreview通信code。source/exportへ保存しない |
| fake provider | networkとcredentialを使わないdeterministic test provider |
| redaction | 既知patternをprovider contextから置換する防御。全secret検出の保証ではない |
| caller-supplied attribution | model実行者の申告値。SynapseGitによる実行検証ではない |
| checkpoint | testと証拠を伴うweighted implementation milestone |

## 6. 絶対に弱めないinvariant

次の変更は、便利に見えても行わない。

- AI応答やProposal作成だけでAccepted pointerを進めない。
- Proposalを部分採用、直接編集、自動採用しない。
- `rejected`と`deferred`でAccepted bytesを変更しない。
- blocking active-behavior warningが残るProposalをAdopt可能にしない。
- EditorとPreviewを同一originにしない。
- Previewへsession secret、AI credential、filesystem path、Synapse authorityを渡さない。
- browser入力から任意のserver path、Ref、OID、permit、authorityを選ばせない。
- import元directoryをAccepted viewとして直接編集しない。
- Targetを座標、class、textだけで推測して自動接続しない。
- stale、ambiguous、detachedなTargetでcontextやProposalを作らない。
- providerへ送るexact contextとCreatorがreviewしたcontextを変えない。
- 検出対象のcredential-like value、Bearer token、common local absolute pathを
  provider contextでredactする。redactionを完全なsecret検出器と主張しない。
- site fileのcontextがredactedなら、ChangeSetV1生成をprovider実行前に拒否する。
- ChangeSetをAccepted上で直接実行しない。cloneへ全操作をapplyしてから全体を検証する。
- stale base、hash mismatch、unsafe path、unknown field、unknown operationを許容しない。
- failed generationまたはfailed validationでProposal workspaceを完成扱いしない。
- provider credentialやraw provider failure bodyをbrowser、project、log、exportへ残さない。
- exportへ会話、Target、bridge、Studio metadata、credential、Synapse internal dataを混入させない。
- SynapseGitがmodelを実行・検証したと表示しない。現在のattributionは
  `caller-supplied`かつ`execution未検証`である。
- `singleProposalPerProject: true`をC7のrecovery設計なしに解除しない。
- Public repository、development push、checkpoint PR、統合済みの`main`
  baselineをproduct publicationやlicense grantと呼ばない。

## 7. Taskからfileへのmap

| Task | 最初に読むfile | 主な変更先 | 必須の近接test |
| --- | --- | --- | --- |
| product wording/boundary | `current-specification.md` | root `README.md`、対象docs | `pnpm check:docs` |
| requirement変更 | `detailed-requirements.md` | 同file、必要なADR、traceability generator | docs checkとtraceability check |
| checkpoint/status変更 | `implementation-status.md` | status、planの該当箇所、generator evidence | full gateの実測結果 |
| API DTO/guard | `packages/contracts/src/index.ts` | TypeScript contract、JSON Schema、Rust DTO/validation | contract、Web、Rust test |
| Preview bridge | `apps/web/src/preview/bridge.ts` | bridge、schema、Rust preview runtime | bridge test、Rust test、Chromium E2E |
| Editor UI/state | `apps/web/src/App.tsx` | `App.tsx`、`studio-reducer.ts`、`styles.css` | `apps/web/src/test/` |
| API client | `apps/web/src/api/client.ts` | client、contract guard | `api-client.test.ts`、contract test |
| local route/authority | `apps/local-server/src/lib.rs` | router、handler、server validation | `apps/local-server/src/tests.rs` |
| retained storage/import | `apps/local-server/src/storage.rs` | storageとserver integration | Rust storage/integration test、E2E |
| ChangeSet | `apps/local-server/src/change_set.rs` | parser、apply、static checks、contract/schema | Rust unit/integration、Web contract test |
| AI provider | `apps/local-server/src/ai_provider.rs` | provider adapterとsafe error mapping | fake/adapter unit test。live testは明示承認時のみ |
| launcher/env | `apps/local-server/src/main.rs` | listener、state-root lease、env validation | binary/launcher test |
| browser vertical slice | `tests/e2e/` | Playwright fixtureと必要なproduct code | `pnpm test:e2e` |
| upstream SynapseGit pin | contract lock、ADR-0004 | workspace dependency、lock、parity script | lock/parity、Rust integration、upstream evidence |

契約変更は一fileで完了しない。TypeScript guard、JSON Schema、Rust validation、
client/server binding、test fixtureを同じ意図で揃える。

## 8. Exact commands

全commandはrepository rootで実行する。

### 8.1 初回setup

```bash
pnpm install --frozen-lockfile
pnpm build
```

toolchainはNode.js 24.14.1、pnpm 10.33.0、Rust 1.95.0へ固定されている。
versionを勝手に上げない。

### 8.2 local applicationの起動

終了時にdataを消してよい安全な起動:

```bash
pnpm dev:server
```

保持する起動:

```bash
LP_STUDIO_STATE_ROOT=.studio-data pnpm dev:server
```

`LP_STUDIO_READY`に出る`editorOrigin`をbrowserで開く。portは通常randomである。
明示state rootは実directoryで、Unixではmode `0700`が必須である。同じrootを
二つのserver processで同時使用できない。

`pnpm dev`はVite UIだけであり、完全なlocal applicationではない。

### 8.3 対象別check

Webだけ:

```bash
pnpm --filter @synapsegit-lp/web check
```

Rustだけ:

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

docsだけ:

```bash
pnpm check:docs
pnpm exec prettier --check README.md docs
```

contract lockだけ:

```bash
pnpm check:locks
```

### 8.4 完了前のfull gate

省略せず、この順で実行する。

```bash
pnpm check
pnpm test
pnpm build
pnpm test:e2e
pnpm format:check
pnpm check:docs
git diff --check
git status --short
```

test数は増減し得る。古い文書の件数ではなく、実際のexit codeと今回の出力を
報告する。OpenAI live testは通常gateに含めない。

## 9. 安全なedit workflow

1. `git status --short --branch`でbranchと既存変更を確認する。
2. 対象checkpoint、変更するbehavior、non-goal、必要testを一文ずつ固定する。
3. 必須読書順に従い、現在のcode pathと既存testを確認する。
4. unrelatedなuser changeがある場合は保持し、同じfileで衝突するなら停止する。
5. 最小patchで実装する。unknown fieldを黙って受け入れる互換処理を足さない。
6. failure、stale、forged、duplicate、limit境界のtestを先に追加または同時追加する。
7. 対象別checkを実行する。失敗原因を直さず期待値だけを緩めない。
8. full gateを実行する。E2Eをunit testで代用しない。
9. `git diff`と`git diff --check`で意図外変更、generated drift、secretを確認する。
10. 実装証拠が揃ってからstatus、traceability generator、known limitationを更新する。
11. commit、push、PR更新は依頼またはcheckpoint protocolの権限範囲だけで行う。
12. product publication、release、tag、merge、license変更は別のHuman actionとして止める。

formatterが広範囲を変更した場合、無関係な差分をそのまま含めない。ただし既存の
user変更をreset、checkout、削除してはいけない。

## 10. よくある誤った推測

| 誤り | 正しい判断 |
| --- | --- |
| C6完了なのでM1は完成 | 65%であり、C7からC11は未完了 |
| contractにDispositionが三つあるのでUIも三つある | current ReviewDrawerはAdoptだけ |
| `deferred`は後で同じProposalを再開できる | terminal Decision。再開は新しいProposalが必要 |
| Proposalはrestart後もDecisionできる | pending authorityのrestart recoveryはC7 |
| 一度Adoptした後も同Projectで何度でも生成できる | current capabilityは一Project、一Proposal |
| `pnpm dev`で製品全体が起動する | Viteだけ。authority serverは`pnpm dev:server` |
| OpenAIは常に利用可能 | server processにkeyがある場合だけbootstrapへavailableとして出る |
| local-firstなのでOpenAIでもdataは外へ出ない | OpenAI選択時はreviewしたcontextを外部送信し、料金が発生し得る |
| redactionが全secretを取り除く | 既知patternに対するbounded defense。利用者の確認を置き換えない |
| redaction済みsite contextなら安全に生成できる | ChangeSetV1ではgeneration自体をprovider call前に拒否 |
| blocking warningならRejectもできない | serverが止めるのはAdopt。Reject/Defer UIはまだない |
| importは元directoryを編集する | managed workspaceへcopyし、元を変更しない |
| Preview DOMのnode handleは永続identity | render-local hintであり、diskやauthorityへ使わない |
| SynapseGitがmodel実行を証明する | 現在はcaller-supplied attribution、execution未検証 |
| host-retained Accepted bytesはSynapseGit checkout | application-owned Accepted viewであり、そのclaimはしない |
| exportがあるのでC8完了 | baseline ZIPはあるがC8全体はplanned |
| exportはdeployまたはGitHub publicationである | Accepted ZIPのlocal生成だけ。remote writeは別のHuman action |
| Public repositoryなので自由に再配布できる | license/brand/release gateは未完了 |
| traceability matrixを直接直せばstatus更新になる | generator入力を直し、生成・checkする |
| shared stream/cancel typeがあるのでruntimeも完成 | typeの存在はcapabilityの証拠ではない |
| checkpoint完了なら割当要件がすべて実装済み | requirementごとにsource、test、matrixを確認する |

## 11. StopしてHumanへ確認する条件

次のどれかに該当したら、実装を継続しない。

- Accepted/Proposal分離、Human Decision、Preview分離、export purityを弱める必要がある。
- code、test、contract、status、ADRが互いに矛盾する。
- API/schema version、strictness、canonicalization、digest計算を変える。
- SynapseGit commit pin、contract hash、capability、claim boundaryを変える。
- license、brand、release、tag、merge、product publicationを扱う。
- OpenAI live call、外部billing、credential使用が必要になる。
- project state、import元、export先、Git historyを削除または上書きする。
- 新dependency、network access、shell/package executionを製品へ追加する。
- secret、token、private path、personal dataをdiff、log、fixtureで発見する。
- redacted site fileをChangeSetV1で編集する必要がある。
- pending Proposal recoveryやsequential ProposalをC7設計なしで実装する必要がある。
- unrelatedな既存変更と同じfileを安全に分離できない。
- implemented、secure、verified、production-readyというclaimをtestで証明できない。

質問には、止まったfileと行動、観察した事実、選択が必要な一点を書く。
安全境界を推測で越える代替案は実行しない。

## 12. Completion checklist

完了報告前に、すべてを確認する。

- [ ] 変更scopeとnon-goalが明確である。
- [ ] 必須文書、対象code、近接testを読んだ。
- [ ] Accepted、Proposal、Human Decisionの境界を維持した。
- [ ] Editor、Preview、provider、SynapseGitのauthority境界を維持した。
- [ ] contract、schema、Rust、Webのshapeが一致する。
- [ ] happy pathとfail-closed pathをtestした。
- [ ] 対象別checkが成功した。
- [ ] full gateが必要な変更では、全commandが成功した。
- [ ] `requirements-traceability.md`を直接編集していない。
- [ ] generated fileがsourceと同期している。
- [ ] diffにsecret、credential、private path、raw provider bodyがない。
- [ ] unrelatedなuser変更を削除、上書き、stageしていない。
- [ ] implementedとplannedを分け、未実行testを証拠にしていない。
- [ ] test数ではなく今回のexit resultを記録した。
- [ ] known limitationと次checkpointを正確に残した。
- [ ] commit/push/publicationの権限範囲を越えていない。
- [ ] M1、production、license、verified executionを過大主張していない。

一項目でも確認できない場合、作業は「完了」ではない。
