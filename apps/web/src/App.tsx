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
  AiProviderDescriptor,
  ArtifactDisposition,
  ContextReview,
  ExportReceipt,
  ImportLimits,
  ImportPreview,
  PreviewDiagnosticCode,
  PreviewDiagnosticMessage,
  PreviewMode,
  PreviewSource,
  PreviewStructureNode,
  Project,
  Proposal,
  TargetKind,
  TargetSelection,
  TargetV1,
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
  postCaptureNode,
  postCapturePage,
  postClearSelection,
  postPreviewMode,
  postRequestStructure,
  readPreviewDiagnostic,
  readPreviewStructure,
  readPreviewTarget,
} from "./preview/bridge";

const TARGET_KINDS: readonly TargetKind[] = [
  "page",
  "block",
  "element",
  "text",
  "point",
  "region",
];

const targetKindLabel: Record<TargetKind, string> = {
  page: "ページ",
  block: "ブロック",
  element: "要素",
  text: "テキスト",
  point: "座標",
  region: "領域",
};

type CaptureCommand =
  | { id: number; action: "page" }
  | {
      id: number;
      action: "node";
      runtimeNodeHandle: string;
      targetKind: TargetKind;
    };

const VIEWPORTS: Record<Exclude<ViewportPreset, "custom">, number> = {
  desktop: 1440,
  tablet: 768,
  mobile: 390,
};

const diagnosticLabel: Record<PreviewDiagnosticCode, string> = {
  csp_blocked:
    "プレビューのセキュリティ制約により、一部の表示または処理を停止しました。",
  site_error: "プレビュー内のサイトでエラーを検出しました。",
  unhandled_rejection: "プレビュー内の処理が完了しませんでした。",
};

const supportsPreviewNavigationIsolation = (): boolean => {
  const navigation = Reflect.get(globalThis, "navigation") as unknown;
  return (
    typeof navigation === "object" &&
    navigation !== null &&
    "addEventListener" in navigation &&
    typeof navigation.addEventListener === "function"
  );
};

const operationLabel: Record<Exclude<StudioOperation, null>, string> = {
  creating_project: "空のLPを作成しています",
  selecting_target: "ターゲットを検証しています",
  assembling_context: "送信コンテキストを組み立てています",
  generating_proposal: "AIで変更案を生成・検証しています",
  committing_decision: "一回限りの承認を取得しDecisionを記録しています",
  refreshing_project: "Accepted状態を再取得しています",
  exporting: "Accepted revisionをエクスポートしています",
};

type HomeOperation =
  | "loading_projects"
  | "opening_project"
  | "previewing_import"
  | "importing_project"
  | null;

const homeOperationLabel: Record<Exclude<HomeOperation, null>, string> = {
  loading_projects: "保存済みプロジェクトを確認しています",
  opening_project: "保存済みプロジェクトを開いています",
  previewing_import: "登録済みディレクトリを検査しています",
  importing_project: "検査済みファイルのコピーを取り込んでいます",
};

class DecisionOutcomeError extends Error {
  readonly phase: "decision" | "refresh";

  constructor(phase: "decision" | "refresh", cause: unknown) {
    super(cause instanceof Error ? cause.message : "");
    this.name = "DecisionOutcomeError";
    this.phase = phase;
  }
}

const errorMessage = (error: unknown): string => {
  if (error instanceof DecisionOutcomeError) {
    const detail = error.message.trim();
    const prefix = detail.length === 0 ? "" : `${detail} `;
    return error.phase === "decision"
      ? `${prefix}Human Decisionの結果は不明です。Accepted LPが変更された可能性があります。Decisionを再実行せず、画面を再読み込みしてAccepted revisionを照合してください。`
      : `${prefix}Human Decisionは記録されましたが、Accepted状態を再取得できず、現在の確認結果は不明です。Decisionを再実行せず、画面を再読み込みしてAccepted revisionを照合してください。`;
  }
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
  target: TargetSelection | null;
  targetKind: TargetKind;
  structure: PreviewStructureNode[];
  disabled: boolean;
  onTargetKind: (kind: TargetKind) => void;
  onCapturePage: () => void;
  onCaptureNode: (node: PreviewStructureNode) => void;
}

function PageTree({
  target,
  targetKind,
  structure,
  disabled,
  onTargetKind,
  onCapturePage,
  onCaptureNode,
}: PageTreeProps) {
  return (
    <nav className="page-tree panel" aria-label="ページと要素">
      <div className="panel-heading">
        <p className="eyebrow">STRUCTURE</p>
        <h2>ページ</h2>
      </div>
      <p className="tree-label">ターゲット種別</p>
      <div
        className="target-kind-grid"
        role="group"
        aria-label="ターゲット種別"
      >
        {TARGET_KINDS.map((kind) => (
          <button
            key={kind}
            type="button"
            disabled={disabled}
            aria-pressed={targetKind === kind}
            onClick={() => onTargetKind(kind)}
          >
            {targetKindLabel[kind]}
          </button>
        ))}
      </div>
      <button
        type="button"
        className="tree-root"
        disabled={disabled}
        aria-current={target?.target.kind === "page" ? "true" : undefined}
        onClick={onCapturePage}
      >
        <span className="tree-file" aria-hidden="true">
          ◇
        </span>
        <span>index.html 全体</span>
      </button>
      <p className="tree-label">プレビューから検出した構造</p>
      <ul className="element-list">
        {structure.map((node) => (
          <li key={node.runtimeNodeHandle}>
            <button
              type="button"
              disabled={disabled}
              title={`${node.label} <${node.tagName}>`}
              style={{
                paddingInlineStart: `${0.65 + Math.min(node.depth, 4) * 0.65}rem`,
              }}
              onClick={() => onCaptureNode(node)}
            >
              <span aria-hidden="true">
                {node.kind === "block" ? "▦" : "↳"}
              </span>
              <span>{node.label}</span>
              <small>{node.tagName}</small>
            </button>
          </li>
        ))}
      </ul>
      {structure.length === 0 ? (
        <p className="tree-empty">プレビューの読み込み後に構造を表示します。</p>
      ) : null}
      <div className="keyboard-hint">
        <kbd>Tab</kbd> で移動 · <kbd>Enter</kbd>{" "}
        で選択。座標・領域はプレビュー上で指定します。
      </div>
    </nav>
  );
}

