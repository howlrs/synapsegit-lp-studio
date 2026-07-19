import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { arch, platform } from "node:os";
import { isAbsolute, resolve } from "node:path";
import { chromium } from "@playwright/test";
import Ajv2020 from "ajv/dist/2020.js";

const RESULT_SCHEMA_VERSION =
  "synapsegit-lp-studio.production-integration-performance-result/1";
const PROFILE_SCHEMA_VERSION =
  "synapsegit-lp-studio.production-integration-performance-profile/1";
const repositoryRoot = resolve(import.meta.dirname, "..");
const defaultProfilePath = resolve(
  repositoryRoot,
  "docs/evidence/fixtures/m1-production-integration-performance-profile.v1.json",
);
const resultSchemaPath = resolve(
  repositoryRoot,
  "docs/evidence/schemas/m1-production-integration-performance-result.schema.v1.json",
);
const scopedPreviewHost = /^pv-[0-9a-f]{32}\.localhost$/u;
let activeStep = "initialization";
const enterStep = (step) => {
  activeStep = step;
};

class MeasurementFailure extends Error {
  constructor(code) {
    super(code);
    this.code = code;
  }
}

const parseArguments = () => {
  const options = {
    profilePath: defaultProfilePath,
    outputPath: null,
    editorOrigin: null,
    changeSetProbe: null,
  };
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--") {
      continue;
    } else if (argument === "--editor-origin") {
      options.editorOrigin = args[++index] ?? null;
    } else if (argument === "--change-set-probe") {
      options.changeSetProbe = resolve(args[++index] ?? "");
    } else if (argument === "--profile") {
      options.profilePath = resolve(args[++index] ?? "");
    } else if (argument === "--output") {
      options.outputPath = resolve(args[++index] ?? "");
    } else {
      throw new MeasurementFailure("unknown_argument");
    }
  }
  let origin;
  try {
    origin = new URL(options.editorOrigin ?? "");
  } catch {
    throw new MeasurementFailure("editor_origin_invalid");
  }
  if (
    origin.protocol !== "http:" ||
    origin.hostname !== "127.0.0.1" ||
    origin.port.length === 0 ||
    origin.pathname !== "/" ||
    origin.search.length !== 0 ||
    origin.hash.length !== 0
  ) {
    throw new MeasurementFailure("editor_origin_not_scoped_loopback");
  }
  if (options.changeSetProbe === null || !isAbsolute(options.changeSetProbe)) {
    throw new MeasurementFailure("change_set_probe_required");
  }
  options.editorOrigin = origin.origin;
  return options;
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const rounded = (value) => Math.round(value * 1000) / 1000;
const percentile95 = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * 0.95) - 1)];
};
const exactArray = (value, expected) =>
  Array.isArray(value) && JSON.stringify(value) === JSON.stringify(expected);
const exactKeys = (value, expected) =>
  value !== null &&
  typeof value === "object" &&
  !Array.isArray(value) &&
  exactArray(Object.keys(value).sort(), [...expected].sort());

const validateProfile = (profile) => {
  if (
    !exactKeys(profile, [
      "schemaVersion",
      "scope",
      "runnerVersion",
      "fixture",
      "geometryMatrix",
      "geometryThresholds",
      "autosave",
      "changeSet",
      "thresholdPolicy",
      "hardGates",
      "manualEvidence",
      "claimBoundary",
    ]) ||
    profile.schemaVersion !== PROFILE_SCHEMA_VERSION ||
    profile.scope !== "packaged_local_production_application_integration" ||
    profile.runnerVersion !==
      "synapsegit-lp-studio.production-integration-performance-runner/1" ||
    !exactKeys(profile.fixture, [
      "zoomRootElementId",
      "targetElementId",
      "outerScrollElementId",
      "innerScrollElementId",
    ]) ||
    profile.fixture.zoomRootElementId !== "production-performance-zoom-root" ||
    profile.fixture.targetElementId !== "production-performance-target" ||
    profile.fixture.outerScrollElementId !==
      "production-performance-outer-scroll" ||
    profile.fixture.innerScrollElementId !==
      "production-performance-inner-scroll" ||
    !exactKeys(profile.geometryMatrix, [
      "viewports",
      "deviceScaleFactors",
      "contentZoomFactors",
      "previewScales",
      "scrollCases",
    ]) ||
    !exactArray(profile.geometryMatrix.viewports, [
      { name: "desktop", width: 1280 },
      { name: "tablet", width: 768 },
      { name: "mobile", width: 375 },
    ]) ||
    !exactArray(profile.geometryMatrix.deviceScaleFactors, [1, 2]) ||
    !exactArray(profile.geometryMatrix.contentZoomFactors, [1, 1.25, 2]) ||
    !exactArray(profile.geometryMatrix.previewScales, [0.75, 1]) ||
    !exactArray(profile.geometryMatrix.scrollCases, [
      { name: "origin", windowTop: 0, outerTop: 0, innerLeft: 0 },
      {
        name: "nested-scroll",
        windowTop: 48,
        outerTop: 72,
        innerLeft: 56,
      },
    ]) ||
    !exactKeys(profile.geometryThresholds, [
      "overlayMaximumErrorCssPx",
      "overlayFeedbackP95Ms",
    ]) ||
    profile.geometryThresholds.overlayMaximumErrorCssPx !== 2 ||
    profile.geometryThresholds.overlayFeedbackP95Ms !== 100 ||
    !exactKeys(profile.autosave, [
      "warmupSampleCount",
      "sampleCount",
      "finalInputToCompletionP95Ms",
    ]) ||
    profile.autosave.warmupSampleCount !== 2 ||
    profile.autosave.sampleCount !== 20 ||
    profile.autosave.finalInputToCompletionP95Ms !== 1000 ||
    !exactKeys(profile.changeSet, [
      "probeSchemaVersion",
      "warmupSampleCount",
      "sampleCount",
      "changedTextFileCount",
      "totalChangedTextBytes",
      "validationAndApplicationP95Ms",
    ]) ||
    profile.changeSet.probeSchemaVersion !==
      "synapsegit-lp-studio.change-set-performance-probe/1" ||
    profile.changeSet.warmupSampleCount !== 2 ||
    profile.changeSet.sampleCount !== 20 ||
    profile.changeSet.changedTextFileCount !== 10 ||
    profile.changeSet.totalChangedTextBytes !== 2_097_152 ||
    profile.changeSet.validationAndApplicationP95Ms !== 2000 ||
    profile.thresholdPolicy !== "advisory_on_uncharacterized_ci" ||
    !exactArray(profile.hardGates, [
      "packaged App import reaches a scoped Preview iframe",
      "all 72 matrix cases across 3 configured and distinct rendered Preview widths, 2 DPRs, 3 content zooms, 2 Preview scales, and 2 scroll states traverse preview_target_runtime, the bound bridge, Target API, and the runtime overlay",
      "maximum overlay-to-element error is at most 2 CSS px",
      "autosave samples complete through the production App and Project metadata API",
      "the production ChangeSet parser validates and applies exactly 10 text files totaling 2 MiB",
      "raw samples and recomputable aggregates are retained without project data or private paths",
    ]) ||
    !exactKeys(profile.manualEvidence, [
      "creatorEndToEndVerification",
      "screenReader",
      "liveProvider",
    ]) ||
    profile.manualEvidence.creatorEndToEndVerification !== "pending" ||
    profile.manualEvidence.screenReader !== "pending" ||
    profile.manualEvidence.liveProvider !==
      "pending_external_credential_and_billing_acknowledgement" ||
    !exactKeys(profile.claimBoundary, [
      "crossBrowserSupport",
      "productionReady",
      "releaseCreated",
    ]) ||
    profile.claimBoundary.crossBrowserSupport !== false ||
    profile.claimBoundary.productionReady !== false ||
    profile.claimBoundary.releaseCreated !== false
  ) {
    throw new MeasurementFailure("profile_invalid");
  }
};

