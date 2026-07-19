import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { App } from "../App";
import {
  HASH_C,
  bootstrapFixture,
  contextResponseFixture,
  importPreviewResponseFixture,
  projectResponseFixture,
  projectsResponseFixture,
  proposalResponseFixture,
  targetResponseFixture,
} from "./fixtures";

const jsonResponse = (value: unknown): Response =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });

describe("C2 browser vertical slice", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("creates, targets, reviews exact context, adopts, and refreshes Accepted state", async () => {
    const requestLog: string[] = [];
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        const method = init?.method ?? "GET";
        requestLog.push(`${method} ${path}`);

        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if (method === "GET" && path === "/api/v1/projects") {
          return jsonResponse(projectsResponseFixture([]));
        }
        if (method === "POST" && path === "/api/v1/projects") {
          return jsonResponse(projectResponseFixture());
        }
        if (path.endsWith("/targets")) {
          return jsonResponse(targetResponseFixture);
        }
        if (path.endsWith("/contexts")) {
          return jsonResponse(contextResponseFixture);
        }
        if (path.endsWith("/proposals")) {
          return jsonResponse({
            ...proposalResponseFixture,
            proposal: {
              ...proposalResponseFixture.proposal,
              unifiedDiff:
                "--- a/index.html\n+++ b/index.html\n-<h1>まだ、白紙です。</h1>\n+<h1>対話から、公開できるLPへ。</h1>\n+<img onerror=alert(1)>",
            },
          });
        }
        if (path.endsWith("/approvals")) {
          return jsonResponse({
            schemaVersion: "1",
            approval: {
              token: "one-shot-host-approval",
              expiresAt: "2026-07-19T12:01:00Z",
              intentId: "11111111-2222-4333-8444-555555555555",
            },
          });
        }
        if (path.endsWith("/decisions")) {
          return jsonResponse({
            schemaVersion: "1",
            decision: {
              reviewId: "review-001",
              proposalId: "proposal-001",
              disposition: "adopted_unchanged",
              status: "committed",
              revisionId: "revision-accepted-002",
              artifactManifestSha256: HASH_C,
            },
            project: projectResponseFixture("revision-accepted-002").project,
          });
        }
        if (method === "GET" && path === "/api/v1/projects/project-001") {
          return jsonResponse(projectResponseFixture("revision-accepted-002"));
        }
        throw new Error(`Unexpected request: ${method} ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const createButton = await screen.findByRole("button", {
      name: "空のLPを作成",
    });
    await waitFor(() => expect(createButton).toBeEnabled());
    fireEvent.click(createButton);
    const acceptedRevision = await screen.findByLabelText("Accepted revision");
    const beforeDecision = acceptedRevision.textContent;

    const mobile = screen.getByRole("button", { name: "モバイル" });
    fireEvent.click(mobile);
    expect(mobile).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTitle("LPプレビュー")).toHaveAttribute(
      "referrerpolicy",
      "no-referrer",
    );

    fireEvent.click(screen.getByRole("button", { name: "ヒーロー見出し" }));
    expect(await screen.findByText("#hero-heading")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "送信内容を確認" }));
    const dialog = await screen.findByRole("dialog", {
      name: "送信内容を確認",
    });
    expect(
      within(dialog).getByText(contextResponseFixture.context.canonicalJson),
    ).toBeInTheDocument();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "変更案を作成" }),
    );

    const review = await screen.findByRole("heading", { name: "変更案を確認" });
    const drawer = review.closest("section");
    expect(drawer).not.toBeNull();
    expect(
      within(drawer as HTMLElement).getByText("caller-supplied"),
    ).toBeInTheDocument();
    expect(
      within(drawer as HTMLElement).getByText(/execution未検証/),
    ).toBeInTheDocument();
    expect(
      within(drawer as HTMLElement).getByText(/<img onerror/),
    ).toBeInTheDocument();
    expect((drawer as HTMLElement).querySelector("img")).toBeNull();

    fireEvent.click(
      within(drawer as HTMLElement).getByRole("button", { name: "変更を採用" }),
    );
    await waitFor(() => {
      expect(screen.getByLabelText("Accepted revision").textContent).not.toBe(
        beforeDecision,
      );
    });
    expect(
      screen.queryByRole("heading", { name: "変更案を確認" }),
    ).not.toBeInTheDocument();

    const approvalIndex = requestLog.findIndex((entry) =>
      entry.endsWith("/approvals"),
    );
    const decisionIndex = requestLog.findIndex((entry) =>
      entry.endsWith("/decisions"),
    );
    const refreshIndex = requestLog.lastIndexOf(
      "GET /api/v1/projects/project-001",
    );
    expect(approvalIndex).toBeGreaterThan(-1);
    expect(decisionIndex).toBeGreaterThan(approvalIndex);
    expect(refreshIndex).toBeGreaterThan(decisionIndex);

    for (const [input, init] of fetchMock.mock.calls.slice(1)) {
      expect(String(input)).toMatch(/^\/api\/v1\//);
      expect(new Headers(init?.headers).get("Authorization")).toBe(
        "Bearer secret-only-in-api-closure",
      );
      expect(init?.credentials).toBe("omit");
    }
  });

  it("lists retained projects and reopens the selected Accepted state", async () => {
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
        if (method === "GET" && path === "/api/v1/projects") {
          return jsonResponse(projectsResponseFixture());
        }
        if (method === "GET" && path === "/api/v1/projects/project-001") {
          return jsonResponse(projectResponseFixture());
        }
        throw new Error(`Unexpected request: ${method} ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const openButton = await screen.findByRole("button", {
      name: "Untitled landing pageを開く",
    });
    fireEvent.click(openButton);

    expect(await screen.findByLabelText("Accepted revision")).toHaveTextContent(
      "revision-a…ed-001",
    );
    expect(screen.getByText("Untitled landing page")).toBeInTheDocument();
  });

  it("reviews an exact registered-root copy before explicit import", async () => {
    const requestBodies: unknown[] = [];
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        const method = init?.method ?? "GET";
        if (init?.body !== undefined) {
          requestBodies.push(JSON.parse(String(init.body)) as unknown);
        }
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if (method === "GET" && path === "/api/v1/projects") {
          return jsonResponse(projectsResponseFixture([]));
        }
        if (method === "POST" && path === "/api/v1/imports/previews") {
          return jsonResponse(importPreviewResponseFixture);
        }
        if (
          method === "POST" &&
          path === "/api/v1/imports/import-preview-001/confirm"
        ) {
          return jsonResponse(projectResponseFixture());
        }
        throw new Error(`Unexpected request: ${method} ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    const importButton = await screen.findByRole("button", {
      name: "登録済みディレクトリを取り込む",
    });
    await waitFor(() => expect(importButton).toBeEnabled());
    fireEvent.click(importButton);

    const dialog = await screen.findByRole("dialog", {
      name: "取り込むファイルを確認",
    });
    expect(within(dialog).getAllByText("index.html")).toHaveLength(2);
    expect(within(dialog).getByText("styles.css")).toBeInTheDocument();
    expect(within(dialog).getByText("notes/draft.txt")).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        importPreviewResponseFixture.importPreview.manifestSha256,
      ),
    ).toBeInTheDocument();
    expect(dialog).toHaveTextContent("取り込み元のファイルは変更しません");
    expect(dialog.textContent).not.toMatch(/(?:\/home\/|\/tmp\/|[A-Za-z]:\\)/);

    fireEvent.click(
      within(dialog).getByRole("button", {
        name: "この内容をコピーして取り込む",
      }),
    );
    expect(await screen.findByLabelText("Accepted revision")).toBeVisible();
    expect(requestBodies).toContainEqual({ schemaVersion: "1" });
    expect(requestBodies).toContainEqual({
      schemaVersion: "1",
      expectedManifestSha256:
        importPreviewResponseFixture.importPreview.manifestSha256,
    });
    expect(JSON.stringify(requestBodies)).not.toContain("/home/");
  });

  it("blocks import when the registered root has no root index.html", async () => {
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
        if (method === "GET" && path === "/api/v1/projects") {
          return jsonResponse(projectsResponseFixture([]));
        }
        if (path === "/api/v1/imports/previews") {
          return jsonResponse({
            ...importPreviewResponseFixture,
            importPreview: {
              ...importPreviewResponseFixture.importPreview,
              entryPoint: null,
            },
          });
        }
        throw new Error(`Unexpected request: ${method} ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    const importButton = await screen.findByRole("button", {
      name: "登録済みディレクトリを取り込む",
    });
    await waitFor(() => expect(importButton).toBeEnabled());
    fireEvent.click(importButton);

    const dialog = await screen.findByRole("dialog", {
      name: "取り込むファイルを確認",
    });
    expect(within(dialog).getByText(/ルート直下の index.html/)).toBeVisible();
    expect(
      within(dialog).getByRole("button", {
        name: "この内容をコピーして取り込む",
      }),
    ).toBeDisabled();
  });
});
