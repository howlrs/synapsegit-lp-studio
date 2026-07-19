import type {
  ApiErrorResponse,
  ApprovalResponse,
  BootstrapResponse,
  ChangeSetV1,
  ContextResponse,
  DecisionResponse,
  ExportResponse,
  ElementTargetV1,
  ImportPreviewResponse,
  Project,
  ProjectResponse,
  ProjectsResponse,
  ProposalResponse,
  TargetResponse,
  TargetResolverResultV1,
} from "@synapsegit-lp/contracts";

export const HASH_A = "a".repeat(64);
export const HASH_B = "b".repeat(64);
export const HASH_C = "c".repeat(64);
export const PREVIEW_SCOPE_BASE = "http://localhost:4174";
export const ACCEPTED_PREVIEW_ORIGIN =
  "http://pv-11111111111111111111111111111111.localhost:4174";
export const PROPOSED_PREVIEW_ORIGIN =
  "http://pv-22222222222222222222222222222222.localhost:4174";

export const bootstrapFixture = (
  editorOrigin = "http://localhost:3000",
): BootstrapResponse => ({
  schemaVersion: "1",
  apiVersion: "v1",
  session: {
    token: "secret-only-in-api-closure",
    expiresAt: "2026-07-19T12:00:00Z",
  },
  editorOrigin,
  previewOrigin: PREVIEW_SCOPE_BASE,
  capabilities: {
    targetKinds: ["page", "block", "element", "text", "point", "region"],
    dispositions: ["adopted_unchanged", "rejected", "deferred"],
    singleProposalPerProject: true,
    importAvailable: true,
    aiProviders: [
      {
        id: "fake",
        label: "Local deterministic fake",
        adapterVersion: "fake-change-set/1",
        external: false,
        availability: "available",
        dataRetentionPolicy: "local_only_not_retained_by_adapter",
        trainingPolicy: "not_applicable",
        policyNotice:
          "No external provider is used by this deterministic local adapter.",
        models: [{ id: "deterministic-v1", label: "Deterministic v1" }],
      },
      {
        id: "openai",
        label: "OpenAI Responses API",
        adapterVersion: "openai-responses/1",
        external: true,
        availability: "not_configured",
        dataRetentionPolicy: "unknown_verify_current_provider_terms",
        trainingPolicy: "unknown_verify_current_provider_terms",
        policyNotice:
          "Review the provider's current account and data-control terms before enabling this external adapter.",
        models: [],
      },
    ],
    limits: {
      maxFiles: 500,
      maxTotalBytes: 50_000_000,
      maxFileBytes: 5_000_000,
      maxPathBytes: 240,
      maxDepth: 12,
    },
  },
});

export const projectFixture = (
  revisionId = "revision-accepted-001",
): Project => ({
  id: "project-001",
  displayName: "Untitled landing page",
  revisionId,
  acceptedManifestSha256: revisionId.endsWith("002") ? HASH_B : HASH_A,
  status: "ready",
  previewUrl: `${ACCEPTED_PREVIEW_ORIGIN}/preview/project-001/${revisionId}/`,
  files: [
    { path: "index.html", byteLength: 1200 },
    { path: "styles.css", byteLength: 640 },
  ],
});

export const projectResponseFixture = (
  revisionId?: string,
): ProjectResponse => ({
  schemaVersion: "1",
  project: projectFixture(revisionId),
});

export const projectsResponseFixture = (
  projects: Project[] = [projectFixture()],
): ProjectsResponse => ({
  schemaVersion: "1",
  projects,
});

export const importPreviewResponseFixture: ImportPreviewResponse = {
  schemaVersion: "1",
  importPreview: {
    id: "import-preview-001",
    displayName: "Registered campaign LP",
    manifestSha256: HASH_C,
    totalBytes: 1840,
    entryPoint: "index.html",
    included: [
      { path: "index.html", byteLength: 1200, sha256: HASH_A },
      { path: "styles.css", byteLength: 640, sha256: HASH_B },
    ],
    excluded: [
      {
        path: "notes/draft.txt",
        reason: "静的LPの許可対象外です。",
      },
    ],
    warnings: ["外部リンクは取り込み後もネットワークを参照します。"],
  },
};