const loadResultValidator = async () => {
  try {
    const schema = JSON.parse(await readFile(resultSchemaPath, "utf8"));
    return new Ajv2020({
      allErrors: true,
      strict: true,
      strictRequired: false,
    }).compile(schema);
  } catch {
    throw new MeasurementFailure("result_schema_invalid");
  }
};

const geometryCaseId = ({
  viewport,
  deviceScaleFactor,
  contentZoomFactor,
  previewScale,
  scrollCase,
}) =>
  `${viewport}:dpr=${deviceScaleFactor}:zoom=${contentZoomFactor}:preview=${previewScale}:scroll=${scrollCase}`;

const expectedCaseIds = (profile) => {
  const ids = [];
  for (const deviceScaleFactor of profile.geometryMatrix.deviceScaleFactors) {
    for (const viewport of profile.geometryMatrix.viewports) {
      for (const contentZoomFactor of profile.geometryMatrix
        .contentZoomFactors) {
        for (const previewScale of profile.geometryMatrix.previewScales) {
          for (const scrollCase of profile.geometryMatrix.scrollCases) {
            ids.push(
              geometryCaseId({
                viewport: viewport.name,
                deviceScaleFactor,
                contentZoomFactor,
                previewScale,
                scrollCase: scrollCase.name,
              }),
            );
          }
        }
      }
    }
  }
  return ids;
};

const runChangeSetProbe = (binary, profile) =>
  new Promise((resolveProbe, rejectProbe) => {
    execFile(
      binary,
      [
        "--warmup-samples",
        String(profile.changeSet.warmupSampleCount),
        "--samples",
        String(profile.changeSet.sampleCount),
      ],
      {
        cwd: repositoryRoot,
        encoding: "utf8",
        timeout: 120_000,
        maxBuffer: 8 * 1024 * 1024,
        env: {
          PATH: process.env.PATH,
          HOME: process.env.HOME,
          LANG: "C.UTF-8",
          LC_ALL: "C.UTF-8",
          TZ: "UTC",
        },
      },
      (error, stdout) => {
        if (error !== null) {
          rejectProbe(new MeasurementFailure("change_set_probe_failed"));
          return;
        }
        try {
          resolveProbe(JSON.parse(stdout.trim().split(/\r?\n/u).at(-1)));
        } catch {
          rejectProbe(
            new MeasurementFailure("change_set_probe_result_invalid"),
          );
        }
      },
    );
  });

