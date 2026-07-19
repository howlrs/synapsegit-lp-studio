import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type FormEvent,
} from "react";
import type {
  ArtifactDisposition,
  ExportReceipt,
  PreviewMode,
  PreviewSource,
  Project,
  Proposal,
  Target,
  ViewportPreset,
} from "@synapsegit-lp/contracts";
import {
  ApiError,
  bootstrapApi,
  type AuthenticatedApi,
  type BootstrappedApi,
  type PublicBootstrap,
} from "./api/client";
import {
  initialStudioState,
  studioReducer,
  type StudioOperation,
} from "./app/studio-reducer";
import {
  createChannelId,
  isAllowedPreviewUrl,
  postClearSelection,
  postPreviewMode,
  readPreviewSelection,
} from "./preview/bridge";

const ELEMENTS = [
  { elementId: "hero-heading", label: "ヒーロー見出し" },
  { elementId: "hero-copy", label: "ヒーロー説明文" },
  { elementId: "hero-cta", label: "ヒーローCTA" },
] as const;

const VIEWPORTS: Record<Exclude<ViewportPreset, "custom">, number> = {
  desktop: 1440,
  tablet: 768,
  mobile: 390,
};

const operationLabel: Record<Exclude<StudioOperation, null>, string> = {
  creating_project: "空のLPを作成しています",
  selecting_target: "ターゲットを検証しています",
  assembling_context: "送信コンテキストを組み立てています",
  generating_proposal: "fake AIで変更案を生成・検証しています",
  committing_decision: "一回限りの承認を取得しDecisionを記録しています",
  refreshing_project: "Accepted状態を再取得しています",
  exporting: "Accepted revisionをエクスポートしています",
};

const errorMessage = (error: unknown): string => {
  if (error instanceof ApiError) {
    const retry = error.retryable
      ? "安全に再試行できます。"
      : "入力と状態を確認してください。";
    return `${error.message} Accepted LPへの未確認の変更は行いません。${retry}`;
  }
  return "予期しないエラーが発生しました。Accepted LPは変更されていません。安全に再試行できます。";
};

const shortIdentity = (value: string): string =>
  value.length > 18 ? `${value.slice(0, 10)}…${value.slice(-6)}` : value;

const utf8Bytes = (value: string): number =>
  new TextEncoder().encode(value).byteLength;

const dispositionLabel = (value: ArtifactDisposition): string => {
  switch (value) {
    case "adopted_unchanged":
      return "変更案をそのまま採用";
    case "rejected":
      return "却下";
    case "deferred":
      return "保留（このProposalは終端）";
  }
};

interface StudioHeaderProps {
  project: Project;
  viewport: ViewportPreset;
  customWidth: number;
  busy: boolean;
  onViewport: (preset: ViewportPreset, customWidth?: number) => void;
  onExport: () => void;
}

function StudioHeader({
  project,
  viewport,
  customWidth,
  busy,
  onViewport,
  onExport,
}: StudioHeaderProps) {
  return (
    <header className="studio-header">
      <div className="brand-lockup">
        <span className="brand-mark" aria-hidden="true">
          S
        </span>
        <div>
          <p className="eyebrow">LOCAL-FIRST CREATIVE WORKSPACE</p>
          <h1>SynapseGit LP Studio</h1>
        </div>
      </div>

      <dl className="project-facts">
        <div>
          <dt>Project</dt>
          <dd>{project.displayName}</dd>
        </div>
        <div>
          <dt>Accepted revision</dt>
          <dd>
            <output aria-label="Accepted revision">
              {shortIdentity(project.revisionId)}
            </output>
          </dd>
        </div>
        <div>
          <dt>保存状態</dt>
          <dd>
            <span className="status-dot" aria-hidden="true" />
            保存済み
          </dd>
        </div>
      </dl>

      <div className="synapse-state" aria-label="SynapseGitの帰属状態">
        <span className="synapse-name">SynapseGit · generic artifact v1</span>
        <strong>caller-supplied</strong>
        <span>execution未検証</span>
      </div>

      <div className="header-actions">
        <fieldset className="viewport-switcher">
          <legend>プレビュー幅</legend>
          <button
            type="button"
            aria-pressed={viewport === "desktop"}
            onClick={() => onViewport("desktop")}
          >
            デスクトップ
          </button>
          <button
            type="button"
            aria-pressed={viewport === "tablet"}
            onClick={() => onViewport("tablet")}
          >
            タブレット
          </button>
          <button
            type="button"
            aria-pressed={viewport === "mobile"}
            onClick={() => onViewport("mobile")}
          >
            モバイル
          </button>
          <label>
            <span className="sr-only">カスタム幅（CSS pixel）</span>
            <input
              type="number"
              min="320"
              max="1920"
              value={customWidth}
              onChange={(event) =>
                onViewport("custom", Number(event.currentTarget.value))
              }
              aria-label="カスタム幅"
            />
          </label>
        </fieldset>
        <button
          type="button"
          className="button button-secondary"
          disabled={busy}
          onClick={onExport}
        >
          Acceptedをエクスポート
        </button>
      </div>
    </header>
  );
}

