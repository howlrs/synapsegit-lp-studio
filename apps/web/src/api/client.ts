import {
  SCHEMA_VERSION,
  isAiAttemptStatusV1,
  isApiErrorResponse,
  isApprovalResponse,
  isBootstrapResponse,
  isContextResponse,
  isDecisionResponse,
  isExportResponse,
  isImportPreviewResponse,
  isProjectDisplayName,
  isPublicationResponse,
  isProjectResponse,
  isProjectsResponse,
  isProviderCredentialResponse,
  isProposalResponse,
  isRecoveryResponse,
  isRetentionCleanupResponse,
  isRetentionResponse,
  isReviewResponse,
  isTargetResponse,
  type ApprovalRequest,
  type AiAttemptStatusV1,
  type ApiErrorDetail,
  type ArtifactDisposition,
  type BootstrapResponse,
  type ContextReview,
  type DecisionResponse,
  type ExportReceipt,
  type ImportPreview,
  type PublicationDraft,
  type Project,
  type ProviderCredentialStatus,
  type Proposal,
  type RecoveryPoint,
  type RetentionCleanupRequest,
  type RetentionCleanupResponse,
  type RetentionInventory,
  type Review,
  type TargetSelection,
  type TargetV1,
  type UpdateProjectMetadataRequest,
} from "@synapsegit-lp/contracts";

export class ApiError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly requestId?: string;
  readonly operationId?: string;
  readonly detail?: ApiErrorDetail;

  constructor(
    message: string,
    options: {
      code: string;
      retryable: boolean;
      requestId?: string;
      operationId?: string;
      detail?: ApiErrorDetail;
    },
  ) {
    super(message);
    this.name = "ApiError";
    this.code = options.code;
    this.retryable = options.retryable;
    if (options.requestId !== undefined) this.requestId = options.requestId;
    if (options.operationId !== undefined)
      this.operationId = options.operationId;
    if (options.detail !== undefined) this.detail = options.detail;
  }
}

export interface PublicBootstrap {
  apiVersion: "v1";
  expiresAt: string;
  editorOrigin: string;
  previewOrigin: string;
  capabilities: BootstrapResponse["capabilities"];
}

export interface AuthenticatedApi {
  createBlankProject(): Promise<Project>;
  listProjects(): Promise<Project[]>;
  getProject(projectId: string): Promise<Project>;
  updateProjectDisplayName(
    projectId: string,
    expectedDisplayName: string,
    displayName: string,
  ): Promise<Project>;
  previewRegisteredImport(): Promise<ImportPreview>;
  confirmRegisteredImport(
    previewId: string,
    expectedManifestSha256: string,
  ): Promise<Project>;
  createTarget(projectId: string, target: TargetV1): Promise<TargetSelection>;
  createContext(
    projectId: string,
    revisionId: string,
    targetId: string,
    resolutionId: string,
    attemptId: string,
    providerId: string,
    requestedModel: string,
    instruction: string,
    credentialSourceId?: "editor_session" | "server_configured",
  ): Promise<ContextReview>;
  getOpenAiCredential(): Promise<ProviderCredentialStatus | null>;
  configureOpenAiCredential(
    apiKey: string,
    expectedCredentialBindingId: string | null,
  ): Promise<ProviderCredentialStatus>;
  clearOpenAiCredential(credentialBindingId: string): Promise<void>;
  createProposal(
    projectId: string,
    contextId: string,
    contextSha256: string,
  ): Promise<Proposal>;
  getAiAttemptStatus(
    projectId: string,
    attemptId: string,
  ): Promise<AiAttemptStatusV1>;
  cancelAiAttempt(
    projectId: string,
    attemptId: string,
  ): Promise<AiAttemptStatusV1>;
  approveDecision(input: {
    reviewId: string;
    proposalId: string;
    expectedRevisionId: string;
    disposition: ArtifactDisposition;
    intentId: string;
  }): Promise<{ token: string; expiresAt: string; intentId: string }>;
  decide(input: {
    reviewId: string;
    approvalToken: string;
    proposalId: string;
    expectedRevisionId: string;
    disposition: ArtifactDisposition;
    intentId: string;
    rationale?: string;
  }): Promise<DecisionResponse>;
  getReview(reviewId: string): Promise<Review>;
  reconcileReview(reviewId: string): Promise<Review>;
  createExport(projectId: string, revisionId: string): Promise<ExportReceipt>;
  downloadExport(downloadUrl: string): Promise<Blob>;
  createPublication(input: {
    projectId: string;
    revisionId: string;
    publicLabel: string;
    title: string;
    summary: string;
    publicDecisionNote?: string;
  }): Promise<PublicationDraft>;
  downloadPublication(downloadUrl: string): Promise<Blob>;
  getRetention(): Promise<RetentionInventory>;
  cleanupRetention(
    input: RetentionCleanupRequest,
  ): Promise<RetentionCleanupResponse>;
  getRecoveryPoints(): Promise<RecoveryPoint[]>;
  downloadRecoveryExport(exportUrl: string): Promise<Blob>;
}

