# SynapseGit LP Studio はじめての利用ガイド

この文書は、LP制作や開発ツールに詳しくない人が、現在の開発版をローカルで
試すためのガイドです。記載内容は65% baseline以降を統合したlocal開発・評価版に
合わせています。

> **重要:** CreatorによるUX確認、manual screen-reader、live provider証跡、
> license/brand/merge/release判断は未完了で、production-readyではありません。
> 実在する顧客情報、公開前の
> 機密情報、production用credentialを使わず、評価用のデータで試してください。

## 1. このアプリでできること

SynapseGit LP Studioは、自分のPC上で静的なランディングページ（LP）を作る
ローカルファーストのWebアプリです。

現在の開発版では、次の流れを試せます。

1. 空のLPを作る、または既存の静的LPをコピーして取り込む。
2. プレビューから変更したい場所を選ぶ。
3. AIへ伝える要望と、実際に送るコンテキストを確認する。
4. AIが返した変更を`Proposal`として別の作業領域に作る。
5. AcceptedとProposedの表示、変更ファイル、diff、検査結果を確認する。
6. Proposal全体を採用、却下、または終端の保留にする。
7. Acceptedだけをstatic ZIPへ出力するか、privacy-filteredなpublication draftを
   exact-byte reviewしてローカルZIPへ出力する。

AIの出力が自動で公開中のLPを書き換えることはありません。現在の画面で
`変更を採用`を押したときだけ、Proposalが新しいAccepted revisionになります。

## 2. 最初に覚える4つの言葉

| 言葉 | このアプリでの意味 |
| --- | --- |
| Project | 1つのLPと、そのローカルな制作状態 |
| Accepted | 人が採用済みの、現在の正式なLP |
| Proposal | AIが作った候補。まだAcceptedではない |
| Target | AIへ変更を頼む対象。ページ、ブロック、要素、テキスト、座標、領域のいずれか |

画面に表示されるrevision IDはLP Studio内のsite snapshotを表し、Git commitでは
ありません。また、local-firstは基本authorityがlocalにあるという意味です。
OpenAIを選んだ場合までofflineという意味ではありません。

画面にある`caller-supplied`と`execution未検証`は、モデル名などの帰属情報を
呼び出し側が渡しており、SynapseGit自身がAIを実行または検証したわけではない、
という注意表示です。

## 3. 現在の対応環境と必要なもの

現在の開発・評価版はLinux x86-64 GNUまたはWSLと、固定したChromium系ブラウザを
local evidence対象にしています。これはrelease supportやcross-browser supportの
宣言ではありません。SafariやFirefoxを含む他の環境は未検証です。

WSLをNode/Rust dependencyで汚したくない場合は
[Docker利用ガイド](docker.md)のlocal-build profileを使用できます。その場合、host側に
必要なのはDocker Desktop/Engine 28以降とChromiumで、Node/pnpm/Rustはimage内で使用します。

Native起動に必要なruntimeは次の固定versionです。

| 項目 | Version |
| --- | --- |
| Node.js | 24.14.1 |
| pnpm | 10.33.0 |
| Rust | 1.95.0 |
| Browser | Navigation APIを利用できるChromium系ブラウザ |

リポジトリのルートで、versionを確認します。

```bash
node --version
pnpm --version
rustc --version
```

期待する値は、それぞれ`v24.14.1`、`10.33.0`、`rustc 1.95.0`です。

## 4. fake providerで起動する

まずは外部AIへ何も送信しないfake providerで試すのが安全です。Dockerでは
`docker compose up --build`を実行し、`http://127.0.0.1:4173`を開きます。以下はnative
toolchainを使う場合の手順です。

リポジトリのルートで次を実行します。

```bash
pnpm install --frozen-lockfile
pnpm build
LP_STUDIO_STATE_ROOT=.studio-data pnpm dev:server
```

起動が完了すると、terminalに次の形式の1行が表示されます。

```text
LP_STUDIO_READY {"editorOrigin":"http://127.0.0.1:...","previewOrigin":"http://127.0.0.1:...","operatingMode":"normal"}
```

Chromium系ブラウザで`editorOrigin`のURLを開いてください。`previewOrigin`は
分離されたプレビュー用listenerなので、直接開く操作画面ではありません。

terminalは利用中ずっと開いたままにします。停止するときは、そのterminalで
`Ctrl+C`を押します。

`LP_STUDIO_STATE_ROOT=.studio-data`を指定すると、Accepted projectは次回起動にも
残ります。指定しない場合はprocess専用の一時directoryが使われ、終了時に
削除されます。

