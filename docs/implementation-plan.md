# SynapseGit LP Studio 実装計画

Status: proposed execution plan

Last updated: 2026-07-19

Requirements: [detailed-requirements.md](detailed-requirements.md)

Traceability: [requirements-traceability.md](requirements-traceability.md)

## 1. 目的

本計画は、M1 Product MVPを100%とした実装順、進捗weight、
SynapseGit利用、Git checkpoint、GitHub可視化、80%時点の
Creatorによるローカル確認を定義する。

実装は80%到達時に一度停止する。起動手順、確認scenario、
既知制限、GitHub上のbranch/PR、SynapseGit recordを報告し、
Creatorの確認結果を受けるまで残り20%へ進まない。

## 2. 実装原則

1. Accepted stateとProposalを常に分離し、AI出力を自動採用しない。
2. PreviewをEditorとは別originに置いてからuntrusted LPを実行する。
3. 危険な境界を縦sliceで早期検証し、画面だけを先に作り込まない。
4. fake AIとlive AIを同一adapter contractで扱い、通常CIはfakeを使う。
5. SynapseGit v0.3.0の未実装機能を実装済みと見なさない。
6. SynapseGitの不足はworkaroundだけで閉じず、重複確認後に
   acceptance-bounded Issueとしてfeedbackする。
7. Git commitはreview可能な小さなcheckpointにし、各checkpointをpushして
   GitHub draft PRの進捗表を更新する。
8. 分割可能な定型実装・test追加・fixture整備はサブエージェントへ委譲し、
   主agentがcontract、統合、security、final verificationを確認する。
   実行surfaceにmodel/effort selectorがない場合、下位model使用を過大主張しない。
9. 既存のuser changeと無関係なfileをstageしない。
10. 80% handoff前に、Creatorがブラウザで主要flowを操作できる状態にする。

## 3. 推奨initial architecture

最終決定はADRに記録するが、初期実装の推奨baselineは次とする。

| Area | 推奨 |
| --- | --- |
| Workspace/package manager | pnpm workspace、Node/Rust toolchainとlockfile固定 |
| Web UI | TypeScript SPA、React、Vite |
| Local server | Rust、Axum、Editor/API用IPv4 loopback listener |
| Preview | session別random portのRust static listener、ephemeral bridge |
| Shared validation | canonical JSON Schema、generated TS/Rust types、golden parity test |
| Local metadata | SQLite command journal + filesystem CAS + materialized Accepted view |
| Test | TS/Rust unit/integration、Playwright browser E2E、deterministic fake AI |
| SynapseGit | exact pinned Rust use-case/companion contract |
| Styling | repository-owned CSS、初期版は大規模UI frameworkへ依存しない |

採用理由は、untrusted Preview、filesystem、SQLite journal、SynapseGit Coreを
同じRust authority境界で直列化し、React側とはgenerated contractだけを共有
できるためである。Studio metadata DBとSynapseGit Ref DBは別databaseとし、
browserへraw DB/Ref authorityを公開しない。M1ではElectron/Tauriを導入せず、
通常browserとloopback launcherを使う。

## 4. 進捗weightとcheckpoint

進捗は「file数」や「生成code量」ではなく、下表のacceptanceを満たした
checkpointのweightで計算する。途中作業は完了percentageへ加算しない。

| Checkpoint | Weight | 累計 | Scope / exit criteria |
| --- | ---: | ---: | --- |
| C0 Requirements and early ADR baseline | 5% | 5% | 詳細要件、計画、traceability、architecture/storage/license ADR、docs検証、draft PR |
| C1 Upstream Synapse generic contract | 15% | 20% | blocker Issue、generic file-tree Proposal、one-disposition durable receipt/query、versioned contract |
| C2 Real-boundary M0 vertical slice | 10% | 30% | blank、element Target、fake AI、review、adopt、exportをreal adapter boundaryでE2E |
| C3 Project/revision/import | 8% | 38% | managed storage、manifest/hash、bounded copy import、drift検出 |
| C4 Separate-origin preview | 8% | 46% | random Editor/Preview origin、session、bridge、Accepted/Proposed static serving |
| C5 Target v1 and resolution | 10% | 56% | page/block/element/text/point/region、coordinate、tree、fail-closed resolver |
| C6 AI context and ChangeSet | 9% | 65% | exact context review、fake/live adapter、ChangeSet v1、isolated Proposal |
| C7 Review, Decision, recovery | 8% | 73% | diff/render、adopt/reject/defer/stale、Synapse-backed SQLite journal/reconciliation |
| C8 Export, publication, integrated E2E | 7% | **80%** | deterministic/pure export、LP publication exact-byte review、history/settings、handoff runbook |
| C9 Security and fault hardening | 10% | 90% | malicious Preview、disk full、process kill、outcome unknown、migration/retention |
| C10 Acceptance evidence | 7% | 97% | a11y、performance、overlay accuracy、browser/security matrix、live-provider evidence |
| C11 M1 completion gate | 3% | 100% | local packaging/support matrix、license/brand status、completion docs、final evidence |

