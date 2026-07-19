import type {
  PublicationResponse,
  ReviewResponse,
} from "@synapsegit-lp/contracts";
import { bootstrapApi } from "../api/client";
import {
  HASH_A,
  HASH_B,
  HASH_C,
  bootstrapFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

const jsonResponse = (value: unknown, status = 200): Response =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });

const resumedProposal = {
  ...proposalResponseFixture.proposal,
  target: targetResponseFixture.target,
  targetResolution: targetResponseFixture.resolution,
  instruction: "保存済みReviewの要望",
  derivedFromProposalId: null,
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

const publicationResponse: PublicationResponse = {
  schemaVersion: "1",
  publication: {
    id: "publication-001",
    revisionId: "revision-accepted-001",
    sha256: HASH_A,
    byteLength: 512,
    files: [
      {
        path: "projection.json",
        mediaType: "application/json",
        sha256: HASH_A,
        byteLength: 3,
        utf8: "{}\n",
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
        sha256: HASH_A,
        byteLength: 3,
        utf8: "{}\n",
      },
    ],
    downloadUrl: "/api/v1/publications/publication-001/download",
    networkWrites: false,
    remotePublication: "separate_human_action",
  },
};

describe("C7/C8 authenticated API client", () => {
  it("restores with GET and reconciles only through the authenticated POST operation", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        if (path === "/api/v1/reviews/review-001") {
          expect(init?.method).toBe("GET");
          return jsonResponse(reviewResponse);
        }
        if (path === "/api/v1/operations/reviews/review-001/reconcile") {
          expect(init?.method).toBe("POST");
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
          });
          return jsonResponse(reviewResponse);
        }
        throw new Error("Unexpected request " + path);
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.getReview("review-001")).resolves.toMatchObject({
      status: "reconciliation_required",
    });
    await expect(
      session.api.reconcileReview("review-001"),
    ).resolves.toMatchObject({ reconciliationRequired: true });

    for (const [, init] of fetchMock.mock.calls.slice(1)) {
      expect(new Headers(init?.headers).get("Authorization")).toBe(
        "Bearer secret-only-in-api-closure",
      );
      expect(init?.credentials).toBe("omit");
    }
  });

  it("generates a local publication draft, then downloads only its allowed URL", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        if (path === "/api/v1/projects/project-001/publications") {
          expect(init?.method).toBe("POST");
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
            revisionId: "revision-accepted-001",
            publicLabel: "Campaign LP",
            title: "Campaign",
            summary: "Reviewed public summary",
            publicDecisionNote: "Approved for this record",
          });
          return jsonResponse(publicationResponse, 201);
        }
        if (
          path ===
          "http://editor.test/api/v1/publications/publication-001/download"
        ) {
          expect(init?.method).toBe("GET");
          expect(new Headers(init?.headers).get("Accept")).toBe(
            "application/zip",
          );
          return new Response(new Blob(["zip"]), { status: 200 });
        }
        throw new Error("Unexpected request " + path);
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    const draft = await session.api.createPublication({
      projectId: "project-001",
      revisionId: "revision-accepted-001",
      publicLabel: "Campaign LP",
      title: "Campaign",
      summary: "Reviewed public summary",
      publicDecisionNote: "Approved for this record",
    });
    await expect(
      session.api.downloadPublication(draft.downloadUrl),
    ).resolves.toBeInstanceOf(Blob);
    expect(
      new Headers(fetchMock.mock.calls[2]?.[1]?.headers).get("Authorization"),
    ).toBe("Bearer secret-only-in-api-closure");

    await expect(
      session.api.downloadPublication("/api/v1/exports/export-001/download"),
    ).rejects.toMatchObject({ code: "invalid_download_origin" });
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });
});
