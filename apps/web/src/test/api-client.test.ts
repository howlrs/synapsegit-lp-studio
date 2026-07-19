import { bootstrapApi } from "../api/client";
import {
  HASH_C,
  apiErrorResponseFixture,
  bootstrapFixture,
  contextResponseFixture,
  importPreviewResponseFixture,
  projectResponseFixture,
  projectsResponseFixture,
} from "./fixtures";

const jsonResponse = (
  value: unknown,
  status = 200,
  headers: HeadersInit = {},
): Response => {
  const responseHeaders = new Headers(headers);
  responseHeaders.set("Content-Type", "application/json");
  return new Response(JSON.stringify(value), {
    status,
    headers: responseHeaders,
  });
};

describe("authenticated API client", () => {
  it("keeps the session token inside a closure and authenticates privileged calls", async () => {
    const storageWrite = vi.spyOn(Storage.prototype, "setItem");
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, _init?: RequestInit) => {
        const url = String(input);
        if (url === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        if (url === "/api/v1/projects/project-001") {
          return jsonResponse(projectResponseFixture());
        }
        throw new Error(`Unexpected request ${url}`);
      },
    );

    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );
    await session.api.getProject("project-001");

    expect(storageWrite).not.toHaveBeenCalled();
    expect(JSON.stringify(session)).not.toContain("secret-only-in-api-closure");

    const bootstrapInit = fetchMock.mock.calls[0]?.[1] as RequestInit;
    const projectInit = fetchMock.mock.calls[1]?.[1] as RequestInit;
    expect(bootstrapInit.credentials).toBe("omit");
    expect(new Headers(bootstrapInit.headers).has("Authorization")).toBe(false);
    expect(projectInit.credentials).toBe("omit");
    expect(new Headers(projectInit.headers).get("Authorization")).toBe(
      "Bearer secret-only-in-api-closure",
    );
    storageWrite.mockRestore();
  });

  it("updates Project metadata through an exact authenticated PATCH CAS request", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        expect(path).toBe("/api/v1/projects/project-001");
        expect(init?.method).toBe("PATCH");
        expect(new Headers(init?.headers).get("Content-Type")).toBe(
          "application/json",
        );
        expect(new Headers(init?.headers).get("Authorization")).toBe(
          "Bearer secret-only-in-api-closure",
        );
        expect(init?.credentials).toBe("omit");
        expect(JSON.parse(String(init?.body))).toEqual({
          schemaVersion: "1",
          expectedDisplayName: "Untitled landing page",
          displayName: "Campaign LP",
        });
        return jsonResponse({
          ...projectResponseFixture(),
          project: {
            ...projectResponseFixture().project,
            displayName: "Campaign LP",
          },
        });
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(
      session.api.updateProjectDisplayName(
        "project-001",
        "Untitled landing page",
        "Campaign LP",
      ),
    ).resolves.toMatchObject({
      id: "project-001",
      displayName: "Campaign LP",
      revisionId: "revision-accepted-001",
    });
  });

  it("rejects invalid Project names before sending authority or bytes", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      throw new Error("Invalid metadata must not be fetched");
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(
      session.api.updateProjectDisplayName(
        "project-001",
        "Untitled landing page",
        "/home/private/project",
      ),
    ).rejects.toMatchObject({
      code: "invalid_project_display_name",
      retryable: false,
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("binds context creation to the selected attempt, provider, and model", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        if (path === "/api/v1/projects/project-001/contexts") {
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
            revisionId: "revision-accepted-001",
            targetId: "target-001",
            resolutionId: "resolution-001",
            attemptId: "attempt-001",
            providerId: "fake",
            requestedModel: "deterministic-v1",
            instruction: "見出しを力強くしてください",
          });
          expect(new Headers(init?.headers).get("Authorization")).toBe(
            "Bearer secret-only-in-api-closure",
          );
          expect(init?.credentials).toBe("omit");
          return jsonResponse(contextResponseFixture);
        }
        throw new Error(`Unexpected request ${path}`);
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(
      session.api.createContext(
        "project-001",
        "revision-accepted-001",
        "target-001",
        "resolution-001",
        "attempt-001",
        "fake",
        "deterministic-v1",
        "見出しを力強くしてください",
      ),
    ).resolves.toMatchObject({
      attemptId: "attempt-001",
      providerId: "fake",
      requestedModel: "deterministic-v1",
    });
  });

  it("rejects a bootstrap that tries to move bearer authority to another origin", async () => {
    const fetchMock = vi.fn(async () =>
      jsonResponse(bootstrapFixture("https://attacker.example")),
    );

    await expect(
      bootstrapApi(fetchMock as unknown as typeof fetch, "http://editor.test"),
    ).rejects.toMatchObject({ code: "editor_origin_mismatch" });
  });

  it("rejects a foreign export URL before attaching Authorization", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      throw new Error("A foreign download must never be fetched");
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(
      session.api.downloadExport(
        "https://attacker.example/api/v1/exports/export-1/download",
      ),
    ).rejects.toMatchObject({ code: "invalid_download_origin" });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("downloads an allowed export with explicit bearer auth and no credentials", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        if (String(input) === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        expect(String(input)).toBe(
          "http://editor.test/api/v1/exports/export-1/download",
        );
        expect(new Headers(init?.headers).get("Authorization")).toBe(
          "Bearer secret-only-in-api-closure",
        );
        expect(init?.credentials).toBe("omit");
        return new Response(new Uint8Array([80, 75, 3, 4]), {
          status: 200,
          headers: { "Content-Type": "application/zip" },
        });
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );
    const blob = await session.api.downloadExport(
      "/api/v1/exports/export-1/download",
    );

    expect(blob.size).toBe(4);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("fails closed when a response does not match schema version 1", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      return jsonResponse({
        ...projectResponseFixture(),
        schemaVersion: "future-version",
      });
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.getProject("project-001")).rejects.toMatchObject({
      code: "invalid_response_schema",
    });
  });

  it("gets and explicitly cancels the exact server-owned AI attempt", async () => {
    const attempt = {
      schemaVersion: "1" as const,
      attemptId: "attempt-001",
      status: "running" as const,
    };
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        expect(path).toBe(
          "/api/v1/projects/project-001/ai-attempts/attempt-001" +
            (init?.method === "POST" ? "/cancel" : ""),
        );
        if (init?.method === "POST") {
          expect(JSON.parse(String(init.body))).toEqual({ schemaVersion: "1" });
          return jsonResponse({ ...attempt, status: "cancelled" });
        }
        expect(init?.method).toBe("GET");
        return jsonResponse(attempt);
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(
      session.api.getAiAttemptStatus("project-001", "attempt-001"),
    ).resolves.toEqual(attempt);
    await expect(
      session.api.cancelAiAttempt("project-001", "attempt-001"),
    ).resolves.toEqual({ ...attempt, status: "cancelled" });
  });

  it("binds structured errors to matching request and operation headers", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      return jsonResponse(apiErrorResponseFixture, 409, {
        "x-request-id": apiErrorResponseFixture.error.requestId,
        "x-operation-id": apiErrorResponseFixture.error.operationId,
      });
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.getProject("project-001")).rejects.toMatchObject({
      code: "revision_conflict",
      requestId: "request-001",
      operationId: "operation-001",
      retryable: false,
      detail: {
        acceptedState: "unchanged",
        recoveryAction: "refresh",
      },
    });
  });

  it("rejects a structured error whose body/header operation binding differs", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      return jsonResponse(apiErrorResponseFixture, 409, {
        "x-request-id": apiErrorResponseFixture.error.requestId,
        "x-operation-id": "operation-mismatch",
      });
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.getProject("project-001")).rejects.toMatchObject({
      code: "invalid_error_correlation",
      retryable: false,
    });
  });

  it("does not make an Accepted outcome claim for an unstructured HTTP error", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      return jsonResponse({ unexpected: true }, 500);
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.getProject("project-001")).rejects.toMatchObject({
      code: "http_error",
      message: "リクエストに失敗しました（HTTP 500）。",
      retryable: true,
    });
  });

  it("lists retained projects and confirms only the reviewed import digest", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        if (path === "/api/v1/projects" && init?.method === "GET") {
          return jsonResponse(projectsResponseFixture());
        }
        if (path === "/api/v1/imports/previews") {
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
          });
          return jsonResponse(importPreviewResponseFixture);
        }
        if (path === "/api/v1/imports/import-preview-001/confirm") {
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
            expectedManifestSha256: HASH_C,
          });
          return jsonResponse(projectResponseFixture());
        }
        throw new Error(`Unexpected request ${path}`);
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.listProjects()).resolves.toHaveLength(1);
    const preview = await session.api.previewRegisteredImport();
    await expect(
      session.api.confirmRegisteredImport(preview.id, preview.manifestSha256),
    ).resolves.toMatchObject({ id: "project-001" });

    for (const [, init] of fetchMock.mock.calls.slice(1)) {
      expect(new Headers(init?.headers).get("Authorization")).toBe(
        "Bearer secret-only-in-api-closure",
      );
      expect(init?.credentials).toBe("omit");
    }
  });

  it("rejects an import preview that leaks an absolute server path", async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      if (String(input) === "/api/v1/bootstrap") {
        return jsonResponse(bootstrapFixture("http://editor.test"));
      }
      return jsonResponse({
        ...importPreviewResponseFixture,
        importPreview: {
          ...importPreviewResponseFixture.importPreview,
          included: [
            {
              ...importPreviewResponseFixture.importPreview.included[0]!,
              path: "/srv/private/index.html",
            },
          ],
        },
      });
    });
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.previewRegisteredImport()).rejects.toMatchObject({
      code: "invalid_response_schema",
    });
  });
});