### 4.1 80%の意味

80%はM1完了ではない。Creatorが価値と操作感を確認できる
「主要flowが動き、残りがintegration hardeningとrelease品質に限定された状態」
を意味する。C0からC8の全exit criteriaと、次の条件を満たす必要がある。

- blank/importの両方でEditorを開ける。
- 全Target kindを選び、AI contextを確認できる。
- fake AIで決定的なProposalを作成できる。
- live providerを設定した環境では同じcontractでProposalを作成できる。
- Accepted/Proposed renderingとdiffを確認できる。
- adopt/reject/defer、stale guard、restart recovery baselineが動く。
- Accepted static exportを生成できる。
- version pinされたreal SynapseGit Proposal/Decision receiptと実保証範囲を
  UIで確認できる。stub/local-onlyをverified/admittedと表示しない。
- privacy-filtered LP publication fileをexact-byte previewできる。
- local start/stop/reset手順と確認checklistがある。
- branchとdraft PRが最新checkpointまでpush済みである。

upstreamがstub-only、export purity/determinismが未確認、
Accepted corruptionまたはoutcome-unknownの未解決riskがある場合、
算術上の作業量が80%相当でも80% gate完了と報告しない。

## 5. Checkpoint実行protocol

各C0–C11で次を順に行う。

1. **Intent**
   - 対象要件ID、base Git commit、変更scope、non-goal、expected testを固定する。
   - SynapseGit利用可能後はcheckpoint intentのdigestをlocal recordへ保存する。
2. **Proposal**
   - 分離したbranch/workspaceで実装する。
   - AI支援による変更はAI-attributed Proposalとして扱い、verified generationを
     SynapseGitが保証しない限りそのように表示しない。
3. **Validation**
   - scopeに応じたunit/integration/E2E/security checkを実行する。
   - diff、untracked file、generated artifact、secretを確認する。
4. **Checkpoint record**
   - file manifest、test summary、known limitation、Git commit候補を
     privacy-filteredなcheckpoint recordへ束縛する。
   - SynapseGit generic contract完成前はtrusted-operator observationとし、
     Human DecisionやCreative AI admissionを偽装しない。
5. **Git/GitHub**
   - 関連fileだけを明示stageし、1つの意図を表すcommitを作る。
   - checkpoint branchへpushし、draft PRの進捗表・check結果・Issue linkを更新する。
6. **Feedback**
   - SynapseGitの不足や回避策が出た場合は既存Issueを検索し、
     commentまたは新規Issueへ最小再現と受入条件を記録する。
7. **Decision**
   - 80%前のcheckpoint pushは進捗公開であり、CreatorのHuman adoptionを
     自動的に意味しない。
   - Creatorによるローカル確認結果は80% handoff後に明示的に記録する。

## 6. Git branch、commit、PR方針

- default branchから `agent/lp-studio-m1` を作る。
- 最初のpushでdraft PRを作り、M1の全checkpointを原則同じPRで可視化する。
- commit messageは `docs:`、`chore:`、`feat:`、`test:`、
  `fix:` 等の短いscopeを使う。
- C1以降、利用可能ならcommit bodyへ
  `Synapse-Checkpoint: <public-safe identifier>` trailerを付ける。
- secret、absolute local path、private prompt、raw provider response、
  internal Synapse authorityをcommit/PRへ含めない。
- checkpointごとにpushする。test未完、Accepted corruption、
  security boundary破損を既知のまま「完了」としてpushしない。
- force push、tag、merge、releaseはこの計画では行わない。
- PRは80% local確認が完了するまでdraftを維持する。

## 7. SynapseGitの開発中利用

現行v0.3.0にはgeneric source-file Proposal contractがないため、
利用を次の段階に分ける。

### Phase S0: C0

