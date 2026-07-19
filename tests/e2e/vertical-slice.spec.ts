import { createHash } from "node:crypto";
import { lstat, readFile, readdir } from "node:fs/promises";
import { basename, join } from "node:path";

import {
  expect,
  test,
  type APIResponse,
  type Page,
  type Response as BrowserResponse,
} from "@playwright/test";
import Ajv2020, { type ValidateFunction } from "ajv/dist/2020.js";
import {
  isApiErrorResponse,
  isApprovalResponse,
  isBootstrapResponse,
  isContextResponse,
  isDecisionResponse,
  isExportResponse,
  isImportPreviewResponse,
  isPreviewActionMessage,
  isPreviewDiagnosticMessage,
  isPreviewStructureMessage,
  isPreviewTargetMessage,
  isProjectResponse,
  isProjectsResponse,
  isProposalResponse,
  isTargetResponse,
  type BootstrapResponse,
  type ExportResponse,
  type ImportPreviewResponse,
  type PreviewDiagnosticMessage,
  type Project,
  type ProjectResponse,
} from "../../packages/contracts/src/index";
import apiSchema from "../../packages/contracts/schemas/api-v1.schema.json";

const INITIAL_HEADING = "まだ、白紙です。";
const INITIAL_COPY =
  "伝えたいことを選び、AIとの対話から最初の一歩をつくります。";
const PROPOSED_HEADING = "対話から、公開できるLPへ。";
const PROMPT_CANARY =
  "ヒーロー見出しを明確にしてください。E2E_PRIVATE_PROMPT_CANARY";
const SCOPED_PREVIEW_HOST = /^pv-[0-9a-f]{32}\.localhost$/;

type RuntimeGuard = (value: unknown) => boolean;

interface ResponseContract {
  definition: string;
  guard: RuntimeGuard;
}

const ajv = new Ajv2020({ allErrors: true, strict: true });
ajv.addKeyword({
  keyword: "x-maxUtf8Bytes",
  type: "string",
  schemaType: "number",
  validate: (limit: number, value: string) =>
    new TextEncoder().encode(value).byteLength <= limit,
});
ajv.addSchema(apiSchema);

const definitionValidators = new Map<string, ValidateFunction>();

function validatorFor(definition: string): ValidateFunction {
  const existing = definitionValidators.get(definition);
  if (existing !== undefined) return existing;
  const validator = ajv.compile({
    $ref: `${apiSchema.$id}#/$defs/${definition}`,
  });
  definitionValidators.set(definition, validator);
  return validator;
}

function expectSchemaValid(definition: string, value: unknown): void {
  const validator = validatorFor(definition);
  expect(
    validator(value),
    `${definition}: ${JSON.stringify(validator.errors)}`,
  ).toBe(true);
}

function expectSchemaRejected(definition: string, value: unknown): void {
  expect(validatorFor(definition)(value)).toBe(false);
}

function responseContract(response: BrowserResponse): ResponseContract | null {
  const url = new URL(response.url());
  const path = url.pathname;
  const method = response.request().method();

  if (!response.ok()) {
    return { definition: "errorResponse", guard: isApiErrorResponse };
  }
  if (path.endsWith("/download")) return null;
  if (method === "GET" && path === "/api/v1/bootstrap") {
    return { definition: "bootstrapResponse", guard: isBootstrapResponse };
  }
  if (
    (method === "POST" && path === "/api/v1/projects") ||
    (method === "GET" && /^\/api\/v1\/projects\/[^/]+$/.test(path)) ||
    (method === "POST" && /^\/api\/v1\/imports\/[^/]+\/confirm$/.test(path))
  ) {
    return { definition: "projectResponse", guard: isProjectResponse };
  }
  if (method === "GET" && path === "/api/v1/projects") {
    return { definition: "projectsResponse", guard: isProjectsResponse };
  }
  if (method === "POST" && path === "/api/v1/imports/previews") {
    return {
      definition: "importPreviewResponse",
      guard: isImportPreviewResponse,
    };
  }
  if (method === "POST" && path.endsWith("/targets")) {
    return { definition: "targetResponse", guard: isTargetResponse };
  }
  if (method === "POST" && path.endsWith("/contexts")) {
    return { definition: "contextResponse", guard: isContextResponse };
  }
  if (method === "POST" && path.endsWith("/proposals")) {
    return { definition: "proposalResponse", guard: isProposalResponse };
  }
  if (method === "POST" && path.endsWith("/approvals")) {
    return { definition: "approvalResponse", guard: isApprovalResponse };
  }
  if (method === "POST" && path.endsWith("/decisions")) {
    return { definition: "decisionResponse", guard: isDecisionResponse };
  }
  if (method === "POST" && path.endsWith("/exports")) {
    return { definition: "exportResponse", guard: isExportResponse };
  }
  throw new Error(`Unclassified JSON API response: ${method} ${path}`);
}

async function validateApiResponse(response: BrowserResponse): Promise<void> {
  const contract = responseContract(response);
  if (contract === null) return;
  expect(response.headers()["content-type"]).toContain("application/json");
  const value = (await response.json()) as unknown;
  expectSchemaValid(contract.definition, value);
  expect(
    contract.guard(value),
    `${contract.definition} runtime guard rejected the live response`,
  ).toBe(true);
}

interface StoredZipEntry {
  name: string;
  data: Buffer;
  dosTime: number;
  dosDate: number;
  unixMode: number;
}

function findEndOfCentralDirectory(archive: Buffer): number {
  const minimumOffset = Math.max(0, archive.length - 65_557);
  for (let offset = archive.length - 22; offset >= minimumOffset; offset -= 1) {
    if (archive.readUInt32LE(offset) === 0x0605_4b50) {
      return offset;
    }
  }
  throw new Error("ZIP end-of-central-directory record is missing");
}