interface PageTreeProps {
  target: Target | null;
  disabled: boolean;
  onSelect: (elementId: string) => void;
}

function PageTree({ target, disabled, onSelect }: PageTreeProps) {
  return (
    <nav className="page-tree panel" aria-label="ページと要素">
      <div className="panel-heading">
        <p className="eyebrow">STRUCTURE</p>
        <h2>ページ</h2>
      </div>
      <div className="tree-root">
        <span className="tree-file" aria-hidden="true">
          ◇
        </span>
        <span>index.html</span>
      </div>
      <p className="tree-label">要素</p>
      <ul className="element-list">
        {ELEMENTS.map((element) => (
          <li key={element.elementId}>
            <button
              type="button"
              disabled={disabled}
              aria-current={
                target?.elementId === element.elementId ? "true" : undefined
              }
              onClick={() => onSelect(element.elementId)}
            >
              <span aria-hidden="true">↳</span>
              {element.label}
            </button>
          </li>
        ))}
      </ul>
      <div className="keyboard-hint">
        <kbd>Tab</kbd> で移動 · <kbd>Enter</kbd> で選択
      </div>
    </nav>
  );
}

interface PreviewPaneProps {
  project: Project;
  proposal: Proposal | null;
  expectedOrigin: string;
  target: Target | null;
  mode: PreviewMode;
  source: PreviewSource;
  viewport: ViewportPreset;
  customWidth: number;
  disabled: boolean;
  onMode: (mode: PreviewMode) => void;
  onSource: (source: PreviewSource) => void;
  onSelection: (elementId: string) => void;
  onBridgeError: (message: string) => void;
  onClear: () => void;
}

