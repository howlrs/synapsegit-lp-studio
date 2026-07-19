import Ajv2020 from "ajv/dist/2020.js";
import {
  isExportResponse,
  isProjectResponse,
  isPublicationResponse,
  isReviewResponse,
  type ExportResponse,
  type ProjectResponse,
  type PublicationResponse,
  type ReviewResponse,
} from "@synapsegit-lp/contracts";
import apiSchema from "../../../../packages/contracts/schemas/api-v1.schema.json";
import {
  HASH_A,
  HASH_B,
  HASH_C,
  projectResponseFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

const HASH_D = "d".repeat(64);
const HASH_E = "e".repeat(64);

const resumedProposal = {
  ...proposalResponseFixture.proposal,
  target: targetResponseFixture.target,
  targetResolution: targetResponseFixture.resolution,
  instruction: "見出しを再調整してください",
  derivedFromProposalId: null,
};

const activeProjectResponse: ProjectResponse = {
  ...projectResponseFixture(),
  project: {
    ...projectResponseFixture().project,
    activeReview: {
      reviewId: "review-001",
      proposalId: "proposal-001",
      baseRevisionId: "revision-accepted-001",
      status: "reconciliation_required",
    },
    history: [
      {
        reviewId: "review-previous",
        proposalId: "proposal-previous",
        baseRevisionId: "revision-accepted-000",
        resultingRevisionId: "revision-accepted-001",
        disposition: "adopted_unchanged",
        artifactManifestSha256: HASH_B,
        baseArtifactManifestSha256: HASH_A,
        resultingAcceptedManifestSha256: HASH_C,
        decisionReceiptSha256: HASH_D,
        recordedAt: "2026-07-19T12:00:00Z",
        publicNote: null,
      },
    ],
  },
};

const reviewResponse: ReviewResponse = {
  schemaVersion: "1",
  review: {
    reviewId: "review-001",
    projectId: "project-001",
    proposalId: "proposal-001",
    status: "reconciliation_required",
    reconciliationRequired: true,
    proposal: resumedProposal,
    decision: null,
  },
};

const failedReviewResponse: ReviewResponse = {
  schemaVersion: "1",
  review: {
    reviewId: "review-001",
    projectId: "project-001",
    proposalId: "proposal-001",
    status: "failed",
    reconciliationRequired: false,
    proposal: resumedProposal,
    decision: null,
  },
};

const exportResponse: ExportResponse = {
  schemaVersion: "1",
  export: {
    id: "export-001",
    revisionId: "revision-accepted-001",
    sha256: HASH_A,
    byteLength: 140,
    downloadUrl: "/api/v1/exports/export-001/download",
    receiptSha256: HASH_B,
    sourceManifestSha256: HASH_C,
    generatedAtUtc: "2026-07-19T12:00:00Z",
    options: {
      entryPoint: "index.html",
      basePathProfile: "relative_static_http",
      externalAssetPolicy: "report",
      artifactFormat: "zip_stored_v1",
    },
    fileManifest: {
      schema: {
        name: "org.synapsegit-lp-studio.export-file-manifest",
        version: 1,
      },
      sha256: HASH_D,
      totalByteLength: 18,
      files: [
        {
          path: "index.html",
          mediaType: "text/html",
          byteLength: 18,
          sha256: HASH_E,
        },
      ],
    },
    validation: {
      profile: "offline_self_contained",
      localReferenceCount: 0,
      externalReferences: [],
      externalOrigins: [],
      dynamicReferenceSources: [],
      warnings: [],
      limitations: [
        {
          code: "ordinary_static_http_profile",
          message: "Direct file use is not guaranteed.",
        },
      ],
    },
    archive: { sha256: HASH_A, byteLength: 140 },
  },
};

const publicationText = "{}\n";
const publicationResponse: PublicationResponse = {
  schemaVersion: "1",
  publication: {
    id: "publication-001",
    revisionId: "revision-accepted-001",
    sha256: HASH_A,
    byteLength: 720,
    files: [
      {
        path: "projection.json",
        mediaType: "application/json",
        sha256: HASH_A,
        byteLength: 3,
        utf8: publicationText,
      },
      {
        path: "story.md",
        mediaType: "text/markdown",
        sha256: HASH_B,
        byteLength: 2,
        utf8: "#\n",
      },
      {
        path: "index.html",
        mediaType: "text/html",
        sha256: HASH_C,
        byteLength: 3,
        utf8: "<>\n",
      },
      {
        path: "manifest.json",
        mediaType: "application/json",
        sha256: HASH_D,
        byteLength: 3,
        utf8: publicationText,
      },
    ],
    downloadUrl: "/api/v1/publications/publication-001/download",
    networkWrites: false,
    remotePublication: "separate_human_action",
  },
};

describe("C7/C8 strict resume and publication contracts", () => {
  const ajv = new Ajv2020({ allErrors: true, strict: true });
  ajv.addKeyword({
    keyword: "x-maxUtf8Bytes",
    type: "string",
    schemaType: "number",
    validate: (limit: number, value: string) =>
      new TextEncoder().encode(value).byteLength <= limit,
  });
  ajv.addKeyword({
    keyword: "x-normalization",
    type: "string",
    schemaType: "string",
    validate: (form: string, value: string) =>
      form === "NFC" && value.normalize("NFC") === value,
  });
  const validateSchema = ajv.compile(apiSchema);

  it.each([
    ["project resume state", activeProjectResponse],
    ["review response", reviewResponse],
    ["failed review response", failedReviewResponse],
    ["detailed export receipt", exportResponse],
    ["publication response", publicationResponse],
    [
      "publication request",
      {
        schemaVersion: "1",
        revisionId: "revision-accepted-001",
        publicLabel: "Campaign",
        title: "Campaign record",
        summary: "Reviewed public summary",
      },
    ],
  ])("keeps %s aligned with the canonical JSON Schema", (_label, value) => {
    expect(validateSchema(value), JSON.stringify(validateSchema.errors)).toBe(
      true,
    );
  });

  it("accepts bound active review/history fields but rejects half manifest history", () => {
    expect(isProjectResponse(activeProjectResponse)).toBe(true);
    const failedActiveReview = structuredClone(activeProjectResponse);
    if (
      failedActiveReview.project.activeReview !== null &&
      failedActiveReview.project.activeReview !== undefined
    ) {
      failedActiveReview.project.activeReview.status = "failed";
    }
    expect(isProjectResponse(failedActiveReview)).toBe(true);
    expect(validateSchema(failedActiveReview)).toBe(true);
    const malformed = structuredClone(activeProjectResponse) as unknown as {
      project: { history: Record<string, unknown>[] };
    };
    delete malformed.project.history[0]?.resultingAcceptedManifestSha256;
    expect(isProjectResponse(malformed)).toBe(false);
  });

  it("requires complete persisted Proposal context and reconciliation semantics", () => {
    expect(isReviewResponse(reviewResponse)).toBe(true);
    const pending: ReviewResponse = {
      ...reviewResponse,
      review: {
        ...reviewResponse.review,
        status: "pending_review",
        reconciliationRequired: false,
      },
    };
    expect(isReviewResponse(pending)).toBe(true);
    expect(validateSchema(pending)).toBe(true);
    expect(
      isReviewResponse({
        ...reviewResponse,
        review: {
          ...reviewResponse.review,
          proposal: { ...resumedProposal, instruction: undefined },
        },
      }),
    ).toBe(false);
    expect(
      isReviewResponse({
        ...reviewResponse,
        review: {
          ...reviewResponse.review,
          reconciliationRequired: false,
        },
      }),
    ).toBe(false);
  });

  it("accepts a terminal denial only as failed with its Proposal retained and every action closed", () => {
    expect(isReviewResponse(failedReviewResponse)).toBe(true);
    expect(validateSchema(failedReviewResponse)).toBe(true);
    for (const review of [
      { ...failedReviewResponse.review, reconciliationRequired: true },
      { ...failedReviewResponse.review, proposal: null },
      {
        ...failedReviewResponse.review,
        decision: {
          proposalId: "proposal-001",
          disposition: "rejected",
          revisionId: "revision-accepted-001",
          artifactManifestSha256: HASH_A,
        },
      },
    ]) {
      const malformed = { schemaVersion: "1", review };
      expect(isReviewResponse(malformed)).toBe(false);
      expect(validateSchema(malformed)).toBe(false);
    }
  });

  it("accepts terminal review only when disposition and resulting manifest bind", () => {
    const terminal: ReviewResponse = {
      schemaVersion: "1",
      review: {
        reviewId: "review-001",
        projectId: "project-001",
        proposalId: "proposal-001",
        status: "adopted",
        reconciliationRequired: false,
        proposal: null,
        decision: {
          proposalId: "proposal-001",
          disposition: "adopted_unchanged",
          revisionId: "revision-accepted-002",
          artifactManifestSha256: HASH_C,
        },
      },
    };
    expect(isReviewResponse(terminal)).toBe(true);
    expect(validateSchema(terminal)).toBe(true);
    const mismatched = {
      ...terminal,
      review: {
        ...terminal.review,
        status: "rejected" as const,
      },
    };
    expect(isReviewResponse(mismatched)).toBe(false);
    expect(validateSchema(mismatched)).toBe(false);
  });

  it("accepts all detailed export fields together and rejects partial or mismatched archives", () => {
    expect(isExportResponse(exportResponse)).toBe(true);
    const partial = structuredClone(exportResponse) as unknown as {
      export: Record<string, unknown>;
    };
    delete partial.export.validation;
    expect(isExportResponse(partial)).toBe(false);
    expect(
      isExportResponse({
        ...exportResponse,
        export: {
          ...exportResponse.export,
          archive: { sha256: HASH_B, byteLength: 140 },
        },
      }),
    ).toBe(false);
  });

  it("accepts exact UTF-8 publication files and rejects byte drift or remote-write claims", () => {
    expect(isPublicationResponse(publicationResponse)).toBe(true);
    expect(
      isPublicationResponse({
        ...publicationResponse,
        publication: {
          ...publicationResponse.publication,
          networkWrites: true,
        },
      }),
    ).toBe(false);
    expect(
      isPublicationResponse({
        ...publicationResponse,
        publication: {
          ...publicationResponse.publication,
          files: publicationResponse.publication.files.map((file, index) =>
            index === 0 ? { ...file, byteLength: 999 } : file,
          ),
        },
      }),
    ).toBe(false);
  });
});