interface PreviewPaneProps {
  project: Project;
  proposal: Proposal | null;
  previewScopeBaseOrigin: string;
  target: TargetSelection | null;
  targetKind: TargetKind;
  captureCommand: CaptureCommand | null;
  mode: PreviewMode;
  source: PreviewSource;
  viewport: ViewportPreset;
  customWidth: number;
  disabled: boolean;
  onMode: (mode: PreviewMode) => void;
  onSource: (source: PreviewSource) => void;
  onTarget: (target: TargetV1) => void;
  onStructure: (nodes: PreviewStructureNode[]) => void;
  onBridgeError: (message: string) => void;
  onClear: () => void;
}

function PreviewPane({
  project,
  proposal,
  previewScopeBaseOrigin,
  target,
  targetKind,
  captureCommand,
  mode,
  source,
  viewport,
  customWidth,
  disabled,
  onMode,
  onSource,
  onTarget,
  onStructure,
  onBridgeError,
  onClear,
}: PreviewPaneProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const handledCaptureCommand = useRef(0);
  const [diagnostic, setDiagnostic] = useState<PreviewDiagnosticMessage | null>(
    null,
  );
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
  const previewUrlAllowed = isAllowedPreviewUrl(
    previewUrl,
    previewScopeBaseOrigin,
  );
  const navigationIsolationAvailable = supportsPreviewNavigationIsolation();
  const scopedPreviewOrigin = previewUrlAllowed
    ? new URL(previewUrl).origin
    : "null";

  const bridgeBinding = useMemo(
    () => ({
      expectedOrigin: scopedPreviewOrigin,
      channelId,
      projectId: project.id,
      snapshotId: bridgeSnapshotId,
      revisionId: bridgeRevisionId,
    }),
    [
      bridgeRevisionId,
      bridgeSnapshotId,
      channelId,
      project.id,
      scopedPreviewOrigin,
    ],
  );

  useEffect(() => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow === null || frameWindow === undefined) return;
    const listener = (event: MessageEvent<unknown>) => {
      const targetDraft = readPreviewTarget(event, {
        ...bridgeBinding,
        expectedSource: frameWindow,
      });
      if (targetDraft !== null) {
        onTarget(targetDraft.target);
        return;
      }
      const structure = readPreviewStructure(event, {
        ...bridgeBinding,
        expectedSource: frameWindow,
      });
      if (structure !== null) {
        onStructure(structure.nodes);
        return;
      }
      const nextDiagnostic = readPreviewDiagnostic(event, {
        ...bridgeBinding,
        expectedSource: frameWindow,
      });
      if (nextDiagnostic !== null) setDiagnostic(nextDiagnostic);
    };
    window.addEventListener("message", listener);
    return () => window.removeEventListener("message", listener);
  }, [bridgeBinding, onStructure, onTarget]);

  useEffect(() => {
    onStructure([]);
  }, [bridgeBinding, onStructure]);

  const synchronizeMode = () => {
    if (!previewUrlAllowed) return;
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow === null || frameWindow === undefined) return;
    postPreviewMode(frameWindow, bridgeBinding, mode, targetKind, 1);
    postRequestStructure(frameWindow, bridgeBinding);
  };

  useEffect(synchronizeMode, [bridgeBinding, mode, targetKind]);

  useEffect(() => {
    if (
      captureCommand === null ||
      captureCommand.id <= handledCaptureCommand.current ||
      !previewUrlAllowed
    ) {
      return;
    }
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow === null || frameWindow === undefined) return;
    handledCaptureCommand.current = captureCommand.id;
    if (captureCommand.action === "page") {
      postCapturePage(frameWindow, bridgeBinding);
      return;
    }
    postCaptureNode(
      frameWindow,
      bridgeBinding,
      captureCommand.runtimeNodeHandle,
      captureCommand.targetKind,
    );
  }, [bridgeBinding, captureCommand, previewUrlAllowed]);

  const clearSelection = () => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (frameWindow !== null && frameWindow !== undefined) {
      postClearSelection(frameWindow, bridgeBinding);
    }
    onClear();
  };

  if (!navigationIsolationAvailable) {
    return (
      <main id="preview" className="preview-pane panel" tabIndex={-1}>
        <div className="error-card" role="alert">
          このブラウザでは安全なプレビュー隔離を利用できないため、LPの実行を停止しました。
        </div>
      </main>
    );
  }

  if (!previewUrlAllowed) {
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
            disabled={disabled}
            aria-pressed={mode === "select"}
            onClick={() => onMode("select")}
          >
            選択モード
          </button>
          <button
            type="button"
            disabled={disabled}
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
            disabled={disabled}
            aria-pressed={activeSource === "accepted"}
            onClick={() => onSource("accepted")}
          >
            Accepted
          </button>
          <button
            type="button"
            disabled={disabled || proposal === null}
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
            disabled={disabled}
            onClick={clearSelection}
          >
            選択解除 <kbd>Esc</kbd>
          </button>
        ) : null}
        <a className="mobile-intent-jump" href="#ai-instruction">
          AIへの要望へ移動
        </a>
      </div>

      {diagnostic?.channelId === channelId &&
      diagnostic.projectId === project.id &&
      diagnostic.snapshotId === bridgeSnapshotId &&
      diagnostic.revisionId === bridgeRevisionId ? (
        <div
          className={`preview-diagnostic preview-diagnostic-${diagnostic.severity}`}
          role={diagnostic.severity === "error" ? "alert" : "status"}
        >
          <strong>プレビュー診断</strong>
          <span>{diagnosticLabel[diagnostic.code]}</span>
          <code>source unavailable</code>
        </div>
      ) : null}

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
  context: ContextReview;
  busy: boolean;
  returnFocus: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onConfirm: () => void;
}