## 5. 空のProjectから1件の変更を完成させる

### 5.1 Projectを作る

1. 起動画面で`空のLPを作成`を押す。
2. `LPプレビュー`の読み込みが終わるまで待つ。
3. 必要なら上部の`デスクトップ`、`タブレット`、`モバイル`で幅を変える。
4. Headerの`Project`欄は表示名です。変更すると自動保存され、opaque project IDや
   filesystem rootは変わりません。`保存済み`を確認してから画面を移動します。
5. Preview toolbarの`75%`/`100%`は表示倍率です。Targetのauthorityやsource bytesを
   変更しません。

### 5.2 Targetを選ぶ

1. プレビュー上部の`選択モード`を選ぶ。
2. 左側の`ターゲット種別`で`要素`を選ぶ。
3. プレビュー内の見出し`まだ、白紙です。`をクリックする。左の要素一覧から
   同じ見出しを選んでもよい。
4. 右側の`選択中のターゲット`に対象が表示され、`Resolution`が`resolved`に
   なったことを確認する。

Targetは`ページ`、`ブロック`、`要素`、`テキスト`、`座標`、`領域`の6種類です。
空のLPでfake providerを試す最初の操作には、見出し、説明文、CTAのいずれかの
`要素`を使ってください。

### 5.3 要望と送信内容を確認する

1. `AI provider`で`Local deterministic fake`を選ぶ。
2. `Model`が`Deterministic v1`であることを確認する。
3. `AIへの要望`へ、たとえば
   `見出しを、LP制作の目的が伝わる短い日本語にしてください`と入力する。
4. `送信内容を確認`を押す。
5. DialogでProvider、Model、対象ファイル、redactionの有無、正確なcontextを
   確認する。
6. 問題がなければ`変更案を作成`を押す。

処理中はphaseと経過秒が表示されます。中止する場合は`AI処理を取り消す`を一度だけ
押してください。cancel済みattemptのlate provider responseはProposalへ昇格しません。
再実行は同じattemptの上書きではなく、新しい送信確認から新attemptとして行います。

fake providerは一般的な生成AIではありません。入力した要望とTargetのbindingを
検証しますが、空templateにある次の3要素へ固定の変更を返すデモ用providerです。

| Target | 変更後の固定text |
| --- | --- |
| ヒーロー見出し | 対話から、公開できるLPへ。 |
| ヒーロー説明文 | 要望を選び、AIとの対話から公開できるLPへ育てます。 |
| ヒーローCTA | 公開LPをつくる |

固定のデモ変更以外を評価する場合はOpenAI adapterを選びます。OpenAI adapterでも、
希望した品質や内容の出力が得られることを保証するものではありません。

### 5.4 Proposalを確認してDecisionする

1. `変更案を確認`で、変更ファイルと`Text diff`を読む。
2. `Acceptedを表示`と`Proposedを表示`を切り替えて見た目を比べる。
3. `Validation`にblocking warningがないことを確認する。
4. 必要なら`非公開メモ（任意）`へ判断理由を書く。
5. 内容に同意するときだけ`変更を採用`を押す。採用しない場合は
   `変更案を却下`または`今回は保留`を選ぶ。
6. Adoptでは上部の`Accepted revision`が変わること、Reject/Deferでは変わらない
   ことを確認する。

現在はProposalの一部だけを採用したり、Proposalを画面内で直接編集したりは
できません。`変更を採用`は変更ファイル全体をそのまま採用します。

自動検査がpassedでも、文章の正確さ、デザイン品質、著作権、完全なsecurityや
accessibilityを保証するものではありません。最終判断は利用者が行います。

### 5.5 さらに変更したい場合

1つのProjectで同時に扱うactive Proposalは1件です。Adopt、Reject、Deferのいずれかで
terminalにした後は、serverを再起動せずに新しいTargetと要望から次のProposalを
作れます。Review historyには各terminal Decisionが残ります。

Deferは同じProposalを再びDecision可能にする一時停止ではありません。続きを作るときも
最新Acceptedをbaseにした新しいProposalとなり、旧Proposalとの関連が記録されます。

Decision前にserverを止めても、同じstate rootで再起動すると保存済みReviewを再開します。
Decision結果が確定した可能性とlocal receipt/pointerが一致しない場合は、画面の
`Decision結果を再照合`を使います。blind retryで二度目のDecisionを送る操作ではありません。

## 6. 既存の静的LPを取り込む

ブラウザから任意のdirectoryを選ぶ方式ではありません。server起動時に、
取り込み元を1つだけ登録します。

