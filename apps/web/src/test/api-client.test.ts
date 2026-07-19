import { bootstrapApi } from "../api/client";
import { bootstrapFixture, projectResponseFixture } from "./fixtures";

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
});
