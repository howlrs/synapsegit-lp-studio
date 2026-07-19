import { render, screen } from "@testing-library/react";
import Ajv2020 from "ajv/dist/2020.js";
import {
  isBootstrapResponse,
  isRecoveryResponse,
  isRetentionCleanupResponse,
  isRetentionResponse,
  type RecoveryResponse,
  type RetentionCleanupResponse,
  type RetentionResponse,
} from "@synapsegit-lp/contracts";
import apiSchema from "../../../../packages/contracts/schemas/api-v1.schema.json";
import { App } from "../App";
import { bootstrapApi } from "../api/client";
import { HASH_A, bootstrapFixture } from "./fixtures";

const jsonResponse = (value: unknown, status = 200): Response =>
  new Response(JSON.stringify(value), {
    status,
    headers: { "Content-Type": "application/json" },
  });

const retentionResponse: RetentionResponse = {
  schemaVersion: "1",
  retention: {
    automaticGc: false,
    telemetry: "absent",
    cleanupRequiresExplicitConfirmation: true,
    projects: [
      {
        projectId: "project-001",
        displayName: "Campaign",
        revisionId: "revision-001",
        acceptedManifestSha256: HASH_A,
        acceptedFileByteLength: 18,
        targetCount: 1,
        conversationContextCount: 0,
        conversationPersistence: "memory_only",
        failedProposal: null,
        terminalDecisionCount: 1,
        artifacts: [
          {
            kind: "static_export",
            id: "export-001",
            projectId: "project-001",
            revisionId: "revision-001",
            sha256: HASH_A,
            payloadByteLength: 140,
            cleanupImpact: "Deletes this local export only.",
          },
        ],
        projectDeletionImpact: "Deletes the complete managed local project.",
      },
    ],
  },
};

const cleanedRetentionResponse: RetentionCleanupResponse = {
  schemaVersion: "1",
  removed: {
    scope: "static_export",
    id: "export-001",
    payloadByteLength: 140,
  },
  retention: {
    ...retentionResponse.retention,
    projects: retentionResponse.retention.projects.map((project) => ({
      ...project,
      artifacts: [],
    })),
  },
};

const recoveryResponse: RecoveryResponse = {
  schemaVersion: "1",
  recoveryPoints: [
    {
      id: "accepted-001",
      kind: "last_accepted",
      projectId: "project-001",
      revisionId: "revision-001",
      artifactManifestSha256: HASH_A,
      diagnostic: {
        verified: true,
        code: "verified_last_accepted",
        manifestSha256: HASH_A,
        fileCount: 1,
        totalBytes: 18,
      },
      exportUrl: "/api/v1/recovery/accepted-001/export",
    },
    {
      id: "backup-corrupt-001",
      kind: "versioned_backup",
      projectId: null,
      revisionId: null,
      artifactManifestSha256: null,
      diagnostic: {
        verified: false,
        code: "manifest_mismatch",
        manifestSha256: null,
        fileCount: 0,
        totalBytes: 0,
      },
      exportUrl: null,
    },
  ],
};

const recoveryBootstrap = (editorOrigin: string) => {
  const value = bootstrapFixture(editorOrigin);
  return {
    ...value,
    capabilities: {
      ...value.capabilities,
      operatingMode: "read_only_recovery" as const,
      recoveryPointCount: recoveryResponse.recoveryPoints.length,
      importAvailable: false,
    },
  };
};

describe("C9 retention and recovery contracts", () => {
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
    ["retention response", retentionResponse],
    ["cleanup response", cleanedRetentionResponse],
    ["recovery response", recoveryResponse],
    ["recovery bootstrap", recoveryBootstrap("http://editor.test")],
    [
      "exact artifact cleanup request",
      {
        schemaVersion: "1",
        scope: "static_export",
        projectId: "project-001",
        artifactId: "export-001",
        expectedSha256: HASH_A,
        confirmation: "export-001",
      },
    ],
  ])("keeps %s aligned with the canonical schema", (_label, value) => {
    expect(validateSchema(value), JSON.stringify(validateSchema.errors)).toBe(
      true,
    );
  });

  it("enforces cross-field bindings beyond JSON Schema", () => {
    expect(isRetentionResponse(retentionResponse)).toBe(true);
    expect(isRetentionCleanupResponse(cleanedRetentionResponse)).toBe(true);
    expect(isRecoveryResponse(recoveryResponse)).toBe(true);
    expect(isBootstrapResponse(recoveryBootstrap("http://editor.test"))).toBe(
      true,
    );

    const foreignArtifact = structuredClone(retentionResponse);
    foreignArtifact.retention.projects[0]!.artifacts[0]!.projectId =
      "project-foreign";
    expect(isRetentionResponse(foreignArtifact)).toBe(false);

    const unverifiedExport = structuredClone(recoveryResponse);
    unverifiedExport.recoveryPoints[1]!.exportUrl =
      "/api/v1/recovery/backup-corrupt-001/export";
    expect(isRecoveryResponse(unverifiedExport)).toBe(false);

    const maximumSnapshot = structuredClone(recoveryResponse);
    maximumSnapshot.recoveryPoints[0]!.diagnostic.fileCount = 1_000;
    expect(isRecoveryResponse(maximumSnapshot)).toBe(true);
    maximumSnapshot.recoveryPoints[0]!.diagnostic.fileCount = 1_001;
    expect(isRecoveryResponse(maximumSnapshot)).toBe(false);

    const completeMode = recoveryBootstrap("http://editor.test");
    const { recoveryPointCount: _recoveryPointCount, ...halfCapabilities } =
      completeMode.capabilities;
    const halfMode = { ...completeMode, capabilities: halfCapabilities };
    expect(isBootstrapResponse(halfMode)).toBe(false);
  });
});