- v0.3.0 version/capabilityと不足をevidenceとして記録する。
- generic-file contractを
  [#22](https://github.com/howlrs/synapsegit/issues/22)、durable pending reviewを
  [#23](https://github.com/howlrs/synapsegit/issues/23)、LP publicationを
  [#17 comment](https://github.com/howlrs/synapsegit/issues/17#issuecomment-5013850363)
  で追跡する。
- unsupported機能をverified/admittedと表示しない。

### Phase S1: C1

- SynapseGit側へgeneric file-tree Proposal、durable one-disposition
  Decision receipt/query、version/capability contractを追加する。
- upstream Issue、branch、commit、test、draft PRをLP Studioのtask/ADRへlinkする。
- LP Studio側へcanonical contract fixtureとadapter parity testを追加する。
- M1開発中はexact contract version + commitをpinする。
  tag/releaseは別の明示的release operationとする。

### Phase S2: C2–C8

- Target v1 digest、base/output manifest、AI attribution、
  validation summaryをcheckpointへbindingする。
- adopt/reject/deferのSQLite command journalをreal Synapse receipt/queryへ接続する。
- exact capability、caller-supplied/verified claim、failure/reconciliationをUIに表示する。
- development checkpointはAI-attributed/trusted-operator evidenceとして記録し、
  CreatorがまだreviewしていないHuman Decisionを作らない。
- LP-specific provider-neutral publicationをC8までにexact-byte review可能にする。

### Phase S3: C9以降

- fault injection、malicious Preview、disk full、outcome unknown、
  accessibility/performance evidenceでcontractをhardeningする。
- 80% local review後、Creatorの実際のDecisionだけをHuman Decisionとして記録する。
- evidence-drivenなupstream follow-upは新規/既存Issueへ反映する。

## 8. Subtask分割

サブエージェントへ委譲できるbounded task:

- shared schema、fixture、unit testの機械的追加
- isolated packageのimplementationとtest
- CSS/componentのaccessibility修正
- malicious preview fixture、path fuzz case、export fixture
- docs/PR statusの事実確認

主agentが直接review・統合するtask:

- trust boundary、session/Preview origin、filesystem authority
- Target/coordinate canonical contract
- Accepted pointerとDecision saga
- SynapseGit mapping/claim/version compatibility
- checkpoint commit scope、push、PR、Issue write
- 80%判定とCreator handoff

## 9. 80% local verification handoff

C8完了後、次を報告して停止する。

1. branch、latest commit、draft PR URL、累計80%の根拠
2. SynapseGit checkpoint/Issue URLと、保証できるclaimの範囲
3. prerequisiteとinstall command
4. exact start commandと表示されるlocal URL
5. sample project/fake AIを使う10分以内の確認scenario
6. live providerを使う場合のsecretをcommitしない設定方法
7. data root、reset/backup方法
8. passしたtest、未実行のmanual test
9. 既知制限と残りC9–C11
10. feedbackの記録場所

推奨確認scenario:

1. blank projectを作成する。
2. desktop/mobile viewportを切り替える。
3. hero block、heading、blank regionを選ぶ。
4. fake AIへ修正を依頼し、送信contextを確認する。
5. Accepted/Proposed renderingとdiffを比較する。
6. 1件をdeferし、新Proposalとして再開されることを確認する。
7. 1件をadoptし、stale Proposalが採用できないことを確認する。
8. restart後にAccepted stateが維持されることを確認する。
9. static exportを開き、Editor metadataがないことを確認する。
10. SynapseGit status/claimとGitHub-ready record previewを確認する。

Creatorから確認結果を受けるまでC9へ進まない。

## 10. Blocker handling

- SynapseGit不足: 詳細要件Section 13.6に従いIssue化し、
  M1 blockerかtemporary compatibility modeかを明示する。
- framework/API不明: primary documentationとinstalled versionで確認し、
  推測したcontractをproduction codeへ入れない。
- dependency download failure: environment/sandbox問題を切り分けて再試行する。
- test failure: checkpointを完了扱いにせず、root causeとAcceptedへの影響を記録する。
- license/brand: C11まで先送りせず、C0でstatus/ownerを可視化し、
  M2のproduction、distribution、external deliverable前に書面条件を解決する。
- user inputが必要なproduct decision: 安全なdefaultで進められなければ停止し、
  選択肢、影響、推奨を示す。

## 11. Status update rule

このfileのcheckpoint tableにstatus、commit、test、Issue/PR linkを追記する。
累計percentageは完了checkpointだけを合計し、見込みや作業中の値を足さない。
GitHub draft PRの表とrepository-local statusが食い違う場合、
Git commitに含まれる本fileを基準に再同期する。