const validateChangeSetProbe = (probe, profile) => {
  if (
    !exactKeys(probe, [
      "schemaVersion",
      "fixture",
      "warmupSampleCount",
      "sampleCount",
      "rawSamplesMs",
      "p95Ms",
      "outputManifestSha256",
    ]) ||
    probe.schemaVersion !== profile.changeSet.probeSchemaVersion ||
    !exactKeys(probe.fixture, [
      "changedTextFileCount",
      "totalChangedTextBytes",
      "operationCount",
      "changeSetSha256",
    ]) ||
    probe.fixture.changedTextFileCount !==
      profile.changeSet.changedTextFileCount ||
    probe.fixture.totalChangedTextBytes !==
      profile.changeSet.totalChangedTextBytes ||
    probe.fixture.operationCount !== profile.changeSet.changedTextFileCount ||
    !/^[0-9a-f]{64}$/u.test(probe.fixture.changeSetSha256) ||
    probe.warmupSampleCount !== profile.changeSet.warmupSampleCount ||
    probe.sampleCount !== profile.changeSet.sampleCount ||
    !Array.isArray(probe.rawSamplesMs) ||
    probe.rawSamplesMs.length !== profile.changeSet.sampleCount ||
    probe.rawSamplesMs.some(
      (sample) => !Number.isFinite(sample) || sample <= 0 || sample > 60_000,
    ) ||
    probe.p95Ms !== percentile95(probe.rawSamplesMs) ||
    !/^[0-9a-f]{64}$/u.test(probe.outputManifestSha256)
  ) {
    throw new MeasurementFailure("change_set_probe_result_invalid");
  }
};

const installBridgeRecorder = async (page) => {
  await page.addInitScript(() => {
    const targetMessages = [];
    Object.defineProperty(window, "__LP_STUDIO_PRODUCTION_TARGET_MESSAGES__", {
      configurable: false,
      enumerable: false,
      value: targetMessages,
      writable: false,
    });
    window.addEventListener("message", (event) => {
      const message = event.data;
      if (
        message !== null &&
        typeof message === "object" &&
        message.type === "synapsegit-lp.target-draft" &&
        message.schemaVersion === "1"
      ) {
        targetMessages.push({ origin: event.origin, message });
        if (targetMessages.length > 200) targetMessages.shift();
      }
    });
  });
};

const importFixtureThroughApp = async (page, editorOrigin, profile) => {
  enterStep("app_navigation");
  await page.goto(`${editorOrigin}/`, { waitUntil: "load", timeout: 15_000 });
  enterStep("app_import_preview");
  await page
    .getByRole("button", {
      name: "登録済みディレクトリを取り込む",
    })
    .click();
  const dialog = page.getByRole("dialog", { name: "取り込むファイルを確認" });
  await dialog.waitFor({ state: "visible", timeout: 15_000 });
  enterStep("app_import_confirm");
  await dialog
    .getByRole("button", { name: "この内容をコピーして取り込む" })
    .click();
  const iframe = page.locator('iframe[title="LPプレビュー"]');
  await iframe.waitFor({ state: "visible", timeout: 15_000 });
  const scopedOrigin = await iframe.evaluate((element) => {
    const url = new URL(element.src);
    return {
      protocol: url.protocol,
      hostname: url.hostname,
      port: url.port,
      origin: url.origin,
      pathname: url.pathname,
      sandbox: element.getAttribute("sandbox"),
    };
  });
  if (
    scopedOrigin.protocol !== "http:" ||
    !scopedPreviewHost.test(scopedOrigin.hostname) ||
    scopedOrigin.port.length === 0 ||
    !/^\/preview\/[^/]+\/[^/]+\/$/u.test(scopedOrigin.pathname) ||
    scopedOrigin.sandbox !== "allow-scripts allow-same-origin"
  ) {
    throw new MeasurementFailure("scoped_preview_iframe_invalid");
  }
  const frame = page.frameLocator('iframe[title="LPプレビュー"]');
  enterStep("app_import_fixture");
  await frame
    .locator(`#${profile.fixture.targetElementId}`)
    .waitFor({ state: "visible", timeout: 15_000 });
  return { iframe, frame, scopedOrigin: scopedOrigin.origin };
};

const isTargetApiResponse = (response) =>
  response.request().method() === "POST" &&
  /^\/api\/v1\/projects\/[^/]+\/targets$/u.test(
    new URL(response.url()).pathname,
  );

const establishAppBridgeBinding = async (
  page,
  frame,
  scopedOrigin,
  profile,
) => {
  enterStep("bridge_mode_toggle");
  const selectionMode = page
    .getByRole("group", { name: "プレビュー操作モード" })
    .getByRole("button", { name: "選択モード", exact: true });
  await selectionMode.click();
  enterStep("bridge_mode_confirmation");
  await page.waitForFunction(
    () =>
      document
        .querySelector('[aria-label="プレビュー操作モード"] button')
        ?.getAttribute("aria-pressed") === "true",
    undefined,
    { timeout: 10_000 },
  );
  await page.evaluate(
    () =>
      new Promise((resolveFrame) =>
        requestAnimationFrame(() => requestAnimationFrame(resolveFrame)),
      ),
  );
  const responsePromise = page.waitForResponse(isTargetApiResponse, {
    timeout: 15_000,
  });
  enterStep("bridge_initial_target");
  await frame.locator(`#${profile.fixture.targetElementId}`).click();
  enterStep("bridge_initial_response");
  const response = await responsePromise;
  if (response.status() !== 201) {
    throw new MeasurementFailure("target_api_binding_failed");
  }
  const targetResponse = await response.json();
  enterStep("bridge_binding_read");
  const binding = await page.evaluate((expectedOrigin) => {
    const messages = window.__LP_STUDIO_PRODUCTION_TARGET_MESSAGES__ ?? [];
    const observed = [...messages]
      .reverse()
      .find((entry) => entry.origin === expectedOrigin);
    if (observed === undefined) return null;
    const { message } = observed;
    return {
      channelId: message.channelId,
      projectId: message.projectId,
      snapshotId: message.snapshotId,
      revisionId: message.revisionId,
      targetId: message.target?.targetId,
    };
  }, scopedOrigin);
  if (
    binding === null ||
    !/^[A-Za-z0-9_-]{16,128}$/u.test(binding.channelId ?? "") ||
    typeof binding.projectId !== "string" ||
    typeof binding.snapshotId !== "string" ||
    typeof binding.revisionId !== "string" ||
    binding.targetId !== targetResponse?.target?.targetId
  ) {
    throw new MeasurementFailure("app_bridge_binding_failed");
  }
  enterStep("bridge_initial_clear");
  await page.getByRole("button", { name: /選択解除/u }).click();
  return binding;
};

