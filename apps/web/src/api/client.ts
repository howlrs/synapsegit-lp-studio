import {
  SCHEMA_VERSION,
  isApiErrorResponse,
  isApprovalResponse,
  isBootstrapResponse,
  isContextResponse,
  isDecisionResponse,
  isExportResponse,
  isImportPreviewResponse,
  isProjectResponse,
  isProjectsResponse,
  isProposalResponse,
  isTargetResponse,
  type ApprovalRequest,
  type ArtifactDisposition,
  type BootstrapResponse,
  type ContextReview,
  type DecisionResponse,
  type ExportReceipt,
  type ImportPreview,
  type Project,
  type Proposal,
  type Target,
} from "@synapsegit-lp/contracts";

export class ApiError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly requestId?: string;

  constructor(
    message: string,
    options: { code: string; retryable: boolean; requestId?: string },
  ) {
    super(message);
    this.name = "ApiError";
    this.code = options.code;
    this.retryable = options.retryable;
    if (options.requestId !== undefined) this.requestId = options.requestId;
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
  previewRegisteredImport(): Promise<ImportPreview>;
  confirmRegisteredImport(
    previewId: string,
    expectedManifestSha256: string,
  ): Promise<Project>;
  createTarget(
    projectId: string,
    revisionId: string,
    elementId: string,
  ): Promise<Target>;
  createContext(
    projectId: string,
    revisionId: string,
    targetId: string,
    instruction: string,
  ): Promise<ContextReview>;
  createProposal(
    projectId: string,
    contextId: string,
    contextSha256: string,
  ): Promise<Proposal>;
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
  createExport(projectId: string, revisionId: string): Promise<ExportReceipt>;
  downloadExport(downloadUrl: string): Promise<Blob>;
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
    throw new ApiError(value.error.message, {
      code: value.error.code,
      retryable: value.error.retryable,
      requestId: value.error.requestId,
    });
  }
  throw new ApiError(
    `リクエストに失敗しました（HTTP ${String(response.status)}）。Accepted LPは変更されていません。`,
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
    !resolved.pathname.startsWith("/api/v1/exports/") ||
    !resolved.pathname.endsWith("/download")
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

    async createTarget(projectId, revisionId, elementId) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/targets`,
        {
          schemaVersion: SCHEMA_VERSION,
          revisionId,
          kind: "element",
          elementId,
        },
        isTargetResponse,
        "ターゲット",
      );
      return result.target;
    },

    async createContext(projectId, revisionId, targetId, instruction) {
      const result = await postJson(
        `/api/v1/projects/${encodeURIComponent(projectId)}/contexts`,
        {
          schemaVersion: SCHEMA_VERSION,
          revisionId,
          targetId,
          instruction,
        },
        isContextResponse,
        "送信コンテキスト",
      );
      return result.context;
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
      const url = ensureSameEditorOrigin(downloadUrl, editorOrigin);
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