function readStoredZipEntries(archive: Buffer): StoredZipEntry[] {
  const eocdOffset = findEndOfCentralDirectory(archive);
  const entryCount = archive.readUInt16LE(eocdOffset + 10);
  let centralOffset = archive.readUInt32LE(eocdOffset + 16);
  const entries: StoredZipEntry[] = [];

  for (let index = 0; index < entryCount; index += 1) {
    expect(archive.readUInt32LE(centralOffset)).toBe(0x0201_4b50);

    const compressionMethod = archive.readUInt16LE(centralOffset + 10);
    const dosTime = archive.readUInt16LE(centralOffset + 12);
    const dosDate = archive.readUInt16LE(centralOffset + 14);
    const compressedSize = archive.readUInt32LE(centralOffset + 20);
    const fileNameLength = archive.readUInt16LE(centralOffset + 28);
    const extraLength = archive.readUInt16LE(centralOffset + 30);
    const commentLength = archive.readUInt16LE(centralOffset + 32);
    const externalAttributes = archive.readUInt32LE(centralOffset + 38);
    const localOffset = archive.readUInt32LE(centralOffset + 42);
    const name = archive
      .subarray(centralOffset + 46, centralOffset + 46 + fileNameLength)
      .toString("utf8");

    expect(
      compressionMethod,
      `${name} must use the deterministic stored profile`,
    ).toBe(0);
    expect(archive.readUInt32LE(localOffset)).toBe(0x0403_4b50);
    const localNameLength = archive.readUInt16LE(localOffset + 26);
    const localExtraLength = archive.readUInt16LE(localOffset + 28);
    const dataOffset = localOffset + 30 + localNameLength + localExtraLength;

    entries.push({
      name,
      data: archive.subarray(dataOffset, dataOffset + compressedSize),
      dosTime,
      dosDate,
      unixMode: (externalAttributes >>> 16) & 0o777,
    });

    centralOffset += 46 + fileNameLength + extraLength + commentLength;
  }

  return entries;
}

async function exportPayload(response: APIResponse): Promise<ExportResponse> {
  expect(response.ok()).toBeTruthy();
  const payload = (await response.json()) as ExportResponse;
  expect(payload.schemaVersion).toBe("1");
  expect(payload.export.sha256).toMatch(/^[a-f0-9]{64}$/);
  expect(payload.export.byteLength).toBeGreaterThan(0);
  return payload;
}

async function downloadExport(
  page: Page,
  payload: ExportResponse,
  sessionToken: string,
): Promise<Buffer> {
  const editorOrigin = new URL(page.url()).origin;
  const response = await page.request.get(
    new URL(payload.export.downloadUrl, editorOrigin).href,
    {
      headers: {
        Authorization: `Bearer ${sessionToken}`,
        Origin: editorOrigin,
      },
    },
  );
  expect(response.ok()).toBeTruthy();
  expect(response.headers()["content-type"]).toContain("application/zip");
  return response.body();
}

async function importSourceSnapshot(
  root: string,
): Promise<{ sha256: string; byteLength: number }> {
  const hash = createHash("sha256");
  let byteLength = 0;
  const paths = await readdir(root, { recursive: true });
  paths.sort();
  for (const path of paths) {
    const metadata = await lstat(join(root, path));
    const kind = metadata.isDirectory()
      ? "directory"
      : metadata.isFile()
        ? "file"
        : metadata.isSymbolicLink()
          ? "symlink"
          : "other";
    hash.update(`${kind}:${path}`, "utf8");
    hash.update(new Uint8Array([0]));
    if (metadata.isFile()) {
      const bytes = await readFile(join(root, path));
      hash.update(bytes);
      byteLength += bytes.byteLength;
    }
  }
  return { sha256: hash.digest("hex"), byteLength };
}

function expectScopedPreviewUrl(
  rawUrl: string,
  previewScopeBaseOrigin: string,
): URL {
  const scoped = new URL(rawUrl);
  const base = new URL(previewScopeBaseOrigin);
  expect(scoped.protocol).toBe("http:");
  expect(scoped.hostname).toMatch(SCOPED_PREVIEW_HOST);
  expect(scoped.port).toBe(base.port);
  expect(scoped.username).toBe("");
  expect(scoped.password).toBe("");
  expect(scoped.search).toBe("");
  expect(scoped.hash).toBe("");
  expect(scoped.pathname).toMatch(/^\/preview\/[^/]+\/[^/]+\/$/);
  return scoped;
}

async function createBlankProject(
  page: Page,
  sessionToken: string,
): Promise<Project> {
  const payload = await page.evaluate(
    async ({ token }) => {
      const response = await fetch("/api/v1/projects", {
        method: "POST",
        credentials: "omit",
        headers: {
          Accept: "application/json",
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ schemaVersion: "1", template: "blank" }),
      });
      return { ok: response.ok, value: (await response.json()) as unknown };
    },
    { token: sessionToken },
  );
  expect(payload.ok).toBe(true);
  expect(isProjectResponse(payload.value)).toBe(true);
  return (payload.value as ProjectResponse).project;
}