function ContextDialog({
  context,
  busy,
  returnFocus,
  onClose,
  onConfirm,
}: ContextDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const redactedSiteContent = context.manifest.entries.some(
    (entry) => entry.redacted,
  );

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
          {context.provider.external
            ? "UIとファイルはローカルに残りますが、以下の選択コンテキストは外部AI providerへ送信されます。"
            : "以下はローカルの決定論的fake AIへ渡す正確なコンテキストです。"}
          session token、ローカル絶対path、credential、Synapse
          authorityは含みません。
        </p>
        <dl className="context-provider-summary">
          <div>
            <dt>Provider</dt>
            <dd>{context.provider.providerId}</dd>
          </div>
          <div>
            <dt>Model</dt>
            <dd>{context.provider.requestedModel}</dd>
          </div>
          <div>
            <dt>Adapter</dt>
            <dd>{context.provider.adapterVersion}</dd>
          </div>
          <div>
            <dt>Attempt</dt>
            <dd>{shortIdentity(context.attemptId)}</dd>
          </div>
        </dl>
        <section
          className="context-manifest"
          aria-labelledby="context-manifest-title"
        >
          <h3 id="context-manifest-title">
            送信manifest（{context.manifest.entries.length} files /{" "}
            {context.manifest.totalIncludedBytes.toLocaleString("ja-JP")}{" "}
            bytes）
          </h3>
          <ul>
            {context.manifest.entries.map((entry) => (
              <li key={entry.path}>
                <code>{entry.path}</code>
                <span>
                  {entry.purpose} · lines {entry.startLine}–{entry.endLine} ·{" "}
                  {entry.includedByteLength.toLocaleString("ja-JP")} bytes
                  {entry.redacted
                    ? ` · redacted (${entry.redactions.join(", ")})`
                    : " · no redaction"}
                </span>
              </li>
            ))}
          </ul>
          <p>
            Screenshot:{" "}
            {context.manifest.screenshotIncluded ? "included" : "off"} · token
            estimate: {context.manifest.estimatedTokens.toLocaleString("ja-JP")}
          </p>
        </section>
        {redactedSiteContent ? (
          <p className="error-card" role="alert">
            機密情報を除いた正確なcontextは引き続き確認できますが、C6ではredactionを含むfileから安全なfull-file
            ChangeSetを生成できないため、変更案の作成は利用できません。
          </p>
        ) : null}
        <p className="digest-line">
          <span>Context SHA-256</span>
          <code>{context.sha256}</code>
        </p>
        <pre className="context-code" tabIndex={0}>
          <code>{context.canonicalJson}</code>
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
            disabled={busy || redactedSiteContent}
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
  target: TargetSelection | null;
  prompt: string;
  providers: AiProviderDescriptor[];
  providerId: string;
  requestedModel: string;
  disabled: boolean;
  reviewButtonRef: React.RefObject<HTMLButtonElement | null>;
  onPrompt: (value: string) => void;
  onProvider: (providerId: string) => void;
  onModel: (model: string) => void;
  onReviewContext: () => void;
}