export interface BootstrappedApi {
  bootstrap: PublicBootstrap;
  api: AuthenticatedApi;
}

type FetchLike = typeof fetch;
type Guard<T> = (value: unknown) => value is T;

const safeJson = async (response: Response): Promise<unknown> => {
  const text = await response.text();
  if (text.length === 0) return undefined;
  try {
    return JSON.parse(text) as unknown;
  } catch {
    throw new ApiError("サーバー応答を読み取れませんでした。", {
      code: "invalid_json_response",
      retryable: false,
    });
  }
};

const validateResponse = <T>(
  value: unknown,
  guard: Guard<T>,
  label: string,
): T => {
  if (!guard(value)) {
    throw new ApiError(`${label}の形式またはバージョンが不正です。`, {
      code: "invalid_response_schema",
      retryable: false,
    });
  }
  return value;
};

const throwResponseError = (response: Response, value: unknown): never => {
  if (isApiErrorResponse(value)) {
    const operationId = response.headers.get("x-operation-id");
    const requestId = response.headers.get("x-request-id");
    if (
      operationId !== value.error.operationId ||
      requestId !== value.error.requestId
    ) {
      throw new ApiError("エラー応答の相関情報が一致しません。", {
        code: "invalid_error_correlation",
        retryable: false,
      });
    }
    throw new ApiError(value.error.message, {
      code: value.error.code,
      retryable: value.error.retryable,
      requestId: value.error.requestId,
      operationId: value.error.operationId,
      detail: value.error.detail,
    });
  }
  throw new ApiError(
    `リクエストに失敗しました（HTTP ${String(response.status)}）。`,
    { code: "http_error", retryable: response.status >= 500 },
  );
};

const requestBootstrap = async (
  fetcher: FetchLike,
): Promise<BootstrapResponse> => {
  const response = await fetcher("/api/v1/bootstrap", {
    method: "GET",
    credentials: "omit",
    cache: "no-store",
    referrerPolicy: "no-referrer",
    headers: { Accept: "application/json" },
  });
  const value = await safeJson(response);
  if (!response.ok) throwResponseError(response, value);
  return validateResponse(value, isBootstrapResponse, "起動情報");
};

const ensureSameEditorOrigin = (
  rawUrl: string,
  editorOrigin: string,
  collection: "exports" | "publications" | "recovery",
): string => {
  let resolved: URL;
  try {
    resolved = new URL(rawUrl, editorOrigin);
  } catch {
    throw new ApiError("ダウンロードURLが不正です。", {
      code: "invalid_download_url",
      retryable: false,
    });
  }
  if (
    resolved.origin !== editorOrigin ||
    resolved.username.length > 0 ||
    resolved.password.length > 0 ||
    resolved.search.length > 0 ||
    resolved.hash.length > 0 ||
    !resolved.pathname.startsWith(`/api/v1/${collection}/`) ||
    !resolved.pathname.endsWith(
      collection === "recovery" ? "/export" : "/download",
    )
  ) {
    throw new ApiError("許可されていないダウンロードURLです。", {
      code: "invalid_download_origin",
      retryable: false,
    });
  }
  return resolved.toString();
};

/**
 * Bootstraps a session and immediately encloses the bearer token in this
 * module's request closure. The returned public object cannot reveal it.
 */