test("blank Targetからfake AI Proposalを採用し、pureなAccepted exportを得る", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  const configuredEditorOrigin = process.env.LP_STUDIO_E2E_EDITOR_ORIGIN;
  const configuredPreviewOrigin = process.env.LP_STUDIO_E2E_PREVIEW_ORIGIN;
  if (
    configuredEditorOrigin === undefined ||
    configuredPreviewOrigin === undefined
  ) {
    throw new Error("E2E origins were not initialized by global setup");
  }

  await page.addInitScript(() => {
    const envelopes: unknown[] = [];
    Object.defineProperty(window, "__LP_STUDIO_E2E_BRIDGE_ENVELOPES__", {
      configurable: false,
      enumerable: false,
      value: envelopes,
      writable: false,
    });
    window.addEventListener(
      "message",
      (event) => {
        const data = event.data as unknown;
        if (
          typeof data === "object" &&
          data !== null &&
          "type" in data &&
          typeof data.type === "string" &&
          data.type.startsWith("synapsegit-lp.")
        ) {
          envelopes.push(structuredClone(data));
        }
      },
      true,
    );
  });

  const apiValidationTasks: Promise<void>[] = [];
  const apiValidationErrors: unknown[] = [];
  let capturedApiResponseCount = 0;
  page.on("response", (response) => {
    const url = new URL(response.url());
    if (
      url.origin === configuredEditorOrigin &&
      url.pathname.startsWith("/api/v1/")
    ) {
      capturedApiResponseCount += 1;
      apiValidationTasks.push(
        validateApiResponse(response).catch((error: unknown) => {
          apiValidationErrors.push(error);
        }),
      );
    }
  });

  const bootstrapResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "GET" &&
      new URL(response.url()).pathname === "/api/v1/bootstrap",
  );

  await page.goto(`${configuredEditorOrigin}/`);
  const bootstrapResponse = await bootstrapResponsePromise;
  expect(bootstrapResponse.ok()).toBeTruthy();
  const bootstrap = (await bootstrapResponse.json()) as BootstrapResponse;
  expect(bootstrap.session.token).not.toBe("");
  const bootstrapWithExtraField = { ...bootstrap, unexpected: true };
  expectSchemaRejected("bootstrapResponse", bootstrapWithExtraField);
  expect(isBootstrapResponse(bootstrapWithExtraField)).toBe(false);
  const editorOrigin = new URL(page.url()).origin;
  expect(editorOrigin).toBe(configuredEditorOrigin);
  const previewScopeBase = new URL(bootstrap.previewOrigin);
  const previewHealthOrigin = new URL(configuredPreviewOrigin);
  expect(previewScopeBase.hostname).toBe("localhost");
  expect(previewScopeBase.port).toBe(previewHealthOrigin.port);
  expect(previewScopeBase.origin).not.toBe(previewHealthOrigin.origin);

  const invalidRequestStatus = await page.evaluate(
    async ({ token }) => {
      const response = await fetch("/api/v1/projects", {
        method: "POST",
        credentials: "omit",
        headers: {
          Accept: "application/json",
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          schemaVersion: "1",
          template: "blank",
          unexpected: true,
        }),
      });
      await response.json();
      return response.status;
    },
    { token: bootstrap.session.token },
  );
  expect(invalidRequestStatus).toBe(400);

  await page.getByRole("button", { name: "空のLPを作成" }).click();

  const previewElement = page.getByTitle("LPプレビュー");
  await expect(previewElement).toBeVisible();
  const previewSource = await previewElement.getAttribute("src");
  expect(previewSource).toBeTruthy();
  const initialPreviewUrl = expectScopedPreviewUrl(
    new URL(previewSource!, editorOrigin).href,
    bootstrap.previewOrigin,
  );
  expect(initialPreviewUrl.origin).not.toBe(editorOrigin);
  expect(initialPreviewUrl.origin).not.toBe(configuredPreviewOrigin);
  await expect(previewElement).toHaveAttribute(
    "sandbox",
    "allow-scripts allow-same-origin",
  );
  await expect(previewElement).toHaveAttribute("allow", "");

  const preview = page.frameLocator('iframe[title="LPプレビュー"]');
  await expect(
    preview.getByRole("heading", { name: INITIAL_HEADING }),
  ).toBeVisible();

  const targetRegion = page.getByRole("complementary", {
    name: "選択中のターゲット",
  });
  const targetKindOutput = targetRegion.locator(".pill");
  const structureButtons = page.locator(".element-list button");
  await expect(structureButtons.first()).toBeVisible();
  const heroBlockButton = structureButtons
    .filter({ has: page.locator("small", { hasText: "section" }) })
    .first();
  const headingTreeButton = structureButtons
    .filter({ has: page.locator("small", { hasText: "h1" }) })
    .first();

  await page.getByRole("button", { name: "ページ", exact: true }).click();
  await page
    .getByRole("button", { name: "index.html 全体", exact: true })
    .click();
  await expect(targetKindOutput).toHaveText("page");
  await expect(targetRegion.locator(".resolution")).toContainText("resolved");

  for (const [label, kind, captureButton] of [
    ["ブロック", "block", heroBlockButton],
    ["要素", "element", headingTreeButton],
    ["テキスト", "text", headingTreeButton],
    ["座標", "point", headingTreeButton],
    ["領域", "region", headingTreeButton],
  ] as const) {
    await page.getByRole("button", { name: label, exact: true }).click();
    await captureButton.click();
    await expect(targetKindOutput).toHaveText(kind);
    await expect(targetRegion.locator(".resolution")).toContainText("resolved");
  }

  await page.getByRole("button", { name: "座標", exact: true }).click();
  const pointResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname.endsWith("/targets"),
  );
  await preview.locator("body").click({ position: { x: 32, y: 48 } });
  expect((await pointResponsePromise).ok()).toBeTruthy();
  await expect(targetKindOutput).toHaveText("point");

  await page.getByRole("button", { name: "領域", exact: true }).click();
  const previewBodyBox = await preview.locator("body").boundingBox();
  expect(previewBodyBox).not.toBeNull();
  const regionResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname.endsWith("/targets"),
  );
  await page.mouse.move(previewBodyBox!.x + 36, previewBodyBox!.y + 52);
  await page.mouse.down();
  await page.mouse.move(previewBodyBox!.x + 180, previewBodyBox!.y + 132, {
    steps: 4,
  });
  await page.mouse.up();
  expect((await regionResponsePromise).ok()).toBeTruthy();
  await expect(targetKindOutput).toHaveText("region");

  await page.getByRole("button", { name: "要素", exact: true }).click();

  const mobileViewport = page.getByRole("button", { name: "モバイル" });
  const desktopViewport = page.getByRole("button", { name: "デスクトップ" });
  const viewportFrame = page.locator(".viewport-frame");
  await mobileViewport.click();
  await expect(mobileViewport).toHaveAttribute("aria-pressed", "true");
  await expect(viewportFrame).toHaveCSS("width", "390px");
  await page.getByRole("spinbutton", { name: "カスタム幅" }).fill("480");
  await expect(viewportFrame).toHaveCSS("width", "480px");
  await desktopViewport.click();
  await expect(desktopViewport).toHaveAttribute("aria-pressed", "true");

  const previewApiAttempt = await preview.locator("body").evaluate(async () => {
    try {
      const response = await fetch("/api/v1/bootstrap");
      return { blockedByBrowser: false, status: response.status };
    } catch {
      return { blockedByBrowser: true, status: null };
    }
  });
  expect(
    previewApiAttempt.blockedByBrowser ||
      (previewApiAttempt.status !== null && previewApiAttempt.status >= 400),
  ).toBeTruthy();

  await preview.getByRole("heading", { name: INITIAL_HEADING }).click();
  await expect(targetRegion).toContainText("ヒーロー見出し");
  await expect(targetRegion).toContainText(/element|要素/i);

  const editorBridgeEnvelopes = await page.evaluate(
    () =>
      (
        window as Window & {
          __LP_STUDIO_E2E_BRIDGE_ENVELOPES__?: unknown[];
        }
      ).__LP_STUDIO_E2E_BRIDGE_ENVELOPES__ ?? [],
  );
  const previewBridgeEnvelopes = await preview.locator("html").evaluate(
    () =>
      (
        window as Window & {
          __LP_STUDIO_E2E_BRIDGE_ENVELOPES__?: unknown[];
        }
      ).__LP_STUDIO_E2E_BRIDGE_ENVELOPES__ ?? [],
  );
  const bridgeEnvelopes = [...editorBridgeEnvelopes, ...previewBridgeEnvelopes];
  const targetEnvelopes = bridgeEnvelopes.filter(
    (value): value is Record<string, unknown> =>
      typeof value === "object" &&
      value !== null &&
      "type" in value &&
      value.type === "synapsegit-lp.target-draft",
  );
  const structureEnvelopes = bridgeEnvelopes.filter(
    (value): value is Record<string, unknown> =>
      typeof value === "object" &&
      value !== null &&
      "type" in value &&
      value.type === "synapsegit-lp.structure",
  );
  const actionEnvelopes = bridgeEnvelopes.filter(
    (value): value is Record<string, unknown> =>
      typeof value === "object" &&
      value !== null &&
      "type" in value &&
      value.type === "synapsegit-lp.action",
  );
  expect(targetEnvelopes.length).toBeGreaterThanOrEqual(6);
  expect(structureEnvelopes.length).toBeGreaterThan(0);
  expect(actionEnvelopes.length).toBeGreaterThan(0);
  for (const envelope of targetEnvelopes) {
    expectSchemaValid("previewTargetMessage", envelope);
    expect(isPreviewTargetMessage(envelope)).toBe(true);
    expect(JSON.stringify(envelope)).not.toContain("runtimeNodeHandle");
  }
  for (const envelope of structureEnvelopes) {
    expectSchemaValid("previewStructureMessage", envelope);
    expect(isPreviewStructureMessage(envelope)).toBe(true);
  }
  for (const envelope of actionEnvelopes) {
    expectSchemaValid("previewActionMessage", envelope);
    expect(isPreviewActionMessage(envelope)).toBe(true);
  }
  const targetWithExtraField = {
    ...targetEnvelopes[0],
    unexpected: true,
  };
  expectSchemaRejected("previewTargetMessage", targetWithExtraField);
  expect(isPreviewTargetMessage(targetWithExtraField)).toBe(false);
  const actionWithExtraField = { ...actionEnvelopes[0], unexpected: true };
  expectSchemaRejected("previewActionMessage", actionWithExtraField);
  expect(isPreviewActionMessage(actionWithExtraField)).toBe(false);

  await page.getByRole("textbox", { name: "AIへの要望" }).fill(PROMPT_CANARY);
  await page.getByRole("button", { name: "送信内容を確認" }).click();

  const contextReview = page.getByRole("dialog", { name: "送信内容を確認" });
  await expect(contextReview).toContainText(INITIAL_HEADING);
  await expect(contextReview).toContainText(/fake/i);
  expect(
    (await contextReview.textContent())?.includes(bootstrap.session.token),
  ).toBeFalsy();
  await contextReview.getByRole("button", { name: "変更案を作成" }).click();

  const review = page.getByRole("region", { name: "変更案を確認" });
  await expect(review).toBeVisible();
  await expect(
    review.getByText(PROPOSED_HEADING, { exact: false }),
  ).toBeVisible();
  await expect(review.getByText(/index\.html/).first()).toBeVisible();
  await expect(review.getByText(/caller-supplied/i).first()).toBeVisible();
  await expect(review.getByText(/execution未検証/i)).toBeVisible();

  await expect(
    preview.getByRole("heading", { name: PROPOSED_HEADING }),
  ).toBeVisible();
  await expect
    .poll(async () => {
      const source = await previewElement.getAttribute("src");
      return source === null ? null : new URL(source, editorOrigin).origin;
    })
    .not.toBe(initialPreviewUrl.origin);
  const proposedPreviewSource = await previewElement.getAttribute("src");
  expect(proposedPreviewSource).not.toBeNull();
  const proposedPreviewUrl = expectScopedPreviewUrl(
    new URL(proposedPreviewSource!, editorOrigin).href,
    bootstrap.previewOrigin,
  );
  const proposedTargetResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname.endsWith("/targets"),
  );
  await preview.locator('[data-lp-id="hero-copy"]').click();
  expect((await proposedTargetResponsePromise).ok()).toBeTruthy();
  await expect(targetRegion).toContainText("ヒーロー説明文");
  await expect(preview.getByText(INITIAL_COPY, { exact: true })).toBeVisible();
  await expect(
    review.getByText("ヒーロー見出し", { exact: true }),
  ).toBeVisible();

  const acceptedRevision = page.getByLabel("Accepted revision");
  const revisionBeforeAdoption = await acceptedRevision.textContent();
  expect(revisionBeforeAdoption).toBeTruthy();

  const decisionResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname.endsWith("/decisions"),
  );
  await page.getByRole("button", { name: "変更を採用" }).click();
  expect((await decisionResponsePromise).ok()).toBeTruthy();

  await expect(
    preview.getByRole("heading", { name: PROPOSED_HEADING }),
  ).toBeVisible();
  await expect(acceptedRevision).not.toHaveText(revisionBeforeAdoption!);
  await expect
    .poll(async () => {
      const source = await previewElement.getAttribute("src");
      return source === null ? null : new URL(source, editorOrigin).origin;
    })
    .not.toBe(proposedPreviewUrl.origin);
  const adoptedPreviewSource = await previewElement.getAttribute("src");
  expect(adoptedPreviewSource).not.toBeNull();
  const adoptedPreviewUrl = expectScopedPreviewUrl(
    new URL(adoptedPreviewSource!, editorOrigin).href,
    bootstrap.previewOrigin,
  );
  expect(adoptedPreviewUrl.origin).not.toBe(initialPreviewUrl.origin);
  const [staleAcceptedResponse, terminalProposalResponse] = await Promise.all([
    page.request.get(initialPreviewUrl.href),
    page.request.get(proposedPreviewUrl.href),
  ]);
  expect(staleAcceptedResponse.status()).toBe(404);
  expect(terminalProposalResponse.status()).toBe(404);
  expect(await staleAcceptedResponse.text()).toBe(
    await terminalProposalResponse.text(),
  );

  const firstExportResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname.endsWith("/exports"),
  );
  await page.getByRole("button", { name: "Acceptedをエクスポート" }).click();
  const firstExportResponse = await firstExportResponsePromise;
  const firstExport = await exportPayload(firstExportResponse);
  await expect(page.getByText(firstExport.export.sha256)).toBeVisible();

  const firstArchive = await downloadExport(
    page,
    firstExport,
    bootstrap.session.token,
  );
  expect(firstArchive).toHaveLength(firstExport.export.byteLength);
  expect(createHash("sha256").update(firstArchive).digest("hex")).toBe(
    firstExport.export.sha256,
  );

  const repeatedExportResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname.endsWith("/exports"),
  );
  await page.getByRole("button", { name: "Acceptedをエクスポート" }).click();
  const repeatedExportResponse = await repeatedExportResponsePromise;
  const repeatedExport = await exportPayload(repeatedExportResponse);
  const repeatedArchive = await downloadExport(
    page,
    repeatedExport,
    bootstrap.session.token,
  );

  expect(repeatedExport.export.revisionId).toBe(firstExport.export.revisionId);
  expect(repeatedExport.export.sha256).toBe(firstExport.export.sha256);
  expect(repeatedExport.export.byteLength).toBe(firstExport.export.byteLength);
  expect(repeatedArchive.equals(firstArchive)).toBeTruthy();

  const entries = readStoredZipEntries(firstArchive);
  expect(entries.map((entry) => entry.name)).toEqual([
    "index.html",
    "styles.css",
  ]);
  for (const entry of entries) {
    expect(entry.dosTime).toBe(0);
    expect(entry.dosDate).toBe(0x21);
    expect(entry.unixMode).toBe(0o644);
    expect(entry.name).not.toContain(".studio");
    expect(entry.data.includes(Buffer.from(PROMPT_CANARY, "utf8"))).toBeFalsy();
    expect(
      entry.data.includes(Buffer.from(bootstrap.session.token, "utf8")),
    ).toBeFalsy();
    expect(
      entry.data.includes(Buffer.from("caller-supplied", "utf8")),
    ).toBeFalsy();
    expect(
      entry.data.includes(Buffer.from("execution未検証", "utf8")),
    ).toBeFalsy();
  }
  expect(entries[0]?.data.toString("utf8")).toContain(PROPOSED_HEADING);
  expect(entries[0]?.data.toString("utf8")).not.toContain(INITIAL_HEADING);

  await Promise.all(apiValidationTasks);
  expect(capturedApiResponseCount).toBeGreaterThanOrEqual(10);
  expect(apiValidationErrors).toEqual([]);
  expect(pageErrors).toEqual([]);
});

