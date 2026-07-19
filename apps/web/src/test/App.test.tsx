import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import type { ProposalResponse } from "@synapsegit-lp/contracts";
import { App } from "../App";
import {
  ACCEPTED_PREVIEW_ORIGIN,
  HASH_C,
  PROPOSED_PREVIEW_ORIGIN,
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

const proposalResponseForAttempt = (attemptId: string): ProposalResponse => ({
  ...proposalResponseFixture,
  proposal: {
    ...proposalResponseFixture.proposal,
    attribution: {
      ...proposalResponseFixture.proposal.attribution,
      attemptId,
    },
  },
});

const proposalBindingMismatchCases: Array<
  [string, (response: ProposalResponse) => ProposalResponse]
> = [
  [
    "Accepted base revision",
    (response) => ({
      ...response,
      proposal: {
        ...response.proposal,
        baseRevisionId: "revision-foreign",
        changeSet: {
          ...response.proposal.changeSet,
          baseRevisionId: "revision-foreign",
        },
      },
    }),
  ],
  [
    "reviewed context digest",
    (response) => ({
      ...response,
      proposal: { ...response.proposal, providerContextSha256: HASH_C },
    }),
  ],
  [
    "attempt",
    (response) => ({
      ...response,
      proposal: {
        ...response.proposal,
        attribution: {
          ...response.proposal.attribution,
          attemptId: "attempt-foreign",
        },
      },
    }),
  ],
  [
    "provider",
    (response) => ({
      ...response,
      proposal: {
        ...response.proposal,
        attribution: {
          ...response.proposal.attribution,
          providerId: "openai",
        },
      },
    }),
  ],
  [
    "requested model",
    (response) => ({
      ...response,
      proposal: {
        ...response.proposal,
        attribution: {
          ...response.proposal.attribution,
          requestedModel: "other-model",
        },
      },
    }),
  ],
  [
    "adapter version",
    (response) => ({
      ...response,
      proposal: {
        ...response.proposal,
        attribution: {
          ...response.proposal.attribution,
          adapterVersion: "other-adapter/1",
        },
      },
    }),
  ],
  [
    "external-provider flag",
    (response) => ({
      ...response,
      proposal: {
        ...response.proposal,
        attribution: { ...response.proposal.attribution, external: true },
      },
    }),
  ],
];

const openRetainedProjectAndSelectTarget = async (): Promise<void> => {
  const openButton = await screen.findByRole("button", {
    name: "Untitled landing pageを開く",
  });
  fireEvent.click(openButton);
  const frame = (await screen.findByTitle("LPプレビュー")) as HTMLIFrameElement;
  await waitFor(() => expect(frame).toHaveAttribute("aria-busy", "false"));
  fireEvent.load(frame);
  fireEvent(
    window,
    new MessageEvent("message", {
      origin: ACCEPTED_PREVIEW_ORIGIN,
      source: frame.contentWindow,
      data: {
        type: "synapsegit-lp.target-draft",
        schemaVersion: "1",
        channelId: "11111111-2222-4333-8444-555555555555",
        projectId: "project-001",
        snapshotId: "revision-accepted-001",
        revisionId: "revision-accepted-001",
        target: targetResponseFixture.target,
      },
    }),
  );
  expect(await screen.findByText("#hero-heading")).toBeInTheDocument();
};

describe("C2 browser vertical slice", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("creates, targets, reviews exact context, adopts, and refreshes Accepted state", async () => {
    const requestLog: string[] = [];
    const requestBodies: unknown[] = [];
    let reviewedAttemptId = "";
    let resolveProposalResponse: ((response: Response) => void) | undefined;
    const deferredProposalResponse = new Promise<Response>((resolve) => {
      resolveProposalResponse = resolve;
    });
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        const method = init?.method ?? "GET";
        requestLog.push(`${method} ${path}`);
        if (init?.body !== undefined) {
          requestBodies.push(JSON.parse(String(init.body)) as unknown);
        }

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
          const request = JSON.parse(String(init?.body)) as {
            attemptId: string;
            instruction: string;
            providerId: string;
            requestedModel: string;
          };
          reviewedAttemptId = request.attemptId;
          return jsonResponse({
            ...contextResponseFixture,
            context: {
              ...contextResponseFixture.context,
              attemptId: request.attemptId,
              providerId: request.providerId,
              requestedModel: request.requestedModel,
              instruction: request.instruction,
              provider: {
                ...contextResponseFixture.context.provider,
                providerId: request.providerId,
                requestedModel: request.requestedModel,
              },
            },
          });
        }
        if (path.endsWith("/proposals")) {
          return deferredProposalResponse;
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
    expect(screen.getByTitle("LPプレビュー")).toHaveAttribute(
      "sandbox",
      "allow-scripts allow-same-origin",
    );
    expect(screen.getByTitle("LPプレビュー")).toHaveAttribute(
      "src",
      `${ACCEPTED_PREVIEW_ORIGIN}/preview/project-001/revision-accepted-001/`,
    );

    const acceptedFrameForTarget = screen.getByTitle(
      "LPプレビュー",
    ) as HTMLIFrameElement;
    fireEvent(
      window,
      new MessageEvent("message", {
        origin: ACCEPTED_PREVIEW_ORIGIN,
        source: acceptedFrameForTarget.contentWindow,
        data: {
          type: "synapsegit-lp.structure",
          schemaVersion: "1",
          channelId: "11111111-2222-4333-8444-555555555555",
          projectId: "project-001",
          snapshotId: "revision-accepted-001",
          revisionId: "revision-accepted-001",
          nodes: [
            {
              runtimeNodeHandle: "node-hero-heading",
              kind: "element",
              label: "ヒーロー見出し",
              tagName: "h1",
              depth: 1,
            },
          ],
        },
      }),
    );
    const dynamicHeading = await screen.findByRole("button", {
      name: /ヒーロー見出し/,
    });
    fireEvent.click(dynamicHeading);
    fireEvent(
      window,
      new MessageEvent("message", {
        origin: ACCEPTED_PREVIEW_ORIGIN,
        source: acceptedFrameForTarget.contentWindow,
        data: {
          type: "synapsegit-lp.target-draft",
          schemaVersion: "1",
          channelId: "11111111-2222-4333-8444-555555555555",
          projectId: "project-001",
          snapshotId: "revision-accepted-001",
          revisionId: "revision-accepted-001",
          target: targetResponseFixture.target,
        },
      }),
    );
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
    fireEvent.keyDown(window, { key: "Escape" });
    expect(dialog).toBeInTheDocument();
    resolveProposalResponse?.(
      jsonResponse({
        ...proposalResponseForAttempt(reviewedAttemptId),
        proposal: {
          ...proposalResponseForAttempt(reviewedAttemptId).proposal,
          unifiedDiff:
            "--- a/index.html\n+++ b/index.html\n-<h1>まだ、白紙です。</h1>\n+<h1>対話から、公開できるLPへ。</h1>\n+<img onerror=alert(1)>",
        },
      }),
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
    expect(drawer).toHaveTextContent("input 460");
    expect(drawer).toHaveTextContent("output 32");
    expect(drawer).toHaveTextContent("total 492 tokens");
    expect(drawer).toHaveTextContent(
      "見出しを、未来への期待が伝わる表現にしてください",
    );
    expect(
      within(drawer as HTMLElement).getByText(/<img onerror/),
    ).toBeInTheDocument();
    expect((drawer as HTMLElement).querySelector("img")).toBeNull();

    const proposedFrame = screen.getByTitle(
      "LPプレビュー",
    ) as HTMLIFrameElement;
    expect(proposedFrame).toHaveAttribute(
      "src",
      `${PROPOSED_PREVIEW_ORIGIN}/preview/project-001/proposal-001/`,
    );
    const proposedPostMessage = vi.spyOn(
      proposedFrame.contentWindow as Window,
      "postMessage",
    );
    fireEvent.click(screen.getByRole("button", { name: "操作モード" }));
    await waitFor(() =>
      expect(proposedPostMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "synapsegit-lp.action",
          snapshotId: "proposal-001",
        }),
        PROPOSED_PREVIEW_ORIGIN,
      ),
    );

    fireEvent.click(screen.getByRole("button", { name: "Accepted" }));
    const acceptedFrame = screen.getByTitle(
      "LPプレビュー",
    ) as HTMLIFrameElement;
    expect(acceptedFrame).toHaveAttribute(
      "src",
      `${ACCEPTED_PREVIEW_ORIGIN}/preview/project-001/revision-accepted-001/`,
    );
    const acceptedPostMessage = vi.spyOn(
      acceptedFrame.contentWindow as Window,
      "postMessage",
    );
    fireEvent.click(screen.getByRole("button", { name: "選択モード" }));
    await waitFor(() =>
      expect(acceptedPostMessage).toHaveBeenCalledWith(
        expect.objectContaining({
          type: "synapsegit-lp.action",
          snapshotId: "revision-accepted-001",
        }),
        ACCEPTED_PREVIEW_ORIGIN,
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "Proposed" }));

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
    expect(requestBodies).toContainEqual({
      schemaVersion: "1",
      target: targetResponseFixture.target,
    });
    expect(requestBodies).toContainEqual({
      schemaVersion: "1",
      revisionId: "revision-accepted-001",
      targetId: "target-001",
      resolutionId: "resolution-001",
      attemptId: "11111111-2222-4333-8444-555555555555",
      providerId: "fake",
      requestedModel: "deterministic-v1",
      instruction: "見出しを、未来への期待が伝わる表現にしてください",
    });

    for (const [input, init] of fetchMock.mock.calls.slice(1)) {
      expect(String(input)).toMatch(/^\/api\/v1\//);
      expect(new Headers(init?.headers).get("Authorization")).toBe(
        "Bearer secret-only-in-api-closure",
      );
      expect(init?.credentials).toBe("omit");
    }
  });

  it("rejects an external context binding that suppresses provider disclosure", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        const method = init?.method ?? "GET";
        if (path === "/api/v1/bootstrap") {
          const bootstrap = bootstrapFixture(window.location.origin);
          return jsonResponse({
            ...bootstrap,
            capabilities: {
              ...bootstrap.capabilities,
              aiProviders: bootstrap.capabilities.aiProviders.map((provider) =>
                provider.id === "openai"
                  ? {
                      ...provider,
                      availability: "available",
                      models: [{ id: "gpt-5.4-mini", label: "gpt-5.4-mini" }],
                    }
                  : provider,
              ),
            },
          });
        }
        if (method === "GET" && path === "/api/v1/projects") {
          return jsonResponse(projectsResponseFixture());
        }
        if (method === "GET" && path === "/api/v1/projects/project-001") {
          return jsonResponse(projectResponseFixture());
        }
        if (path.endsWith("/targets")) {
          return jsonResponse(targetResponseFixture);
        }
        if (path.endsWith("/contexts")) {
          const request = JSON.parse(String(init?.body)) as {
            attemptId: string;
            instruction: string;
            providerId: string;
            requestedModel: string;
          };
          return jsonResponse({
            ...contextResponseFixture,
            context: {
              ...contextResponseFixture.context,
              attemptId: request.attemptId,
              providerId: request.providerId,
              requestedModel: request.requestedModel,
              instruction: request.instruction,
              provider: {
                providerId: request.providerId,
                adapterVersion: "openai-responses/1",
                requestedModel: request.requestedModel,
                external: false,
              },
            },
          });
        }
        throw new Error(`Unexpected request: ${method} ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await openRetainedProjectAndSelectTarget();
    fireEvent.change(screen.getByLabelText("AI provider"), {
      target: { value: "openai" },
    });
    expect(screen.getByLabelText("Model")).toHaveValue("gpt-5.4-mini");
    fireEvent.click(screen.getByRole("button", { name: "送信内容を確認" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Targetまたはprovider bindingが一致しません",
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(
      fetchMock.mock.calls.some(([input]) =>
        String(input).endsWith("/proposals"),
      ),
    ).toBe(false);
  });

  it("reviews redacted context but keeps full-file Proposal generation unavailable", async () => {
    const secretCanary = "sk-C6_REDACTION_SECRET_CANARY_123456789";
    const redactedHtml = `<meta data-api-key="[LP_STUDIO_REDACTED]">`;
    const redactedCanonicalJson = JSON.stringify({
      untrustedSiteContent: [{ path: "index.html", content: redactedHtml }],
    });
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
        if (path.endsWith("/targets")) {
          return jsonResponse(targetResponseFixture);
        }
        if (path.endsWith("/contexts")) {
          const request = JSON.parse(String(init?.body)) as {
            attemptId: string;
            instruction: string;
            providerId: string;
            requestedModel: string;
          };
          return jsonResponse({
            ...contextResponseFixture,
            context: {
              ...contextResponseFixture.context,
              attemptId: request.attemptId,
              providerId: request.providerId,
              requestedModel: request.requestedModel,
              instruction: request.instruction,
              provider: {
                ...contextResponseFixture.context.provider,
                providerId: request.providerId,
                requestedModel: request.requestedModel,
              },
              manifest: {
                ...contextResponseFixture.context.manifest,
                entries: contextResponseFixture.context.manifest.entries.map(
                  (entry, index) =>
                    index === 0
                      ? {
                          ...entry,
                          redacted: true,
                          redactions: ["credential_assignment"],
                        }
                      : entry,
                ),
              },
              canonicalJson: redactedCanonicalJson,
            },
          });
        }
        if (path.endsWith("/proposals")) {
          return jsonResponse(proposalResponseFixture);
        }
        throw new Error(`Unexpected request: ${method} ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);
    await openRetainedProjectAndSelectTarget();
    fireEvent.click(screen.getByRole("button", { name: "送信内容を確認" }));
    const dialog = await screen.findByRole("dialog", {
      name: "送信内容を確認",
    });

    expect(within(dialog).getByText(redactedCanonicalJson)).toBeInTheDocument();
    expect(dialog).not.toHaveTextContent(secretCanary);
    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "redactionを含むfileから安全なfull-file ChangeSetを生成できない",
    );
    const generate = within(dialog).getByRole("button", {
      name: "変更案を作成",
    });
    expect(generate).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: "送信内容を閉じる" }),
    ).toBeEnabled();
    fireEvent.click(generate);
    expect(
      fetchMock.mock.calls.some(([input]) =>
        String(input).endsWith("/proposals"),
      ),
    ).toBe(false);
  });

  it.each(proposalBindingMismatchCases)(
    "rejects a Proposal with mismatched %s binding",
    async (_name, mutateProposal) => {
      let reviewedAttemptId = "";
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
          if (path.endsWith("/targets")) {
            return jsonResponse(targetResponseFixture);
          }
          if (path.endsWith("/contexts")) {
            const request = JSON.parse(String(init?.body)) as {
              attemptId: string;
              instruction: string;
              providerId: string;
              requestedModel: string;
            };
            reviewedAttemptId = request.attemptId;
            return jsonResponse({
              ...contextResponseFixture,
              context: {
                ...contextResponseFixture.context,
                attemptId: request.attemptId,
                providerId: request.providerId,
                requestedModel: request.requestedModel,
                instruction: request.instruction,
                provider: {
                  ...contextResponseFixture.context.provider,
                  providerId: request.providerId,
                  requestedModel: request.requestedModel,
                },
              },
            });
          }
          if (path.endsWith("/proposals")) {
            return jsonResponse(
              mutateProposal(proposalResponseForAttempt(reviewedAttemptId)),
            );
          }
          throw new Error(`Unexpected request: ${method} ${path}`);
        },
      );
      vi.stubGlobal("fetch", fetchMock);

      render(<App />);
      await openRetainedProjectAndSelectTarget();
      fireEvent.click(screen.getByRole("button", { name: "送信内容を確認" }));
      const dialog = await screen.findByRole("dialog", {
        name: "送信内容を確認",
      });
      fireEvent.click(
        within(dialog).getByRole("button", { name: "変更案を作成" }),
      );

      expect(await screen.findByRole("alert")).toHaveTextContent(
        "reviewed provider context bindingが一致しません",
      );
      expect(
        screen.queryByRole("heading", { name: "変更案を確認" }),
      ).not.toBeInTheDocument();
    },
  );

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

  it("shows only privacy-safe diagnostics from the exact Accepted frame", async () => {
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
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Untitled landing pageを開く",
      }),
    );
    const frame = (await screen.findByTitle(
      "LPプレビュー",
    )) as HTMLIFrameElement;
    const diagnostic = {
      type: "synapsegit-lp.diagnostic",
      schemaVersion: "1",
      channelId: "11111111-2222-4333-8444-555555555555",
      projectId: "project-001",
      snapshotId: "revision-accepted-001",
      revisionId: "revision-accepted-001",
      severity: "warning",
      code: "csp_blocked",
      sourceUnavailable: true,
    };

    fireEvent(
      window,
      new MessageEvent("message", {
        origin: ACCEPTED_PREVIEW_ORIGIN,
        source: frame.contentWindow,
        data: {
          ...diagnostic,
          message: "file:///private/site/index.html",
        },
      }),
    );
    expect(screen.queryByText("source unavailable")).not.toBeInTheDocument();

    fireEvent(
      window,
      new MessageEvent("message", {
        origin: ACCEPTED_PREVIEW_ORIGIN,
        source: frame.contentWindow,
        data: diagnostic,
      }),
    );
    expect(await screen.findByText("source unavailable")).toBeVisible();
    expect(screen.getByText(/プレビューのセキュリティ制約/)).toBeVisible();
    expect(document.body.textContent).not.toContain("file:///private");
  });

  it("fails closed without mounting site content when navigation isolation is unavailable", async () => {
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
    vi.stubGlobal("navigation", undefined);

    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Untitled landing pageを開く",
      }),
    );

    const isolationError = await screen.findByRole("alert");
    expect(isolationError).toHaveTextContent(
      "このブラウザでは安全なプレビュー隔離を利用できないため、LPの実行を停止しました。",
    );
    expect(screen.queryByTitle("LPプレビュー")).not.toBeInTheDocument();
    expect(isolationError).not.toHaveTextContent("pv-");
    expect(isolationError).not.toHaveTextContent("preview/project");
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
