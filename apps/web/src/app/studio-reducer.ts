import type {
  ArtifactDisposition,
  ContextReview,
  ExportReceipt,
  PreviewMode,
  PreviewSource,
  Project,
  Proposal,
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
  | "exporting"
  | null;

export interface StudioState {
  project: Project | null;
  target: TargetSelection | null;
  contextReview: ContextReview | null;
  proposal: Proposal | null;
  proposalTarget: TargetV1 | null;
  proposalInstruction: string | null;
  previewMode: PreviewMode;
  previewSource: PreviewSource;
  viewportPreset: ViewportPreset;
  customWidth: number;
  operation: StudioOperation;
  error: string | null;
  announcement: string;
  lastDecision: ArtifactDisposition | null;
  exportReceipt: ExportReceipt | null;
}

export const initialStudioState: StudioState = {
  project: null,
  target: null,
  contextReview: null,
  proposal: null,
  proposalTarget: null,
  proposalInstruction: null,
  previewMode: "select",
  previewSource: "accepted",
  viewportPreset: "desktop",
  customWidth: 1080,
  operation: null,
  error: null,
  announcement: "",
  lastDecision: null,
  exportReceipt: null,
};

export type StudioAction =
  | { type: "OPERATION_STARTED"; operation: Exclude<StudioOperation, null> }
  | {
      type: "PROJECT_LOADED";
      project: Project;
      origin?: "created" | "opened" | "imported";
    }
  | { type: "PROJECT_REFRESHED"; project: Project; afterDecision: boolean }
  | { type: "TARGET_SELECTED"; target: TargetSelection }
  | { type: "TARGET_CLEARED" }
  | { type: "CONTEXT_READY"; context: ContextReview }
  | { type: "CONTEXT_CLOSED" }
  | { type: "PROPOSAL_READY"; proposal: Proposal; instruction: string }
  | { type: "DECISION_COMMITTED"; disposition: ArtifactDisposition }
  | { type: "EXPORT_READY"; receipt: ExportReceipt }
  | { type: "SET_PREVIEW_MODE"; mode: PreviewMode }
  | { type: "SET_PREVIEW_SOURCE"; source: PreviewSource }
  | { type: "SET_VIEWPORT"; preset: ViewportPreset; customWidth?: number }
  | { type: "FAILED"; message: string }
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
        proposalTarget: action.afterDecision ? null : state.proposalTarget,
        proposalInstruction: action.afterDecision
          ? null
          : state.proposalInstruction,
        previewSource: "accepted",
        operation: null,
        announcement: action.afterDecision
          ? `Human Decisionを記録し、Accepted revision ${action.project.revisionId} を再取得しました。`
          : `Accepted revision ${action.project.revisionId} を再取得しました。`,
      };
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
        announcement: "変更案が完成しました。採用前に内容を確認してください。",
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
    case "FAILED":
      return {
        ...state,
        operation: null,
        error: action.message,
        announcement: action.message,
      };
    case "DISMISS_ERROR":
      return { ...state, error: null };
  }
};