const measureAutosaveSample = async (page, projectId, sampleName) => {
  const input = page.locator("#project-display-name");
  const status = page.locator("#project-display-name-status");
  enterStep("autosave_instrumentation");
  await page.evaluate(() => {
    const inputElement = document.getElementById("project-display-name");
    const statusElement = document.getElementById(
      "project-display-name-status",
    );
    if (!(inputElement instanceof HTMLInputElement) || statusElement === null) {
      throw new Error("autosave controls missing");
    }
    const state = { started: null, completed: null };
    const observer = new MutationObserver(() => {
      if (
        state.started !== null &&
        statusElement.textContent?.trim().startsWith("保存済み") === true
      ) {
        state.completed = performance.now();
        observer.disconnect();
      }
    });
    observer.observe(statusElement, {
      childList: true,
      characterData: true,
      subtree: true,
    });
    inputElement.addEventListener(
      "input",
      () => {
        state.started = performance.now();
      },
      { once: true },
    );
    window.__LP_STUDIO_AUTOSAVE_MEASUREMENT__ = state;
  });
  const responsePromise = page.waitForResponse(
    (response) =>
      response.request().method() === "PATCH" &&
      new URL(response.url()).pathname ===
        `/api/v1/projects/${encodeURIComponent(projectId)}`,
    { timeout: 15_000 },
  );
  enterStep("autosave_final_input");
  await input.fill(sampleName);
  await status.waitFor({ state: "visible" });
  enterStep("autosave_api_response");
  const response = await responsePromise;
  if (response.status() !== 200) {
    throw new MeasurementFailure("autosave_api_failed");
  }
  const responseValue = await response.json();
  if (
    responseValue?.project?.id !== projectId ||
    responseValue?.project?.displayName !== sampleName
  ) {
    throw new MeasurementFailure("autosave_project_binding_failed");
  }
  await page.waitForFunction(
    () => Number.isFinite(window.__LP_STUDIO_AUTOSAVE_MEASUREMENT__?.completed),
    undefined,
    { timeout: 15_000 },
  );
  const duration = await page.evaluate(() => {
    const state = window.__LP_STUDIO_AUTOSAVE_MEASUREMENT__;
    if (
      !Number.isFinite(state?.started) ||
      !Number.isFinite(state?.completed) ||
      state.completed <= state.started
    ) {
      return null;
    }
    return state.completed - state.started;
  });
  if (duration === null) {
    throw new MeasurementFailure("autosave_measurement_invalid");
  }
  return rounded(duration);
};

const measureAutosave = async (page, projectId, profile) => {
  const total =
    profile.autosave.warmupSampleCount + profile.autosave.sampleCount;
  const samples = [];
  for (let index = 0; index < total; index += 1) {
    const duration = await measureAutosaveSample(
      page,
      projectId,
      `Production measurement ${String(index + 1).padStart(2, "0")}`,
    );
    if (index >= profile.autosave.warmupSampleCount) samples.push(duration);
  }
  return samples;
};

const setMatrixViewport = async (page, viewport) => {
  enterStep("matrix_viewport");
  const custom = page.getByLabel("カスタム幅");
  await custom.fill(String(viewport.width));
  await page.waitForFunction(
    (expectedWidth) => {
      const frame = document.querySelector(".viewport-frame");
      const iframe = frame?.querySelector('iframe[title="LPプレビュー"]');
      return (
        frame instanceof HTMLElement &&
        iframe instanceof HTMLIFrameElement &&
        frame.style.width === `${String(expectedWidth)}px` &&
        frame.offsetWidth === expectedWidth &&
        frame.offsetWidth - iframe.clientWidth >= 0 &&
        frame.offsetWidth - iframe.clientWidth <= 2
      );
    },
    viewport.width,
    { timeout: 10_000 },
  );
};

const setPreviewScale = async (page, previewScale) => {
  enterStep("matrix_preview_scale");
  const suffix = previewScale === 0.75 ? "75" : "100";
  await page.getByTestId(`preview-scale-${suffix}`).click();
  await page.waitForFunction(
    (expectedScale) =>
      document
        .querySelector(".viewport-frame")
        ?.getAttribute("data-preview-scale") === String(expectedScale),
    previewScale,
    { timeout: 10_000 },
  );
  await page.evaluate(
    () =>
      new Promise((resolveFrame) =>
        requestAnimationFrame(() => requestAnimationFrame(resolveFrame)),
      ),
  );
};

const setContentGeometry = async (frame, profile, zoom, scrollCase) => {
  enterStep("matrix_content_geometry");
  return frame.locator("html").evaluate(
    (_html, options) => {
      const zoomRoot = document.getElementById(options.zoomRootId);
      if (!(zoomRoot instanceof HTMLElement)) {
        throw new Error("production geometry zoom root missing");
      }
      zoomRoot.style.zoom = String(options.zoom);
      window.scrollTo(0, options.scrollCase.windowTop);
      const outer = document.getElementById(options.outerId);
      const inner = document.getElementById(options.innerId);
      if (!(outer instanceof HTMLElement) || !(inner instanceof HTMLElement)) {
        throw new Error("production geometry fixture missing");
      }
      outer.scrollTop = options.scrollCase.outerTop;
      inner.scrollLeft = options.scrollCase.innerLeft;
      return new Promise((resolveFrame) =>
        requestAnimationFrame(() => requestAnimationFrame(resolveFrame)),
      );
    },
    {
      zoom,
      scrollCase,
      zoomRootId: profile.fixture.zoomRootElementId,
      outerId: profile.fixture.outerScrollElementId,
      innerId: profile.fixture.innerScrollElementId,
    },
  );
};