### 6.1 取り込み元を準備する

取り込み元は、build済みの静的site directoryにします。少なくともroot直下に
`index.html`が必要です。state rootとは別の実directoryを使い、symlinkは使わないで
ください。

例:

```text
/home/me/example-lp/
  index.html
  assets/
    style.css
    app.js
```

### 6.2 importを有効にして起動する

`/absolute/path/to/built-lp`を実際の絶対pathに置き換えます。

```bash
LP_STUDIO_STATE_ROOT=.studio-data \
LP_STUDIO_IMPORT_ROOT=/absolute/path/to/built-lp \
pnpm dev:server
```

`LP_STUDIO_IMPORT_ROOT`と`LP_STUDIO_STATE_ROOT`は、同じdirectoryでも親子directoryでも
いけません。

### 6.3 画面でコピー内容を確認する

1. `editorOrigin`を開く。
2. `登録済みディレクトリを取り込む`を押す。
3. `取り込むファイルを確認`Dialogで、Entry point、コピー対象、コピーしない
   ファイル、bytes、SHA-256、上限、warningを読む。
4. RootのEntry pointが`index.html`であることを確認する。
5. 内容に同意するときだけ`この内容をコピーして取り込む`を押す。
6. 新しいProjectのプレビューを確認する。

取り込み元は変更されません。確認したファイルだけが新しいProjectへコピー
されます。取り込み後のAI操作は空のProjectと同じですが、fake providerが変更できる
のは、空templateと同じIDと変更前textを持つ固定3要素だけです。一般的な既存LPで
生成を評価する場合はOpenAI adapterを選びます。

## 7. OpenAI adapterを使う

OpenAIを使わない場合、この章は飛ばしてください。

### 7.1 fakeとの違い

| 項目 | Local deterministic fake | OpenAI Responses API |
| --- | --- | --- |
| 外部送信 | しない | Reviewした選択contextをOpenAIへ送る |
| API key | 不要 | 必要。server processだけへ渡す |
| 料金 | 発生しない | 契約内容により発生する場合がある |
| 出力 | 空template向けの固定変更 | ModelがChangeSetを生成する |
| 主な用途 | 操作確認、test | 評価用の実生成 |

### 7.2 OpenAIを有効にして起動する

`YOUR_OPENAI_API_KEY`を自分のkeyに置き換えます。

```bash
OPENAI_API_KEY='YOUR_OPENAI_API_KEY' \
LP_STUDIO_OPENAI_MODEL='gpt-5.4-mini' \
LP_STUDIO_STATE_ROOT=.studio-data \
pnpm dev:server
```

`LP_STUDIO_OPENAI_MODEL`は省略可能で、現在のdefaultは`gpt-5.4-mini`です。起動後、
画面の`AI provider`で`OpenAI Responses API`を選べるようになります。

API keyをsource code、Project file、prompt、スクリーンショットへ書かないで
ください。上の書き方がshell historyへ残る環境では、利用者のsecret managerなど
安全な方法で環境変数をserver processへ渡してください。利用後に現在のshellから
消す場合は次を実行します。

```bash
unset OPENAI_API_KEY
```

### 7.3 外部送信前に確認する

`送信内容を確認`Dialogに、外部へ送る正確なcontext、対象ファイル、bytes、
redactionの有無が表示されます。内容を読み、外部送信と料金に同意できる場合だけ
`変更案を作成`を押してください。

API keyはbrowserへ返されず、Projectやexportにも保存されません。ただし、
redactionを完全なsecret検出器として信用しないでください。site fileには最初から
credentialや個人情報を入れないことが安全です。

OpenAIの出力も必ずProposalになり、path、file形式、base revision、参照先、
active behaviorなどをローカルで検査してから表示します。検査を通らない出力は
Acceptedへ適用されません。

## 8. Adopt、Reject、Deferの意味

SynapseGitとの契約では、Human Decisionを次の3種類に限定しています。

| Decision | 意味 | Acceptedへの影響 |
| --- | --- | --- |
| Adopt | Proposal全体を変更せず採用し、その内容を新しいAcceptedにする | 変わる |
| Reject | Proposalを却下して終端にする | 変わらない |
| Defer | Proposalを保留として終端にする。後で同じProposalを再利用する意味ではない | 変わらない |

Review画面は3種類を明示的に分けます。どれもProposal全体に対するHuman-onlyの
terminal Decisionで、同じProposalへ二度目のDecisionはできません。Deferからの
継続も最新baseの新Proposalです。Decision結果が不確かなときは画面のreconciliationを
使い、browser consoleやAPIの直接呼び出しで再送しないでください。