test("scoped Preview originが権限・storage・navigationをproject/session間で隔離する", async ({
  page,
}) => {
  const editorOrigin = process.env.LP_STUDIO_E2E_EDITOR_ORIGIN;
  const previewHealthOrigin = process.env.LP_STUDIO_E2E_PREVIEW_ORIGIN;
  if (editorOrigin === undefined || previewHealthOrigin === undefined) {
    throw new Error("E2E origins were not initialized by global setup");
  }

  const bootstrapResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "GET" &&
      new URL(response.url()).pathname === "/api/v1/bootstrap",
  );
  await page.goto(`${editorOrigin}/`);
  const bootstrap = (await (
    await bootstrapResponsePromise
  ).json()) as BootstrapResponse;
  expect(isBootstrapResponse(bootstrap)).toBe(true);

  const projectA = await createBlankProject(page, bootstrap.session.token);
  const projectB = await createBlankProject(page, bootstrap.session.token);
  const scopedA = expectScopedPreviewUrl(
    projectA.previewUrl,
    bootstrap.previewOrigin,
  );
  const scopedB = expectScopedPreviewUrl(
    projectB.previewUrl,
    bootstrap.previewOrigin,
  );
  expect(scopedA.origin).not.toBe(scopedB.origin);
  expect(scopedA.origin).not.toBe(editorOrigin);
  expect(scopedA.origin).not.toBe(previewHealthOrigin);

  const editorDocumentResponse = await page.request.get(`${editorOrigin}/`);
  expect(editorDocumentResponse.ok()).toBe(true);
  const editorCsp = editorDocumentResponse.headers()["content-security-policy"];
  expect(editorCsp).toContain("frame-ancestors 'none'");
  expect(editorCsp).toContain(
    `frame-src http://*.localhost:${new URL(bootstrap.previewOrigin).port}`,
  );

  const acceptedResponse = await page.request.get(projectA.previewUrl);
  expect(acceptedResponse.ok()).toBe(true);
  const previewHeaders = acceptedResponse.headers();
  const previewCsp = previewHeaders["content-security-policy"];
  expect(previewCsp).toContain("default-src 'none'");
  expect(previewCsp).toMatch(/script-src 'self' 'nonce-[^']+'/);
  expect(previewCsp).toContain("worker-src 'none'");
  expect(previewCsp).toContain("connect-src 'none'");
  expect(previewCsp).toContain("frame-src 'none'");
  expect(previewCsp).toContain("webrtc 'block'");
  expect(previewCsp).toContain("form-action 'none'");
  expect(previewCsp).toContain(`frame-ancestors ${editorOrigin}`);
  expect(previewHeaders["clear-site-data"]).toBe(
    '"cache", "cookies", "storage"',
  );
  expect(previewHeaders["cache-control"]).toBe("no-store");
  expect(previewHeaders["x-content-type-options"]).toBe("nosniff");
  expect(previewHeaders["referrer-policy"]).toBe("no-referrer");
  expect(previewHeaders["x-dns-prefetch-control"]).toBe("off");
  const acceptedHtml = await acceptedResponse.text();
  expect(acceptedHtml).not.toContain(bootstrap.session.token);
  expect(acceptedHtml).not.toContain("LP_STUDIO_STATE_ROOT");

  const changedHex = scopedA.hostname[3] === "0" ? "1" : "0";
  const tamperedHost = new URL(projectA.previewUrl);
  tamperedHost.hostname = `${scopedA.hostname.slice(0, 3)}${changedHex}${scopedA.hostname.slice(4)}`;
  const foreignProject = new URL(projectA.previewUrl);
  foreignProject.pathname = foreignProject.pathname.replace(
    `/preview/${projectA.id}/`,
    "/preview/prj_foreign/",
  );
  const foreignSnapshot = new URL(projectA.previewUrl);
  foreignSnapshot.pathname = foreignSnapshot.pathname.replace(
    `/${projectA.revisionId}/`,
    "/rev_foreign/",
  );
  const missingSlash = new URL(projectA.previewUrl);
  missingSlash.pathname = missingSlash.pathname.slice(0, -1);
  const unscoped = new URL(projectA.previewUrl);
  unscoped.hostname = "localhost";
  const listenerOrigin = new URL(projectA.previewUrl);
  listenerOrigin.hostname = "127.0.0.1";

  const rejectedResponses = await Promise.all(
    [
      tamperedHost,
      foreignProject,
      foreignSnapshot,
      missingSlash,
      unscoped,
      listenerOrigin,
    ].map((url) => page.request.get(url.href)),
  );
  const rejectedBodies = await Promise.all(
    rejectedResponses.map((response) => response.text()),
  );
  for (const response of rejectedResponses) {
    expect(response.status()).toBe(404);
    expect(response.headers()["cache-control"]).toBe("no-store");
    expect(response.headers()["x-content-type-options"]).toBe("nosniff");
  }
  expect(new Set(rejectedBodies).size).toBe(1);

  const mountPreview = async (id: string, url: string) => {
    await page.evaluate(
      ({ frameId, previewUrl }) =>
        new Promise<void>((resolveLoad, rejectLoad) => {
          const iframe = document.createElement("iframe");
          iframe.id = frameId;
          iframe.title = frameId;
          iframe.sandbox.add("allow-scripts", "allow-same-origin");
          iframe.referrerPolicy = "no-referrer";
          iframe.onload = () => resolveLoad();
          iframe.onerror = () => rejectLoad(new Error("preview load failed"));
          iframe.src = previewUrl;
          document.body.append(iframe);
        }),
      { frameId: id, previewUrl: url },
    );
    const handle = await page.locator(`#${id}`).elementHandle();
    const frame = await handle?.contentFrame();
    if (frame === null || frame === undefined) {
      throw new Error(`Preview frame ${id} was not attached`);
    }
    expect(frame.url()).toBe(url);
    return frame;
  };

  const frameA = await mountPreview("isolation-preview-a", projectA.previewUrl);
  const frameB = await mountPreview("isolation-preview-b", projectB.previewUrl);
  const storageSupport = await frameA.evaluate(async () => {
    localStorage.setItem("lp-scope-canary", "project-a");
    document.cookie = "lp_scope_canary=project-a; SameSite=Strict";
    window.name = "project-a-window-name";
    await new Promise<void>((resolveDatabase, rejectDatabase) => {
      const request = indexedDB.open("lp-scope-canary", 1);
      request.onerror = () => rejectDatabase(request.error);
      request.onupgradeneeded = () =>
        request.result.createObjectStore("values");
      request.onsuccess = () => {
        request.result.close();
        resolveDatabase();
      };
    });
    const cacheAvailable = "caches" in window;
    if (cacheAvailable) await caches.open("lp-scope-canary");
    const received: unknown[] = [];
    const channel = new BroadcastChannel("lp-scope-canary");
    channel.onmessage = (event) => received.push(event.data);
    Object.assign(window, {
      __LP_STUDIO_E2E_BROADCAST_CHANNEL__: channel,
      __LP_STUDIO_E2E_BROADCAST_RECEIVED__: received,
    });
    return { cacheAvailable };
  });
  expect(storageSupport.cacheAvailable).toBe(true);

  const isolatedState = await frameB.evaluate(async () => {
    const databaseNames =
      typeof indexedDB.databases === "function"
        ? (await indexedDB.databases()).map((database) => database.name)
        : [];
    const cacheNames = "caches" in window ? await caches.keys() : [];
    const channel = new BroadcastChannel("lp-scope-canary");
    channel.postMessage("must-not-cross-project-origin");
    await new Promise((resolveWait) => setTimeout(resolveWait, 100));
    channel.close();
    return {
      localStorage: localStorage.getItem("lp-scope-canary"),
      cookie: document.cookie,
      windowName: window.name,
      databaseNames,
      cacheNames,
    };
  });
  expect(isolatedState.localStorage).toBeNull();
  expect(isolatedState.cookie).not.toContain("lp_scope_canary");
  expect(isolatedState.windowName).toBe("");
  expect(isolatedState.databaseNames).not.toContain("lp-scope-canary");
  expect(isolatedState.cacheNames).not.toContain("lp-scope-canary");
  const broadcastReceived = await frameA.evaluate(
    () =>
      (
        window as Window & {
          __LP_STUDIO_E2E_BROADCAST_RECEIVED__?: unknown[];
        }
      ).__LP_STUDIO_E2E_BROADCAST_RECEIVED__ ?? [],
  );
  expect(broadcastReceived).toEqual([]);

  const privilegedRequests: string[] = [];
  const workerRequests: string[] = [];
  const blockedNavigations: string[] = [];
  const observeRequest = (request: { url(): string }) => {
    if (request.url() === `${editorOrigin}/api/v1/bootstrap`) {
      privilegedRequests.push(request.url());
    }
    if (request.url().endsWith("/styles.css"))
      workerRequests.push(request.url());
    if (request.url().includes("blocked.invalid")) {
      blockedNavigations.push(request.url());
    }
  };
  page.on("request", observeRequest);
  const capabilityAttempts = await frameB.evaluate(async (origin) => {
    let editorFetch = "resolved";
    try {
      await fetch(`${origin}/api/v1/bootstrap`, { credentials: "include" });
    } catch {
      editorFetch = "blocked";
    }
    let serviceWorker = "unavailable";
    if ("serviceWorker" in navigator) {
      try {
        await navigator.serviceWorker.register("styles.css");
        serviceWorker = "registered";
      } catch {
        serviceWorker = "blocked";
      }
    }
    let peerConnection = "unavailable";
    if ("RTCPeerConnection" in window) {
      try {
        const connection = new RTCPeerConnection({
          iceServers: [{ urls: "stun:blocked.invalid:3478" }],
        });
        connection.close();
        peerConnection = "opened";
      } catch {
        peerConnection = "blocked";
      }
    }
    let editorDom = "reachable";
    try {
      void parent.document.documentElement;
    } catch {
      editorDom = "blocked";
    }
    let documentReplacement = "replaced";
    try {
      document.open();
      document.write("<h1>replacement bypass</h1>");
      document.close();
    } catch {
      // A throwing deny shim is also fail-closed.
    }
    if (document.querySelector('[data-lp-id="hero-heading"]') !== null) {
      documentReplacement = "blocked";
    }
    return {
      editorFetch,
      serviceWorker,
      peerConnection,
      editorDom,
      documentReplacement,
      popup: window.open("about:blank") === null ? "blocked" : "opened",
    };
  }, editorOrigin);
  expect(capabilityAttempts).toEqual({
    editorFetch: "blocked",
    serviceWorker: "blocked",
    peerConnection: "blocked",
    editorDom: "blocked",
    documentReplacement: "blocked",
    popup: "blocked",
  });
  expect(privilegedRequests).toEqual([]);
  expect(workerRequests).toEqual([]);

  await frameB.evaluate(() => {
    const refresh = document.createElement("meta");
    refresh.httpEquiv = "refresh";
    refresh.content =
      "0;url=https://blocked.invalid/meta-refresh-navigation-canary";
    document.head.append(refresh);
  });
  await page.waitForTimeout(200);
  expect(frameB.url()).toBe(projectB.previewUrl);
  expect(blockedNavigations).toEqual([]);

  await frameB.evaluate(() => {
    const NativeUrl = URL;
    Object.defineProperty(globalThis, "URL", {
      configurable: true,
      value: class ForgedUrl extends NativeUrl {
        constructor() {
          super(window.location.href);
        }
      },
    });
    Object.defineProperty(NavigationDestination.prototype, "url", {
      configurable: true,
      get: () => window.location.href,
    });
    Object.defineProperty(Event.prototype, "preventDefault", {
      configurable: true,
      value: () => undefined,
    });
    window.location.href = "https://blocked.invalid/preview-navigation-canary";
  });
  await page.waitForTimeout(200);
  expect(frameB.url()).toBe(projectB.previewUrl);
  expect(blockedNavigations).toEqual([]);

  const nestedNavigationAttempt = await frameB.evaluate(() => {
    const nested = document.createElement("iframe");
    nested.src = "about:blank";
    document.body.append(nested);
    try {
      nested.contentWindow?.eval(
        'parent.location.href="https://blocked.invalid/nested-navigation-canary"',
      );
      return "attempted";
    } catch {
      return "blocked";
    }
  });
  expect(["attempted", "blocked"]).toContain(nestedNavigationAttempt);
  await page.waitForTimeout(200);
  expect(frameB.url()).toBe(projectB.previewUrl);
  expect(blockedNavigations).toEqual([]);
  page.off("request", observeRequest);

  await frameA.goto(projectB.previewUrl);
  expect(
    await frameA.evaluate(() => ({
      localStorage: localStorage.getItem("lp-scope-canary"),
      cookie: document.cookie,
      windowName: window.name,
    })),
  ).toEqual({ localStorage: null, cookie: "", windowName: "" });

  const secondBootstrap = await page.evaluate(async () => {
    const response = await fetch("/api/v1/bootstrap", {
      credentials: "omit",
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
    return (await response.json()) as unknown;
  });
  expect(isBootstrapResponse(secondBootstrap)).toBe(true);
  const secondSessionProject = await page.evaluate(
    async ({ projectId, token }) => {
      const response = await fetch(
        `/api/v1/projects/${encodeURIComponent(projectId)}`,
        {
          credentials: "omit",
          cache: "no-store",
          headers: {
            Accept: "application/json",
            Authorization: `Bearer ${token}`,
          },
        },
      );
      return (await response.json()) as unknown;
    },
    {
      projectId: projectA.id,
      token: (secondBootstrap as BootstrapResponse).session.token,
    },
  );
  expect(isProjectResponse(secondSessionProject)).toBe(true);
  const secondSessionUrl = (secondSessionProject as ProjectResponse).project
    .previewUrl;
  expectScopedPreviewUrl(
    secondSessionUrl,
    (secondBootstrap as BootstrapResponse).previewOrigin,
  );
  expect(new URL(secondSessionUrl).origin).not.toBe(scopedA.origin);
  await frameA.goto(secondSessionUrl);
  expect(
    await frameA.evaluate(() => ({
      localStorage: localStorage.getItem("lp-scope-canary"),
      cookie: document.cookie,
      windowName: window.name,
    })),
  ).toEqual({ localStorage: null, cookie: "", windowName: "" });
});

test("登録済みルートを正確にレビューしてコピーし、元sourceを変更しない", async ({
  page,
}) => {
  const editorOrigin = process.env.LP_STUDIO_E2E_EDITOR_ORIGIN;
  const importRoot = process.env.LP_STUDIO_E2E_IMPORT_ROOT;
  const expectedSourceSha256 = process.env.LP_STUDIO_E2E_IMPORT_SOURCE_SHA256;
  const expectedSourceBytes = process.env.LP_STUDIO_E2E_IMPORT_SOURCE_BYTES;
  if (
    editorOrigin === undefined ||
    importRoot === undefined ||
    expectedSourceSha256 === undefined ||
    expectedSourceBytes === undefined
  ) {
    throw new Error("E2E import fixture was not initialized by global setup");
  }

  const before = await importSourceSnapshot(importRoot);
  expect(before).toEqual({
    sha256: expectedSourceSha256,
    byteLength: Number(expectedSourceBytes),
  });

  await page.addInitScript(() => {
    const envelopes: unknown[] = [];
    Object.defineProperty(window, "__LP_STUDIO_E2E_BRIDGE_ENVELOPES__", {
      configurable: false,
      enumerable: false,
      value: envelopes,
      writable: false,
    });
    window.addEventListener(
      "message",
      (event) => {
        const data = event.data as unknown;
        if (
          typeof data === "object" &&
          data !== null &&
          "type" in data &&
          typeof data.type === "string" &&
          data.type.startsWith("synapsegit-lp.")
        ) {
          envelopes.push(structuredClone(data));
        }
      },
      true,
    );
  });
  const blockedExternalRequests: string[] = [];
  const blockedExternalFailures: string[] = [];
  const blockedExternalResponses: string[] = [];
  const missingAssetStatuses: number[] = [];
  page.on("request", (request) => {
    if (request.url().includes("blocked.invalid")) {
      blockedExternalRequests.push(request.url());
    }
  });
  page.on("requestfailed", (request) => {
    if (request.url().includes("blocked.invalid")) {
      blockedExternalFailures.push(request.failure()?.errorText ?? "unknown");
    }
  });
  page.on("response", (response) => {
    if (response.url().includes("blocked.invalid")) {
      blockedExternalResponses.push(response.url());
    }
    if (response.url().endsWith("/assets/missing.png")) {
      missingAssetStatuses.push(response.status());
    }
  });

  await page.goto(`${editorOrigin}/`);
  const previewResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      new URL(response.url()).pathname === "/api/v1/imports/previews",
  );
  await page
    .getByRole("button", { name: "登録済みディレクトリを取り込む" })
    .click();
  const previewResponse = await previewResponsePromise;
  expect(previewResponse.ok()).toBeTruthy();
  const previewPayload =
    (await previewResponse.json()) as ImportPreviewResponse;
  expectSchemaValid("importPreviewResponse", previewPayload);
  expect(isImportPreviewResponse(previewPayload)).toBe(true);
  expect(previewPayload.importPreview.entryPoint).toBe("index.html");
  expect(
    previewPayload.importPreview.included.map((file) => file.path),
  ).toEqual(["about.html", "assets/app.js", "assets/theme.css", "index.html"]);
  expect(previewPayload.importPreview.excluded).toEqual([
    { path: ".env", reason: "credential_material" },
  ]);

  const dialog = page.getByRole("dialog", {
    name: "取り込むファイルを確認",
  });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("取り込み元のファイルは変更しません");
  await expect(dialog).toContainText("assets/theme.css");
  await expect(dialog).toContainText("assets/app.js");
  await expect(dialog).toContainText("about.html");
  await expect(dialog).toContainText("index.html");
  await expect(dialog).toContainText(".env");
  await expect(dialog).toContainText("credential_material");
  await expect(dialog).toContainText(
    previewPayload.importPreview.manifestSha256,
  );
  await expect(dialog).toContainText("適用した上限");
  await expect(page.locator("body")).not.toContainText(importRoot);

  const confirmResponsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "POST" &&
      /^\/api\/v1\/imports\/[^/]+\/confirm$/.test(
        new URL(response.url()).pathname,
      ),
  );
  const confirmRequestPromise = page.waitForRequest(
    (request) =>
      request.method() === "POST" &&
      /^\/api\/v1\/imports\/[^/]+\/confirm$/.test(
        new URL(request.url()).pathname,
      ),
  );
  await dialog
    .getByRole("button", { name: "この内容をコピーして取り込む" })
    .click();
  const [confirmRequest, confirmResponse] = await Promise.all([
    confirmRequestPromise,
    confirmResponsePromise,
  ]);
  expect(confirmResponse.status()).toBe(201);
  expect(confirmRequest.postDataJSON()).toEqual({
    schemaVersion: "1",
    expectedManifestSha256: previewPayload.importPreview.manifestSha256,
  });
  expect(confirmRequest.postData()).not.toContain(importRoot);
  await expect(page.getByLabel("Accepted revision")).toBeVisible();
  await expect(page.getByTitle("LPプレビュー")).toBeVisible();
  await expect(
    page
      .frameLocator('iframe[title="LPプレビュー"]')
      .getByRole("heading", { name: "登録ルートから始めるLP" }),
  ).toBeVisible();
  const importedPreview = page.frameLocator('iframe[title="LPプレビュー"]');
  await expect(importedPreview.locator("html")).toHaveAttribute(
    "data-self-script",
    "executed",
  );
  expect(
    await importedPreview.locator("html").evaluate(
      () =>
        (
          window as Window & {
            __LP_STUDIO_INLINE_SCRIPT_MUST_NOT_RUN__?: boolean;
          }
        ).__LP_STUDIO_INLINE_SCRIPT_MUST_NOT_RUN__,
    ),
  ).toBeUndefined();
  await expect(
    page.getByText("source unavailable", { exact: true }),
  ).toBeVisible();
  await expect
    .poll(async () => {
      const envelopes = await page.evaluate(
        () =>
          (
            window as Window & {
              __LP_STUDIO_E2E_BRIDGE_ENVELOPES__?: unknown[];
            }
          ).__LP_STUDIO_E2E_BRIDGE_ENVELOPES__ ?? [],
      );
      return envelopes.filter((value): value is PreviewDiagnosticMessage =>
        isPreviewDiagnosticMessage(value),
      );
    })
    .toEqual(
      expect.arrayContaining([
        expect.objectContaining({ code: "csp_blocked" }),
        expect.objectContaining({ code: "site_error" }),
        expect.objectContaining({ code: "unhandled_rejection" }),
      ]),
    );
  const diagnosticEnvelopes = await page.evaluate(
    () =>
      (
        window as Window & {
          __LP_STUDIO_E2E_BRIDGE_ENVELOPES__?: unknown[];
        }
      ).__LP_STUDIO_E2E_BRIDGE_ENVELOPES__ ?? [],
  );
  for (const envelope of diagnosticEnvelopes.filter(
    (value): value is PreviewDiagnosticMessage =>
      isPreviewDiagnosticMessage(value),
  )) {
    expectSchemaValid("previewDiagnosticMessage", envelope);
    expect(Object.keys(envelope).sort()).toEqual(
      [
        "channelId",
        "code",
        "projectId",
        "revisionId",
        "schemaVersion",
        "severity",
        "snapshotId",
        "sourceUnavailable",
        "type",
      ].sort(),
    );
    const serialized = JSON.stringify(envelope);
    expect(serialized).not.toContain("blocked.invalid");
    expect(serialized).not.toContain("E2E isolated rejection");
    expect(serialized).not.toContain(importRoot);
  }
  expect(blockedExternalRequests).toContain(
    "https://blocked.invalid/e2e-preview-canary.png",
  );
  await expect
    .poll(() => blockedExternalFailures.length)
    .toBe(blockedExternalRequests.length);
  expect(blockedExternalResponses).toEqual([]);
  for (const failure of blockedExternalFailures) {
    expect(failure).toMatch(/^(?:csp|net::ERR_BLOCKED_BY_CLIENT)$/i);
  }
  expect(missingAssetStatuses).toEqual([404]);
  await page.getByRole("button", { name: "操作モード" }).click();
  await importedPreview.getByRole("link", { name: "同じLP内の詳細へ" }).click();
  await expect(
    importedPreview.getByRole("heading", { name: "同じLP内の詳細ページ" }),
  ).toBeVisible();
  await importedPreview.getByRole("link", { name: "トップへ戻る" }).click();
  await expect(
    importedPreview.getByRole("heading", { name: "登録ルートから始めるLP" }),
  ).toBeVisible();
  await expect(page.locator("body")).not.toContainText(importRoot);

  const afterImport = await importSourceSnapshot(importRoot);
  expect(afterImport).toEqual(before);

  await page.reload();
  const displayName = basename(importRoot);
  const openRetained = page.getByRole("button", {
    name: `${displayName}を開く`,
  });
  await expect(openRetained).toBeVisible();
  await openRetained.click();
  await expect(
    page
      .frameLocator('iframe[title="LPプレビュー"]')
      .getByRole("heading", { name: "登録ルートから始めるLP" }),
  ).toBeVisible();
  await expect(page.locator("body")).not.toContainText(importRoot);

  const afterReopen = await importSourceSnapshot(importRoot);
  expect(afterReopen).toEqual(before);
});