function PreviewPane({
  project,
  proposal,
  expectedOrigin,
  target,
  mode,
  source,
  viewport,
  customWidth,
  disabled,
  onMode,
  onSource,
  onSelection,
  onBridgeError,
  onClear,
}: PreviewPaneProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const activeSource =
    source === "proposed" && proposal === null ? "accepted" : source;
  const previewUrl =
    activeSource === "proposed" && proposal !== null
      ? proposal.previewUrl
      : project.previewUrl;
  const channelId = useMemo(
    () => createChannelId(),
    [activeSource, previewUrl, project.id, project.revisionId],
  );
  const width = viewport === "custom" ? customWidth : VIEWPORTS[viewport];
  const bridgeSnapshotId =
    activeSource === "proposed" && proposal !== null
      ? proposal.id
      : project.revisionId;
  const bridgeRevisionId =
    activeSource === "proposed" && proposal !== null
      ? proposal.baseRevisionId
      : project.revisionId;

  const bridgeBinding = useMemo(
    () => ({
      expectedOrigin,
      channelId,
      projectId: project.id,
      snapshotId: bridgeSnapshotId,
      revisionId: bridgeRevisionId,
    }),
    [bridgeRevisionId, bridgeSnapshotId, channelId, expectedOrigin, project.id],
  );

  useEffect(() => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow === null || frameWindow === undefined) return;
    const listener = (event: MessageEvent<unknown>) => {
      const selection = readPreviewSelection(event, {
        ...bridgeBinding,
        expectedSource: frameWindow,
      });
      if (selection !== null) onSelection(selection.elementId);
    };
    window.addEventListener("message", listener);
    return () => window.removeEventListener("message", listener);
  }, [bridgeBinding, onSelection]);

  const synchronizeMode = () => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow === null || frameWindow === undefined) return;
    postPreviewMode(frameWindow, bridgeBinding, mode);
  };

  useEffect(synchronizeMode, [bridgeBinding, mode]);

  const clearSelection = () => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow !== null && frameWindow !== undefined) {
      postClearSelection(frameWindow, bridgeBinding);
    }
    onClear();
  };

  if (!isAllowedPreviewUrl(previewUrl, expectedOrigin)) {
    return (
      <main id="preview" className="preview-pane panel" tabIndex={-1}>
        <div className="error-card" role="alert">
          Preview
          URLのoriginが起動時の許可originと一致しません。読み込みを停止しました。
        </div>
      </main>
    );
  }

  return (
    <main id="preview" className="preview-pane panel" tabIndex={-1}>
      <div className="preview-toolbar">
        <div
          className="segmented"
          role="group"
          aria-label="プレビュー操作モード"
        >
          <button
            type="button"
            aria-pressed={mode === "select"}
            onClick={() => onMode("select")}
          >
            選択モード
          </button>
          <button
            type="button"
            aria-pressed={mode === "interact"}
            onClick={() => onMode("interact")}
          >
            操作モード
          </button>
        </div>
        <div
          className="segmented source-switcher"
          role="group"
          aria-label="表示元"
        >
          <button
            type="button"
            aria-pressed={activeSource === "accepted"}
            onClick={() => onSource("accepted")}
          >
            Accepted
          </button>
          <button
            type="button"
            disabled={proposal === null}
            aria-pressed={activeSource === "proposed"}
            onClick={() => onSource("proposed")}
          >
            Proposed
          </button>
        </div>
        {target !== null ? (
          <button
            type="button"
            className="text-button"
            onClick={clearSelection}
          >
            選択解除 <kbd>Esc</kbd>
          </button>
        ) : null}
        <a className="mobile-intent-jump" href="#ai-instruction">
          AIへの要望へ移動
        </a>
      </div>

      <div className="canvas-shell">
        <div
          className="viewport-frame"
          style={{ width }}
          data-viewport={viewport}
        >
          <div className={`source-label source-label-${activeSource}`}>
            <span aria-hidden="true">●</span>
            {activeSource === "accepted" ? "Accepted" : "Proposed"} rendering
          </div>
          <iframe
            key={`${previewUrl}:${channelId}`}
            ref={iframeRef}
            src={previewUrl}
            title="LPプレビュー"
            sandbox="allow-scripts allow-same-origin"
            referrerPolicy="no-referrer"
            allow=""
            onLoad={synchronizeMode}
            onError={() =>
              onBridgeError(
                "LPプレビューを読み込めませんでした。Accepted LPは変更されていません。",
              )
            }
            aria-busy={disabled}
          />
        </div>
      </div>
    </main>
  );
}

interface ContextDialogProps {
  canonicalJson: string;
  sha256: string;
  busy: boolean;
  returnFocus: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onConfirm: () => void;
}

function ContextDialog({
  canonicalJson,
  sha256,
  busy,
  returnFocus,
  onClose,
  onConfirm,
}: ContextDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    return () => returnFocus.current?.focus();
  }, [returnFocus]);

  return (
    <div className="dialog-backdrop">
      <section
        className="dialog-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="context-dialog-title"
        aria-describedby="context-dialog-description"
      >
        <div className="dialog-heading">
          <div>
            <p className="eyebrow">EXACT CONTEXT REVIEW</p>
            <h2 id="context-dialog-title">送信内容を確認</h2>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="icon-button"
            aria-label="送信内容を閉じる"
            disabled={busy}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <p id="context-dialog-description">
          以下はローカルfake AIへ渡す正確なcanonical JSONです。session
          token、ローカル絶対path、Synapse authorityは含みません。
        </p>
        <p className="digest-line">
          <span>Context SHA-256</span>
          <code>{sha256}</code>
        </p>
        <pre className="context-code" tabIndex={0}>
          <code>{canonicalJson}</code>
        </pre>
        <div className="dialog-actions">
          <button
            type="button"
            className="button button-quiet"
            disabled={busy}
            onClick={onClose}
          >
            戻る
          </button>
          <button
            type="button"
            className="button button-primary"
            disabled={busy}
            onClick={onConfirm}
          >
            変更案を作成
          </button>
        </div>
      </section>
    </div>
  );
}

interface TargetComposerProps {
  project: Project;
  target: Target | null;
  prompt: string;
  disabled: boolean;
  reviewButtonRef: React.RefObject<HTMLButtonElement | null>;
  onPrompt: (value: string) => void;
  onReviewContext: () => void;
}