export const elementTargetFixture: ElementTargetV1 = {
  schemaVersion: 1,
  targetId: "target-001",
  captureRevisionId: "revision-accepted-001",
  captureSource: "accepted",
  pagePath: "index.html",
  kind: "element",
  label: "ヒーロー見出し",
  viewport: {
    cssWidth: 1440,
    cssHeight: 900,
    scrollX: 0,
    scrollY: 0,
    devicePixelRatio: 1,
    visualViewportScale: 1,
    previewScale: 1,
  },
  document: { cssWidth: 1440, cssHeight: 1800, layoutEpoch: 1 },
  elementAnchor: {
    tagName: "H1",
    uniqueElementId: "hero-heading",
    role: "heading",
    accessibleName: "まだ、白紙です。",
    domPath: "main:nth-child(1)>section:nth-child(1)>h1:nth-child(1)",
    siblingIndex: 0,
  },
};

export const targetResolutionFixture: Extract<
  TargetResolverResultV1,
  { status: "resolved" }
> = {
  schemaVersion: 1,
  resolverVersion: 1,
  targetId: "target-001",
  captureRevisionId: "revision-accepted-001",
  resolvedRevisionId: "revision-accepted-001",
  status: "resolved",
  selectedCandidateId: "candidate-hero-heading",
  candidates: [
    {
      candidateId: "candidate-hero-heading",
      score: 0.98,
      reasons: ["unique_id", "semantic_fingerprint"],
      summary: "Unique #hero-heading on index.html",
      elementAnchor: {
        tagName: "H1",
        uniqueElementId: "hero-heading",
        role: "heading",
        accessibleName: "まだ、白紙です。",
        domPath: "main:nth-child(1)>section:nth-child(1)>h1:nth-child(1)",
        siblingIndex: 0,
      },
    },
  ],
};

export const targetResponseFixture: TargetResponse = {
  schemaVersion: "1",
  target: elementTargetFixture,
  resolution: targetResolutionFixture,
  resolutionId: "resolution-001",
};

export const contextResponseFixture: ContextResponse = {
  schemaVersion: "1",
  context: {
    id: "context-001",
    revisionId: "revision-accepted-001",
    targetId: "target-001",
    targetResolutionId: "resolution-001",
    attemptId: "attempt-001",
    providerId: "fake",
    requestedModel: "deterministic-v1",
    provider: {
      providerId: "fake",
      adapterVersion: "fake-change-set/1",
      requestedModel: "deterministic-v1",
      external: false,
    },
    manifest: {
      entries: [
        {
          path: "index.html",
          mediaType: "text/html",
          purpose: "entrypoint",
          sourceByteLength: 1200,
          includedByteLength: 1200,
          startLine: 1,
          endLine: 42,
          sha256: HASH_A,
          estimatedTokens: 300,
          redacted: false,
          truncated: false,
          redactions: [],
        },
        {
          path: "styles.css",
          mediaType: "text/css",
          purpose: "dependency",
          sourceByteLength: 640,
          includedByteLength: 640,
          startLine: 1,
          endLine: 28,
          sha256: HASH_B,
          estimatedTokens: 160,
          redacted: false,
          truncated: false,
          redactions: [],
        },
      ],
      totalIncludedBytes: 1840,
      estimatedTokens: 460,
      screenshotIncluded: false,
    },
    instruction: "見出しを力強くしてください",
    canonicalJson:
      '{"instruction":"見出しを力強くしてください","target":{"elementId":"hero-heading"}}',
    sha256: HASH_B,
  },
};

