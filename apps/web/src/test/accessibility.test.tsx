import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { App } from "../App";
import {
  ACCEPTED_PREVIEW_ORIGIN,
  bootstrapFixture,
  contextResponseFixture,
  importPreviewResponseFixture,
  projectResponseFixture,
  projectsResponseFixture,
  targetResponseFixture,
} from "./fixtures";

const jsonResponse = (value: unknown): Response =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });

const renderRetainedProject = async (
  extraFetch?: (
    path: string,
    init: RequestInit | undefined,
  ) => Response | Promise<Response> | null,
): Promise<void> => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = String(input);
      const extra = extraFetch?.(path, init);
      if (extra !== null && extra !== undefined) return extra;
      if (path === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture(window.location.origin));
      }
      if ((init?.method ?? "GET") === "GET" && path === "/api/v1/projects") {
        return jsonResponse(projectsResponseFixture());
      }
      if (path === "/api/v1/projects/project-001") {
        return jsonResponse(projectResponseFixture());
      }
      throw new Error(`Unexpected request: ${init?.method ?? "GET"} ${path}`);
    }),
  );
  render(<App />);
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Untitled landing pageを開く",
    }),
  );
  await screen.findByLabelText("Accepted revision");
};

const selectFixtureTarget = async (): Promise<void> => {
  const frame = (await screen.findByTitle("LPプレビュー")) as HTMLIFrameElement;
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
  await screen.findByText("#hero-heading");
};

describe("C10 modal keyboard and background isolation", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("contains import-review focus, makes the home inert, and returns focus on Escape", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture(window.location.origin));
        }
        if ((init?.method ?? "GET") === "GET" && path === "/api/v1/projects") {
          return jsonResponse(projectsResponseFixture([]));
        }
        if (path === "/api/v1/imports/previews") {
          return jsonResponse(importPreviewResponseFixture);
        }
        throw new Error(`Unexpected request: ${init?.method ?? "GET"} ${path}`);
      }),
    );
    render(<App />);

    const trigger = await screen.findByRole("button", {
      name: "登録済みディレクトリを取り込む",
    });
    await waitFor(() => expect(trigger).toBeEnabled());
    fireEvent.click(trigger);

    const dialog = await screen.findByRole("dialog", {
      name: "取り込むファイルを確認",
    });
    const close = within(dialog).getByRole("button", {
      name: "取り込み確認を閉じる",
    });
    const confirm = within(dialog).getByRole("button", {
      name: "この内容をコピーして取り込む",
    });
    expect(close).toHaveFocus();
    expect(document.querySelector("main.project-home")).toHaveAttribute(
      "inert",
    );
    expect(document.querySelector("main.project-home")).toHaveAttribute(
      "aria-hidden",
      "true",
    );

    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(confirm).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    expect(document.querySelector("main.project-home")).not.toHaveAttribute(
      "inert",
    );
    expect(document.querySelector("main.project-home")).not.toHaveAttribute(
      "aria-hidden",
    );
  });

  it("contains exact-context focus and restores the generation trigger", async () => {
    await renderRetainedProject((path, init) => {
      if (path.endsWith("/targets")) return jsonResponse(targetResponseFixture);
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
            instruction: request.instruction,
            providerId: request.providerId,
            requestedModel: request.requestedModel,
            provider: {
              ...contextResponseFixture.context.provider,
              providerId: request.providerId,
              requestedModel: request.requestedModel,
            },
          },
        });
      }
      return null;
    });
    await selectFixtureTarget();

    const trigger = screen.getByRole("button", { name: "送信内容を確認" });
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", {
      name: "送信内容を確認",
    });
    const close = within(dialog).getByRole("button", {
      name: "送信内容を閉じる",
    });
    const confirm = within(dialog).getByRole("button", {
      name: "変更案を作成",
    });
    expect(close).toHaveFocus();
    expect(document.querySelector("header.studio-header")).toHaveAttribute(
      "inert",
    );

    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(confirm).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    expect(document.querySelector("header.studio-header")).not.toHaveAttribute(
      "inert",
    );
  });

  it("keeps active-attempt cancellation keyboard operable and restores focus", async () => {
    const pendingProposal = new Promise<Response>(() => undefined);
    let attemptId = "";
    let cancelCalls = 0;
    await renderRetainedProject((path, init) => {
      if (path.endsWith("/targets")) return jsonResponse(targetResponseFixture);
      if (path.endsWith("/contexts")) {
        const request = JSON.parse(String(init?.body)) as {
          attemptId: string;
          instruction: string;
          providerId: string;
          requestedModel: string;
        };
        attemptId = request.attemptId;
        return jsonResponse({
          ...contextResponseFixture,
          context: {
            ...contextResponseFixture.context,
            attemptId,
            instruction: request.instruction,
            providerId: request.providerId,
            requestedModel: request.requestedModel,
            provider: {
              ...contextResponseFixture.context.provider,
              providerId: request.providerId,
              requestedModel: request.requestedModel,
            },
          },
        });
      }
      if (path.endsWith("/proposals")) return pendingProposal;
      if (path.endsWith("/cancel")) {
        cancelCalls += 1;
        return jsonResponse({
          schemaVersion: "1",
          attemptId,
          status: "cancelled",
        });
      }
      return null;
    });
    await selectFixtureTarget();

    const trigger = screen.getByRole("button", { name: "送信内容を確認" });
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", {
      name: "送信内容を確認",
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "変更案を作成" }),
    );

    const cancel = await within(dialog).findByRole("button", {
      name: "AI処理を取り消す",
    });
    await waitFor(() => expect(cancel).toHaveFocus());
    expect(
      within(dialog).getByRole("button", { name: "送信内容を閉じる" }),
    ).toBeDisabled();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(
      within(dialog)
        .getByText(/"instruction"/)
        .closest("pre"),
    ).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(cancel).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(dialog).toBeInTheDocument();

    fireEvent.click(cancel);
    expect(
      await screen.findByText(/AI処理を取り消しました。送信内容を確認し直す/),
    ).toBeInTheDocument();
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    expect(cancelCalls).toBe(1);
  });

  it("contains publication focus and restores the publication trigger", async () => {
    await renderRetainedProject();

    const trigger = screen.getByRole("button", { name: "GitHub-ready記録" });
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", {
      name: "GitHub-ready記録",
    });
    const close = within(dialog).getByRole("button", {
      name: "publication reviewを閉じる",
    });
    const summary = within(dialog).getByRole("textbox", {
      name: "Public summary",
    });
    expect(close).toHaveFocus();
    expect(document.querySelector("header.studio-header")).toHaveAttribute(
      "inert",
    );

    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(summary).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(dialog).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    expect(document.querySelector("header.studio-header")).not.toHaveAttribute(
      "inert",
    );
  });
});