## 9. AcceptedをZIPで出力する

1. 上部の`Acceptedをエクスポート`を押す。
2. Browserのdownloadを許可する。
3. `Accepted export receipt`のRevision、file数、bytes、SHA-256を確認する。
4. Downloadされた`synapsegit-lp-<revision先頭12文字>.zip`を保管する。

ZIPに入るのは、その操作を開始した時点のAccepted site fileだけです。pendingな
Proposal、要望、会話、Target、API key、provider raw response、Studio metadata、
SynapseGit内部dataは含めません。

出力物は通常の静的hostingへ置ける構成です。`file://`でHTMLを直接開く互換性は
保証していないため、確認時は通常の静的HTTP serverを使ってください。LP Studioが
hostingやGitHubへの公開を自動で行うことはありません。

### 9.1 GitHub-ready publication draftをローカル生成する

1. `GitHub-ready filesを生成`を押す。
2. public title、summary、decision noteを入力する。これらはsource factではなく
   `author_supplied`として記録される。
3. file tree、各fileのexact bytes、redaction、checksum、limitationsを読む。
4. 問題がなければ確認済みpublication ZIPをローカルへdownloadする。

この操作はGit commit、push、Issue、PR、release、その他のnetwork writeを行いません。
画面にも`GitHubへ公開: 別Human操作`と表示されます。publication draftは公開・署名・
authorship・rights・production readinessの証明ではありません。

### 9.2 保持データを確認・削除する

HomeとEditorの「保持データと手動削除」には、ProjectごとのAccepted payload、
memory-only context、Failed Proposal、static export、publication draftが表示されます。
自動GCとtelemetryはありません。

削除する場合は、画面に表示されたProject / Proposal / Artifact IDを確認欄へ正確に
入力します。Project全体の削除は、表示中のAccepted revisionとmanifestもserver側で
再照合します。表示bytesは対象payloadの大きさであり、共有CASが別Projectやretained
revisionから参照されている場合の実空き容量増加を保証しません。削除済みのローカル
projectionを、外部の記録まで削除したものとは扱わないでください。Failed Proposalや
個別artifactの削除はAcceptedとSynapse記録を保持しますが、Project全体の削除はその
Project内のlocal Synapse repository/journalとowned recovery backupも対象にします。
別途export済みのarchiveは削除しません。画面のcleanup impactを必ず確認してください。

### 9.3 Read-only recoveryを使う

保存領域の通常検証に失敗しても、last Acceptedまたはversioned backupを検証できる
場合、起動行の`operatingMode`が`read_only_recovery`になり、専用画面を開きます。
この画面は自動修復を行いません。Proposal、Decision、Accepted更新、publication、
cleanupは停止し、`VERIFIED`のrecovery pointだけをZIPへexportできます。

`UNVERIFIED`の項目は、未知・破損・中断したentryを削除せずに隔離した診断です。
その項目はexportできません。元のstate rootを直接編集せず、まず検証済みZIPとstate
root全体の別媒体backupを確保してください。

## 10. 困ったとき

### `pnpm build`が失敗する