export const bootstrapApi = async (
  fetcher: FetchLike = globalThis.fetch,
  expectedEditorOrigin: string = globalThis.location.origin,
): Promise<BootstrappedApi> => {
  const response = await requestBootstrap(fetcher);
  const editorOrigin = new URL(response.editorOrigin).origin;
  if (editorOrigin !== new URL(expectedEditorOrigin).origin) {
    throw new ApiError("起動情報のEditor originが現在の画面と一致しません。", {
      code: "editor_origin_mismatch",
      retryable: false,
    });
  }
  const token = response.session.token;

  const requestJson = async <T>(
    path: string,
    init: Omit<RequestInit, "credentials">,
    guard: Guard<T>,
    label: string,
  ): Promise<T> => {
    const headers = new Headers(init.headers);
    headers.set("Accept", "application/json");
    headers.set("Authorization", `Bearer ${token}`);
    const apiResponse = await fetcher(path, {
      ...init,
      headers,
      credentials: "omit",
      cache: "no-store",
      referrerPolicy: "no-referrer",
    });
    const value = await safeJson(apiResponse);
    if (!apiResponse.ok) throwResponseError(apiResponse, value);
    return validateResponse(value, guard, label);
  };

  const postJson = <T>(
    path: string,
    body: object,
    guard: Guard<T>,
    label: string,
  ): Promise<T> =>
    requestJson(
      path,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      },
      guard,
      label,
    );

  const putJson = <T>(
    path: string,
    body: object,
    guard: Guard<T>,
    label: string,
  ): Promise<T> =>
    requestJson(
      path,
      {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      },
      guard,
      label,
    );

  const downloadArtifact = async (
    downloadUrl: string,
    collection: "exports" | "publications" | "recovery",
  ): Promise<Blob> => {
    const url = ensureSameEditorOrigin(downloadUrl, editorOrigin, collection);
    const download = await fetcher(url, {
      method: "GET",
      credentials: "omit",
      cache: "no-store",
      referrerPolicy: "no-referrer",
      headers: {
        Accept: "application/zip",
        Authorization: `Bearer ${token}`,
      },
    });
    if (!download.ok) {
      const value = await safeJson(download);
      throwResponseError(download, value);
    }
    return download.blob();
  };

  const api: AuthenticatedApi = {
    async createBlankProject() {
      const result = await postJson(
        "/api/v1/projects",
        { schemaVersion: SCHEMA_VERSION, template: "blank" },
        isProjectResponse,
        "プロジェクト",
      );
      return result.project;
    },

    async listProjects() {
      const result = await requestJson(
        "/api/v1/projects",
        { method: "GET" },
        isProjectsResponse,
        "保存済みプロジェクト一覧",
      );
      return result.projects;
    },

    async getProject(projectId) {
      const result = await requestJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}`,
        { method: "GET" },
        isProjectResponse,
        "プロジェクト",
      );
      return result.project;
    },

    async updateProjectDisplayName(
      projectId,
      expectedDisplayName,
      displayName,
    ) {
      if (
        !isProjectDisplayName(expectedDisplayName) ||
        !isProjectDisplayName(displayName)
      ) {
        throw new ApiError("プロジェクト表示名が不正です。", {
          code: "invalid_project_display_name",
          retryable: false,
        });
      }
      const body: UpdateProjectMetadataRequest = {
        schemaVersion: SCHEMA_VERSION,
        expectedDisplayName,
        displayName,
      };
      const result = await requestJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}`,
        {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        },
        isProjectResponse,
        "プロジェクト表示名",
      );
      return result.project;
    },

    async previewRegisteredImport() {
      const result = await postJson(
        "/api/v1/imports/previews",
        { schemaVersion: SCHEMA_VERSION },
        isImportPreviewResponse,
        "取り込みプレビュー",
      );
      return result.importPreview;
    },

    async confirmRegisteredImport(previewId, expectedManifestSha256) {
      const result = await postJson(
        `/api/v1/imports/${encodeURIComponent(previewId)}/confirm`,
        {
          schemaVersion: SCHEMA_VERSION,
          expectedManifestSha256,
        },
        isProjectResponse,
        "取り込み結果",
      );
      return result.project;
    },

    async createTarget(projectId, target) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/targets`,
        {
          schemaVersion: SCHEMA_VERSION,
          target,
        },
        isTargetResponse,
        "ターゲット",
      );
      return {
        target: result.target,
        resolution: result.resolution,
        resolutionId: result.resolutionId,
      };
    },

    async createContext(
      projectId,
      revisionId,
      targetId,
      resolutionId,
      attemptId,
      providerId,
      requestedModel,
      instruction,
      credentialSourceId,
    ) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/contexts`,
        {
          schemaVersion: SCHEMA_VERSION,
          revisionId,
          targetId,
          resolutionId,
          attemptId,
          providerId,
          requestedModel,
          ...(credentialSourceId === undefined ? {} : { credentialSourceId }),
          instruction,
        },
        isContextResponse,
        "送信コンテキスト",
      );
      return result.context;
    },

    async getOpenAiCredential() {
      const result = await requestJson(
        "/api/v1/session/provider-credentials/openai",
        { method: "GET" },
        isProviderCredentialResponse,
        "OpenAI資格情報",
      );
      return result.credential;
    },

    async configureOpenAiCredential(apiKey, expectedCredentialBindingId) {
      const result = await putJson(
        "/api/v1/session/provider-credentials/openai",
        { schemaVersion: SCHEMA_VERSION, expectedCredentialBindingId, apiKey },
        isProviderCredentialResponse,
        "OpenAI資格情報",
      );
      if (result.credential === null)
        throw new ApiError("資格情報の登録結果が不正です。", {
          code: "invalid_response_schema",
          retryable: false,
        });
      return result.credential;
    },

    async clearOpenAiCredential(credentialBindingId) {
      await requestJson(
        "/api/v1/session/provider-credentials/openai",
        {
          method: "DELETE",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            schemaVersion: SCHEMA_VERSION,
            credentialBindingId,
          }),
        },
        isProviderCredentialResponse,
        "OpenAI資格情報の削除",
      );
    },

    async createProposal(projectId, contextId, contextSha256) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/proposals`,
        { schemaVersion: SCHEMA_VERSION, contextId, contextSha256 },
        isProposalResponse,
        "変更案",
      );
      return result.proposal;
    },

    async getAiAttemptStatus(projectId, attemptId) {
      return requestJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/ai-attempts/${encodeURIComponent(attemptId)}`,
        { method: "GET" },
        isAiAttemptStatusV1,
        "AI処理状態",
      );
    },

    async cancelAiAttempt(projectId, attemptId) {
      return postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/ai-attempts/${encodeURIComponent(attemptId)}/cancel`,
        { schemaVersion: SCHEMA_VERSION },
        isAiAttemptStatusV1,
        "AI処理の取消",
      );
    },

    async approveDecision(input) {
      const body: ApprovalRequest = {
        schemaVersion: SCHEMA_VERSION,
        proposalId: input.proposalId,
        expectedRevisionId: input.expectedRevisionId,
        disposition: input.disposition,
        intentId: input.intentId,
      };
      const result = await postJson(
        `/api/v1/reviews/${encodeURIComponent(input.reviewId)}/approvals`,
        body,
        isApprovalResponse,
        "採用承認",
      );
      return result.approval;
    },

    async decide(input) {
      const body = {
        schemaVersion: SCHEMA_VERSION,
        approvalToken: input.approvalToken,
        proposalId: input.proposalId,
        expectedRevisionId: input.expectedRevisionId,
        disposition: input.disposition,
        intentId: input.intentId,
        ...(input.rationale === undefined
          ? {}
          : { rationale: input.rationale }),
      };
      return postJson(
        `/api/v1/reviews/${encodeURIComponent(input.reviewId)}/decisions`,
        body,
        isDecisionResponse,
        "Human Decision",
      );
    },

    async getReview(reviewId) {
      const result = await requestJson(
        `/api/v1/reviews/${encodeURIComponent(reviewId)}`,
        { method: "GET" },
        isReviewResponse,
        "保存済みレビュー",
      );
      return result.review;
    },

    async reconcileReview(reviewId) {
      const result = await postJson(
        `/api/v1/operations/reviews/${encodeURIComponent(reviewId)}/reconcile`,
        { schemaVersion: SCHEMA_VERSION },
        isReviewResponse,
        "Decision再照合",
      );
      return result.review;
    },

    async createExport(projectId, revisionId) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/exports`,
        { schemaVersion: SCHEMA_VERSION, revisionId },
        isExportResponse,
        "エクスポート",
      );
      return result.export;
    },

    async downloadExport(downloadUrl) {
      return downloadArtifact(downloadUrl, "exports");
    },

    async createPublication(input) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(input.projectId)}/publications`,
        {
          schemaVersion: SCHEMA_VERSION,
          revisionId: input.revisionId,
          publicLabel: input.publicLabel,
          title: input.title,
          summary: input.summary,
          ...(input.publicDecisionNote === undefined
            ? {}
            : { publicDecisionNote: input.publicDecisionNote }),
        },
        isPublicationResponse,
        "ローカルpublication draft",
      );
      return result.publication;
    },

    async downloadPublication(downloadUrl) {
      return downloadArtifact(downloadUrl, "publications");
    },

    async getRetention() {
      const result = await requestJson(
        "/api/v1/retention",
        { method: "GET" },
        isRetentionResponse,
        "保持データ一覧",
      );
      return result.retention;
    },

    async cleanupRetention(input) {
      return postJson(
        "/api/v1/retention/cleanup",
        { schemaVersion: SCHEMA_VERSION, ...input },
        isRetentionCleanupResponse,
        "保持データ削除",
      );
    },

    async getRecoveryPoints() {
      const result = await requestJson(
        "/api/v1/recovery",
        { method: "GET" },
        isRecoveryResponse,
        "read-only recovery一覧",
      );
      return result.recoveryPoints;
    },

    async downloadRecoveryExport(exportUrl) {
      return downloadArtifact(exportUrl, "recovery");
    },
  };

  return {
    bootstrap: {
      apiVersion: response.apiVersion,
      expiresAt: response.session.expiresAt,
      editorOrigin,
      previewOrigin: new URL(response.previewOrigin).origin,
      capabilities: response.capabilities,
    },
    api,
  };
};