const clearSelection = async (page) => {
  const clear = page.getByRole("button", { name: /選択解除/u });
  if (await clear.isVisible()) await clear.click();
};

const measureGeometryCase = async ({
  page,
  iframe,
  frame,
  scopedOrigin,
  binding,
  profile,
  viewport,
  deviceScaleFactor,
  contentZoomFactor,
  previewScale,
  scrollCase,
}) => {
  enterStep("matrix_clear_selection");
  await clearSelection(page);
  await setPreviewScale(page, previewScale);
  await setContentGeometry(frame, profile, contentZoomFactor, scrollCase);
  enterStep("matrix_feedback_instrumentation");
  const overlay = frame.locator('[data-synapsegit-preview-overlay="true"]');
  await overlay.waitFor({ state: "detached", timeout: 10_000 });
  await frame.locator("html").evaluate(() => {
    const state = { started: null, completed: null };
    window.addEventListener(
      "pointerdown",
      () => {
        state.started = performance.now();
      },
      { capture: true, once: true },
    );
    const observer = new MutationObserver(() => {
      if (
        state.started === null ||
        document.querySelector('[data-synapsegit-preview-overlay="true"]') ===
          null
      ) {
        return;
      }
      requestAnimationFrame(() => {
        state.completed = performance.now();
        observer.disconnect();
      });
    });
    observer.observe(document.documentElement, { childList: true });
    window.__LP_STUDIO_OVERLAY_FEEDBACK_MEASUREMENT__ = state;
  });
  const responsePromise = page.waitForResponse(isTargetApiResponse, {
    timeout: 15_000,
  });
  enterStep("matrix_target_click");
  const target = frame.locator(`#${profile.fixture.targetElementId}`);
  const pointerBox = await target.boundingBox();
  const targetCenterHit = await target.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    const centerX = rect.x + rect.width / 2;
    const centerY = rect.y + rect.height / 2;
    const hit = document.elementFromPoint(centerX, centerY);
    return hit === element || element.contains(hit);
  });
  if (pointerBox === null || !targetCenterHit) {
    throw new MeasurementFailure("production_target_hit_test_failed");
  }
  await page.mouse.click(
    pointerBox.x + pointerBox.width / 2,
    pointerBox.y + pointerBox.height / 2,
  );
  enterStep("matrix_overlay_wait");
  await overlay.waitFor({ state: "visible", timeout: 10_000 });
  await frame.locator("html").evaluate(() => {
    const deadline = performance.now() + 10_000;
    return new Promise((resolveMeasurement, rejectMeasurement) => {
      const poll = () => {
        if (
          Number.isFinite(
            window.__LP_STUDIO_OVERLAY_FEEDBACK_MEASUREMENT__?.completed,
          )
        ) {
          resolveMeasurement();
          return;
        }
        if (performance.now() >= deadline) {
          rejectMeasurement(new Error("overlay feedback measurement timeout"));
          return;
        }
        requestAnimationFrame(poll);
      };
      poll();
    });
  });
  enterStep("matrix_target_response");
  const response = await responsePromise;
  if (response.status() !== 201) {
    throw new MeasurementFailure("target_api_round_trip_failed");
  }
  const responseValue = await response.json();
  enterStep("matrix_bridge_read");
  const bridgeValue = await page.evaluate(
    ({ expectedOrigin, expectedChannel, expectedTargetId }) => {
      const messages = window.__LP_STUDIO_PRODUCTION_TARGET_MESSAGES__ ?? [];
      const observed = [...messages]
        .reverse()
        .find(
          (entry) =>
            entry.origin === expectedOrigin &&
            entry.message?.channelId === expectedChannel &&
            entry.message?.target?.targetId === expectedTargetId,
        );
      return observed?.message?.target ?? null;
    },
    {
      expectedOrigin: scopedOrigin,
      expectedChannel: binding.channelId,
      expectedTargetId: responseValue?.target?.targetId,
    },
  );
  enterStep("matrix_measurement_read");
  const [
    targetBox,
    overlayBox,
    appliedScroll,
    frameIdentity,
    overlayFeedbackDurationMs,
  ] = await Promise.all([
    target.boundingBox(),
    overlay.boundingBox(),
    frame.locator("html").evaluate(
      (_html, options) => {
        const zoomRoot = document.getElementById(options.zoomRootId);
        const outer = document.getElementById(options.outerId);
        const inner = document.getElementById(options.innerId);
        return {
          windowTop: window.scrollY,
          outerTop: outer?.scrollTop,
          innerLeft: inner?.scrollLeft,
          zoom: Number(zoomRoot?.style.zoom),
        };
      },
      {
        zoomRootId: profile.fixture.zoomRootElementId,
        outerId: profile.fixture.outerScrollElementId,
        innerId: profile.fixture.innerScrollElementId,
      },
    ),
    iframe.evaluate((element) => ({
      origin: new URL(element.src).origin,
      previewScale: Number(
        element.closest(".viewport-frame")?.getAttribute("data-preview-scale"),
      ),
      renderedOuterFrameWidthCssPx:
        element.closest(".viewport-frame")?.offsetWidth,
      renderedIframeWidthCssPx: element.clientWidth,
    })),
    frame.locator("html").evaluate(() => {
      const state = window.__LP_STUDIO_OVERLAY_FEEDBACK_MEASUREMENT__;
      if (
        !Number.isFinite(state?.started) ||
        !Number.isFinite(state?.completed) ||
        state.completed < state.started
      ) {
        return null;
      }
      return state.completed - state.started;
    }),
  ]);
  if (
    targetBox === null ||
    overlayBox === null ||
    bridgeValue === null ||
    overlayFeedbackDurationMs === null
  ) {
    throw new MeasurementFailure("production_overlay_measurement_missing");
  }
  const viewportValue = bridgeValue.viewport;
  if (responseValue?.target?.targetId !== bridgeValue.targetId) {
    throw new MeasurementFailure("production_target_binding_mismatch");
  }
  if (
    responseValue?.target?.viewport?.previewScale !== previewScale ||
    viewportValue?.previewScale !== previewScale ||
    frameIdentity.previewScale !== previewScale
  ) {
    throw new MeasurementFailure("production_preview_scale_mismatch");
  }
  if (viewportValue?.devicePixelRatio !== deviceScaleFactor) {
    throw new MeasurementFailure("production_device_scale_mismatch");
  }
  if (frameIdentity.origin !== scopedOrigin) {
    throw new MeasurementFailure("production_preview_origin_mismatch");
  }
  if (
    frameIdentity.renderedOuterFrameWidthCssPx !== viewport.width ||
    frameIdentity.renderedIframeWidthCssPx < 1 ||
    frameIdentity.renderedOuterFrameWidthCssPx -
      frameIdentity.renderedIframeWidthCssPx <
      0 ||
    frameIdentity.renderedOuterFrameWidthCssPx -
      frameIdentity.renderedIframeWidthCssPx >
      2 ||
    viewportValue?.cssWidth !== frameIdentity.renderedIframeWidthCssPx
  ) {
    throw new MeasurementFailure("production_viewport_width_mismatch");
  }
  if (appliedScroll.zoom !== contentZoomFactor) {
    throw new MeasurementFailure("production_content_zoom_mismatch");
  }
  if (appliedScroll.windowTop !== scrollCase.windowTop)
    throw new MeasurementFailure("production_window_scroll_mismatch");
  if (appliedScroll.outerTop !== scrollCase.outerTop)
    throw new MeasurementFailure("production_outer_scroll_mismatch");
  if (appliedScroll.innerLeft !== scrollCase.innerLeft)
    throw new MeasurementFailure("production_inner_scroll_mismatch");
  const overlayErrorCssPx = rounded(
    Math.max(
      Math.abs(overlayBox.x - targetBox.x),
      Math.abs(overlayBox.y - targetBox.y),
      Math.abs(overlayBox.width - targetBox.width),
      Math.abs(overlayBox.height - targetBox.height),
    ),
  );
  return {
    caseId: geometryCaseId({
      viewport: viewport.name,
      deviceScaleFactor,
      contentZoomFactor,
      previewScale,
      scrollCase: scrollCase.name,
    }),
    viewport: viewport.name,
    configuredViewportWidthCssPx: viewport.width,
    deviceScaleFactor,
    contentZoomFactor,
    previewScale,
    scrollCase: scrollCase.name,
    windowScrollTopCssPx: appliedScroll.windowTop,
    outerScrollTopCssPx: appliedScroll.outerTop,
    innerScrollLeftCssPx: appliedScroll.innerLeft,
    renderedOuterFrameWidthCssPx: frameIdentity.renderedOuterFrameWidthCssPx,
    renderedIframeWidthCssPx: frameIdentity.renderedIframeWidthCssPx,
    runtimeViewportWidthCssPx: viewportValue.cssWidth,
    runtimeViewportHeightCssPx: viewportValue.cssHeight,
    runtimeDevicePixelRatio: viewportValue.devicePixelRatio,
    runtimePreviewScale: viewportValue.previewScale,
    overlayErrorCssPx,
    overlayFeedbackDurationMs: rounded(overlayFeedbackDurationMs),
    scopedPreviewOriginVerified: true,
    appBridgeTargetVerified: true,
    targetApiRoundTripVerified: true,
    actualOverlayRendered: true,
  };
};