function TargetComposer({
  project,
  target,
  prompt,
  disabled,
  reviewButtonRef,
  onPrompt,
  onReviewContext,
}: TargetComposerProps) {
  const instructionBytes = utf8Bytes(prompt);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onReviewContext();
  };

  return (
    <aside className="target-panel panel" aria-labelledby="target-panel-title">
      <div className="panel-heading">
        <p className="eyebrow">INTENT</p>
        <h2 id="target-panel-title">選択中のターゲット</h2>
      </div>
      {target === null ? (
        <div className="empty-target">
          <span className="target-crosshair" aria-hidden="true">
            ⌖
          </span>
          <p>プレビューまたは左の要素一覧から、修正対象を選択してください。</p>
        </div>
      ) : (
        <dl className="target-card">
          <div>
            <dt>Kind</dt>
            <dd>
              <span className="pill">element</span>
            </dd>
          </div>
          <div>
            <dt>Page</dt>
            <dd>index.html</dd>
          </div>
          <div>
            <dt>Label</dt>
            <dd>{target.label}</dd>
          </div>
          <div>
            <dt>Element</dt>
            <dd>
              <code>#{target.elementId}</code>
            </dd>
          </div>
          <div>
            <dt>Capture revision</dt>
            <dd>{shortIdentity(target.revisionId)}</dd>
          </div>
          <div>
            <dt>Resolution</dt>
            <dd>
              <span className="resolved">✓ resolved</span>
            </dd>
          </div>
        </dl>
      )}

      <form className="prompt-composer" onSubmit={submit}>
        <label htmlFor="ai-instruction">AIへの要望</label>
        <textarea
          id="ai-instruction"
          value={prompt}
          maxLength={2000}
          rows={6}
          placeholder="例：見出しを、より自信が伝わる表現にしてください"
          disabled={disabled}
          onChange={(event) => onPrompt(event.currentTarget.value)}
        />
        <div className="composer-meta">
          <span>fake-ai · deterministic-v1</span>
          <span>{instructionBytes} / 2000 UTF-8 bytes</span>
        </div>
        <p className="composer-boundary">
          変更はまずProposalとして作成されます。Accepted
          revisionは自動変更されません。
        </p>
        <button
          ref={reviewButtonRef}
          type="submit"
          className="button button-primary button-wide"
          disabled={
            disabled ||
            target === null ||
            prompt.trim().length === 0 ||
            instructionBytes > 2000 ||
            target.revisionId !== project.revisionId
          }
        >
          送信内容を確認
        </button>
      </form>
    </aside>
  );
}

interface ReviewDrawerProps {
  proposal: Proposal;
  target: Target | null;
  prompt: string;
  busy: boolean;
  rationale: string;
  onRationale: (value: string) => void;
  onAdopt: () => void;
  onSource: (source: PreviewSource) => void;
}

