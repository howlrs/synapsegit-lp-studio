# SynapseGit LP Studio documentation

このdirectoryには、利用手順、現在の実装状態、将来要件、設計判断を置きます。
文書ごとに役割が違います。要件に書かれているだけでは実装済みではありません。

## 読者別の入口

| 読者・目的 | 最初に読む文書 | 次に読む文書 |
| --- | --- | --- |
| 初めて起動する | [利用ガイド](user-guide.md) | [root README](../README.md) |
| 現在できることを知る | [実装ステータス](implementation-status.md) | [要件traceability](requirements-traceability.md) |
| AI agentとして作業する | [AI agent guide](ai-agent-guide.md) | [実装ステータス](implementation-status.md) |
| 要件を実装する | [詳細要件](detailed-requirements.md) | [実装計画](implementation-plan.md) |
| 設計理由を調べる | [ADR一覧](adr/README.md) | 関連する個別ADR |
| SynapseGit境界を調べる | [contract lock](synapsegit-contract.lock.json) | [upstream feedback](upstream-issues/README.md) |
| 製品の将来像を読む | [current specification](current-specification.md) | [詳細要件](detailed-requirements.md) |

## 文書の優先順位

同じ機能について文書が異なるように見える場合は、次の順序で判断します。

1. 実行中のcode、strict contract、test
2. [実装ステータス](implementation-status.md)
3. [要件traceability](requirements-traceability.md)の各requirement status
4. accepted ADRと[SynapseGit contract lock](synapsegit-contract.lock.json)
5. [詳細要件](detailed-requirements.md)と[実装計画](implementation-plan.md)
6. [current specification](current-specification.md)

上位の情報ほど、「現在の実装事実」の判断に強い根拠を持ちます。
下位の要件・計画・仕様は、未実装の目標を含みます。

## 文書一覧

### 利用・参加

- [利用ガイド](user-guide.md): install、起動、blank/import、Proposal review、
  export、troubleshooting
- [AI agent guide](ai-agent-guide.md): 読む順序、正本、不変条件、
  task-to-file map、安全な変更手順
- [root README](../README.md): 5分quick start、capability、制限、repository入口

### 現在地と計画

- [実装ステータス](implementation-status.md): 現在のweighted progress、
  checkpoint evidence、既知制限、次のcheckpoint
- [要件traceability](requirements-traceability.md): generated
  requirement-to-checkpoint/status matrix。手動編集禁止
- [実装計画](implementation-plan.md): checkpoint順序、weight、gate
- [詳細要件](detailed-requirements.md): implementation-readyな要求baseline。
  `MUST`であってもstatusがplannedなら未実装
- [current specification](current-specification.md): 製品方向と将来flow。
  現在のcapability一覧ではない

### Architectureと外部境界

- [ADR一覧](adr/README.md): accepted M1 architecture/security/storage/provider decisions
- [SynapseGit contract lock](synapsegit-contract.lock.json): exact upstream Git
  revision、contract hash、claim boundary
- [SynapseGit upstream feedback](upstream-issues/README.md): このintegrationから
  作成したIssueとpublication follow-up

## 現在地の短い要約

- Status date: 2026-07-19
- Completed: C0–C6、65%
- C0–C6 integration: merged PR #1 commit `c9a22b1`, contained in `main`
- Next: C7 Review / Decision / recovery
- Product status: local evaluation build、not production-ready
- Browser evidence: Chromium
- Provider: deterministic fake、optional OpenAI when configured
- Important limit: `singleProposalPerProject: true`
- Current UI Decision: adopt only
- Export: Accepted static ZIP

この要約より詳しい主張には、
[実装ステータス](implementation-status.md)のevidenceを引用してください。

## Documentation check

文書を変更したら次を実行します。

~~~bash
pnpm check:docs
pnpm format:check
~~~

`pnpm check:docs`はlink、fence、trailing whitespace、requirement ID、
checkpoint weight、generated traceability、SynapseGit contract lockを検査します。
