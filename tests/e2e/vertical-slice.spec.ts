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
  isPreviewSelectionMessage,
  isProjectResponse,
  isProjectsResponse,
  isProposalResponse,
  isTargetResponse,
  type BootstrapResponse,
  type ExportResponse,
  type ImportPreviewResponse,
} from "../../packages/contracts/src/index";
import apiSchema from "../../packages/contracts/schemas/api-v1.schema.json";

const INITIAL_HEADING = "まだ、白紙です。";
const INITIAL_COPY =
  "伝えたいことを選び、AIとの対話から最初の一歩をつくります。";
const PROPOSED_HEADING = "対話から、公開できるLPへ。";
const PROMPT_CANARY =
  "ヒーロー見出しを明確にしてください。E2E_PRIVATE_PROMPT_CANARY";

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

test("blank Targetからfake AI Proposalを採用し、pureなAccepted exportを得る", async ({
  page,
}) => {
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
  const previewOrigin = new URL(previewSource!, editorOrigin).origin;
  expect(previewOrigin).toBe(configuredPreviewOrigin);
  expect(previewOrigin).not.toBe(editorOrigin);

  const preview = page.frameLocator('iframe[title="LPプレビュー"]');
  await expect(
    preview.getByRole("heading", { name: INITIAL_HEADING }),
  ).toBeVisible();

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
  const targetRegion = page.getByRole("complementary", {
    name: "選択中のターゲット",
  });
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
  const selectionEnvelopes = bridgeEnvelopes.filter(
    (value): value is Record<string, unknown> =>
      typeof value === "object" &&
      value !== null &&
      "type" in value &&
      value.type === "synapsegit-lp.selection",
  );
  const actionEnvelopes = bridgeEnvelopes.filter(
    (value): value is Record<string, unknown> =>
      typeof value === "object" &&
      value !== null &&
      "type" in value &&
      value.type === "synapsegit-lp.action",
  );
  expect(selectionEnvelopes.length).toBeGreaterThan(0);
  expect(actionEnvelopes.length).toBeGreaterThan(0);
  for (const envelope of selectionEnvelopes) {
    expectSchemaValid("previewSelectionMessage", envelope);
    expect(isPreviewSelectionMessage(envelope)).toBe(true);
  }
  for (const envelope of actionEnvelopes) {
    expectSchemaValid("previewActionMessage", envelope);
    expect(isPreviewActionMessage(envelope)).toBe(true);
  }
  const selectionWithExtraField = {
    ...selectionEnvelopes[0],
    unexpected: true,
  };
  expectSchemaRejected("previewSelectionMessage", selectionWithExtraField);
  expect(isPreviewSelectionMessage(selectionWithExtraField)).toBe(false);
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
  ).toEqual(["assets/theme.css", "index.html"]);
  expect(previewPayload.importPreview.excluded).toEqual([
    { path: ".env", reason: "credential_material" },
  ]);

  const dialog = page.getByRole("dialog", {
    name: "取り込むファイルを確認",
  });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("取り込み元のファイルは変更しません");
  await expect(dialog).toContainText("assets/theme.css");
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