function TargetComposer({
  project,
  target,
  prompt,
  providers,
  providerId,
  requestedModel,
  disabled,
  reviewButtonRef,
  onPrompt,
  onProvider,
  onModel,
  onReviewContext,
}: TargetComposerProps) {
  const instructionBytes = utf8Bytes(prompt);
  const capturedTarget = target?.target ?? null;
  const resolution = target?.resolution ?? null;
  const provider = providers.find((candidate) => candidate.id === providerId);
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
      {capturedTarget === null || resolution === null ? (
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
              <span className="pill">{capturedTarget.kind}</span>
            </dd>
          </div>
          <div>
            <dt>Page</dt>
            <dd>{capturedTarget.pagePath}</dd>
          </div>
          <div>
            <dt>Label</dt>
            <dd>{capturedTarget.label}</dd>
          </div>
          {"elementAnchor" in capturedTarget ? (
            <div>
              <dt>Anchor</dt>
              <dd>
                <code>
                  {capturedTarget.elementAnchor.uniqueElementId === undefined
                    ? capturedTarget.elementAnchor.tagName.toLowerCase()
                    : `#${capturedTarget.elementAnchor.uniqueElementId}`}
                </code>
              </dd>
            </div>
          ) : null}
          <div>
            <dt>Capture revision</dt>
            <dd>{shortIdentity(capturedTarget.captureRevisionId)}</dd>
          </div>
          <div>
            <dt>Capture source</dt>
            <dd>
              {capturedTarget.captureSource === "accepted"
                ? "Accepted"
                : `Proposed · ${shortIdentity(capturedTarget.captureProposalId)}`}
            </dd>
          </div>
          <div>
            <dt>Resolution</dt>
            <dd>
              <span className={`resolution resolution-${resolution.status}`}>
                {resolution.status === "resolved" ? "✓" : "!"}{" "}
                {resolution.status}
              </span>
            </dd>
          </div>
          <div>
            <dt>Candidates</dt>
            <dd>{resolution.candidates.length}</dd>
          </div>
        </dl>
      )}

      {capturedTarget?.kind === "point" || capturedTarget?.kind === "region" ? (
        <p className="target-resolution-note">
          座標・領域は視覚的な要望の手がかりです。AIへの送信前に、サーバーが現在のAccepted
          revision上の意味的な対象へ解決します。
        </p>
      ) : null}
      {resolution !== null && resolution.status !== "resolved" ? (
        <div className="resolution-warning" role="status">
          <p>
            {resolution.status === "ambiguous"
              ? "候補を一意に決められません。プレビュー上でより具体的な対象を選び直してください。"
              : "現在のAccepted revisionから対象を再検出できません。対象を選び直してください。"}
          </p>
          {resolution.candidates.length > 0 ? (
            <ul>
              {resolution.candidates.map((candidate) => (
                <li key={candidate.candidateId}>
                  <span>{candidate.summary}</span>
                  <strong>{candidate.reasons.join(" · ")}</strong>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}

      <form className="prompt-composer" onSubmit={submit}>
        <div className="provider-fields">
          <label htmlFor="ai-provider">AI provider</label>
          <select
            id="ai-provider"
            value={providerId}
            disabled={disabled}
            onChange={(event) => onProvider(event.currentTarget.value)}
          >
            {providers.map((candidate) => (
              <option
                key={candidate.id}
                value={candidate.id}
                disabled={candidate.availability !== "available"}
              >
                {candidate.label}
                {candidate.availability === "available" ? "" : "（未設定）"}
              </option>
            ))}
          </select>
          <label htmlFor="ai-model">Model</label>
          <select
            id="ai-model"
            value={requestedModel}
            disabled={disabled || provider?.availability !== "available"}
            onChange={(event) => onModel(event.currentTarget.value)}
          >
            {(provider?.models ?? []).map((model) => (
              <option key={model.id} value={model.id}>
                {model.label}
              </option>
            ))}
          </select>
        </div>
        {provider?.external === true ? (
          <p className="provider-disclosure">
            選択したcontextは外部providerへ送信されます。送信前にexact
            bytesを確認します。契約に応じてprovider料金が発生する場合があります。
          </p>
        ) : null}
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
          <span>
            {provider?.id ?? "provider未選択"} ·{" "}
            {requestedModel || "model未選択"}
          </span>
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
            capturedTarget === null ||
            resolution?.status !== "resolved" ||
            prompt.trim().length === 0 ||
            instructionBytes > 2000 ||
            provider?.availability !== "available" ||
            !provider.models.some((model) => model.id === requestedModel) ||
            capturedTarget.captureRevisionId !== project.revisionId ||
            resolution.resolvedRevisionId !== project.revisionId
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
  target: TargetV1 | null;
  prompt: string;
  busy: boolean;
  decisionReconciliationRequired: boolean;
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
  decisionReconciliationRequired,
  rationale,
  onRationale,
  onAdopt,
  onSource,
}: ReviewDrawerProps) {
  const hardError = proposal.validation.status === "failed";
  const blockingWarning = proposal.validation.checks.some(
    (check) => check.blocking,
  );
  const rationaleBytes = utf8Bytes(rationale);
  const reportedUsage = [
    { label: "input", tokens: proposal.attribution.usage?.inputTokens },
    { label: "output", tokens: proposal.attribution.usage?.outputTokens },
    { label: "total", tokens: proposal.attribution.usage?.totalTokens },
  ]
    .map(({ label, tokens }) =>
      tokens === undefined
        ? null
        : `${label} ${tokens.toLocaleString("ja-JP")}`,
    )
    .filter((value): value is string => value !== null)
    .join(" · ");
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
          <span>
            {proposal.attribution.providerId} ·{" "}
            {proposal.attribution.reportedModel}
          </span>
          <code>{proposal.attribution.adapterVersion}</code>
          <span>
            request {shortIdentity(proposal.attribution.providerRequestId)}
          </span>
          <span className="unverified">! execution未検証</span>
          {reportedUsage === "" ? null : (
            <span className="provider-usage">{reportedUsage} tokens</span>
          )}
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
                {change.fromPath === undefined ? null : (
                  <code>{change.fromPath} →</code>
                )}
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
              <li
                key={check.id}
                className={check.blocking ? "validation-blocking" : undefined}
              >
                <strong>
                  {check.status === "passed" ? "✓" : "!"} {check.label}
                </strong>
                <span>{check.message}</span>
                {check.destinations.length === 0 ? null : (
                  <ul>
                    {check.destinations.map((destination) => (
                      <li key={destination}>
                        <code>{destination}</code>
                      </li>
                    ))}
                  </ul>
                )}
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
          <details className="change-set-details">
            <summary>
              ChangeSet v{proposal.changeSet.version} ·{" "}
              {proposal.changeSet.operations.length} operations
            </summary>
            <p>
              SHA-256 <code>{proposal.changeSetSha256}</code>
            </p>
            <ol>
              {proposal.changeSet.operations.map((operation, index) => (
                <li key={`${operation.op}:${String(index)}`}>
                  <code>{operation.op}</code>{" "}
                  {"path" in operation
                    ? operation.path
                    : `${operation.from} → ${operation.to}`}
                </li>
              ))}
            </ol>
          </details>
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
            disabled={busy || decisionReconciliationRequired}
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
          disabled={
            busy ||
            decisionReconciliationRequired ||
            hardError ||
            blockingWarning ||
            rationaleBytes > 2000
          }
          onClick={onAdopt}
        >
          変更を採用
        </button>
        {decisionReconciliationRequired ? (
          <p className="blocking-decision-note" role="status">
            Decision結果を再照合するまで採用操作はロックされています。画面を再読み込みし、Accepted
            revisionを確認してください。
          </p>
        ) : null}
        {blockingWarning ? (
          <p className="blocking-decision-note" role="alert">
            新しいactive
            behaviorを含むため、このProposalは採用できません。要望を修正して新しいProposalを作成してください。
          </p>
        ) : null}
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

interface ImportReviewDialogProps {
  preview: ImportPreview;
  limits: ImportLimits;
  busy: boolean;
  returnFocus: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onConfirm: () => void;
}

function ImportReviewDialog({
  preview,
  limits,
  busy,
  returnFocus,
  onClose,
  onConfirm,
}: ImportReviewDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const listener = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", listener);
    return () => {
      window.removeEventListener("keydown", listener);
      returnFocus.current?.focus();
    };
  }, [busy, onClose, returnFocus]);

  return (
    <div className="dialog-backdrop">
      <section
        className="dialog-card import-review-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-dialog-title"
        aria-describedby="import-dialog-description"
      >
        <div className="dialog-heading">
          <div>
            <p className="eyebrow">REGISTERED ROOT · EXACT COPY REVIEW</p>
            <h2 id="import-dialog-title">取り込むファイルを確認</h2>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="icon-button"
            aria-label="取り込み確認を閉じる"
            disabled={busy}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <p id="import-dialog-description">
          登録済みディレクトリから、下記のファイルだけを新しいプロジェクトへコピーします。取り込み元のファイルは変更しません。
        </p>

        <dl className="import-summary">
          <div>
            <dt>表示名</dt>
            <dd>{preview.displayName}</dd>
          </div>
          <div>
            <dt>Entry point</dt>
            <dd>
              {preview.entryPoint === null ? (
                <span className="import-entry-missing">未検出</span>
              ) : (
                <code>{preview.entryPoint}</code>
              )}
            </dd>
          </div>
          <div>
            <dt>合計</dt>
            <dd>
              {preview.included.length.toLocaleString("ja-JP")} files ·{" "}
              {preview.totalBytes.toLocaleString("ja-JP")} bytes
            </dd>
          </div>
          <div className="import-summary-digest">
            <dt>Manifest SHA-256</dt>
            <dd>
              <code>{preview.manifestSha256}</code>
            </dd>
          </div>
        </dl>

        <section aria-labelledby="import-included-title">
          <h3 id="import-included-title">
            コピー対象（{preview.included.length}）
          </h3>
          <div className="import-table-scroll">
            <table className="import-file-table">
              <thead>
                <tr>
                  <th scope="col">相対path</th>
                  <th scope="col">Bytes</th>
                  <th scope="col">SHA-256</th>
                </tr>
              </thead>
              <tbody>
                {preview.included.map((file) => (
                  <tr key={`${file.path}:${file.sha256}`}>
                    <td>
                      <code>{file.path}</code>
                    </td>
                    <td>{file.byteLength.toLocaleString("ja-JP")}</td>
                    <td>
                      <code>{file.sha256}</code>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <div className="import-review-columns">
          <section aria-labelledby="import-excluded-title">
            <h3 id="import-excluded-title">
              コピーしないファイル（{preview.excluded.length}）
            </h3>
            {preview.excluded.length === 0 ? (
              <p className="import-empty">ありません</p>
            ) : (
              <ul className="import-excluded-list">
                {preview.excluded.map((file) => (
                  <li key={`${file.path}:${file.reason}`}>
                    <code>{file.path}</code>
                    <span>{file.reason}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section aria-labelledby="import-limits-title">
            <h3 id="import-limits-title">適用した上限</h3>
            <dl className="import-limits">
              <div>
                <dt>ファイル数</dt>
                <dd>{limits.maxFiles.toLocaleString("ja-JP")}</dd>
              </div>
              <div>
                <dt>合計bytes</dt>
                <dd>{limits.maxTotalBytes.toLocaleString("ja-JP")}</dd>
              </div>
              <div>
                <dt>1ファイルbytes</dt>
                <dd>{limits.maxFileBytes.toLocaleString("ja-JP")}</dd>
              </div>
              <div>
                <dt>path bytes</dt>
                <dd>{limits.maxPathBytes.toLocaleString("ja-JP")}</dd>
              </div>
              <div>
                <dt>階層</dt>
                <dd>{limits.maxDepth.toLocaleString("ja-JP")}</dd>
              </div>
            </dl>
          </section>
        </div>

        {preview.warnings.length > 0 ? (
          <section className="import-warnings" aria-labelledby="warnings-title">
            <h3 id="warnings-title">確認事項</h3>
            <ul>
              {preview.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </section>
        ) : null}

        {preview.entryPoint !== "index.html" ? (
          <p className="error-card import-entry-error" role="alert">
            ルート直下の index.html
            を確認できないため、このプレビューは取り込めません。登録済みディレクトリを修正してから、もう一度検査してください。
          </p>
        ) : null}

        <div className="dialog-actions">
          <button
            type="button"
            className="button button-quiet"
            disabled={busy}
            onClick={onClose}
          >
            キャンセル
          </button>
          <button
            type="button"
            className="button button-primary"
            disabled={busy || preview.entryPoint !== "index.html"}
            onClick={onConfirm}
          >
            この内容をコピーして取り込む
          </button>
        </div>
      </section>
    </div>
  );
}

interface StudioProps {
  session: BootstrappedApi;
}

function Studio({ session }: StudioProps) {
  const [state, dispatch] = useReducer(studioReducer, initialStudioState);
  const [retainedProjects, setRetainedProjects] = useState<Project[] | null>(
    null,
  );
  const [homeOperation, setHomeOperation] = useState<HomeOperation>(null);
  const [homeError, setHomeError] = useState<string | null>(null);
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(
    null,
  );
  const [prompt, setPrompt] = useState(
    "見出しを、未来への期待が伝わる表現にしてください",
  );
  const providers = session.bootstrap.capabilities.aiProviders;
  const initialProvider =
    providers.find(
      (provider) =>
        provider.id === "fake" && provider.availability === "available",
    ) ?? providers.find((provider) => provider.availability === "available");
  const [providerId, setProviderId] = useState(initialProvider?.id ?? "");
  const [requestedModel, setRequestedModel] = useState(
    initialProvider?.models[0]?.id ?? "",
  );
  const [rationale, setRationale] = useState("");
  const [targetKind, setTargetKind] = useState<TargetKind>("element");
  const [structure, setStructure] = useState<PreviewStructureNode[]>([]);
  const [captureCommand, setCaptureCommand] = useState<CaptureCommand | null>(
    null,
  );
  const captureCommandId = useRef(0);
  const targetRequestId = useRef(0);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const importButtonRef = useRef<HTMLButtonElement>(null);
  const busy = state.operation !== null || homeOperation !== null;

  const run = useCallback(async (task: () => Promise<void>) => {
    try {
      await task();
    } catch (error) {
      dispatch({
        type: "FAILED",
        message: errorMessage(error),
        ...(error instanceof DecisionOutcomeError
          ? { requiresDecisionReconciliation: true }
          : {}),
      });
    }
  }, []);

  const runHome = useCallback(
    async (
      operation: Exclude<HomeOperation, null>,
      task: () => Promise<void>,
    ) => {
      setHomeOperation(operation);
      setHomeError(null);
      try {
        await task();
      } catch (error) {
        setHomeError(errorMessage(error));
      } finally {
        setHomeOperation(null);
      }
    },
    [],
  );

  useEffect(() => {
    let active = true;
    setHomeOperation("loading_projects");
    setHomeError(null);
    void session.api
      .listProjects()
      .then((projects) => {
        if (active) setRetainedProjects(projects);
      })
      .catch((error: unknown) => {
        if (active) setHomeError(errorMessage(error));
      })
      .finally(() => {
        if (active) setHomeOperation(null);
      });
    return () => {
      active = false;
    };
  }, [session.api]);

  const createProject = () => {
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "creating_project" });
      const project = await session.api.createBlankProject();
      dispatch({ type: "PROJECT_LOADED", project, origin: "created" });
    });
  };

  const openProject = (projectId: string) => {
    void runHome("opening_project", async () => {
      const project = await session.api.getProject(projectId);
      dispatch({ type: "PROJECT_LOADED", project, origin: "opened" });
    });
  };

  const previewRegisteredImport = () => {
    void runHome("previewing_import", async () => {
      const preview = await session.api.previewRegisteredImport();
      setImportPreview(preview);
    });
  };

  const closeImportPreview = useCallback(() => {
    if (homeOperation === null) setImportPreview(null);
  }, [homeOperation]);

  const confirmRegisteredImport = () => {
    if (importPreview === null) return;
    const preview = importPreview;
    void runHome("importing_project", async () => {
      const project = await session.api.confirmRegisteredImport(
        preview.id,
        preview.manifestSha256,
      );
      setImportPreview(null);
      dispatch({ type: "PROJECT_LOADED", project, origin: "imported" });
    });
  };

  const selectTarget = useCallback(
    (targetDraft: TargetV1) => {
      if (state.project === null || state.operation !== null) return;
      const project = state.project;
      const proposal = state.proposal;
      const expectedCaptureSource =
        state.previewSource === "proposed" && proposal !== null
          ? "proposal"
          : "accepted";
      if (
        targetDraft.captureRevisionId !== project.revisionId ||
        targetDraft.captureSource !== expectedCaptureSource ||
        (targetDraft.captureSource === "proposal" &&
          targetDraft.captureProposalId !== proposal?.id)
      ) {
        dispatch({
          type: "FAILED",
          message:
            "表示中のプレビューとTargetのcapture bindingが一致しません。Accepted LPは変更されていません。",
        });
        return;
      }
      const requestId = ++targetRequestId.current;
      void (async () => {
        dispatch({ type: "OPERATION_STARTED", operation: "selecting_target" });
        try {
          const target = await session.api.createTarget(
            project.id,
            targetDraft,
          );
          if (requestId !== targetRequestId.current) return;
          if (
            target.target.targetId !== targetDraft.targetId ||
            target.target.captureRevisionId !== targetDraft.captureRevisionId ||
            target.target.captureSource !== targetDraft.captureSource ||
            target.target.captureProposalId !== targetDraft.captureProposalId ||
            target.target.pagePath !== targetDraft.pagePath ||
            target.target.kind !== targetDraft.kind
          ) {
            throw new ApiError(
              "Target APIのcapture bindingが要求と一致しません。",
              { code: "target_binding_mismatch", retryable: false },
            );
          }
          dispatch({ type: "TARGET_SELECTED", target });
        } catch (error) {
          if (requestId === targetRequestId.current) {
            dispatch({ type: "FAILED", message: errorMessage(error) });
          }
        }
      })();
    },
    [
      session.api,
      state.operation,
      state.previewSource,
      state.project,
      state.proposal,
    ],
  );

  const requestPageCapture = useCallback(() => {
    setTargetKind("page");
    dispatch({ type: "SET_PREVIEW_MODE", mode: "select" });
    if (state.target !== null && state.operation === null) {
      dispatch({ type: "TARGET_CLEARED" });
    }
    captureCommandId.current += 1;
    setCaptureCommand({ id: captureCommandId.current, action: "page" });
  }, [state.operation, state.target]);

  const requestNodeCapture = useCallback(
    (node: PreviewStructureNode) => {
      captureCommandId.current += 1;
      setCaptureCommand(
        targetKind === "page"
          ? { id: captureCommandId.current, action: "page" }
          : {
              id: captureCommandId.current,
              action: "node",
              runtimeNodeHandle: node.runtimeNodeHandle,
              targetKind,
            },
      );
    },
    [targetKind],
  );

  const changeTargetKind = useCallback(
    (kind: TargetKind) => {
      setTargetKind(kind);
      dispatch({ type: "SET_PREVIEW_MODE", mode: "select" });
      if (state.target !== null && state.operation === null) {
        dispatch({ type: "TARGET_CLEARED" });
      }
    },
    [state.operation, state.target],
  );

  const clearTarget = useCallback(() => {
    if (state.operation === null) dispatch({ type: "TARGET_CLEARED" });
  }, [state.operation]);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || state.operation !== null) return;
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
  }, [clearTarget, state.contextReview, state.operation, state.target]);

  const reviewContext = () => {
    if (state.project === null || state.target === null) return;
    const project = state.project;
    const selection = state.target;
    const instruction = prompt.trim();
    const provider = providers.find(
      (candidate) =>
        candidate.id === providerId && candidate.availability === "available",
    );
    if (
      instruction.length === 0 ||
      provider === undefined ||
      !provider.models.some((model) => model.id === requestedModel) ||
      selection.resolution.status !== "resolved" ||
      selection.resolution.resolvedRevisionId !== project.revisionId
    ) {
      return;
    }
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "assembling_context" });
      const attemptId = crypto.randomUUID();
      const context = await session.api.createContext(
        project.id,
        project.revisionId,
        selection.target.targetId,
        selection.resolutionId,
        attemptId,
        provider.id,
        requestedModel,
        instruction,
      );
      if (
        context.revisionId !== project.revisionId ||
        context.targetId !== selection.target.targetId ||
        context.targetResolutionId !== selection.resolutionId ||
        context.attemptId !== attemptId ||
        context.providerId !== provider.id ||
        context.requestedModel !== requestedModel ||
        context.provider.providerId !== provider.id ||
        context.provider.requestedModel !== requestedModel ||
        context.provider.adapterVersion !== provider.adapterVersion ||
        context.provider.external !== provider.external
      ) {
        throw new ApiError(
          "送信コンテキストのTargetまたはprovider bindingが一致しません。",
          {
            code: "context_binding_mismatch",
            retryable: false,
          },
        );
      }
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
      if (
        proposal.baseRevisionId !== project.revisionId ||
        proposal.changeSet.baseRevisionId !== project.revisionId ||
        proposal.providerContextSha256 !== contextReview.sha256 ||
        proposal.attribution.attemptId !== contextReview.attemptId ||
        proposal.attribution.providerId !== contextReview.provider.providerId ||
        proposal.attribution.requestedModel !==
          contextReview.provider.requestedModel ||
        proposal.attribution.adapterVersion !==
          contextReview.provider.adapterVersion ||
        proposal.attribution.external !== contextReview.provider.external
      ) {
        throw new ApiError(
          "変更案のAccepted revisionまたはreviewed provider context bindingが一致しません。",
          { code: "proposal_binding_mismatch", retryable: false },
        );
      }
      dispatch({
        type: "PROPOSAL_READY",
        proposal,
        instruction: contextReview.instruction,
      });
    });
  };

  const adopt = () => {
    if (
      state.project === null ||
      state.proposal === null ||
      state.decisionReconciliationRequired
    )
      return;
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
      try {
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
      } catch (error) {
        throw new DecisionOutcomeError("decision", error);
      }
      dispatch({
        type: "DECISION_COMMITTED",
        disposition: "adopted_unchanged",
      });
      let refreshed: Project;
      try {
        refreshed = await session.api.getProject(project.id);
      } catch (error) {
        throw new DecisionOutcomeError("refresh", error);
      }
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
      <>
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

            <div className="home-actions">
              <button
                type="button"
                className="button button-primary home-create"
                disabled={busy}
                onClick={createProject}
              >
                空のLPを作成
              </button>
              {session.bootstrap.capabilities.importAvailable ? (
                <button
                  ref={importButtonRef}
                  type="button"
                  className="button button-secondary home-import"
                  disabled={busy}
                  onClick={previewRegisteredImport}
                >
                  登録済みディレクトリを取り込む
                </button>
              ) : null}
            </div>
            {session.bootstrap.capabilities.importAvailable ? (
              <p className="home-import-boundary">
                取り込み前にファイル・除外理由・上限・digestを確認します。選んだファイルはプロジェクトへコピーされ、取り込み元は変更されません。
              </p>
            ) : null}

            <section
              className="retained-projects"
              aria-labelledby="retained-projects-title"
              aria-busy={homeOperation === "loading_projects"}
            >
              <div className="retained-projects-heading">
                <div>
                  <p className="eyebrow">RETAINED LOCALLY</p>
                  <h2 id="retained-projects-title">保存済みプロジェクト</h2>
                </div>
                <span>{retainedProjects?.length ?? 0}</span>
              </div>
              {retainedProjects === null ? (
                <p className="retained-empty">一覧を読み込んでいます…</p>
              ) : retainedProjects.length === 0 ? (
                <p className="retained-empty">
                  保存済みプロジェクトはまだありません。
                </p>
              ) : (
                <ul className="retained-project-list">
                  {retainedProjects.map((project) => (
                    <li key={project.id}>
                      <div>
                        <strong>{project.displayName}</strong>
                        <span>
                          {project.files.length} files · revision{" "}
                          <code>{shortIdentity(project.revisionId)}</code>
                        </span>
                      </div>
                      <button
                        type="button"
                        className="button button-quiet"
                        disabled={busy}
                        onClick={() => openProject(project.id)}
                        aria-label={`${project.displayName}を開く`}
                      >
                        開く
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            {state.operation !== null ? (
              <p role="status">{operationLabel[state.operation]}…</p>
            ) : homeOperation !== null ? (
              <p role="status">{homeOperationLabel[homeOperation]}…</p>
            ) : null}
            {state.error !== null || homeError !== null ? (
              <p className="error-card" role="alert">
                {state.error ?? homeError}
              </p>
            ) : null}
          </section>
        </main>

        {importPreview !== null ? (
          <ImportReviewDialog
            preview={importPreview}
            limits={session.bootstrap.capabilities.limits}
            busy={homeOperation === "importing_project"}
            returnFocus={importButtonRef}
            onClose={closeImportPreview}
            onConfirm={confirmRegisteredImport}
          />
        ) : null}
      </>
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
          targetKind={targetKind}
          structure={structure}
          disabled={busy}
          onTargetKind={changeTargetKind}
          onCapturePage={requestPageCapture}
          onCaptureNode={requestNodeCapture}
        />
        <PreviewPane
          project={state.project}
          proposal={state.proposal}
          previewScopeBaseOrigin={session.bootstrap.previewOrigin}
          target={state.target}
          targetKind={targetKind}
          captureCommand={captureCommand}
          mode={state.previewMode}
          source={state.previewSource}
          viewport={state.viewportPreset}
          customWidth={state.customWidth}
          disabled={busy}
          onMode={(mode) => dispatch({ type: "SET_PREVIEW_MODE", mode })}
          onSource={(source) =>
            dispatch({ type: "SET_PREVIEW_SOURCE", source })
          }
          onTarget={selectTarget}
          onStructure={setStructure}
          onBridgeError={(message) => dispatch({ type: "FAILED", message })}
          onClear={clearTarget}
        />
        <TargetComposer
          project={state.project}
          target={state.target}
          prompt={prompt}
          providers={providers}
          providerId={providerId}
          requestedModel={requestedModel}
          disabled={
            busy || state.proposal !== null || state.contextReview !== null
          }
          reviewButtonRef={reviewButtonRef}
          onPrompt={setPrompt}
          onProvider={(nextProviderId) => {
            const next = providers.find(
              (provider) => provider.id === nextProviderId,
            );
            setProviderId(nextProviderId);
            setRequestedModel(next?.models[0]?.id ?? "");
          }}
          onModel={setRequestedModel}
          onReviewContext={reviewContext}
        />
      </div>

      {state.proposal !== null ? (
        <ReviewDrawer
          proposal={state.proposal}
          target={state.proposalTarget}
          prompt={state.proposalInstruction ?? ""}
          busy={busy}
          decisionReconciliationRequired={state.decisionReconciliationRequired}
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
          context={state.contextReview}
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