export const allOperationsChangeSetFixture: ChangeSetV1 = {
  schema: "org.synapsegit-lp-studio.change-set",
  version: 1,
  baseRevisionId: "revision-accepted-001",
  summary: "すべてのChangeSet v1 operationを検証します",
  operations: [
    {
      op: "replace_text",
      path: "index.html",
      expectedSha256: HASH_A,
      mediaType: "text/html",
      content: "<!doctype html><title>Updated</title>",
    },
    {
      op: "create_text",
      path: "styles/hero.css",
      mediaType: "text/css",
      content: ".hero { color: navy; }",
    },
    {
      op: "rename",
      from: "styles.css",
      to: "styles/base.css",
      expectedSha256: HASH_B,
    },
    {
      op: "delete",
      path: "notes.txt",
      expectedSha256: HASH_C,
    },
  ],
};

const proposalChangeSetFixture: ChangeSetV1 = {
  schema: "org.synapsegit-lp-studio.change-set",
  version: 1,
  baseRevisionId: "revision-accepted-001",
  summary: "ヒーロー見出しを更新します",
  operations: [
    {
      op: "replace_text",
      path: "index.html",
      expectedSha256: HASH_A,
      mediaType: "text/html",
      content: "<!doctype html><h1>対話から、公開できるLPへ。</h1>",
    },
  ],
};

export const proposalResponseFixture: ProposalResponse = {
  schemaVersion: "1",
  proposal: {
    id: "proposal-001",
    reviewId: "review-001",
    baseRevisionId: "revision-accepted-001",
    status: "pending_review",
    summary: "ヒーロー見出しを更新します",
    artifactManifestSha256: HASH_C,
    reviewContextSha256: HASH_B,
    providerContextSha256: HASH_B,
    changeSetSha256: HASH_A,
    changeSet: proposalChangeSetFixture,
    attribution: {
      attemptId: "attempt-001",
      providerRequestId: "fake-request-001",
      providerId: "fake",
      adapterVersion: "fake-change-set/1",
      requestedModel: "deterministic-v1",
      reportedModel: "deterministic-v1",
      external: false,
      usage: { inputTokens: 460, outputTokens: 32, totalTokens: 492 },
    },
    sourceAttribution: "caller_supplied_ai_attributed",
    executionVerified: false,
    previewUrl: `${PROPOSED_PREVIEW_ORIGIN}/preview/project-001/proposal-001/`,
    changes: [{ path: "index.html", kind: "modified" }],
    unifiedDiff:
      "--- a/index.html\n+++ b/index.html\n-<h1>まだ、白紙です。</h1>\n+<h1>対話から、公開できるLPへ。</h1>",
    validation: {
      status: "passed",
      checks: [
        {
          id: "entrypoint",
          label: "Entry point",
          status: "passed",
          message: "index.htmlを確認しました。",
          blocking: false,
          destinations: [],
        },
      ],
    },
  },
};

export const approvalResponseFixture: ApprovalResponse = {
  schemaVersion: "1",
  approval: {
    token: "one-shot-approval-token",
    expiresAt: "2026-07-19T12:00:00Z",
    intentId: "intent-001",
  },
};

export const decisionResponseFixture: DecisionResponse = {
  schemaVersion: "1",
  decision: {
    reviewId: "review-001",
    proposalId: "proposal-001",
    disposition: "adopted_unchanged",
    status: "committed",
    revisionId: "revision-accepted-002",
    artifactManifestSha256: HASH_B,
  },
  project: projectFixture("revision-accepted-002"),
};

export const exportResponseFixture: ExportResponse = {
  schemaVersion: "1",
  export: {
    id: "export-001",
    revisionId: "revision-accepted-002",
    sha256: HASH_C,
    byteLength: 1840,
    downloadUrl: "/api/v1/exports/export-001/download",
  },
};

export const apiErrorResponseFixture: ApiErrorResponse = {
  schemaVersion: "1",
  error: {
    code: "revision_conflict",
    message: "Accepted revision changed.",
    requestId: "request-001",
    operationId: "operation-001",
    retryable: false,
    detail: {
      acceptedState: "unchanged",
      recoveryAction: "refresh",
    },
  },
};
