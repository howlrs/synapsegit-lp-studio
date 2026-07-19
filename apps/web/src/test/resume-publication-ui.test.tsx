import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import type {
  ExportResponse,
  Project,
  PublicationResponse,
  ReviewResponse,
} from "@synapsegit-lp/contracts";
import { App } from "../App";
import {
  HASH_A,
  HASH_B,
  HASH_C,
  bootstrapFixture,
  projectFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

const HASH_D = "d".repeat(64);
const HASH_E = "e".repeat(64);

const jsonResponse = (value: unknown, status = 200): Response =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });

const historyEntry = {
  reviewId: "review-previous",
  proposalId: "proposal-previous",
  baseRevisionId: "revision-accepted-000",
  resultingRevisionId: "revision-accepted-001",
  disposition: "adopted_unchanged" as const,
  artifactManifestSha256: HASH_A,
  baseArtifactManifestSha256: HASH_A,
  resultingAcceptedManifestSha256: HASH_A,
  decisionReceiptSha256: HASH_D,
  recordedAt: "2026-07-19T12:00:00Z",
  publicNote: null,
};

const activeProject: Project = {
  ...projectFixture(),
  activeReview: {
    reviewId: "review-001",
    proposalId: "proposal-001",
    baseRevisionId: "revision-accepted-001",
    status: "reconciliation_required",
  },
  history: [historyEntry],
};

const resumedProposal = {
  ...proposalResponseFixture.proposal,
  target: targetResponseFixture.target,
  targetResolution: targetResponseFixture.resolution,
  instruction: "保存済みの見出し変更を確認してください",
  derivedFromProposalId: null,
};

const reconciliationReview: ReviewResponse = {
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

const failedProject: Project = {
  ...activeProject,
  activeReview: {
    reviewId: "review-001",
    proposalId: "proposal-001",
    baseRevisionId: "revision-accepted-001",
    status: "failed",
  },
};

const failedReview: ReviewResponse = {
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

const terminalReview: ReviewResponse = {
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
      artifactManifestSha256: HASH_B,
    },
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
        byteLength: 8,
        utf8: "# Story\n",
      },
      {
        path: "index.html",
        mediaType: "text/html",
        sha256: HASH_C,
        byteLength: 15,
        utf8: "<h1>Story</h1>\n",
      },
      {
        path: "manifest.json",
        mediaType: "application/json",
        sha256: HASH_D,
        byteLength: 3,
        utf8: "{}\n",
      },
    ],
    downloadUrl: "/api/v1/publications/publication-001/download",
    networkWrites: false,
    remotePublication: "separate_human_action",
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