function ReviewDrawer({
  proposal,
  target,
  prompt,
  busy,
  rationale,
  onRationale,
  onAdopt,
  onSource,
}: ReviewDrawerProps) {
  const hardError = proposal.validation.status === "failed";
  const rationaleBytes = utf8Bytes(rationale);
  return (
    <section className="review-drawer" aria-labelledby="review-title">
      <div className="review-heading">
        <div>
          <p className="eyebrow">HUMAN REVIEW REQUIRED</p>
          <h2 id="review-title">変更案を確認</h2>
        </div>
        <span className="proposal-status">pending review</span>
      </div>

      <div className="review-summary-grid">
        <dl>
          <div>
            <dt>Proposal</dt>
            <dd>{shortIdentity(proposal.id)}</dd>
          </div>
          <div>
            <dt>Base</dt>
            <dd>{shortIdentity(proposal.baseRevisionId)}</dd>
          </div>
          <div>
            <dt>Target</dt>
            <dd>{target?.label ?? "再選択が必要"}</dd>
          </div>
          <div>
            <dt>要望</dt>
            <dd>{prompt}</dd>
          </div>
        </dl>
        <div className="attribution-card">
          <span>Source attribution</span>
          <strong>caller-supplied</strong>
          <code>{proposal.sourceAttribution}</code>
          <span className="unverified">! execution未検証</span>
        </div>
      </div>

      <div className="review-columns">
        <section aria-labelledby="files-title">
          <h3 id="files-title">変更ファイル（{proposal.changes.length}）</h3>
          <ul className="changed-files">
            {proposal.changes.map((change) => (
              <li key={`${change.kind}:${change.path}`}>
                <span className={`change-kind change-${change.kind}`}>
                  {change.kind}
                </span>
                <code>{change.path}</code>
              </li>
            ))}
          </ul>
          <h3>Validation</h3>
          <p
            className={`validation-summary validation-${proposal.validation.status}`}
          >
            {proposal.validation.status === "passed" ? "✓" : "!"}{" "}
            {proposal.validation.status}
          </p>
          <ul className="validation-list">
            {proposal.validation.checks.map((check) => (
              <li key={check.id}>
                <strong>
                  {check.status === "passed" ? "✓" : "!"} {check.label}
                </strong>
                <span>{check.message}</span>
              </li>
            ))}
          </ul>
          <p className="validation-caveat">
            自動検査の合格は、完全なアクセシビリティ準拠を保証しません。
          </p>
        </section>
        <section aria-labelledby="diff-title">
          <div className="diff-heading">
            <h3 id="diff-title">Text diff</h3>
            <div className="text-tabs">
              <button type="button" onClick={() => onSource("accepted")}>
                Acceptedを表示
              </button>
              <button type="button" onClick={() => onSource("proposed")}>
                Proposedを表示
              </button>
            </div>
          </div>
          <pre className="diff-view" tabIndex={0}>
            <code>{proposal.unifiedDiff}</code>
          </pre>
        </section>
      </div>

      <div className="decision-bar">
        <div>
          <strong>{dispositionLabel("adopted_unchanged")}</strong>
          <p>
            {proposal.changes.length}
            ファイルをProposalのまま採用します。部分採用・直接編集は行いません。
          </p>
        </div>
        <label className="rationale-field">
          <span>非公開メモ（任意）</span>
          <input
            type="text"
            maxLength={2000}
            value={rationale}
            disabled={busy}
            onChange={(event) => onRationale(event.currentTarget.value)}
            aria-describedby="rationale-limit"
          />
          <small id="rationale-limit">
            {rationaleBytes} / 2000 UTF-8 bytes
          </small>
        </label>
        <button
          type="button"
          className="button button-adopt"
          disabled={busy || hardError || rationaleBytes > 2000}
          onClick={onAdopt}
        >
          変更を採用
        </button>
      </div>
    </section>
  );
}

function ExportReceiptCard({
  receipt,
  fileCount,
}: {
  receipt: ExportReceipt;
  fileCount: number;
}) {
  return (
    <section className="export-receipt" aria-labelledby="export-receipt-title">
      <div>
        <p className="eyebrow">EXPORT COMPLETE</p>
        <h2 id="export-receipt-title">Accepted export receipt</h2>
      </div>
      <dl>
        <div>
          <dt>Revision</dt>
          <dd>{shortIdentity(receipt.revisionId)}</dd>
        </div>
        <div>
          <dt>Files</dt>
          <dd>{fileCount}</dd>
        </div>
        <div>
          <dt>Bytes</dt>
          <dd>{receipt.byteLength.toLocaleString("ja-JP")}</dd>
        </div>
        <div>
          <dt>SHA-256 checksum</dt>
          <dd>
            <code>{receipt.sha256}</code>
          </dd>
        </div>
      </dl>
    </section>
  );
}

interface StudioProps {
  session: BootstrappedApi;
}