const measureBrowserApplication = async (browser, profile, editorOrigin) => {
  const cases = [];
  let autosaveSamples;
  for (const deviceScaleFactor of profile.geometryMatrix.deviceScaleFactors) {
    const context = await browser.newContext({
      viewport: { width: 1920, height: 1200 },
      deviceScaleFactor,
      reducedMotion: "reduce",
    });
    try {
      const page = await context.newPage();
      await installBridgeRecorder(page);
      const { iframe, frame, scopedOrigin } = await importFixtureThroughApp(
        page,
        editorOrigin,
        profile,
      );
      const binding = await establishAppBridgeBinding(
        page,
        frame,
        scopedOrigin,
        profile,
      );
      if (autosaveSamples === undefined) {
        autosaveSamples = await measureAutosave(
          page,
          binding.projectId,
          profile,
        );
      }
      for (const viewport of profile.geometryMatrix.viewports) {
        await setMatrixViewport(page, viewport);
        for (const contentZoomFactor of profile.geometryMatrix
          .contentZoomFactors) {
          for (const previewScale of profile.geometryMatrix.previewScales) {
            for (const scrollCase of profile.geometryMatrix.scrollCases) {
              cases.push(
                await measureGeometryCase({
                  page,
                  iframe,
                  frame,
                  scopedOrigin,
                  binding,
                  profile,
                  viewport,
                  deviceScaleFactor,
                  contentZoomFactor,
                  previewScale,
                  scrollCase,
                }),
              );
            }
          }
        }
      }
    } finally {
      await context.close();
    }
  }
  if (autosaveSamples === undefined) {
    throw new MeasurementFailure("autosave_measurement_missing");
  }
  return { cases, autosaveSamples };
};

