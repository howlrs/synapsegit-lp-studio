import { bootstrapApi } from "../api/client";
import {
  HASH_C,
  bootstrapFixture,
  contextResponseFixture,
  importPreviewResponseFixture,
  projectResponseFixture,
  projectsResponseFixture,
} from "./fixtures";

const jsonResponse = (value: unknown, status = 200): Response =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });

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