Node.js、pnpm、Rustのversionを[現在の対応環境と必要なもの](#3-現在の対応環境と必要なもの)
と照合してください。依存関係を変更せず、`pnpm install --frozen-lockfile`から
やり直します。

### 起動したが、どのURLを開けばよいかわからない

起動のたびにportが変わります。terminalの最新の`LP_STUDIO_READY`を探し、
`editorOrigin`だけを開いてください。

### `LP_STUDIO_STATE_ROOT is already in use`と表示される

同じstate rootを2つのserverが同時に使っています。先に起動したserverを`Ctrl+C`で
停止してから再実行してください。

### state rootのpermission errorが出る

Linux/WSLでは、既存のstate rootはmode `0700`である必要があります。自分だけが
読み書きできるdirectoryだと確認してから、次を実行します。

```bash
chmod 700 .studio-data
```

### `登録済みディレクトリを取り込む`が表示されない

serverを停止し、実在する絶対pathを`LP_STUDIO_IMPORT_ROOT`へ指定して起動し直します。
取り込み元とstate rootが同一または親子関係になっていないことも確認してください。

### プレビューを読み込めない、またはsecurity制約の診断が出る

Navigation APIを使えるChromium系ブラウザで`editorOrigin`を開き直してください。
Active PreviewはNavigation APIを必要とし、境界を設定できないbrowserでは安全側に
停止します。外部通信、form送信、popup、top navigationなどもPreview内では制限
されます。

### `送信内容を確認`を押せない

Targetが選択済みで、`Resolution`が`resolved`であり、要望が空でなく、選択providerと
modelが利用可能かを確認してください。`ambiguous`または`detached`なら、より具体的な
要素を選び直します。

### fake providerがtargetをsupportしない

fake providerは空templateのヒーロー見出し、説明文、CTAだけを対象にします。
それぞれを一度変更すると、同じ固定変更はもう適用できません。別の対象を選ぶか、
評価用のOpenAI adapterを使ってください。

### Contextに`redacted`があり、`変更案を作成`を押せない

C6のChangeSetはfile全体を置き換える方式です。site fileの一部を伏せたまま安全に
元のsecretを保持できないため、redactionを含むsite fileの生成はproviderを呼ぶ前に
停止します。取り込み元からcredential、token、個人情報、ローカル絶対pathなどを
除き、cleanな別Projectとして取り込み直してください。

### Proposalにblocking warningがあり、採用できない

新しい外部origin、form action、script、iframe、download、inline event handler、
analytics、cookieなどのactive behaviorを検出すると、Adoptを止めます。現在のUIには
Reject/Deferがあるため、安全でないProposalをterminalにできます。Accepted fileは
変更されません。

### Projectのfileを直接変更した後、操作が止まる

Accepted manifestとのずれを検出すると、Proposal、Decision、exportを安全側に
停止します。`.studio-data`内のfileを直接編集しないでください。元の静的siteを修正し、
新しいProjectとして取り込むのが安全です。

### serverを再起動したら`Decision結果を再照合`と表示される

SynapseGit側のterminal Decisionと、local receiptまたはAccepted pointerの更新の間で
serverが停止した可能性があります。表示されたReview IDとProposal IDを確認し、
`Decision結果を再照合`を押してください。AcceptedやProposal directoryを直接編集したり、
同じDecisionをAPIへ再送したりしないでください。

## 11. Privacyと安全性

- ServerはIPv4 loopbackへbindし、Editor/APIとPreviewを別originに分離します。
- Previewへ読み込むHTML、CSS、JavaScriptはuntrusted contentとして扱います。
- importはsourceを直接編集せず、確認したfileをProjectへコピーします。
- fake providerは外部AIへcontextを送信しません。
- OpenAI adapterは確認Dialogに表示した選択contextを外部へ送信します。
- API keyはserver側だけで使います。Projectやexportへ保存しません。
- Reviewのprivate rationaleはDecision request内でだけ扱い、raw textやdigestを
  Project、Synapse record、log、export、publicationへ永続化しません。
- 静的exportにprompt、credential、Target、Studio/SynapseGit内部dataを混ぜません。
- AIの生成結果と自動検査を、人によるsecurity、legal、accessibility、内容確認の
  代わりにしません。

Loopback、sandbox、redactionはリスクを減らしますが、local evaluationの境界を
production security保証へ拡張するものではありません。
信頼できないLPをproduction dataと同じ環境で開かないでください。

## 12. 現在の主な制限

利用者やAI agentは、次を「実装済み」と誤解しないでください。

- Production-readyではなく、release、distribution permission、production利用許諾は
  まだありません。
- 対応実績はChromium系browserとLinux x86-64 GNU/WSL評価環境に限られます。
- Active PreviewはChromiumのNavigation APIへ依存します。
- 1つのProjectで同時に扱えるactive Proposalは1件で、複数ready Proposalの並列比較は
  ありません。
- redactionを含むsite fileからのChangeSet生成はできません。
- 部分採用、Proposalの直接編集、一般的なvisual editor、共同編集はありません。
- OpenAIのlive network/billing testは通常CIで実行していません。
- publication draftはローカル生成だけで、自動deployやGitHub remote writeは行いません。
- migration、read-only recovery、manual retentionとDecision durable boundaryの
  returned-fault/実process-kill matrixは自動検証済みです。
- 500 files / 50 MiB / 10,000 DOM nodesのsynthetic検査に加え、packaged production
  Appの実Preview bridge/overlay 72条件、表示名autosave、10 files / 2 MiB ChangeSetを
  測定します。性能値は記録した評価環境にのみ適用し、一般的なproduction性能保証では
  ありません。
- 自動browser smokeは完全なaccessibility、WCAG、Creator UX、screen reader確認を証明しません。

開発状況と根拠は[Implementation status](implementation-status.md)、製品全体の方向は
[Current specification](current-specification.md)、AI/ChangeSet境界の判断は
[ADR-0008](adr/0008-ai-provider-context-and-change-set.md)を参照してください。
