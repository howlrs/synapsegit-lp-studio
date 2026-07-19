import type {
  ArtifactDisposition,
  ApiErrorAcceptedState,
  ApiErrorRecoveryAction,
  ContextReview,
  ExportReceipt,
  PublicationDraft,
  PreviewMode,
  PreviewSource,
  Project,
  Proposal,
  Review,
  TargetSelection,
  TargetV1,
  ViewportPreset,
} from "@synapsegit-lp/contracts";

export type StudioOperation =
  | "creating_project"
  | "selecting_target"
  | "assembling_context"
  | "generating_proposal"
  | "committing_decision"
  | "refreshing_project"
  | "restoring_review"
  | "reconciling_review"
  | "exporting"
  | "generating_publication"
  | "downloading_publication"
  | null;

export type PreviewScale = 0.75 | 1;

export interface LastErrorDiagnostic {
  operation: Exclude<StudioOperation, null> | "unknown";
  code: string;
  requestId: string | null;
  operationId: string | null;
  retryable: boolean;
  acceptedState: ApiErrorAcceptedState | null;
  recoveryAction: ApiErrorRecoveryAction | null;
}

export interface StudioState {
  project: Project | null;
  target: TargetSelection | null;
  contextReview: ContextReview | null;
  proposal: Proposal | null;
  review: Review | null;
  proposalTarget: TargetV1 | null;
  proposalInstruction: string | null;
  previewMode: PreviewMode;
  previewSource: PreviewSource;
  viewportPreset: ViewportPreset;
  customWidth: number;
  previewScale: PreviewScale;
  operation: StudioOperation;
  decisionReconciliationRequired: boolean;
  reviewTerminalFailure: boolean;
  error: string | null;
  lastError: LastErrorDiagnostic | null;
  announcement: string;
  lastDecision: ArtifactDisposition | null;
  exportReceipt: ExportReceipt | null;
  publicationDraft: PublicationDraft | null;
}

export const initialStudioState: StudioState = {
  project: null,
  target: null,
  contextReview: null,
  proposal: null,
  review: null,
  proposalTarget: null,
  proposalInstruction: null,
  previewMode: "select",
  previewSource: "accepted",
  viewportPreset: "desktop",
  customWidth: 1080,
  previewScale: 1,
  operation: null,
  decisionReconciliationRequired: false,
  reviewTerminalFailure: false,
  error: null,
  lastError: null,
  announcement: "",
  lastDecision: null,
  exportReceipt: null,
  publicationDraft: null,
};

export type StudioAction =
  | { type: "OPERATION_STARTED"; operation: Exclude<StudioOperation, null> }
  | {
      type: "PROJECT_LOADED";
      project: Project;
      origin?: "created" | "opened" | "imported";
    }
  | { type: "PROJECT_REFRESHED"; project: Project; afterDecision: boolean }
  | {
      type: "PROJECT_DISPLAY_NAME_SAVED";
      projectId: string;
      displayName: string;
    }
  | { type: "TARGET_SELECTED"; target: TargetSelection }
  | { type: "TARGET_CLEARED" }
  | { type: "CONTEXT_READY"; context: ContextReview }
  | { type: "CONTEXT_CLOSED" }
  | { type: "PROPOSAL_READY"; proposal: Proposal; instruction: string }
  | { type: "REVIEW_RESTORED"; review: Review }
  | { type: "REVIEW_RECONCILED"; review: Review }
  | { type: "DECISION_COMMITTED"; disposition: ArtifactDisposition }
  | { type: "EXPORT_READY"; receipt: ExportReceipt }
  | { type: "PUBLICATION_READY"; publication: PublicationDraft }
  | { type: "PUBLICATION_DOWNLOADED" }
  | { type: "PUBLICATION_CLOSED" }
  | { type: "SET_PREVIEW_MODE"; mode: PreviewMode }
  | { type: "SET_PREVIEW_SOURCE"; source: PreviewSource }
  | { type: "SET_VIEWPORT"; preset: ViewportPreset; customWidth?: number }
  | { type: "SET_PREVIEW_SCALE"; scale: PreviewScale }
  | { type: "PROPOSAL_ATTEMPT_CLEARED"; announcement: string }
  | { type: "PROPOSAL_ATTEMPT_STATUS"; announcement: string }
  | {
      type: "FAILED";
      message: string;
      code: string;
      requestId: string | null;
      operationId?: string | null;
      retryable: boolean;
      acceptedState?: ApiErrorAcceptedState | null;
      recoveryAction?: ApiErrorRecoveryAction | null;
      clearContextReview?: true;
      requiresDecisionReconciliation?: true;
    }
  | { type: "DISMISS_ERROR" };

