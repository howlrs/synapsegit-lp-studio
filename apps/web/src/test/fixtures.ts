import type {
  ApiErrorResponse,
  ApprovalResponse,
  BootstrapResponse,
  ContextResponse,
  DecisionResponse,
  ExportResponse,
  Project,
  ProjectResponse,
  ProposalResponse,
  TargetResponse,
} from "@synapsegit-lp/contracts";

export const HASH_A = "a".repeat(64);
export const HASH_B = "b".repeat(64);
export const HASH_C = "c".repeat(64);

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
  previewOrigin: "https://preview.test",
  capabilities: {
    targetKinds: ["element"],
    dispositions: ["adopted_unchanged", "rejected", "deferred"],
    singleProposalPerProject: true,
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
  previewUrl: `https://preview.test/projects/project-001/${revisionId}/index.html`,
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

export const targetResponseFixture: TargetResponse = {
  schemaVersion: "1",
  target: {
    id: "target-001",
    revisionId: "revision-accepted-001",
    kind: "element",
    elementId: "hero-heading",
    label: "ヒーロー見出し",
  },
};

export const contextResponseFixture: ContextResponse = {
  schemaVersion: "1",
  context: {
    id: "context-001",
    revisionId: "revision-accepted-001",
    targetId: "target-001",
    instruction: "見出しを力強くしてください",
    canonicalJson:
      '{"instruction":"見出しを力強くしてください","target":{"elementId":"hero-heading"}}',
    sha256: HASH_B,
  },
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
    sourceAttribution: "caller_supplied_ai_attributed",
    executionVerified: false,
    previewUrl:
      "https://preview.test/projects/project-001/proposals/proposal-001/index.html",
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
    retryable: false,
  },
};
