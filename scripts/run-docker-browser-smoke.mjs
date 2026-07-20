import { chromium } from "@playwright/test";

class SmokeFailure extends Error {}

const parseArguments = () => {
  const options = {
    editorOrigin: "http://127.0.0.1:4173",
    expectImport: false,
    expectOpenAi: false,
    mode: null,
  };
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--") {
      continue;
    } else if (argument === "--editor-origin") {
      options.editorOrigin = args[++index];
    } else if (argument === "--create") {
      options.mode = "create";
    } else if (argument === "--expect-existing") {
      options.mode = "expect-existing";
    } else if (argument === "--expect-import") {
      options.expectImport = true;
    } else if (argument === "--expect-openai") {
      options.expectOpenAi = true;
    } else {
      throw new SmokeFailure("unknown_argument");
    }
  }
  let editor;
  try {
    editor = new URL(options.editorOrigin);
  } catch {
    throw new SmokeFailure("editor_origin_invalid");
  }
  if (
    editor.protocol !== "http:" ||
    editor.hostname !== "127.0.0.1" ||
    editor.port.length === 0 ||
    editor.pathname !== "/" ||
    options.mode === null
  ) {
    throw new SmokeFailure("arguments_invalid");
  }
  return {
    editorOrigin: editor.origin,
    expectImport: options.expectImport,
    expectOpenAi: options.expectOpenAi,
    mode: options.mode,
  };
};

const main = async () => {
  const options = parseArguments();
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    let pageFailed = false;
    page.on("pageerror", () => {
      pageFailed = true;
    });
    const bootstrapResponsePromise = page.waitForResponse(
      (response) =>
        response.request().method() === "GET" &&
        new URL(response.url()).pathname === "/api/v1/bootstrap",
    );
    await page.goto(`${options.editorOrigin}/`, {
      waitUntil: "domcontentloaded",
    });
    const bootstrapResponse = await bootstrapResponsePromise;
    if (!bootstrapResponse.ok()) throw new SmokeFailure("bootstrap_failed");
    const bootstrap = await bootstrapResponse.json();
    if (
      bootstrap?.schemaVersion !== "1" ||
      bootstrap?.editorOrigin !== options.editorOrigin ||
      bootstrap?.previewOrigin !== "http://localhost:4174" ||
      typeof bootstrap?.session?.token !== "string" ||
      bootstrap.session.token.length === 0 ||
      (options.expectImport &&
        bootstrap?.capabilities?.importAvailable !== true) ||
      (options.expectOpenAi &&
        !bootstrap?.capabilities?.aiProviders?.some(
          (provider) =>
            provider?.id === "openai" && provider?.availability === "available",
        ))
    ) {
      throw new SmokeFailure("bootstrap_contract_invalid");
    }

    if (options.mode === "create") {
      await page.getByRole("button", { name: "空のLPを作成" }).click();
      const previewElement = page.getByTitle("LPプレビュー");
      await previewElement.waitFor({ state: "visible" });
      const source = await previewElement.getAttribute("src");
      if (source === null) throw new SmokeFailure("preview_source_missing");
      const preview = new URL(source, options.editorOrigin);
      if (
        preview.protocol !== "http:" ||
        !/^pv-[0-9a-f]{32}\.localhost$/u.test(preview.hostname) ||
        preview.port !== "4174" ||
        !/^\/preview\/[^/]+\/[^/]+\/$/u.test(preview.pathname)
      ) {
        throw new SmokeFailure("preview_scope_invalid");
      }
      await page
        .frameLocator('iframe[title="LPプレビュー"]')
        .getByRole("heading", { name: "まだ、白紙です。" })
        .waitFor({ state: "visible" });
    }

    const projectsResponse = await page.request.get(
      `${options.editorOrigin}/api/v1/projects`,
      {
        headers: {
          Authorization: `Bearer ${bootstrap.session.token}`,
          Origin: options.editorOrigin,
        },
      },
    );
    if (!projectsResponse.ok()) throw new SmokeFailure("project_list_failed");
    const projects = await projectsResponse.json();
    if (
      projects?.schemaVersion !== "1" ||
      !Array.isArray(projects?.projects) ||
      projects.projects.length === 0
    ) {
      throw new SmokeFailure("persisted_project_missing");
    }
    if (pageFailed) throw new SmokeFailure("browser_page_error");

    process.stdout.write(
      `${JSON.stringify({
        schemaVersion: "synapsegit-lp-studio.docker-browser-smoke/1",
        status: "passed",
        mode: options.mode,
        projectCountMinimum: 1,
        previewScopeVerified: options.mode === "create",
        importAvailableVerified: options.expectImport,
        openAiSecretVerified: options.expectOpenAi,
      })}\n`,
    );
  } finally {
    await browser.close();
  }
};

await main().catch((error) => {
  process.stderr.write(
    `docker_browser_smoke_failed: ${
      error instanceof SmokeFailure ? error.message : "unexpected_failure"
    }\n`,
  );
  process.exitCode = 1;
});