const clampWidth = (value: number): number =>
  Math.min(1920, Math.max(320, Math.round(value)));

export const studioReducer = (
  state: StudioState,
  action: StudioAction,
): StudioState => {
  switch (action.type) {
    case "OPERATION_STARTED":
      return { ...state, operation: action.operation, error: null };
    case "PROJECT_LOADED":
      return {
        ...initialStudioState,
        project: action.project,
        decisionReconciliationRequired:
          action.project.activeReview?.status === "reconciliation_required",
        reviewTerminalFailure: action.project.activeReview?.status === "failed",
        announcement: `${
          action.origin === "opened"
            ? "保存済みプロジェクトを開きました。"
            : action.origin === "imported"
              ? "登録済みディレクトリのコピーを取り込みました。"
              : "プロジェクトを作成しました。"
        }Accepted revision ${action.project.revisionId}`,
      };
    case "PROJECT_REFRESHED":
      return {
        ...state,
        project: action.project,
        target:
          !action.afterDecision &&
          state.target?.resolution.resolvedRevisionId ===
            action.project.revisionId
            ? state.target
            : null,
        contextReview: null,
        proposal: action.afterDecision ? null : state.proposal,
        review: action.afterDecision ? null : state.review,
        proposalTarget: action.afterDecision ? null : state.proposalTarget,
        proposalInstruction: action.afterDecision
          ? null
          : state.proposalInstruction,
        previewSource: "accepted",
        operation: null,
        decisionReconciliationRequired: action.afterDecision
          ? false
          : state.decisionReconciliationRequired,
        reviewTerminalFailure: action.afterDecision
          ? false
          : action.project.activeReview?.status === "failed" ||
            state.reviewTerminalFailure,
        announcement: action.afterDecision
          ? `Human Decisionを記録し、Accepted revision ${action.project.revisionId} を再取得しました。`
          : `Accepted revision ${action.project.revisionId} を再取得しました。`,
      };
    case "PROJECT_DISPLAY_NAME_SAVED":
      return state.project?.id === action.projectId
        ? {
            ...state,
            project: { ...state.project, displayName: action.displayName },
            announcement: `プロジェクト表示名「${action.displayName}」を保存しました。`,
          }
        : state;
    case "TARGET_SELECTED":
      return {
        ...state,
        target: action.target,
        contextReview: null,
        operation: null,
        error: null,
        announcement: `ターゲット「${action.target.target.label}」を選択しました。解決結果は${action.target.resolution.status}です。`,
      };
    case "TARGET_CLEARED":
      return {
        ...state,
        target: null,
        contextReview: null,
        announcement: "ターゲット選択を解除しました。",
      };
    case "CONTEXT_READY":
      return {
        ...state,
        contextReview: action.context,
        operation: null,
        announcement: "AIへ送信する正確なコンテキストを確認してください。",
      };
    case "CONTEXT_CLOSED":
      return { ...state, contextReview: null };
    case "PROPOSAL_READY":
      return {
        ...state,
        contextReview: null,
        proposal: action.proposal,
        proposalTarget: state.target?.target ?? null,
        proposalInstruction: action.instruction,
        previewSource: "proposed",
        operation: null,
        reviewTerminalFailure: false,
        announcement: "変更案が完成しました。採用前に内容を確認してください。",
      };
    case "REVIEW_RESTORED":
      return {
        ...state,
        review: action.review,
        proposal: action.review.proposal,
        proposalTarget: action.review.proposal?.target ?? null,
        proposalInstruction: action.review.proposal?.instruction ?? null,
        previewSource:
          action.review.proposal === null ? "accepted" : "proposed",
        operation: null,
        decisionReconciliationRequired: action.review.reconciliationRequired,
        reviewTerminalFailure: action.review.status === "failed",
        announcement:
          action.review.status === "failed"
            ? "保存済みReviewは永続的に失敗しています。再照合とDecision操作はできません。"
            : action.review.proposal === null
              ? "保存済みReviewの終端状態を確認しました。"
              : action.review.reconciliationRequired
                ? "保存済みReviewを復元しました。Decision結果の再照合が必要です。"
                : "保存済みの未決Reviewを復元しました。",
      };
    case "REVIEW_RECONCILED":
      return {
        ...state,
        review: action.review,
        proposal: action.review.proposal,
        proposalTarget: action.review.proposal?.target ?? null,
        proposalInstruction: action.review.proposal?.instruction ?? null,
        previewSource:
          action.review.proposal === null ? "accepted" : state.previewSource,
        operation: null,
        decisionReconciliationRequired: action.review.reconciliationRequired,
        reviewTerminalFailure: action.review.status === "failed",
        error: null,
        announcement:
          action.review.status === "failed"
            ? "Reviewは永続的な失敗として確定しました。再照合とDecision操作はできません。"
            : action.review.reconciliationRequired
              ? "Decision結果はまだ確定できません。再実行せず、再照合を続けてください。"
              : action.review.proposal === null
                ? "Decision結果を再照合し、終端状態を確認しました。"
                : "Decision結果を再照合し、未決Reviewを再開できます。",
      };
    case "DECISION_COMMITTED":
      // A Decision response is not authority for Accepted UI state. Only a
      // subsequent PROJECT_REFRESHED action may update the Accepted revision.
      return {
        ...state,
        operation: "refreshing_project",
        lastDecision: action.disposition,
        announcement:
          "Decisionを記録しました。Accepted状態を再取得しています。",
      };
    case "EXPORT_READY":
      return {
        ...state,
        exportReceipt: action.receipt,
        operation: null,
        announcement: `Accepted revision ${action.receipt.revisionId} をエクスポートしました。`,
      };
    case "PUBLICATION_READY":
      return {
        ...state,
        publicationDraft: action.publication,
        operation: null,
        announcement:
          "GitHub-ready filesをローカル生成しました。remote write前にexact bytesを確認してください。",
      };
    case "PUBLICATION_DOWNLOADED":
      return {
        ...state,
        operation: null,
        announcement:
          "確認済みpublication ZIPをローカルへダウンロードしました。remote writeは行っていません。",
      };
    case "PUBLICATION_CLOSED":
      return { ...state, publicationDraft: null };
    case "SET_PREVIEW_MODE":
      return { ...state, previewMode: action.mode };
    case "SET_PREVIEW_SOURCE":
      return action.source === state.previewSource
        ? state
        : {
            ...state,
            previewSource: action.source,
            target: null,
            contextReview: null,
            announcement:
              "表示元を切り替えました。Targetを現在のプレビューから選び直してください。",
          };
    case "SET_VIEWPORT":
      return {
        ...state,
        viewportPreset: action.preset,
        customWidth:
          action.customWidth === undefined
            ? state.customWidth
            : clampWidth(action.customWidth),
      };
    case "SET_PREVIEW_SCALE":
      return { ...state, previewScale: action.scale };
    case "PROPOSAL_ATTEMPT_CLEARED":
      return {
        ...state,
        contextReview: null,
        operation: null,
        error: null,
        announcement: action.announcement,
      };
    case "PROPOSAL_ATTEMPT_STATUS":
      return { ...state, error: null, announcement: action.announcement };
    case "FAILED":
      return {
        ...state,
        contextReview:
          action.clearContextReview === true ? null : state.contextReview,
        operation: null,
        decisionReconciliationRequired:
          state.decisionReconciliationRequired ||
          action.requiresDecisionReconciliation === true,
        error: action.message,
        lastError: {
          operation: state.operation ?? "unknown",
          code: action.code,
          requestId: action.requestId,
          operationId: action.operationId ?? null,
          retryable: action.retryable,
          acceptedState: action.acceptedState ?? null,
          recoveryAction: action.recoveryAction ?? null,
        },
        announcement: action.message,
      };
    case "DISMISS_ERROR":
      return { ...state, error: null };
  }
};