const selfVerify = (result, profile) => {
  const cases = result.geometry.cases;
  const expectedIds = expectedCaseIds(profile);
  const actualIds = cases.map((entry) => entry.caseId);
  const viewportByName = new Map(
    profile.geometryMatrix.viewports.map((entry) => [entry.name, entry]),
  );
  const scrollByName = new Map(
    profile.geometryMatrix.scrollCases.map((entry) => [entry.name, entry]),
  );
  const caseIdentitiesPassed = cases.every((entry) => {
    const viewport = viewportByName.get(entry.viewport);
    const scrollCase = scrollByName.get(entry.scrollCase);
    return (
      viewport?.width === entry.configuredViewportWidthCssPx &&
      profile.geometryMatrix.deviceScaleFactors.includes(
        entry.deviceScaleFactor,
      ) &&
      profile.geometryMatrix.contentZoomFactors.includes(
        entry.contentZoomFactor,
      ) &&
      profile.geometryMatrix.previewScales.includes(entry.previewScale) &&
      scrollCase?.windowTop === entry.windowScrollTopCssPx &&
      scrollCase?.outerTop === entry.outerScrollTopCssPx &&
      scrollCase?.innerLeft === entry.innerScrollLeftCssPx &&
      entry.runtimeDevicePixelRatio === entry.deviceScaleFactor &&
      entry.runtimePreviewScale === entry.previewScale &&
      entry.runtimeViewportWidthCssPx === entry.renderedIframeWidthCssPx &&
      entry.renderedOuterFrameWidthCssPx ===
        entry.configuredViewportWidthCssPx &&
      entry.renderedOuterFrameWidthCssPx - entry.renderedIframeWidthCssPx >=
        0 &&
      entry.renderedOuterFrameWidthCssPx - entry.renderedIframeWidthCssPx <=
        2 &&
      entry.caseId ===
        geometryCaseId({
          viewport: entry.viewport,
          deviceScaleFactor: entry.deviceScaleFactor,
          contentZoomFactor: entry.contentZoomFactor,
          previewScale: entry.previewScale,
          scrollCase: entry.scrollCase,
        })
    );
  });
  const maximumErrorCssPx = Math.max(
    ...cases.map((entry) => entry.overlayErrorCssPx),
  );
  const distinctRenderedViewportWidthCount = new Set(
    cases.map((entry) => entry.renderedIframeWidthCssPx),
  ).size;
  const expectedAdvisoryCodes = [];
  if (!result.geometry.overlayFeedbackThresholdPassed) {
    expectedAdvisoryCodes.push("overlay_feedback_reference_threshold_exceeded");
  }
  if (!result.autosave.thresholdPassed) {
    expectedAdvisoryCodes.push("autosave_reference_threshold_exceeded");
  }
  if (!result.changeSet.thresholdPassed) {
    expectedAdvisoryCodes.push("change_set_reference_threshold_exceeded");
  }
  if (
    !exactArray(actualIds, expectedIds) ||
    !caseIdentitiesPassed ||
    new Set(actualIds).size !== 72 ||
    result.geometry.caseCount !== cases.length ||
    result.geometry.distinctRenderedViewportWidthCount !==
      distinctRenderedViewportWidthCount ||
    distinctRenderedViewportWidthCount !== 3 ||
    result.geometry.maximumErrorCssPx !== maximumErrorCssPx ||
    maximumErrorCssPx > 2 ||
    result.geometry.overlayFeedbackP95Ms !==
      percentile95(cases.map((entry) => entry.overlayFeedbackDurationMs)) ||
    result.geometry.overlayFeedbackThresholdPassed !==
      result.geometry.overlayFeedbackP95Ms <=
        result.geometry.overlayFeedbackReferenceThresholdMs ||
    result.autosave.p95Ms !== percentile95(result.autosave.rawSamplesMs) ||
    result.autosave.thresholdPassed !==
      result.autosave.p95Ms <= result.autosave.referenceThresholdMs ||
    result.changeSet.p95Ms !== percentile95(result.changeSet.rawSamplesMs) ||
    result.changeSet.thresholdPassed !==
      result.changeSet.p95Ms <= result.changeSet.referenceThresholdMs ||
    !exactArray(result.advisoryCodes ?? [], expectedAdvisoryCodes)
  ) {
    throw new MeasurementFailure("result_self_verification_failed");
  }
  const serialized = JSON.stringify(result);
  if (
    ["/home/", "/tmp/", "Bearer ", "sessionToken", "projectId"].some((canary) =>
      serialized.includes(canary),
    )
  ) {
    throw new MeasurementFailure("result_privacy_boundary_failed");
  }
};