describe("C7/C8 restart and local publication UI", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("restores a persisted review with GET and uses explicit POST reconciliation", async () => {
    let reconciled = false;
    const finalProject: Project = {
      ...projectFixture("revision-accepted-002"),
      activeReview: null,
      history: [
        historyEntry,
        {
          reviewId: "review-001",
          proposalId: "proposal-001",
          baseRevisionId: "revision-accepted-001",
          resultingRevisionId: "revision-accepted-002",
          disposition: "adopted_unchanged",
          artifactManifestSha256: HASH_B,
          baseArtifactManifestSha256: HASH_A,
          resultingAcceptedManifestSha256: HASH_B,
          decisionReceiptSha256: HASH_C,
          recordedAt: "2026-07-19T12:05:00Z",
          publicNote: null,
        },
      ],
    };
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        const method = init?.method ?? "GET";
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if (path === "/api/v1/projects") {
          return jsonResponse({
            schemaVersion: "1",
            projects: [activeProject],
          });
        }
        if (path === "/api/v1/projects/project-001") {
          return jsonResponse({
            schemaVersion: "1",
            project: reconciled ? finalProject : activeProject,
          });
        }
        if (path === "/api/v1/reviews/review-001") {
          expect(method).toBe("GET");
          return jsonResponse(reconciliationReview);
        }
        if (path === "/api/v1/operations/reviews/review-001/reconcile") {
          expect(method).toBe("POST");
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
          });
          reconciled = true;
          return jsonResponse(terminalReview);
        }
        throw new Error("Unexpected request " + method + " " + path);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Untitled landing pageを開く",
      }),
    );

    const reviewHeading = await screen.findByRole("heading", {
      name: "変更案を確認",
    });
    const drawer = reviewHeading.closest("section");
    expect(drawer).not.toBeNull();
    expect(
      within(drawer as HTMLElement).getByText(
        "保存済みの見出し変更を確認してください",
      ),
    ).toBeInTheDocument();
    expect(
      within(drawer as HTMLElement).getByRole("button", {
        name: "変更を採用",
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("heading", { name: "Review history" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "GitHub-ready記録" }),
    ).toBeDisabled();

    fireEvent.click(
      within(drawer as HTMLElement).getByRole("button", {
        name: "Decision結果を再照合",
      }),
    );

    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: "変更案を確認" }),
      ).not.toBeInTheDocument(),
    );
    expect(screen.getByLabelText("Accepted revision")).toHaveTextContent(
      "revision-a…ed-002",
    );
    expect(
      fetchMock.mock.calls.some(
        ([path, init]) =>
          String(path).includes("/operations/reviews/") &&
          init?.method === "POST",
      ),
    ).toBe(true);
  });

  it("renders a terminal denial as a permanent failure with reconciliation and Decisions disabled", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        const method = init?.method ?? "GET";
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if (path === "/api/v1/projects") {
          return jsonResponse({
            schemaVersion: "1",
            projects: [failedProject],
          });
        }
        if (path === "/api/v1/projects/project-001") {
          return jsonResponse({ schemaVersion: "1", project: failedProject });
        }
        if (path === "/api/v1/reviews/review-001") {
          expect(method).toBe("GET");
          return jsonResponse(failedReview);
        }
        throw new Error("Unexpected request " + method + " " + path);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Untitled landing pageを開く",
      }),
    );

    const failureHeading = await screen.findByRole("heading", {
      name: "変更案の処理に失敗",
    });
    const drawer = failureHeading.closest("section");
    expect(drawer).not.toBeNull();
    expect(
      within(drawer as HTMLElement).getByText(/永続的な失敗として確定/),
    ).toBeInTheDocument();
    for (const name of [
      "変更を採用",
      "変更案を却下",
      "今回は保留",
      "Decision結果を再照合",
    ]) {
      expect(
        within(drawer as HTMLElement).getByRole("button", { name }),
      ).toBeDisabled();
    }
    expect(
      screen.getByText("永続的失敗（再照合・Decision不可）"),
    ).toBeInTheDocument();
    expect(
      fetchMock.mock.calls.some(([path]) =>
        String(path).includes("/operations/reviews/"),
      ),
    ).toBe(false);
  });

  it("shows the detailed static-export receipt after the local ZIP download", async () => {
    const retainedProject: Project = {
      ...projectFixture(),
      activeReview: null,
      history: [historyEntry],
    };
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if (path === "/api/v1/projects") {
          return jsonResponse({
            schemaVersion: "1",
            projects: [retainedProject],
          });
        }
        if (path === "/api/v1/projects/project-001") {
          return jsonResponse({
            schemaVersion: "1",
            project: retainedProject,
          });
        }
        if (path === "/api/v1/projects/project-001/exports") {
          expect(init?.method).toBe("POST");
          return jsonResponse(exportResponse, 201);
        }
        if (path.endsWith("/api/v1/exports/export-001/download")) {
          expect(init?.method ?? "GET").toBe("GET");
          return new Response(new Blob(["zip"]), { status: 200 });
        }
        throw new Error("Unexpected request " + path);
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    const objectUrl = vi.fn(() => "blob:export");
    const revokeObjectUrl = vi.fn();
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: objectUrl,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revokeObjectUrl,
    });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Untitled landing pageを開く",
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Acceptedをエクスポート",
      }),
    );

    expect(
      await screen.findByRole("heading", {
        name: "Accepted export receipt",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("offline self-contained（bounded検査）"),
    ).toBeInTheDocument();
    expect(screen.getByText(HASH_B)).toBeInTheDocument();
    expect(screen.getByText(HASH_C)).toBeInTheDocument();
    expect(
      screen.getByText("ordinary_static_http_profile"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Generated manifest (1)" }),
    ).toBeInTheDocument();
    expect(objectUrl).toHaveBeenCalledTimes(1);
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:export");
    expect(click).toHaveBeenCalledTimes(1);
  });

  it("separates local generation, exact-byte review, and explicit ZIP download", async () => {
    const retainedProject: Project = {
      ...projectFixture(),
      activeReview: null,
      history: [historyEntry],
    };
    const downloadRequests: string[] = [];
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if (path === "/api/v1/projects") {
          return jsonResponse({
            schemaVersion: "1",
            projects: [retainedProject],
          });
        }
        if (path === "/api/v1/projects/project-001") {
          return jsonResponse({
            schemaVersion: "1",
            project: retainedProject,
          });
        }
        if (path === "/api/v1/projects/project-001/publications") {
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
            revisionId: "revision-accepted-001",
            publicLabel: "Untitled landing page",
            title: "Untitled landing page",
            summary: "公開用の要約",
            publicDecisionNote: "公開用Decision note",
          });
          return jsonResponse(publicationResponse, 201);
        }
        if (
          path.includes("/api/v1/publications/") &&
          path.endsWith("/download")
        ) {
          downloadRequests.push(path);
          return new Response(new Blob(["zip"]), { status: 200 });
        }
        throw new Error("Unexpected request " + path);
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    const objectUrl = vi.fn(() => "blob:publication");
    const revokeObjectUrl = vi.fn();
    Object.defineProperty(URL, "createObjectURL", {
      configurable: true,
      value: objectUrl,
    });
    Object.defineProperty(URL, "revokeObjectURL", {
      configurable: true,
      value: revokeObjectUrl,
    });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Untitled landing pageを開く",
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "GitHub-ready記録" }),
    );
    fireEvent.change(screen.getByLabelText("Public summary"), {
      target: { value: "公開用の要約" },
    });
    fireEvent.change(
      screen.getByLabelText(
        "Public decision note（任意・private rationaleとは別）",
      ),
      { target: { value: "公開用Decision note" } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "GitHub-ready filesを生成" }),
    );

    expect(
      await screen.findByRole("heading", {
        name: "生成bytesを確認（4 files）",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("Network writes: none")).toBeInTheDocument();
    expect(
      screen.getByText("GitHubへ公開: 別Human操作（この画面では実行しません）"),
    ).toBeInTheDocument();
    expect(downloadRequests).toHaveLength(0);

    fireEvent.click(
      screen.getByRole("button", {
        name: "確認したZIPをダウンロード",
      }),
    );
    await waitFor(() => expect(downloadRequests).toHaveLength(1));
    expect(objectUrl).toHaveBeenCalledTimes(1);
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:publication");
    expect(click).toHaveBeenCalledTimes(1);
  });
});
