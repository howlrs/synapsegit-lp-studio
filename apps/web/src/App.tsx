import {
  useCallback,
  useEffect,
  useLayoutEffect,
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
  PublicationDraft,
  PreviewDiagnosticCode,
  PreviewDiagnosticMessage,
  PreviewMode,
  PreviewSource,
  PreviewStructureNode,
  Project,
  Proposal,
  RecoveryPoint,
  RetentionCleanupRequest,
  RetentionInventory,
  Review,
  TargetKind,
  TargetSelection,
  TargetV1,
  ViewportPreset,
} from "@synapsegit-lp/contracts";
import {
  PROJECT_DISPLAY_NAME_LIMITS,
  PROJECT_METADATA_AUTOSAVE_DEBOUNCE_MS,
  isProjectDisplayName,
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
  type PreviewScale,
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

const dispositionLabel: Record<ArtifactDisposition, string> = {
  adopted_unchanged: "採用",
  rejected: "却下",
  deferred: "保留（終端）",
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

const MODAL_FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

const modalFocusableElements = (dialog: HTMLElement): HTMLElement[] =>
  Array.from(
    dialog.querySelectorAll<HTMLElement>(MODAL_FOCUSABLE_SELECTOR),
  ).filter(
    (element) =>
      !element.hidden &&
      element.getAttribute("aria-hidden") !== "true" &&
      element.closest("[hidden], [inert]") === null,
  );

interface ModalDialogOptions {
  busy: boolean;
  dialogRef: React.RefObject<HTMLElement | null>;
  initialFocusRef: React.RefObject<HTMLElement | null>;
  returnFocusRef: React.RefObject<HTMLElement | null>;
  onClose: () => void;
}

function useModalDialog({
  busy,
  dialogRef,
  initialFocusRef,
  returnFocusRef,
  onClose,
}: ModalDialogOptions): void {
  const busyRef = useRef(busy);
  const onCloseRef = useRef(onClose);

  useLayoutEffect(() => {
    busyRef.current = busy;
    onCloseRef.current = onClose;
  }, [busy, onClose]);

  useLayoutEffect(() => {
    const dialog = dialogRef.current;
    if (dialog === null) return;

    const modalRoot = dialog.closest<HTMLElement>("[data-modal-root]");
    const appRoot = modalRoot?.parentElement ?? null;
    const background =
      modalRoot === null || appRoot === null
        ? []
        : Array.from(appRoot.children)
            .filter(
              (element): element is HTMLElement =>
                element instanceof HTMLElement && element !== modalRoot,
            )
            .map((element) => ({
              element,
              hadInert: element.hasAttribute("inert"),
              ariaHidden: element.getAttribute("aria-hidden"),
            }));

    for (const { element } of background) {
      element.setAttribute("inert", "");
      element.setAttribute("aria-hidden", "true");
    }

    initialFocusRef.current?.focus({ preventScroll: true });
    if (!dialog.contains(document.activeElement)) {
      dialog.focus({ preventScroll: true });
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (!busyRef.current) onCloseRef.current();
        return;
      }
      if (event.key !== "Tab") return;

      const focusable = modalFocusableElements(dialog);
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus({ preventScroll: true });
        return;
      }
      const first = focusable[0]!;
      const last = focusable.at(-1)!;
      const active = document.activeElement;
      if (event.shiftKey) {
        if (active === first || !dialog.contains(active)) {
          event.preventDefault();
          last.focus({ preventScroll: true });
        }
      } else if (active === last || !dialog.contains(active)) {
        event.preventDefault();
        first.focus({ preventScroll: true });
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      for (const { element, hadInert, ariaHidden } of background) {
        if (!hadInert) element.removeAttribute("inert");
        if (ariaHidden === null) element.removeAttribute("aria-hidden");
        else element.setAttribute("aria-hidden", ariaHidden);
      }
      const returnTarget = returnFocusRef.current;
      queueMicrotask(() => {
        if (
          returnTarget?.isConnected === true &&
          !(returnTarget instanceof HTMLButtonElement && returnTarget.disabled)
        ) {
          returnTarget.focus({ preventScroll: true });
        }
      });
    };
  }, [dialogRef, initialFocusRef, returnFocusRef]);
}

const operationLabel: Record<Exclude<StudioOperation, null>, string> = {
  creating_project: "空のLPを作成しています",
  selecting_target: "ターゲットを検証しています",
  assembling_context: "送信コンテキストを組み立てています",
  generating_proposal: "AIで変更案を生成・検証しています",
  committing_decision: "一回限りの承認を取得しDecisionを記録しています",
  refreshing_project: "Accepted状態を再取得しています",
  restoring_review: "保存済みReviewを復元しています",
  reconciling_review: "Decision結果を再照合しています",
  exporting: "Accepted revisionをエクスポートしています",
  generating_publication: "GitHub-ready filesをローカル生成しています",
  downloading_publication: "確認済みpublication ZIPを取得しています",
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
  readonly disposition: ArtifactDisposition;
  readonly safeCause: unknown;

  constructor(
    phase: "decision" | "refresh",
    disposition: ArtifactDisposition,
    cause: unknown,
  ) {
    super(cause instanceof Error ? cause.message : "");
    this.name = "DecisionOutcomeError";
    this.phase = phase;
    this.disposition = disposition;
    this.safeCause = cause;
  }
}

const errorDiagnostic = (
  error: unknown,
): {
  code: string;
  requestId: string | null;
  operationId: string | null;
  retryable: boolean;
  acceptedState: "unchanged" | "reconciliation_required" | null;
  recoveryAction:
    | "retry"
    | "refresh"
    | "reconcile"
    | "correct_request"
    | "manual_recovery"
    | null;
} => {
  const cause = error instanceof DecisionOutcomeError ? error.safeCause : error;
  if (cause instanceof ApiError) {
    return {
      code: cause.code,
      requestId: cause.requestId ?? null,
      operationId: cause.operationId ?? null,
      retryable: cause.retryable,
      acceptedState: cause.detail?.acceptedState ?? null,
      recoveryAction: cause.detail?.recoveryAction ?? null,
    };
  }
  if (error instanceof DecisionOutcomeError) {
    return {
      code:
        error.phase === "decision"
          ? "decision_outcome_unknown"
          : "accepted_refresh_outcome_unknown",
      requestId: null,
      operationId: null,
      retryable: false,
      acceptedState: null,
      recoveryAction: null,
    };
  }
  return {
    code: "unexpected_client_error",
    requestId: null,
    operationId: null,
    retryable: true,
    acceptedState: null,
    recoveryAction: null,
  };
};

const acceptedStateMessage = {
  unchanged: "Accepted LPは変更されていません。",
  reconciliation_required: "Accepted状態は再照合が必要です。",
} as const;

const recoveryActionMessage = {
  retry: "新しい操作として安全に再試行できます。",
  refresh: "プロジェクト状態を再取得してください。",
  reconcile: "Decision結果を再照合してください。",
  correct_request: "入力内容を修正してから再試行してください。",
  manual_recovery: "read-only recoveryの案内に従ってください。",
} as const;

const errorMessage = (error: unknown): string => {
  if (error instanceof DecisionOutcomeError) {
    const detail = error.message.trim();
    const prefix = detail.length === 0 ? "" : `${detail} `;
    return error.phase === "decision"
      ? error.disposition === "adopted_unchanged"
        ? `${prefix}Human Decisionの結果は不明です。Accepted LPが変更された可能性があります。Decisionを再実行せず、画面を再読み込みしてAccepted revisionを照合してください。`
        : `${prefix}Human Decisionの結果は不明です。Accepted LPは変更しないDecisionですが、Proposalの終端状態を再照合するまでDecisionを再実行しないでください。`
      : `${prefix}Human Decisionは記録されましたが、Accepted状態を再取得できず、現在の確認結果は不明です。Decisionを再実行せず、画面を再読み込みしてAccepted revisionを照合してください。`;
  }
  if (error instanceof ApiError) {
    const accepted =
      error.detail === undefined
        ? "Accepted状態はこの応答から確定できません。"
        : acceptedStateMessage[error.detail.acceptedState];
    const recovery =
      error.detail === undefined
        ? error.retryable
          ? "状態を確認してから再試行してください。"
          : "入力と状態を確認してください。"
        : recoveryActionMessage[error.detail.recoveryAction];
    return `${error.message} ${accepted}${recovery}`;
  }
  return "予期しないエラーが発生しました。Accepted状態は確認できません。画面を再読み込みして状態を確認してください。";
};

const shortIdentity = (value: string): string =>
  value.length > 18 ? `${value.slice(0, 10)}…${value.slice(-6)}` : value;

const utf8Bytes = (value: string): number =>
  new TextEncoder().encode(value).byteLength;

interface ExpectedReviewBinding {
  reviewId: string;
  proposalId: string;
  baseRevisionId: string;
}

const assertReviewBinding = (
  project: Project,
  expected: ExpectedReviewBinding,
  review: Review,
): void => {
  if (
    review.projectId !== project.id ||
    review.reviewId !== expected.reviewId ||
    review.proposalId !== expected.proposalId ||
    (review.proposal !== null &&
      review.proposal.baseRevisionId !== expected.baseRevisionId)
  ) {
    throw new ApiError(
      "保存済みReviewのproject/Proposal bindingが一致しません。",
      {
        code: "review_binding_mismatch",
        retryable: false,
      },
    );
  }
};

interface StudioHeaderProps {
  project: Project;
  api: AuthenticatedApi;
  viewport: ViewportPreset;
  customWidth: number;
  busy: boolean;
  exportButtonRef: React.RefObject<HTMLButtonElement | null>;
  publicationButtonRef: React.RefObject<HTMLButtonElement | null>;
  onViewport: (preset: ViewportPreset, customWidth?: number) => void;
  onExport: () => void;
  onPublication: () => void;
  onDisplayNameSaved: (projectId: string, displayName: string) => void;
}

type ProjectNameSaveState = "saved" | "waiting" | "saving" | "error";

function ProjectDisplayNameEditor({
  project,
  api,
  onSaved,
}: {
  project: Project;
  api: AuthenticatedApi;
  onSaved: (projectId: string, displayName: string) => void;
}) {
  const [draft, setDraft] = useState(project.displayName);
  const [saveState, setSaveState] = useState<ProjectNameSaveState>("saved");
  const [error, setError] = useState<string | null>(null);
  const confirmedName = useRef(project.displayName);
  const draftRef = useRef(project.displayName);
  const requestSequence = useRef(0);
  const projectGeneration = useRef(0);
  const saveChain = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    projectGeneration.current += 1;
    requestSequence.current += 1;
    confirmedName.current = project.displayName;
    draftRef.current = project.displayName;
    setDraft(project.displayName);
    setSaveState("saved");
    setError(null);
  }, [project.id]);

  useEffect(() => {
    draftRef.current = draft;
    if (draft === confirmedName.current) {
      setSaveState("saved");
      setError(null);
      return;
    }
    if (!isProjectDisplayName(draft)) {
      requestSequence.current += 1;
      setSaveState("error");
      setError(
        `表示名はNFCで1〜${PROJECT_DISPLAY_NAME_LIMITS.codePoints}文字・${PROJECT_DISPLAY_NAME_LIMITS.utf8Bytes} UTF-8 bytes以内とし、空白だけ、制御文字、絶対pathを含めないでください。`,
      );
      return;
    }

    const sequence = ++requestSequence.current;
    const generation = projectGeneration.current;
    setSaveState("waiting");
    setError(null);
    const timer = window.setTimeout(() => {
      const requestedDisplayName = draftRef.current;
      if (
        sequence !== requestSequence.current ||
        generation !== projectGeneration.current ||
        !isProjectDisplayName(requestedDisplayName)
      ) {
        return;
      }
      saveChain.current = saveChain.current.then(async () => {
        if (generation !== projectGeneration.current) return;
        const expectedDisplayName = confirmedName.current;
        if (requestedDisplayName === expectedDisplayName) return;
        if (sequence === requestSequence.current) setSaveState("saving");
        try {
          const response = await api.updateProjectDisplayName(
            project.id,
            expectedDisplayName,
            requestedDisplayName,
          );
          if (
            response.id !== project.id ||
            response.displayName !== requestedDisplayName
          ) {
            throw new ApiError("表示名更新のProject bindingが一致しません。", {
              code: "project_metadata_binding_mismatch",
              retryable: false,
            });
          }
          if (generation !== projectGeneration.current) return;
          confirmedName.current = requestedDisplayName;
          onSaved(project.id, requestedDisplayName);
          if (
            sequence === requestSequence.current &&
            draftRef.current === requestedDisplayName
          ) {
            setSaveState("saved");
            setError(null);
          }
        } catch (saveError) {
          if (
            generation === projectGeneration.current &&
            sequence === requestSequence.current
          ) {
            setSaveState("error");
            setError(errorMessage(saveError));
          }
        }
      });
    }, PROJECT_METADATA_AUTOSAVE_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [api, draft, onSaved, project.id]);

  const status =
    saveState === "waiting"
      ? "入力待ち"
      : saveState === "saving"
        ? "保存中"
        : saveState === "error"
          ? "保存エラー"
          : "保存済み";

  return (
    <div className="project-name-editor">
      <label htmlFor="project-display-name">Project</label>
      <input
        id="project-display-name"
        type="text"
        value={draft}
        aria-invalid={saveState === "error"}
        aria-describedby="project-display-name-status"
        onChange={(event) =>
          setDraft(event.currentTarget.value.normalize("NFC"))
        }
      />
      <span
        id="project-display-name-status"
        className={`project-name-status project-name-status-${saveState}`}
        role="status"
        aria-live="polite"
      >
        {status}
        {error === null ? null : <span className="sr-only">: {error}</span>}
      </span>
    </div>
  );
}

function StudioHeader({
  project,
  api,
  viewport,
  customWidth,
  busy,
  exportButtonRef,
  publicationButtonRef,
  onViewport,
  onExport,
  onPublication,
  onDisplayNameSaved,
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
          <dt className="sr-only">Project</dt>
          <dd>
            <ProjectDisplayNameEditor
              project={project}
              api={api}
              onSaved={onDisplayNameSaved}
            />
          </dd>
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
          ref={exportButtonRef}
          type="button"
          className="button button-secondary"
          disabled={busy}
          onClick={onExport}
        >
          Acceptedをエクスポート
        </button>
        <button
          ref={publicationButtonRef}
          type="button"
          className="button button-quiet"
          disabled={
            busy || project.activeReview?.status === "reconciliation_required"
          }
          title={
            project.activeReview?.status === "reconciliation_required"
              ? "Decision結果を再照合してからpublication recordを生成してください"
              : undefined
          }
          onClick={onPublication}
        >
          GitHub-ready記録
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
        で選択。座標は要素の中心、領域は要素のboxとしてkeyboard指定できます。
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
  previewScale: PreviewScale;
  disabled: boolean;
  onMode: (mode: PreviewMode) => void;
  onSource: (source: PreviewSource) => void;
  onPreviewScale: (scale: PreviewScale) => void;
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
  previewScale,
  disabled,
  onMode,
  onSource,
  onPreviewScale,
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
  const [previewPaused, setPreviewPaused] = useState(false);
  const [previewGeneration, setPreviewGeneration] = useState(0);
  const activeSource =
    source === "proposed" && proposal === null ? "accepted" : source;
  const previewUrl =
    activeSource === "proposed" && proposal !== null
      ? proposal.previewUrl
      : project.previewUrl;
  const channelId = useMemo(
    () => createChannelId(),
    [
      activeSource,
      previewGeneration,
      previewUrl,
      project.id,
      project.revisionId,
    ],
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
    postPreviewMode(frameWindow, bridgeBinding, mode, targetKind, previewScale);
    postRequestStructure(frameWindow, bridgeBinding);
  };

  useEffect(synchronizeMode, [bridgeBinding, mode, previewScale, targetKind]);

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

  const stopPreview = () => {
    setPreviewPaused(true);
    setDiagnostic(null);
    onStructure([]);
    onClear();
  };

  const restartPreview = () => {
    setDiagnostic(null);
    setPreviewGeneration((generation) => generation + 1);
    setPreviewPaused(false);
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
        <div
          className="segmented preview-scale-switcher"
          role="group"
          aria-label="プレビュー表示倍率"
        >
          <button
            type="button"
            data-testid="preview-scale-75"
            disabled={disabled}
            aria-pressed={previewScale === 0.75}
            onClick={() => onPreviewScale(0.75)}
          >
            75%
          </button>
          <button
            type="button"
            data-testid="preview-scale-100"
            disabled={disabled}
            aria-pressed={previewScale === 1}
            onClick={() => onPreviewScale(1)}
          >
            100%
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
        <button
          type="button"
          className="text-button preview-lifecycle-control"
          onClick={previewPaused ? restartPreview : stopPreview}
        >
          {previewPaused ? "プレビューを再読み込み" : "プレビューを停止"}
        </button>
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

      {previewPaused ? (
        <div className="canvas-shell preview-paused" role="status">
          <div className="error-card">
            プレビューの実行を停止しました。EditorとAccepted
            LPは引き続き利用できます。
          </div>
        </div>
      ) : (
        <div className="canvas-shell">
          <div
            className="viewport-frame"
            style={{
              width,
              transform: `scale(${previewScale})`,
              transformOrigin: "top center",
            }}
            data-viewport={viewport}
            data-preview-scale={previewScale}
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
          <a
            className="preview-focus-exit"
            href="#ai-instruction"
            onClick={(event) => {
              event.preventDefault();
              const instruction = document.getElementById("ai-instruction");
              instruction?.focus({ preventScroll: true });
              instruction?.scrollIntoView({ block: "center" });
            }}
          >
            プレビューを抜けてEditorへ移動
          </a>
        </div>
      )}
    </main>
  );
}

interface ContextDialogProps {
  context: ContextReview;
  busy: boolean;
  generating: boolean;
  elapsedSeconds: number;
  cancelPending: boolean;
  returnFocus: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onConfirm: () => void;
  onCancel: () => void;
}

function ContextDialog({
  context,
  busy,
  generating,
  elapsedSeconds,
  cancelPending,
  returnFocus,
  onClose,
  onConfirm,
  onCancel,
}: ContextDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const redactedSiteContent = context.manifest.entries.some(
    (entry) => entry.redacted,
  );

  useModalDialog({
    busy,
    dialogRef,
    initialFocusRef: closeRef,
    returnFocusRef: returnFocus,
    onClose,
  });

  useEffect(() => {
    if (generating) cancelRef.current?.focus({ preventScroll: true });
  }, [generating]);

  return (
    <div className="dialog-backdrop" data-modal-root>
      <section
        ref={dialogRef}
        className="dialog-card"
        role="dialog"
        tabIndex={-1}
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
        {generating ? (
          <div
            className="ai-attempt-progress"
            role="status"
            aria-live="polite"
            aria-label="AI処理状態"
          >
            <span className="spinner" aria-hidden="true" />
            <div>
              <strong>AI処理フェーズ</strong>
              <span>Provider応答待機・ChangeSet検証</span>
              <span>開始から {elapsedSeconds} 秒</span>
            </div>
            <button
              ref={cancelRef}
              type="button"
              className="button button-quiet"
              disabled={cancelPending}
              onClick={onCancel}
            >
              {cancelPending ? "AI処理を取り消しています" : "AI処理を取り消す"}
            </button>
          </div>
        ) : null}
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
        {provider === undefined ? null : (
          <div className="provider-policy" aria-label="Provider data policy">
            <p>
              Retention: <code>{provider.dataRetentionPolicy}</code> · Training:{" "}
              <code>{provider.trainingPolicy}</code>
            </p>
            <p>{provider.policyNotice}</p>
          </div>
        )}
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
  terminalFailure: boolean;
  rationale: string;
  onRationale: (value: string) => void;
  onDecision: (disposition: ArtifactDisposition) => void;
  onReconcile: () => void;
  onSource: (source: PreviewSource) => void;
}

function ReviewDrawer({
  proposal,
  target,
  prompt,
  busy,
  decisionReconciliationRequired,
  terminalFailure,
  rationale,
  onRationale,
  onDecision,
  onReconcile,
  onSource,
}: ReviewDrawerProps) {
  const hardError = proposal.validation.status === "failed";
  const decisionLocked = decisionReconciliationRequired || terminalFailure;
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
          <p className="eyebrow">
            {terminalFailure
              ? "TERMINAL REVIEW FAILURE"
              : "HUMAN REVIEW REQUIRED"}
          </p>
          <h2 id="review-title">
            {terminalFailure ? "変更案の処理に失敗" : "変更案を確認"}
          </h2>
        </div>
        <span
          className={`proposal-status${terminalFailure ? " proposal-status-failed" : ""}`}
        >
          {terminalFailure ? "failed permanently" : "pending review"}
        </span>
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
        <div className="decision-introduction">
          <strong>Proposal全体へのHuman Decision</strong>
          <p>
            Adoptは{proposal.changes.length}
            ファイルをProposalのまま採用します。Reject／DeferではAcceptedは変わりません。
            DeferもこのProposalに対する終端Decisionです。
          </p>
        </div>
        <label className="rationale-field">
          <span>非公開メモ（任意）</span>
          <input
            type="text"
            maxLength={2000}
            value={rationale}
            disabled={busy || decisionLocked}
            onChange={(event) => onRationale(event.currentTarget.value)}
            aria-describedby="rationale-limit"
          />
          <small id="rationale-limit">
            {rationaleBytes} / 2000 UTF-8 bytes
          </small>
        </label>
        <div className="decision-options" aria-label="Human Decision">
          <button
            type="button"
            className="button button-adopt"
            disabled={
              busy ||
              decisionLocked ||
              hardError ||
              blockingWarning ||
              rationaleBytes > 2000
            }
            onClick={() => onDecision("adopted_unchanged")}
          >
            変更を採用
          </button>
          <button
            type="button"
            className="button button-secondary"
            disabled={busy || decisionLocked || rationaleBytes > 2000}
            onClick={() => onDecision("rejected")}
          >
            変更案を却下
          </button>
          <button
            type="button"
            className="button button-quiet"
            disabled={busy || decisionLocked || rationaleBytes > 2000}
            onClick={() => onDecision("deferred")}
          >
            今回は保留
          </button>
        </div>
        {terminalFailure ? (
          <div className="blocking-decision-note" role="alert">
            <p>
              このReviewは永続的な失敗として確定しています。Decision結果の再照合や新しいDecisionは実行できません。Accepted
              revisionは変更されていません。
            </p>
            <button type="button" className="button button-secondary" disabled>
              Decision結果を再照合
            </button>
          </div>
        ) : decisionReconciliationRequired ? (
          <div className="blocking-decision-note" role="status">
            <p>
              Decision結果を再照合するまで全Decision操作はロックされています。Decisionを再実行せず、durable
              receiptとAccepted状態を確認してください。
            </p>
            <button
              type="button"
              className="button button-secondary"
              disabled={busy}
              onClick={onReconcile}
            >
              Decision結果を再照合
            </button>
          </div>
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
  const details =
    receipt.receiptSha256 === undefined ||
    receipt.sourceManifestSha256 === undefined ||
    receipt.generatedAtUtc === undefined ||
    receipt.options === undefined ||
    receipt.fileManifest === undefined ||
    receipt.validation === undefined ||
    receipt.archive === undefined
      ? null
      : {
          receiptSha256: receipt.receiptSha256,
          sourceManifestSha256: receipt.sourceManifestSha256,
          generatedAtUtc: receipt.generatedAtUtc,
          options: receipt.options,
          fileManifest: receipt.fileManifest,
          validation: receipt.validation,
          archive: receipt.archive,
        };
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
        {details !== null ? (
          <>
            <div>
              <dt>Hosting profile</dt>
              <dd>
                {details.validation.profile === "offline_self_contained"
                  ? "offline self-contained（bounded検査）"
                  : "standalone static（external依存あり）"}
              </dd>
            </div>
            <div>
              <dt>Entry point / base path</dt>
              <dd>
                <code>{details.options.entryPoint}</code> ·{" "}
                <code>{details.options.basePathProfile}</code>
              </dd>
            </div>
            <div>
              <dt>Source manifest SHA-256</dt>
              <dd>
                <code>{details.sourceManifestSha256}</code>
              </dd>
            </div>
            <div>
              <dt>Receipt SHA-256</dt>
              <dd>
                <code>{details.receiptSha256}</code>
              </dd>
            </div>
            <div>
              <dt>Generated</dt>
              <dd>
                <time dateTime={details.generatedAtUtc}>
                  {details.generatedAtUtc}
                </time>
              </dd>
            </div>
          </>
        ) : null}
      </dl>
      {details !== null ? (
        <div className="export-detail-grid">
          <section aria-labelledby="export-manifest-title">
            <h3 id="export-manifest-title">
              Generated manifest ({details.fileManifest.files.length})
            </h3>
            <ul className="export-file-list">
              {details.fileManifest.files.map((file) => (
                <li key={file.path + ":" + file.sha256}>
                  <code>{file.path}</code>
                  <span>
                    {file.byteLength.toLocaleString("ja-JP")} bytes ·{" "}
                    {file.mediaType}
                  </span>
                </li>
              ))}
            </ul>
          </section>
          <section aria-labelledby="export-validation-title">
            <h3 id="export-validation-title">Validation boundary</h3>
            <p>
              Static local references:{" "}
              {details.validation.localReferenceCount.toLocaleString("ja-JP")}
            </p>
            {details.validation.externalOrigins.length > 0 ? (
              <>
                <h4>External origins</h4>
                <ul>
                  {details.validation.externalOrigins.map((origin) => (
                    <li key={origin}>
                      <code>{origin}</code>
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            {details.validation.warnings.length > 0 ? (
              <>
                <h4>Warnings</h4>
                <ul>
                  {details.validation.warnings.map((warning) => (
                    <li key={warning.code}>
                      <strong>{warning.code}</strong>: {warning.message}
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            <h4>Known limitations</h4>
            <ul>
              {details.validation.limitations.map((limitation) => (
                <li key={limitation.code}>
                  <strong>{limitation.code}</strong>: {limitation.message}
                </li>
              ))}
            </ul>
          </section>
        </div>
      ) : (
        <p className="receipt-legacy-note">
          このserverはarchive checksumのみを返しました。詳細validation
          receiptはC8対応serverで表示されます。
        </p>
      )}
    </section>
  );
}

function ReviewRecoveryPanel({
  project,
  busy,
  onRestore,
  onReconcile,
}: {
  project: Project;
  busy: boolean;
  onRestore: () => void;
  onReconcile: () => void;
}) {
  const activeReview = project.activeReview;
  if (activeReview === undefined || activeReview === null) return null;
  const reconciliationRequired =
    activeReview.status === "reconciliation_required";
  const terminalFailure = activeReview.status === "failed";
  return (
    <section className="review-recovery panel" aria-labelledby="recovery-title">
      <div>
        <p className="eyebrow">RESTART-SAFE REVIEW</p>
        <h2 id="recovery-title">
          {terminalFailure
            ? "このReviewは永続的に失敗しています"
            : reconciliationRequired
              ? "Decision結果の再照合が必要です"
              : "保存済みReviewを再開できます"}
        </h2>
        <p>
          Review <code>{shortIdentity(activeReview.reviewId)}</code> · Proposal{" "}
          <code>{shortIdentity(activeReview.proposalId)}</code>
        </p>
      </div>
      <button
        type="button"
        className="button button-secondary"
        disabled={busy}
        onClick={reconciliationRequired ? onReconcile : onRestore}
      >
        {terminalFailure
          ? "失敗したReviewの詳細を読み込む"
          : reconciliationRequired
            ? "Decision結果を再照合"
            : "保存済みReviewを読み込む"}
      </button>
    </section>
  );
}

function ProjectHistoryPanel({ project }: { project: Project }) {
  if (project.activeReview === undefined && project.history === undefined) {
    return null;
  }
  const history = project.history ?? [];
  return (
    <section className="project-history panel" aria-labelledby="history-title">
      <div className="panel-heading">
        <p className="eyebrow">LOCAL DECISION RECORDS</p>
        <h2 id="history-title">Review history</h2>
      </div>
      {project.activeReview !== null && project.activeReview !== undefined ? (
        <div className="active-review-summary">
          <strong>Active Review</strong>
          <span>
            {project.activeReview.status === "failed"
              ? "永続的失敗（再照合・Decision不可）"
              : project.activeReview.status === "reconciliation_required"
                ? "Decision結果の再照合が必要"
                : "Human Decision待ち"}
          </span>
          <code>{shortIdentity(project.activeReview.reviewId)}</code>
        </div>
      ) : null}
      {history.length === 0 ? (
        <p>終端Decisionはまだありません。</p>
      ) : (
        <ol className="history-list">
          {history.map((entry) => (
            <li key={entry.reviewId + ":" + entry.proposalId}>
              <div>
                <strong>{dispositionLabel[entry.disposition]}</strong>
                <time dateTime={entry.recordedAt}>{entry.recordedAt}</time>
              </div>
              <dl>
                <div>
                  <dt>Proposal</dt>
                  <dd>
                    <code>{shortIdentity(entry.proposalId)}</code>
                  </dd>
                </div>
                <div>
                  <dt>Result revision</dt>
                  <dd>
                    <code>{shortIdentity(entry.resultingRevisionId)}</code>
                  </dd>
                </div>
                <div>
                  <dt>Decision receipt</dt>
                  <dd>
                    <code>{entry.decisionReceiptSha256}</code>
                  </dd>
                </div>
                {entry.resultingAcceptedManifestSha256 !== undefined ? (
                  <div>
                    <dt>Result Accepted manifest</dt>
                    <dd>
                      <code>{entry.resultingAcceptedManifestSha256}</code>
                    </dd>
                  </div>
                ) : null}
              </dl>
              {entry.publicNote ? <p>Public note: {entry.publicNote}</p> : null}
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

interface RetentionPanelProps {
  inventory: RetentionInventory | null;
  projectId?: string;
  busy: boolean;
  error: string | null;
  onCleanup: (request: RetentionCleanupRequest) => void;
}

function RetentionPanel({
  inventory,
  projectId,
  busy,
  error,
  onCleanup,
}: RetentionPanelProps) {
  const [confirmations, setConfirmations] = useState<Record<string, string>>(
    {},
  );
  const projects =
    inventory?.projects.filter(
      (project) => projectId === undefined || project.projectId === projectId,
    ) ?? [];
  const confirmation = (key: string): string => confirmations[key] ?? "";
  const updateConfirmation = (key: string, value: string) => {
    setConfirmations((current) => ({ ...current, [key]: value }));
  };

  return (
    <section
      className="retention-panel panel"
      aria-labelledby="retention-title"
    >
      <div className="panel-heading">
        <p className="eyebrow">SETTINGS · LOCAL RETENTION</p>
        <h2 id="retention-title">保持データと手動削除</h2>
      </div>
      <p>
        自動GC: <strong>無効</strong> · Telemetry: <strong>なし</strong>
      </p>
      <p>
        削除は対象IDの再入力が必要です。表示bytesは対象payloadの大きさであり、共有CAS等を含む実際の空き容量増加を保証しません。
      </p>
      {inventory === null ? (
        <p role="status">保持データを確認しています…</p>
      ) : projects.length === 0 ? (
        <p>対象となる保持データはありません。</p>
      ) : (
        <div className="retention-projects">
          {projects.map((project) => {
            const contextKey = `context:${project.projectId}`;
            const projectKey = `project:${project.projectId}`;
            const failedKey = `proposal:${project.failedProposal?.proposalId ?? "none"}`;
            return (
              <details key={project.projectId} open={projectId !== undefined}>
                <summary>
                  <strong>{project.displayName}</strong> ·{" "}
                  {project.artifacts.length} artifacts
                </summary>
                <dl className="retention-facts">
                  <div>
                    <dt>Project ID</dt>
                    <dd>
                      <code>{project.projectId}</code>
                    </dd>
                  </div>
                  <div>
                    <dt>Accepted payload</dt>
                    <dd>
                      {project.acceptedFileByteLength.toLocaleString("ja-JP")}{" "}
                      bytes
                    </dd>
                  </div>
                  <div>
                    <dt>Targets / terminal Decisions</dt>
                    <dd>
                      {project.targetCount} / {project.terminalDecisionCount}
                    </dd>
                  </div>
                </dl>

                <div className="retention-item">
                  <div>
                    <strong>会話・送信context</strong>
                    <p>
                      {project.conversationContextCount}件 · memory
                      only（再起動時には保持しません）
                    </p>
                  </div>
                  <label>
                    <span>確認: Project ID</span>
                    <input
                      value={confirmation(contextKey)}
                      disabled={busy || project.conversationContextCount === 0}
                      onChange={(event) =>
                        updateConfirmation(
                          contextKey,
                          event.currentTarget.value,
                        )
                      }
                    />
                  </label>
                  <button
                    type="button"
                    className="button button-quiet"
                    disabled={
                      busy ||
                      project.conversationContextCount === 0 ||
                      confirmation(contextKey) !== project.projectId
                    }
                    onClick={() =>
                      onCleanup({
                        scope: "conversation_context",
                        projectId: project.projectId,
                        confirmation: confirmation(contextKey),
                      })
                    }
                  >
                    Contextを消去
                  </button>
                </div>

                {project.failedProposal === null ? null : (
                  <div className="retention-item retention-warning">
                    <div>
                      <strong>Failed Proposal</strong>
                      <p>{project.failedProposal.cleanupImpact}</p>
                      <code>{project.failedProposal.proposalId}</code>
                    </div>
                    <label>
                      <span>確認: Proposal ID</span>
                      <input
                        value={confirmation(failedKey)}
                        disabled={busy}
                        onChange={(event) =>
                          updateConfirmation(
                            failedKey,
                            event.currentTarget.value,
                          )
                        }
                      />
                    </label>
                    <button
                      type="button"
                      className="button button-quiet"
                      disabled={
                        busy ||
                        confirmation(failedKey) !==
                          project.failedProposal.proposalId
                      }
                      onClick={() =>
                        onCleanup({
                          scope: "failed_proposal",
                          projectId: project.projectId,
                          proposalId: project.failedProposal!.proposalId,
                          reviewId: project.failedProposal!.reviewId,
                          confirmation: confirmation(failedKey),
                        })
                      }
                    >
                      Failed Proposalを消去
                    </button>
                  </div>
                )}

                {project.artifacts.map((artifact) => {
                  const artifactKey = `${artifact.kind}:${artifact.id}`;
                  return (
                    <div className="retention-item" key={artifactKey}>
                      <div>
                        <strong>
                          {artifact.kind === "static_export"
                            ? "Static export"
                            : "Publication draft"}
                        </strong>
                        <p>{artifact.cleanupImpact}</p>
                        <code>{artifact.id}</code>
                        <span>
                          {artifact.payloadByteLength.toLocaleString("ja-JP")}{" "}
                          bytes
                        </span>
                      </div>
                      <label>
                        <span>確認: Artifact ID</span>
                        <input
                          value={confirmation(artifactKey)}
                          disabled={busy}
                          onChange={(event) =>
                            updateConfirmation(
                              artifactKey,
                              event.currentTarget.value,
                            )
                          }
                        />
                      </label>
                      <button
                        type="button"
                        className="button button-quiet"
                        disabled={
                          busy || confirmation(artifactKey) !== artifact.id
                        }
                        onClick={() =>
                          onCleanup({
                            scope: artifact.kind,
                            projectId: project.projectId,
                            artifactId: artifact.id,
                            expectedSha256: artifact.sha256,
                            confirmation: confirmation(artifactKey),
                          })
                        }
                      >
                        この記録を消去
                      </button>
                    </div>
                  );
                })}

                <div className="retention-item retention-danger">
                  <div>
                    <strong>プロジェクト全体</strong>
                    <p>{project.projectDeletionImpact}</p>
                  </div>
                  <label>
                    <span>確認: Project ID</span>
                    <input
                      value={confirmation(projectKey)}
                      disabled={busy}
                      onChange={(event) =>
                        updateConfirmation(
                          projectKey,
                          event.currentTarget.value,
                        )
                      }
                    />
                  </label>
                  <button
                    type="button"
                    className="button button-quiet"
                    disabled={
                      busy || confirmation(projectKey) !== project.projectId
                    }
                    onClick={() =>
                      onCleanup({
                        scope: "project",
                        projectId: project.projectId,
                        expectedRevisionId: project.revisionId,
                        expectedManifestSha256: project.acceptedManifestSha256,
                        confirmation: confirmation(projectKey),
                      })
                    }
                  >
                    プロジェクトを完全削除
                  </button>
                </div>
              </details>
            );
          })}
        </div>
      )}
      {error === null ? null : (
        <p className="error-card" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

interface PublicationSettings {
  publicLabel: string;
  title: string;
  summary: string;
  publicDecisionNote?: string;
}

interface PublicationReviewDialogProps {
  project: Project;
  draft: PublicationDraft | null;
  busy: boolean;
  returnFocus: React.RefObject<HTMLButtonElement | null>;
  onClose: () => void;
  onGenerate: (settings: PublicationSettings) => void;
  onDownload: () => void;
}

function PublicationReviewDialog({
  project,
  draft,
  busy,
  returnFocus,
  onClose,
  onGenerate,
  onDownload,
}: PublicationReviewDialogProps) {
  const closeRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLElement>(null);
  const [publicLabel, setPublicLabel] = useState(project.displayName);
  const [title, setTitle] = useState(project.displayName);
  const [summary, setSummary] = useState("");
  const [publicDecisionNote, setPublicDecisionNote] = useState("");
  const [selectedPath, setSelectedPath] = useState<string | null>(
    draft?.files[0]?.path ?? null,
  );
  const hasDecision = (project.history?.length ?? 0) > 0;
  const valuesValid =
    utf8Bytes(publicLabel.trim()) > 0 &&
    utf8Bytes(publicLabel.trim()) <= 256 &&
    utf8Bytes(title.trim()) > 0 &&
    utf8Bytes(title.trim()) <= 256 &&
    utf8Bytes(summary.trim()) > 0 &&
    utf8Bytes(summary.trim()) <= 4_096 &&
    utf8Bytes(publicDecisionNote.trim()) <= 2_000;
  const selectedFile =
    draft?.files.find((file) => file.path === selectedPath) ??
    draft?.files[0] ??
    null;

  useModalDialog({
    busy,
    dialogRef,
    initialFocusRef: closeRef,
    returnFocusRef: returnFocus,
    onClose,
  });

  useEffect(() => {
    setSelectedPath(draft?.files[0]?.path ?? null);
  }, [draft?.id, draft?.files]);

  const generate = () => {
    if (!valuesValid) return;
    const note = publicDecisionNote.trim();
    onGenerate({
      publicLabel: publicLabel.trim(),
      title: title.trim(),
      summary: summary.trim(),
      ...(hasDecision && note.length > 0 ? { publicDecisionNote: note } : {}),
    });
  };

  return (
    <div className="dialog-backdrop" data-modal-root>
      <section
        ref={dialogRef}
        className="dialog-card publication-dialog"
        role="dialog"
        tabIndex={-1}
        aria-modal="true"
        aria-labelledby="publication-dialog-title"
        aria-describedby="publication-dialog-description"
      >
        <div className="dialog-heading">
          <div>
            <p className="eyebrow">LOCAL · EXACT-BYTE REVIEW</p>
            <h2 id="publication-dialog-title">GitHub-ready記録</h2>
          </div>
          <button
            ref={closeRef}
            type="button"
            className="icon-button"
            aria-label="publication reviewを閉じる"
            disabled={busy}
            onClick={onClose}
          >
            ×
          </button>
        </div>
        <p id="publication-dialog-description">
          public
          fieldだけからprivacy-filteredな派生記録をローカル生成します。生成・ダウンロードはGit
          commit、push、Issue、PR、releaseを行いません。
        </p>

        <fieldset className="publication-settings">
          <legend>公開用settings（author supplied）</legend>
          <label>
            <span>Public label</span>
            <input
              aria-label="Public label"
              value={publicLabel}
              disabled={busy}
              onChange={(event) => setPublicLabel(event.currentTarget.value)}
            />
            <small>{utf8Bytes(publicLabel)} / 256 UTF-8 bytes</small>
          </label>
          <label>
            <span>Public title</span>
            <input
              aria-label="Public title"
              value={title}
              disabled={busy}
              onChange={(event) => setTitle(event.currentTarget.value)}
            />
            <small>{utf8Bytes(title)} / 256 UTF-8 bytes</small>
          </label>
          <label>
            <span>Public summary</span>
            <textarea
              aria-label="Public summary"
              value={summary}
              disabled={busy}
              onChange={(event) => setSummary(event.currentTarget.value)}
            />
            <small>{utf8Bytes(summary)} / 4096 UTF-8 bytes</small>
          </label>
          {hasDecision ? (
            <label>
              <span>Public decision note（任意・private rationaleとは別）</span>
              <textarea
                aria-label="Public decision note（任意・private rationaleとは別）"
                value={publicDecisionNote}
                disabled={busy}
                onChange={(event) =>
                  setPublicDecisionNote(event.currentTarget.value)
                }
              />
              <small>{utf8Bytes(publicDecisionNote)} / 2000 UTF-8 bytes</small>
            </label>
          ) : (
            <p>
              Decision未完了のためpublic decision noteは含めません。incomplete
              historyとして明示されます。
            </p>
          )}
        </fieldset>

        <button
          type="button"
          className="button button-primary"
          disabled={busy || !valuesValid}
          onClick={generate}
        >
          GitHub-ready filesを生成
        </button>

        {draft !== null ? (
          <section
            className="publication-exact-review"
            aria-labelledby="publication-files-title"
          >
            <div className="publication-receipt">
              <h3 id="publication-files-title">
                生成bytesを確認（{draft.files.length} files）
              </h3>
              <p>
                Revision <code>{shortIdentity(draft.revisionId)}</code> ·{" "}
                {draft.byteLength.toLocaleString("ja-JP")} bytes
              </p>
              <p>
                Bundle SHA-256 <code>{draft.sha256}</code>
              </p>
              <strong>Network writes: none</strong>
            </div>
            <div className="publication-file-review">
              <nav aria-label="Publication files">
                <ul>
                  {draft.files.map((file) => (
                    <li key={file.path + ":" + file.sha256}>
                      <button
                        type="button"
                        aria-pressed={selectedFile?.path === file.path}
                        onClick={() => setSelectedPath(file.path)}
                      >
                        <code>{file.path}</code>
                        <span>{file.byteLength} bytes</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </nav>
              {selectedFile !== null ? (
                <article aria-label={selectedFile.path + " exact bytes"}>
                  <h4>
                    <code>{selectedFile.path}</code>
                  </h4>
                  <p>
                    {selectedFile.mediaType} · SHA-256{" "}
                    <code>{selectedFile.sha256}</code>
                  </p>
                  <pre>{selectedFile.utf8}</pre>
                </article>
              ) : null}
            </div>
            <div className="dialog-actions">
              <button
                type="button"
                className="button button-secondary"
                disabled={busy}
                onClick={onDownload}
              >
                確認したZIPをダウンロード
              </button>
              <span>GitHubへ公開: 別Human操作（この画面では実行しません）</span>
            </div>
          </section>
        ) : null}
      </section>
    </div>
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
  const dialogRef = useRef<HTMLElement>(null);

  useModalDialog({
    busy,
    dialogRef,
    initialFocusRef: closeRef,
    returnFocusRef: returnFocus,
    onClose,
  });

  return (
    <div className="dialog-backdrop" data-modal-root>
      <section
        ref={dialogRef}
        className="dialog-card import-review-dialog"
        role="dialog"
        tabIndex={-1}
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
  const [retention, setRetention] = useState<RetentionInventory | null>(null);
  const [retentionBusy, setRetentionBusy] = useState(false);
  const [retentionError, setRetentionError] = useState<string | null>(null);
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
  const [publicationOpen, setPublicationOpen] = useState(false);
  const [targetKind, setTargetKind] = useState<TargetKind>("element");
  const [structure, setStructure] = useState<PreviewStructureNode[]>([]);
  const [captureCommand, setCaptureCommand] = useState<CaptureCommand | null>(
    null,
  );
  const [proposalElapsedSeconds, setProposalElapsedSeconds] = useState(0);
  const [proposalCancelPending, setProposalCancelPending] = useState(false);
  const captureCommandId = useRef(0);
  const targetRequestId = useRef(0);
  const proposalRunSequence = useRef(0);
  const proposalStartedAt = useRef<number | null>(null);
  const proposalCancelPendingRef = useRef(false);
  const reviewButtonRef = useRef<HTMLButtonElement>(null);
  const importButtonRef = useRef<HTMLButtonElement>(null);
  const exportButtonRef = useRef<HTMLButtonElement>(null);
  const publicationButtonRef = useRef<HTMLButtonElement>(null);
  const busy =
    state.operation !== null || homeOperation !== null || retentionBusy;

  useEffect(() => {
    if (state.operation !== "generating_proposal") return;
    proposalStartedAt.current ??= Date.now();
    const updateElapsed = () => {
      const startedAt = proposalStartedAt.current;
      if (startedAt !== null) {
        setProposalElapsedSeconds(
          Math.max(0, Math.floor((Date.now() - startedAt) / 1_000)),
        );
      }
    };
    updateElapsed();
    const interval = window.setInterval(updateElapsed, 1_000);
    return () => window.clearInterval(interval);
  }, [state.operation]);
  const saveProjectDisplayName = useCallback(
    (projectId: string, displayName: string) =>
      dispatch({
        type: "PROJECT_DISPLAY_NAME_SAVED",
        projectId,
        displayName,
      }),
    [],
  );

  const run = useCallback(async (task: () => Promise<void>) => {
    try {
      await task();
    } catch (error) {
      const diagnostic = errorDiagnostic(error);
      dispatch({
        type: "FAILED",
        message: errorMessage(error),
        ...diagnostic,
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

  const refreshRetention = useCallback(async () => {
    try {
      const next = await session.api.getRetention();
      setRetention(next);
      setRetentionError(null);
      return next;
    } catch (error) {
      void error;
      return null;
    }
  }, [session.api]);

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
    void session.api
      .getRetention()
      .then((retentionInventory) => {
        if (active) setRetention(retentionInventory);
      })
      .catch((error: unknown) => {
        void error;
      });
    return () => {
      active = false;
    };
  }, [session.api]);

  const cleanupRetention = useCallback(
    (request: RetentionCleanupRequest) => {
      if (retentionBusy) return;
      void (async () => {
        setRetentionBusy(true);
        setRetentionError(null);
        try {
          const response = await session.api.cleanupRetention(request);
          setRetention(response.retention);
          if (response.removed.scope === "project") {
            setRetainedProjects(
              (projects) =>
                projects?.filter(
                  (project) => project.id !== response.removed.id,
                ) ?? null,
            );
            if (state.project?.id === response.removed.id) {
              window.location.reload();
            }
          } else if (
            response.removed.scope === "failed_proposal" &&
            state.project?.id === request.projectId
          ) {
            const refreshed = await session.api.getProject(request.projectId);
            dispatch({
              type: "PROJECT_LOADED",
              project: refreshed,
              origin: "opened",
            });
          }
        } catch (error) {
          setRetentionError(errorMessage(error));
        } finally {
          setRetentionBusy(false);
        }
      })();
    },
    [retentionBusy, session.api, state.project?.id],
  );

  const restoreProjectReview = useCallback(
    (project: Project) => {
      const activeReview = project.activeReview;
      if (activeReview === undefined || activeReview === null) return;
      void run(async () => {
        dispatch({ type: "OPERATION_STARTED", operation: "restoring_review" });
        const review = await session.api.getReview(activeReview.reviewId);
        assertReviewBinding(project, activeReview, review);
        dispatch({ type: "REVIEW_RESTORED", review });
        if (review.proposal === null) {
          const refreshed = await session.api.getProject(project.id);
          if (
            review.decision !== null &&
            refreshed.revisionId !== review.decision.revisionId
          ) {
            throw new ApiError(
              "保存済みDecisionとAccepted revisionが一致しません。",
              { code: "review_restore_binding_mismatch", retryable: false },
            );
          }
          dispatch({
            type: "PROJECT_REFRESHED",
            project: refreshed,
            afterDecision: true,
          });
        }
      });
    },
    [run, session.api],
  );

  const loadProject = useCallback(
    (project: Project, origin: "created" | "opened" | "imported") => {
      dispatch({ type: "PROJECT_LOADED", project, origin });
      restoreProjectReview(project);
    },
    [restoreProjectReview],
  );

  const createProject = () => {
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "creating_project" });
      const project = await session.api.createBlankProject();
      await refreshRetention();
      loadProject(project, "created");
    });
  };

  const openProject = (projectId: string) => {
    void runHome("opening_project", async () => {
      const project = await session.api.getProject(projectId);
      loadProject(project, "opened");
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
      await refreshRetention();
      setImportPreview(null);
      loadProject(project, "imported");
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
          code: "preview_capture_binding_mismatch",
          requestId: null,
          retryable: false,
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
            dispatch({
              type: "FAILED",
              message: errorMessage(error),
              ...errorDiagnostic(error),
            });
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
      if (state.contextReview === null && state.target !== null) {
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
    const runSequence = ++proposalRunSequence.current;
    proposalStartedAt.current = Date.now();
    proposalCancelPendingRef.current = false;
    setProposalElapsedSeconds(0);
    setProposalCancelPending(false);
    void (async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "generating_proposal" });
      try {
        const proposal = await session.api.createProposal(
          project.id,
          contextReview.id,
          contextReview.sha256,
        );
        if (runSequence !== proposalRunSequence.current) return;
        if (
          proposal.baseRevisionId !== project.revisionId ||
          proposal.changeSet.baseRevisionId !== project.revisionId ||
          proposal.providerContextSha256 !== contextReview.sha256 ||
          proposal.attribution.attemptId !== contextReview.attemptId ||
          proposal.attribution.providerId !==
            contextReview.provider.providerId ||
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
      } catch (error) {
        if (runSequence !== proposalRunSequence.current) return;
        proposalRunSequence.current += 1;
        proposalStartedAt.current = null;
        proposalCancelPendingRef.current = false;
        setProposalCancelPending(false);
        if (
          error instanceof ApiError &&
          error.code === "ai_attempt_cancelled"
        ) {
          dispatch({
            type: "PROPOSAL_ATTEMPT_CLEARED",
            announcement:
              "AI処理を取り消しました。送信内容を確認し直すと、新しいattemptで再試行できます。",
          });
        } else {
          dispatch({
            type: "FAILED",
            message: errorMessage(error),
            ...errorDiagnostic(error),
            clearContextReview: true,
          });
        }
      } finally {
        if (runSequence === proposalRunSequence.current) {
          proposalStartedAt.current = null;
          setProposalCancelPending(false);
        }
      }
    })();
  };

  const cancelProposalAttempt = () => {
    if (
      proposalCancelPending ||
      proposalCancelPendingRef.current ||
      state.operation !== "generating_proposal" ||
      state.project === null ||
      state.contextReview === null
    ) {
      return;
    }
    const project = state.project;
    const attemptId = state.contextReview.attemptId;
    const activeRun = proposalRunSequence.current;
    proposalCancelPendingRef.current = true;
    setProposalCancelPending(true);

    const clearAttempt = (announcement: string) => {
      if (activeRun !== proposalRunSequence.current) return;
      proposalRunSequence.current += 1;
      proposalStartedAt.current = null;
      proposalCancelPendingRef.current = false;
      dispatch({ type: "PROPOSAL_ATTEMPT_CLEARED", announcement });
    };

    const handleObservedStatus = async (
      status: Awaited<ReturnType<AuthenticatedApi["getAiAttemptStatus"]>>,
      refreshed: Project,
    ) => {
      if (status.attemptId !== attemptId || refreshed.id !== project.id) {
        throw new ApiError("AI処理状態のbindingが一致しません。", {
          code: "ai_attempt_status_binding_mismatch",
          retryable: false,
        });
      }
      if (status.status === "cancelled") {
        clearAttempt(
          "AI処理を取り消しました。送信内容を確認し直すと、新しいattemptで再試行できます。",
        );
        return;
      }
      if (
        status.status === "timed_out" ||
        status.status === "provider_failed" ||
        status.status === "validation_failed"
      ) {
        if (activeRun !== proposalRunSequence.current) return;
        proposalRunSequence.current += 1;
        proposalStartedAt.current = null;
        proposalCancelPendingRef.current = false;
        const acceptedUnchanged = refreshed.revisionId === project.revisionId;
        dispatch({
          type: "FAILED",
          message: `AI処理は ${status.status} で終了しました。${
            acceptedUnchanged
              ? "Accepted状態は変更されていません。"
              : "Accepted revisionが変わったため再照合が必要です。"
          }送信内容を確認し直すと、新しいattemptで再試行できます。`,
          code: `ai_attempt_${status.status}`,
          requestId: null,
          operationId: attemptId,
          retryable: true,
          acceptedState: acceptedUnchanged
            ? "unchanged"
            : "reconciliation_required",
          recoveryAction: acceptedUnchanged ? "retry" : "refresh",
          clearContextReview: true,
        });
        return;
      }
      if (status.status === "proposal_ready" || status.status === "completed") {
        if (activeRun !== proposalRunSequence.current) return;
        proposalRunSequence.current += 1;
        proposalStartedAt.current = null;
        proposalCancelPendingRef.current = false;
        loadProject(refreshed, "opened");
        if (
          refreshed.activeReview === null ||
          refreshed.activeReview === undefined
        ) {
          dispatch({
            type: "FAILED",
            message:
              "AI処理は完了していますがReviewを復元できません。Accepted状態を再取得しました。",
            code: "ai_attempt_completion_not_recoverable",
            requestId: null,
            operationId: attemptId,
            retryable: false,
            acceptedState: "unchanged",
            recoveryAction: "refresh",
          });
        }
        return;
      }
      dispatch({
        type: "PROPOSAL_ATTEMPT_STATUS",
        announcement: `取消と完了が競合したため状態を再取得しました。AI処理は現在 ${status.status} です。`,
      });
    };

    void (async () => {
      try {
        const status = await session.api.cancelAiAttempt(project.id, attemptId);
        if (status.attemptId !== attemptId) {
          throw new ApiError("AI処理取消のbindingが一致しません。", {
            code: "ai_attempt_cancel_binding_mismatch",
            retryable: false,
          });
        }
        if (status.status === "cancelled") {
          clearAttempt(
            "AI処理を取り消しました。送信内容を確認し直すと、新しいattemptで再試行できます。",
          );
          return;
        }
        const refreshed = await session.api.getProject(project.id);
        await handleObservedStatus(status, refreshed);
      } catch (cancelError) {
        try {
          const [status, refreshed] = await Promise.all([
            session.api.getAiAttemptStatus(project.id, attemptId),
            session.api.getProject(project.id),
          ]);
          await handleObservedStatus(status, refreshed);
        } catch (observationError) {
          if (activeRun !== proposalRunSequence.current) return;
          proposalRunSequence.current += 1;
          proposalStartedAt.current = null;
          proposalCancelPendingRef.current = false;
          const reportedError =
            observationError instanceof ApiError
              ? observationError
              : cancelError;
          dispatch({
            type: "FAILED",
            message: errorMessage(reportedError),
            ...errorDiagnostic(reportedError),
            clearContextReview: true,
          });
        }
      } finally {
        proposalCancelPendingRef.current = false;
        setProposalCancelPending(false);
      }
    })();
  };

  const decideProposal = (disposition: ArtifactDisposition) => {
    if (
      state.project === null ||
      state.proposal === null ||
      state.decisionReconciliationRequired ||
      state.reviewTerminalFailure
    )
      return;
    const project = state.project;
    const proposal = state.proposal;
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "committing_decision" });
      const intentId = crypto.randomUUID();
      let approval: Awaited<ReturnType<AuthenticatedApi["approveDecision"]>>;
      try {
        approval = await session.api.approveDecision({
          reviewId: proposal.reviewId,
          proposalId: proposal.id,
          expectedRevisionId: project.revisionId,
          disposition,
          intentId,
        });
      } catch (error) {
        if (error instanceof ApiError) throw error;
        throw new ApiError(
          error instanceof Error && error.message.trim().length > 0
            ? error.message
            : "一回限りの承認を取得できませんでした。",
          {
            code: "approval_failed_before_decision",
            retryable: true,
            detail: { acceptedState: "unchanged", recoveryAction: "retry" },
          },
        );
      }
      try {
        const response = await session.api.decide({
          reviewId: proposal.reviewId,
          approvalToken: approval.token,
          proposalId: proposal.id,
          expectedRevisionId: project.revisionId,
          disposition,
          intentId,
          ...(rationale.trim().length === 0
            ? {}
            : { rationale: rationale.trim() }),
        });
        if (
          response.decision.reviewId !== proposal.reviewId ||
          response.decision.proposalId !== proposal.id ||
          response.decision.disposition !== disposition
        ) {
          throw new ApiError("Human Decisionのbindingが一致しません。", {
            code: "decision_binding_mismatch",
            retryable: false,
          });
        }
      } catch (error) {
        throw new DecisionOutcomeError("decision", disposition, error);
      }
      dispatch({
        type: "DECISION_COMMITTED",
        disposition,
      });
      let refreshed: Project;
      try {
        refreshed = await session.api.getProject(project.id);
      } catch (error) {
        throw new DecisionOutcomeError("refresh", disposition, error);
      }
      dispatch({
        type: "PROJECT_REFRESHED",
        project: refreshed,
        afterDecision: true,
      });
    });
  };

  const reconcileActiveReview = () => {
    if (state.project === null) return;
    const project = state.project;
    const activeReview = project.activeReview;
    if (
      state.reviewTerminalFailure ||
      state.review?.status === "failed" ||
      activeReview?.status === "failed"
    ) {
      return;
    }
    const expected: ExpectedReviewBinding | null =
      state.review !== null && state.review.proposal !== null
        ? {
            reviewId: state.review.reviewId,
            proposalId: state.review.proposalId,
            baseRevisionId: state.review.proposal.baseRevisionId,
          }
        : state.proposal !== null
          ? {
              reviewId: state.proposal.reviewId,
              proposalId: state.proposal.id,
              baseRevisionId: state.proposal.baseRevisionId,
            }
          : activeReview === undefined || activeReview === null
            ? null
            : activeReview;
    if (expected === null) return;
    void run(async () => {
      dispatch({ type: "OPERATION_STARTED", operation: "reconciling_review" });
      const review = await session.api.reconcileReview(expected.reviewId);
      assertReviewBinding(project, expected, review);
      dispatch({ type: "REVIEW_RECONCILED", review });
      if (!review.reconciliationRequired) {
        const refreshed = await session.api.getProject(project.id);
        if (
          review.decision !== null &&
          refreshed.revisionId !== review.decision.revisionId
        ) {
          throw new ApiError(
            "再照合したDecisionとAccepted revisionが一致しません。",
            { code: "reconciliation_binding_mismatch", retryable: false },
          );
        }
        dispatch({
          type: "PROJECT_REFRESHED",
          project: refreshed,
          afterDecision: review.proposal === null,
        });
      }
    });
  };

  const restoreCurrentReview = () => {
    if (state.project !== null) restoreProjectReview(state.project);
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
      requestAnimationFrame(() => {
        if (exportButtonRef.current?.disabled === false) {
          exportButtonRef.current.focus({ preventScroll: true });
        }
      });
      await refreshRetention();
    });
  };

  const closePublication = useCallback(() => {
    if (state.operation !== null) return;
    setPublicationOpen(false);
    dispatch({ type: "PUBLICATION_CLOSED" });
  }, [state.operation]);

  const generatePublication = (settings: PublicationSettings) => {
    if (state.project === null) return;
    const project = state.project;
    void run(async () => {
      dispatch({
        type: "OPERATION_STARTED",
        operation: "generating_publication",
      });
      const publication = await session.api.createPublication({
        projectId: project.id,
        revisionId: project.revisionId,
        ...settings,
      });
      if (publication.revisionId !== project.revisionId) {
        throw new ApiError(
          "publication draftのAccepted revision bindingが一致しません。",
          { code: "publication_binding_mismatch", retryable: false },
        );
      }
      dispatch({ type: "PUBLICATION_READY", publication });
      await refreshRetention();
    });
  };

  const downloadPublication = () => {
    const publication = state.publicationDraft;
    if (publication === null) return;
    void run(async () => {
      dispatch({
        type: "OPERATION_STARTED",
        operation: "downloading_publication",
      });
      const blob = await session.api.downloadPublication(
        publication.downloadUrl,
      );
      const objectUrl = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = objectUrl;
      anchor.download =
        "synapsegit-lp-publication-" +
        publication.revisionId.slice(0, 12) +
        ".zip";
      anchor.rel = "noopener";
      anchor.click();
      URL.revokeObjectURL(objectUrl);
      dispatch({ type: "PUBLICATION_DOWNLOADED" });
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
                        {project.activeReview !== undefined &&
                        project.activeReview !== null ? (
                          <span className="retained-review-state">
                            {project.activeReview.status === "failed"
                              ? "Reviewは永続的に失敗"
                              : project.activeReview.status ===
                                  "reconciliation_required"
                                ? "Decision再照合が必要"
                                : "未決Reviewあり"}
                          </span>
                        ) : project.history !== undefined ? (
                          <span>
                            {project.history.length} terminal decisions
                          </span>
                        ) : null}
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

            <RetentionPanel
              inventory={retention}
              busy={busy}
              error={retentionError}
              onCleanup={cleanupRetention}
            />

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
        api={session.api}
        viewport={state.viewportPreset}
        customWidth={state.customWidth}
        busy={busy}
        exportButtonRef={exportButtonRef}
        publicationButtonRef={publicationButtonRef}
        onViewport={(preset, customWidth) =>
          dispatch({
            type: "SET_VIEWPORT",
            preset,
            ...(customWidth === undefined ? {} : { customWidth }),
          })
        }
        onExport={exportAccepted}
        onPublication={() => setPublicationOpen(true)}
        onDisplayNameSaved={saveProjectDisplayName}
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

      {state.lastError !== null ? (
        <details className="last-error-diagnostic">
          <summary>
            Last error: <code>{state.lastError.code}</code> ·{" "}
            <code>{state.lastError.operation}</code>
          </summary>
          <dl>
            <div>
              <dt>Operation</dt>
              <dd>
                <code>{state.lastError.operation}</code>
              </dd>
            </div>
            <div>
              <dt>Safe error code</dt>
              <dd>
                <code>{state.lastError.code}</code>
              </dd>
            </div>
            <div>
              <dt>Retryable</dt>
              <dd>{state.lastError.retryable ? "yes" : "no"}</dd>
            </div>
            {state.lastError.requestId === null ? null : (
              <div>
                <dt>Request ID</dt>
                <dd>
                  <code>{state.lastError.requestId}</code>
                </dd>
              </div>
            )}
            {state.lastError.operationId === null ? null : (
              <div>
                <dt>Operation ID</dt>
                <dd>
                  <code>{state.lastError.operationId}</code>
                </dd>
              </div>
            )}
            {state.lastError.acceptedState === null ? null : (
              <div>
                <dt>Accepted state</dt>
                <dd>{acceptedStateMessage[state.lastError.acceptedState]}</dd>
              </div>
            )}
            {state.lastError.recoveryAction === null ? null : (
              <div>
                <dt>Recovery action</dt>
                <dd>{recoveryActionMessage[state.lastError.recoveryAction]}</dd>
              </div>
            )}
          </dl>
          <p>
            この診断にはprompt、file本文、credential、provider raw
            responseを含めません。
          </p>
        </details>
      ) : null}

      {state.operation !== null ? (
        <div className="operation-bar" role="status" aria-live="polite">
          <span className="spinner" aria-hidden="true" />
          {operationLabel[state.operation]}…
        </div>
      ) : null}

      {state.proposal === null ? (
        <ReviewRecoveryPanel
          project={state.project}
          busy={busy}
          onRestore={restoreCurrentReview}
          onReconcile={reconcileActiveReview}
        />
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
          previewScale={state.previewScale}
          disabled={busy}
          onMode={(mode) => dispatch({ type: "SET_PREVIEW_MODE", mode })}
          onSource={(source) =>
            dispatch({ type: "SET_PREVIEW_SOURCE", source })
          }
          onPreviewScale={(scale) =>
            dispatch({ type: "SET_PREVIEW_SCALE", scale })
          }
          onTarget={selectTarget}
          onStructure={setStructure}
          onBridgeError={(message) =>
            dispatch({
              type: "FAILED",
              message,
              code: "preview_bridge_error",
              requestId: null,
              retryable: true,
            })
          }
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
          terminalFailure={state.reviewTerminalFailure}
          rationale={rationale}
          onRationale={setRationale}
          onDecision={decideProposal}
          onReconcile={reconcileActiveReview}
          onSource={(source) =>
            dispatch({ type: "SET_PREVIEW_SOURCE", source })
          }
        />
      ) : null}

      <ProjectHistoryPanel project={state.project} />

      <RetentionPanel
        inventory={retention}
        projectId={state.project.id}
        busy={busy}
        error={retentionError}
        onCleanup={cleanupRetention}
      />

      {state.exportReceipt !== null ? (
        <ExportReceiptCard
          receipt={state.exportReceipt}
          fileCount={state.project.files.length}
        />
      ) : null}

      {publicationOpen ? (
        <PublicationReviewDialog
          project={state.project}
          draft={state.publicationDraft}
          busy={busy}
          returnFocus={publicationButtonRef}
          onClose={closePublication}
          onGenerate={generatePublication}
          onDownload={downloadPublication}
        />
      ) : null}

      {state.contextReview !== null ? (
        <ContextDialog
          context={state.contextReview}
          busy={busy}
          generating={state.operation === "generating_proposal"}
          elapsedSeconds={proposalElapsedSeconds}
          cancelPending={proposalCancelPending}
          returnFocus={reviewButtonRef}
          onClose={() => dispatch({ type: "CONTEXT_CLOSED" })}
          onConfirm={createProposal}
          onCancel={cancelProposalAttempt}
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

function RecoveryStudio({ session }: { session: BootstrappedApi }) {
  const [points, setPoints] = useState<RecoveryPoint[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void session.api
      .getRecoveryPoints()
      .then((result) => {
        if (active) setPoints(result);
      })
      .catch((cause: unknown) => {
        if (active) setError(errorMessage(cause));
      });
    return () => {
      active = false;
    };
  }, [session.api]);

  const download = (point: RecoveryPoint) => {
    if (point.exportUrl === null || downloading !== null) return;
    void (async () => {
      setDownloading(point.id);
      setError(null);
      try {
        const blob = await session.api.downloadRecoveryExport(point.exportUrl!);
        const objectUrl = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = objectUrl;
        anchor.download = `synapsegit-lp-recovery-${point.id}.zip`;
        anchor.rel = "noopener";
        anchor.click();
        URL.revokeObjectURL(objectUrl);
      } catch (cause) {
        setError(errorMessage(cause));
      } finally {
        setDownloading(null);
      }
    })();
  };

  return (
    <main className="recovery-screen">
      <section className="recovery-card" aria-labelledby="recovery-mode-title">
        <p className="eyebrow">READ-ONLY RECOVERY · FAIL CLOSED</p>
        <h1 id="recovery-mode-title">保存領域を読み取り専用で開きました</h1>
        <p>
          通常の整合性検証に失敗したため、自動修復は行っていません。元データを保持したまま、検証できたlast
          Acceptedまたはversioned backupだけを一覧・exportできます。
        </p>
        <ul className="home-boundaries">
          <li>Proposal生成、Decision、Accepted更新は停止</li>
          <li>Publication draft生成と保持データ削除は停止</li>
          <li>
            Exportは検証済みsnapshotからmemory上で生成し、保存領域へ書き込みません
          </li>
        </ul>
        {points === null ? (
          <p role="status">Recovery pointを検証しています…</p>
        ) : (
          <div className="recovery-points">
            {points.map((point) => (
              <article key={point.id} className="recovery-point">
                <div>
                  <span
                    className={
                      point.diagnostic.verified ? "pill" : "proposal-status"
                    }
                  >
                    {point.diagnostic.verified ? "VERIFIED" : "UNVERIFIED"}
                  </span>
                  <strong>
                    {point.kind === "last_accepted"
                      ? "Last Accepted"
                      : "Versioned backup"}
                  </strong>
                </div>
                <dl>
                  <div>
                    <dt>Recovery point</dt>
                    <dd>
                      <code>{point.id}</code>
                    </dd>
                  </div>
                  <div>
                    <dt>Diagnostic code</dt>
                    <dd>
                      <code>{point.diagnostic.code}</code>
                    </dd>
                  </div>
                  <div>
                    <dt>Files / bytes</dt>
                    <dd>
                      {point.diagnostic.fileCount} /{" "}
                      {point.diagnostic.totalBytes.toLocaleString("ja-JP")}
                    </dd>
                  </div>
                  <div>
                    <dt>Revision</dt>
                    <dd>
                      {point.revisionId === null ? (
                        "検証不可"
                      ) : (
                        <code>{point.revisionId}</code>
                      )}
                    </dd>
                  </div>
                  <div>
                    <dt>Manifest</dt>
                    <dd>
                      {point.artifactManifestSha256 === null ? (
                        "検証不可"
                      ) : (
                        <code>{point.artifactManifestSha256}</code>
                      )}
                    </dd>
                  </div>
                </dl>
                <button
                  type="button"
                  className="button button-secondary"
                  disabled={point.exportUrl === null || downloading !== null}
                  onClick={() => download(point)}
                >
                  {downloading === point.id
                    ? "Recovery exportを生成中…"
                    : "検証済みAcceptedをexport"}
                </button>
              </article>
            ))}
          </div>
        )}
        {error === null ? null : (
          <p className="error-card" role="alert">
            {error}
          </p>
        )}
      </section>
    </main>
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

  return session.bootstrap.capabilities.operatingMode ===
    "read_only_recovery" ? (
    <RecoveryStudio session={session} />
  ) : (
    <Studio session={session} />
  );
}

export type { AuthenticatedApi, PublicBootstrap };