describe("C9 authenticated retention and recovery client", () => {
  it("binds cleanup confirmation and recovery downloads to authenticated local routes", async () => {
    const fetchMock = vi.fn(
      async (
        input: RequestInfo | URL,
        init?: RequestInit,
      ): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(bootstrapFixture("http://editor.test"));
        }
        if (path === "/api/v1/retention" && init?.method === "GET") {
          return jsonResponse(retentionResponse);
        }
        if (path === "/api/v1/retention/cleanup") {
          expect(init?.method).toBe("POST");
          expect(JSON.parse(String(init?.body))).toEqual({
            schemaVersion: "1",
            scope: "static_export",
            projectId: "project-001",
            artifactId: "export-001",
            expectedSha256: HASH_A,
            confirmation: "export-001",
          });
          return jsonResponse(cleanedRetentionResponse);
        }
        if (path === "/api/v1/recovery" && init?.method === "GET") {
          return jsonResponse(recoveryResponse);
        }
        if (path === "http://editor.test/api/v1/recovery/accepted-001/export") {
          expect(init?.method).toBe("GET");
          expect(new Headers(init?.headers).get("Accept")).toBe(
            "application/zip",
          );
          return new Response(new Blob(["zip"]), { status: 200 });
        }
        throw new Error(`Unexpected request ${path}`);
      },
    );
    const session = await bootstrapApi(
      fetchMock as unknown as typeof fetch,
      "http://editor.test",
    );

    await expect(session.api.getRetention()).resolves.toEqual(
      retentionResponse.retention,
    );
    await expect(
      session.api.cleanupRetention({
        scope: "static_export",
        projectId: "project-001",
        artifactId: "export-001",
        expectedSha256: HASH_A,
        confirmation: "export-001",
      }),
    ).resolves.toEqual(cleanedRetentionResponse);
    await expect(session.api.getRecoveryPoints()).resolves.toHaveLength(2);
    await expect(
      session.api.downloadRecoveryExport(
        "/api/v1/recovery/accepted-001/export",
      ),
    ).resolves.toBeInstanceOf(Blob);

    for (const [, init] of fetchMock.mock.calls.slice(1)) {
      expect(new Headers(init?.headers).get("Authorization")).toBe(
        "Bearer secret-only-in-api-closure",
      );
      expect(init?.credentials).toBe("omit");
    }
  });
});

describe("C9 read-only recovery UI", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders only verified snapshot export affordances in recovery mode", async () => {
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL): Promise<Response> => {
        const path = String(input);
        if (path === "/api/v1/bootstrap") {
          return jsonResponse(recoveryBootstrap(window.location.origin));
        }
        if (path === "/api/v1/recovery") {
          return jsonResponse(recoveryResponse);
        }
        throw new Error(`Unexpected request ${path}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<App />);

    expect(
      await screen.findByRole("heading", {
        name: "保存領域を読み取り専用で開きました",
      }),
    ).toBeInTheDocument();
    expect(await screen.findByText("Last Accepted")).toBeInTheDocument();
    expect(screen.getByText("Versioned backup")).toBeInTheDocument();
    const buttons = screen.getAllByRole("button", {
      name: "検証済みAcceptedをexport",
    });
    expect(buttons).toHaveLength(2);
    expect(buttons[0]).toBeEnabled();
    expect(buttons[1]).toBeDisabled();
    expect(screen.queryByText("保持データと手動削除")).not.toBeInTheDocument();
  });
});