const main = async () => {
  const options = parseArguments();
  const validateResult = await loadResultValidator();
  const profileBytes = await readFile(options.profilePath);
  const profile = JSON.parse(profileBytes.toString("utf8"));
  validateProfile(profile);
  const changeSetProbePromise = runChangeSetProbe(
    options.changeSetProbe,
    profile,
  );
  const browser = await chromium.launch({ headless: true });
  let measurement;
  const browserVersion = browser.version();
  try {
    measurement = await measureBrowserApplication(
      browser,
      profile,
      options.editorOrigin,
    );
  } finally {
    await browser.close();
  }
  const changeSetProbe = await changeSetProbePromise;
  validateChangeSetProbe(changeSetProbe, profile);
  const maximumErrorCssPx = Math.max(
    ...measurement.cases.map((entry) => entry.overlayErrorCssPx),
  );
  const distinctRenderedViewportWidthCount = new Set(
    measurement.cases.map((entry) => entry.renderedIframeWidthCssPx),
  ).size;
  const overlayFeedbackP95Ms = percentile95(
    measurement.cases.map((entry) => entry.overlayFeedbackDurationMs),
  );
  const overlayFeedbackThresholdPassed =
    overlayFeedbackP95Ms <= profile.geometryThresholds.overlayFeedbackP95Ms;
  const autosaveP95Ms = percentile95(measurement.autosaveSamples);
  const autosaveThresholdPassed =
    autosaveP95Ms <= profile.autosave.finalInputToCompletionP95Ms;
  const changeSetThresholdPassed =
    changeSetProbe.p95Ms <= profile.changeSet.validationAndApplicationP95Ms;
  const advisoryCodes = [];
  if (!overlayFeedbackThresholdPassed)
    advisoryCodes.push("overlay_feedback_reference_threshold_exceeded");
  if (!autosaveThresholdPassed)
    advisoryCodes.push("autosave_reference_threshold_exceeded");
  if (!changeSetThresholdPassed)
    advisoryCodes.push("change_set_reference_threshold_exceeded");
  const result = {
    schemaVersion: RESULT_SCHEMA_VERSION,
    status: "passed",
    scope: profile.scope,
    profileSha256: sha256(profileBytes),
    environment: {
      platform: platform(),
      architecture: arch(),
      node: process.version,
      browser: browserVersion,
    },
    productionIntegration: {
      measurement: "measured",
      status: "pass",
      packagedApplication: true,
      scopedPreviewIframe: true,
      previewTargetRuntime: true,
      boundBridgeRoundTrip: true,
      targetApiRoundTrip: true,
      actualRuntimeOverlay: true,
      caseCount: measurement.cases.length,
    },
    geometry: {
      caseCount: measurement.cases.length,
      expectedCaseCount: 72,
      distinctRenderedViewportWidthCount,
      rawCaseRecordsIncluded: true,
      maximumErrorCssPx,
      thresholdCssPx: profile.geometryThresholds.overlayMaximumErrorCssPx,
      overlayFeedbackP95Ms,
      overlayFeedbackReferenceThresholdMs:
        profile.geometryThresholds.overlayFeedbackP95Ms,
      overlayFeedbackThresholdPassed,
      overlayFeedbackGatePolicy: profile.thresholdPolicy,
      checksPassed: measurement.cases.length === 72 && maximumErrorCssPx <= 2,
      cases: measurement.cases,
    },
    autosave: {
      productionApplicationMeasured: true,
      finalInputEventMeasured: true,
      durableApiCompletionMeasured: true,
      warmupSampleCount: profile.autosave.warmupSampleCount,
      sampleCount: measurement.autosaveSamples.length,
      rawSamplesMs: measurement.autosaveSamples,
      p95Ms: autosaveP95Ms,
      referenceThresholdMs: profile.autosave.finalInputToCompletionP95Ms,
      thresholdPassed: autosaveThresholdPassed,
      functionalChecksPassed: true,
      gatePolicy: profile.thresholdPolicy,
    },
    changeSet: {
      productionParserMeasured: true,
      probeSchemaVersion: changeSetProbe.schemaVersion,
      changedTextFileCount: changeSetProbe.fixture.changedTextFileCount,
      totalChangedTextBytes: changeSetProbe.fixture.totalChangedTextBytes,
      operationCount: changeSetProbe.fixture.operationCount,
      changeSetSha256: changeSetProbe.fixture.changeSetSha256,
      outputManifestSha256: changeSetProbe.outputManifestSha256,
      warmupSampleCount: changeSetProbe.warmupSampleCount,
      sampleCount: changeSetProbe.sampleCount,
      rawSamplesMs: changeSetProbe.rawSamplesMs,
      p95Ms: changeSetProbe.p95Ms,
      referenceThresholdMs: profile.changeSet.validationAndApplicationP95Ms,
      thresholdPassed: changeSetThresholdPassed,
      functionalChecksPassed: true,
      gatePolicy: profile.thresholdPolicy,
    },
    manualEvidence: profile.manualEvidence,
    claimBoundary: profile.claimBoundary,
    ...(advisoryCodes.length === 0 ? {} : { advisoryCodes }),
  };
  selfVerify(result, profile);
  if (!validateResult(result)) {
    throw new MeasurementFailure("result_schema_validation_failed");
  }
  const serialized = `${JSON.stringify(result)}\n`;
  if (options.outputPath !== null) {
    await writeFile(options.outputPath, serialized, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    });
  }
  process.stdout.write(serialized);
};

await main().catch((error) => {
  const errorCode =
    error instanceof MeasurementFailure
      ? error.code
      : `unexpected_${activeStep}`;
  process.stderr.write(
    `${JSON.stringify({
      schemaVersion: RESULT_SCHEMA_VERSION,
      status: "failed",
      scope: "packaged_local_production_application_integration",
      errorCode,
      manualEvidence: {
        creatorEndToEndVerification: "pending",
        screenReader: "pending",
        liveProvider: "pending_external_credential_and_billing_acknowledgement",
      },
      claimBoundary: {
        crossBrowserSupport: false,
        productionReady: false,
        releaseCreated: false,
      },
    })}\n`,
  );
  process.exitCode = 1;
});