function Studio({ session }: StudioProps) {
  const [state, dispatch] = useReducer(studioReducer, initialStudioState);
  const [prompt, setPrompt] = useState(
    "見出しを、未来への期待が伝わる表現にしてください",
  );
  const [rationale, setRationale] = useState("");
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const busy = state.operation !== null;

  const run = useCallback(async (task: () => Promise<void>) => {
    try {
      await task();
    } catch (error) {
      dispatch({ type: "FAILED", message: errorMessage(error) });
    }
  }, []);

  const createProject = () => {
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "creating_project" });
      const project = await session.api.createBlankProject();
      dispatch({ type: "PROJECT_LOADED", project });
    });
  };

  const selectTarget = useCallback(
    (elementId: string) => {
      if (state.project === null || state.operation !== null) return;
      const project = state.project;
      void run(async () => {
        dispatch({ type: "OPERATION_STARTED", operation: "selecting_target" });
        const target = await session.api.createTarget(
          project.id,
          project.revisionId,
          elementId,
        );
        dispatch({ type: "TARGET_SELECTED", target });
      });
    },
    [run, session.api, state.operation, state.project],
  );

  const clearTarget = useCallback(() => {
    if (state.operation === null) dispatch({ type: "TARGET_CLEARED" });
  }, [state.operation]);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (state.contextReview !== null) {
        event.preventDefault();
        dispatch({ type: "CONTEXT_CLOSED" });
      } else if (state.target !== null) {
        event.preventDefault();
        clearTarget();
      }
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [clearTarget, state.contextReview, state.target]);

  const reviewContext = () => {
    if (state.project === null || state.target === null) return;
    const project = state.project;
    const target = state.target;
    const instruction = prompt.trim();
    if (instruction.length === 0) return;
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "assembling_context" });
      const context = await session.api.createContext(
        project.id,
        project.revisionId,
        target.id,
        instruction,
      );
      dispatch({ type: "CONTEXT_READY", context });
    });
  };

  const createProposal = () => {
    if (state.project === null || state.contextReview === null) return;
    const project = state.project;
    const contextReview = state.contextReview;
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "generating_proposal" });
      const proposal = await session.api.createProposal(
        project.id,
        contextReview.id,
        contextReview.sha256,
      );
      dispatch({ type: "PROPOSAL_READY", proposal });
    });
  };

  const adopt = () => {
    if (state.project === null || state.proposal === null) return;
    const project = state.project;
    const proposal = state.proposal;
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "committing_decision" });
      const intentId = crypto.randomUUID();
      const approval = await session.api.approveDecision({
        reviewId: proposal.reviewId,
        proposalId: proposal.id,
        expectedRevisionId: project.revisionId,
        disposition: "adopted_unchanged",
        intentId,
      });
      await session.api.decide({
        reviewId: proposal.reviewId,
        approvalToken: approval.token,
        proposalId: proposal.id,
        expectedRevisionId: project.revisionId,
        disposition: "adopted_unchanged",
        intentId,
        ...(rationale.trim().length === 0
          ? {}
          : { rationale: rationale.trim() }),
      });
      dispatch({
        type: "DECISION_COMMITTED",
        disposition: "adopted_unchanged",
      });
      const refreshed = await session.api.getProject(project.id);
      dispatch({
        type: "PROJECT_REFRESHED",
        project: refreshed,
        afterDecision: true,
      });
    });
  };

  const exportAccepted = () => {
    if (state.project === null) return;
    const project = state.project;
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "exporting" });
      const receipt = await session.api.createExport(
        project.id,
        project.revisionId,
      );
      const blob = await session.api.downloadExport(receipt.downloadUrl);
      const objectUrl = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = objectUrl;
      anchor.download = `synapsegit-lp-${receipt.revisionId.slice(0, 12)}.zip`;
      anchor.rel = "noopener";
      anchor.click();
      URL.revokeObjectURL(objectUrl);
      dispatch({ type: "EXPORT_READY", receipt });
    });
  };

  if (state.project === null) {
    return (
      <main className="project-home">
        <div className="home-glow" aria-hidden="true" />
        <section className="home-card" aria-labelledby="home-title">
          <div className="home-brand" aria-hidden="true">
            SG
          </div>
          <p className="eyebrow">SYNAPSEGIT · LOCAL LP WORKSPACE</p>
          <h1 id="home-title">対話から、採用可能なLPへ。</h1>
          <p>
            ブラウザ上で要素を選び、AIへ要望を伝え、変更案をHuman
            Reviewしてから採用します。すべてのAccepted状態はローカルに保持されます。
          </p>
          <ul className="home-boundaries">
            <li>AIの出力はProposal。自動採用しません</li>
            <li>caller-supplied / execution未検証を明示</li>
            <li>静的exportには会話・tokenを含めません</li>
          </ul>
          <button
            type="button"
            className="button button-primary home-create"
            disabled={busy}
            onClick={createProject}
          >
            空のLPを作成
          </button>
          {state.operation !== null ? (
            <p role="status">{operationLabel[state.operation]}…</p>
          ) : null}
          {state.error !== null ? (
            <p className="error-card" role="alert">
              {state.error}
            </p>
          ) : null}
        </section>
      </main>
    );
  }

  return (
    <>
      <a className="skip-link" href="#preview">
        プレビューへ移動
      </a>
      <StudioHeader
        project={state.project}
        viewport={state.viewportPreset}
        customWidth={state.customWidth}
        busy={busy}
        onViewport={(preset, customWidth) =>
          dispatch({
            type: "SET_VIEWPORT",
            preset,
            ...(customWidth === undefined ? {} : { customWidth }),
          })
        }
        onExport={exportAccepted}
      />

      {state.error !== null ? (
        <div className="global-error" role="alert">
          <p>{state.error}</p>
          <button
            type="button"
            className="text-button"
            onClick={() => dispatch({ type: "DISMISS_ERROR" })}
          >
            閉じる
          </button>
        </div>
      ) : null}

      {state.operation !== null ? (
        <div className="operation-bar" role="status" aria-live="polite">
          <span className="spinner" aria-hidden="true" />
          {operationLabel[state.operation]}…
        </div>
      ) : null}

      <div className="studio-grid">
        <PageTree
          target={state.target}
          disabled={busy || state.proposal !== null}
          onSelect={selectTarget}
        />
        <PreviewPane
          project={state.project}
          proposal={state.proposal}
          expectedOrigin={session.bootstrap.previewOrigin}
          target={state.target}
          mode={state.previewMode}
          source={state.previewSource}
          viewport={state.viewportPreset}
          customWidth={state.customWidth}
          disabled={busy}
          onMode={(mode) => dispatch({ type: "SET_PREVIEW_MODE", mode })}
          onSource={(source) =>
            dispatch({ type: "SET_PREVIEW_SOURCE", source })
          }
          onSelection={selectTarget}
          onBridgeError={(message) => dispatch({ type: "FAILED", message })}
          onClear={clearTarget}
        />
        <TargetComposer
          project={state.project}
          target={state.target}
          prompt={prompt}
          disabled={busy || state.proposal !== null}
          reviewButtonRef={reviewButtonRef}
          onPrompt={setPrompt}
          onReviewContext={reviewContext}
        />
      </div>

      {state.proposal !== null ? (
        <ReviewDrawer
          proposal={state.proposal}
          target={state.proposalTarget}
          prompt={prompt}
          busy={busy}
          rationale={rationale}
          onRationale={setRationale}
          onAdopt={adopt}
          onSource={(source) =>
            dispatch({ type: "SET_PREVIEW_SOURCE", source })
          }
        />
      ) : null}

      {state.exportReceipt !== null ? (
        <ExportReceiptCard
          receipt={state.exportReceipt}
          fileCount={state.project.files.length}
        />
      ) : null}

      {state.contextReview !== null ? (
        <ContextDialog
          canonicalJson={state.contextReview.canonicalJson}
          sha256={state.contextReview.sha256}
          busy={busy}
          returnFocus={reviewButtonRef}
          onClose={() => dispatch({ type: "CONTEXT_CLOSED" })}
          onConfirm={createProposal}
        />
      ) : null}

      <div className="sr-only" aria-live="polite" aria-atomic="true">
        {state.announcement}
      </div>
      <div className="sr-only" aria-live="assertive" aria-atomic="true">
        {state.error}
      </div>
    </>
  );
}

export function App() {
  const [session, setSession] = useState<BootstrappedApi | null>(null);
  const [bootError, setBootError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    if (window.location.hash.length > 0) {
      window.history.replaceState(
        null,
        "",
        `${window.location.pathname}${window.location.search}`,
      );
    }
    void bootstrapApi()
      .then((result) => {
        if (active) setSession(result);
      })
      .catch((error: unknown) => {
        if (active) setBootError(errorMessage(error));
      });
    return () => {
      active = false;
    };
  }, []);

  if (bootError !== null) {
    return (
      <main className="boot-screen">
        <section className="error-card" role="alert">
          <h1>LP Studioを起動できません</h1>
          <p>{bootError}</p>
          <button
            type="button"
            className="button button-secondary"
            onClick={() => window.location.reload()}
          >
            再読み込み
          </button>
        </section>
      </main>
    );
  }

  if (session === null) {
    return (
      <main className="boot-screen" aria-busy="true">
        <div className="boot-mark" aria-hidden="true">
          S
        </div>
        <h1>SynapseGit LP Studio</h1>
        <p role="status">ローカルセッションを検証しています…</p>
      </main>
    );
  }

  return <Studio session={session} />;
}

export type { AuthenticatedApi, PublicBootstrap };
