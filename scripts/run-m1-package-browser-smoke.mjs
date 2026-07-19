import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { arch, platform } from "node:os";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";
import Ajv2020 from "ajv/dist/2020.js";

const RESULT_SCHEMA_VERSION =
  "synapsegit-lp-studio.package-browser-smoke-result/1";
const repositoryRoot = resolve(import.meta.dirname, "..");
const defaultProfilePath = resolve(
  repositoryRoot,
  "docs/evidence/fixtures/m1-package-browser-smoke-profile.v1.json",
);
const resultSchemaPath = resolve(
  repositoryRoot,
  "docs/evidence/schemas/m1-package-browser-smoke-result.schema.v1.json",
);

class SmokeFailure extends Error {
  constructor(code) {
    super(code);
    this.code = code;
  }
}

const parseArguments = () => {
  const options = { profilePath: defaultProfilePath, outputPath: null };
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--") {
      continue;
    } else if (argument === "--editor-origin") {
      options.editorOrigin = args[++index];
    } else if (argument === "--profile") {
      options.profilePath = resolve(args[++index]);
    } else if (argument === "--output") {
      options.outputPath = resolve(args[++index]);
    } else {
      throw new SmokeFailure("unknown_argument");
    }
  }
  if (typeof options.editorOrigin !== "string") {
    throw new SmokeFailure("editor_origin_required");
  }
  let origin;
  try {
    origin = new URL(options.editorOrigin);
  } catch {
    throw new SmokeFailure("editor_origin_invalid");
  }
  if (
    origin.protocol !== "http:" ||
    origin.hostname !== "127.0.0.1" ||
    origin.port.length === 0 ||
    origin.pathname !== "/" ||
    origin.search.length !== 0 ||
    origin.hash.length !== 0
  ) {
    throw new SmokeFailure("editor_origin_not_scoped_loopback");
  }
  options.editorOrigin = origin.origin;
  return options;
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const percentile95 = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * 0.95) - 1)];
};
const rounded = (value) => Math.round(value * 100) / 100;
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
      "performanceThresholdPolicy",
      "warmupReloads",
      "sampleCount",
      "viewports",
      "thresholds",
      "accessibilitySmoke",
      "manualEvidence",
      "coverageBoundary",
    ]) ||
    profile.schemaVersion !==
      "synapsegit-lp-studio.package-browser-smoke-profile/1" ||
    profile.scope !== "local_internal_evaluation_editor_root" ||
    profile.performanceThresholdPolicy !== "advisory_on_uncharacterized_ci" ||
    profile.warmupReloads !== 1 ||
    profile.sampleCount !== 3 ||
    !exactKeys(profile.viewports, ["desktop", "narrow"]) ||
    !exactKeys(profile.viewports.desktop, ["width", "height"]) ||
    profile.viewports.desktop.width !== 1280 ||
    profile.viewports.desktop.height !== 800 ||
    !exactKeys(profile.viewports.narrow, ["width", "height"]) ||
    profile.viewports.narrow.width !== 320 ||
    profile.viewports.narrow.height !== 640 ||
    !exactKeys(profile.thresholds, [
      "navigationDurationP95Ms",
      "responseStartP95Ms",
      "domNodeCountMaximum",
      "narrowHorizontalOverflowMaximumPx",
    ]) ||
    profile.thresholds.navigationDurationP95Ms !== 5000 ||
    profile.thresholds.responseStartP95Ms !== 2000 ||
    profile.thresholds.domNodeCountMaximum !== 5000 ||
    profile.thresholds.narrowHorizontalOverflowMaximumPx !== 1 ||
    !exactKeys(profile.accessibilitySmoke, [
      "requiredDocumentLanguage",
      "requiredMainLandmarksMinimum",
      "duplicateIdMaximum",
      "namelessInteractiveControlMaximum",
      "imageWithoutAltMaximum",
      "unlabelledFormControlMaximum",
      "positiveTabIndexMaximum",
      "keyboardInitialTabMustFocusVisibleControl",
      "reducedMotionRenderRequired",
      "forcedColorsRenderRequired",
    ]) ||
    profile.accessibilitySmoke.requiredDocumentLanguage !== "ja" ||
    profile.accessibilitySmoke.requiredMainLandmarksMinimum !== 1 ||
    profile.accessibilitySmoke.duplicateIdMaximum !== 0 ||
    profile.accessibilitySmoke.namelessInteractiveControlMaximum !== 0 ||
    profile.accessibilitySmoke.imageWithoutAltMaximum !== 0 ||
    profile.accessibilitySmoke.unlabelledFormControlMaximum !== 0 ||
    profile.accessibilitySmoke.positiveTabIndexMaximum !== 0 ||
    profile.accessibilitySmoke.keyboardInitialTabMustFocusVisibleControl !==
      true ||
    profile.accessibilitySmoke.reducedMotionRenderRequired !== true ||
    profile.accessibilitySmoke.forcedColorsRenderRequired !== true ||
    !exactKeys(profile.manualEvidence, [
      "creatorEndToEndVerification",
      "keyboardZoomAndContrast",
      "screenReader",
      "liveProvider",
    ]) ||
    profile.manualEvidence.creatorEndToEndVerification !== "pending" ||
    profile.manualEvidence.keyboardZoomAndContrast !== "pending" ||
    profile.manualEvidence.screenReader !== "pending" ||
    profile.manualEvidence.liveProvider !==
      "pending_external_credential_and_billing_acknowledgement" ||
    !exactKeys(profile.coverageBoundary, [
      "thisProfileMeasures",
      "thisProfileDoesNotComplete",
    ]) ||
    !exactArray(profile.coverageBoundary.thisProfileMeasures, [
      "packaged Editor root navigation timing",
      "basic document and control accessibility invariants",
      "initial keyboard focus",
      "320 CSS pixel rendering",
      "reduced-motion and forced-colors rendering",
    ]) ||
    !exactArray(profile.coverageBoundary.thisProfileDoesNotComplete, [
      "the ADR-0011 50 MiB and 10000-node performance corpus",
      "Target overlay geometry across DPR, zoom, transforms, and nested scrolling",
      "manual keyboard-only Creator workflow",
      "manual screen-reader review",
      "WCAG conformance",
      "live-provider evidence",
      "production, release, distribution, or platform support claims",
    ])
  ) {
    throw new SmokeFailure("profile_invalid");
  }
};

const loadResultValidator = async () => {
  let schema;
  try {
    schema = JSON.parse(await readFile(resultSchemaPath, "utf8"));
    return new Ajv2020({
      allErrors: true,
      strict: true,
      strictRequired: false,
    }).compile(schema);
  } catch {
    throw new SmokeFailure("result_schema_invalid");
  }
};

const documentAudit = async (page) =>
  page.evaluate(() => {
    const visible = (element) => {
      const style = window.getComputedStyle(element);
      const rect = element.getBoundingClientRect();
      return (
        style.visibility !== "hidden" &&
        style.display !== "none" &&
        rect.width > 0 &&
        rect.height > 0
      );
    };
    const ids = [...document.querySelectorAll("[id]")].map(
      (element) => element.id,
    );
    const duplicateIds = ids.filter(
      (id, index) => id.length > 0 && ids.indexOf(id) !== index,
    );
    const controls = [
      ...document.querySelectorAll(
        'button, a[href], input, select, textarea, [role="button"], [role="link"], [role="checkbox"], [role="radio"], [role="switch"], [tabindex]',
      ),
    ].filter(visible);
    const accessibleName = (element) => {
      const labelledBy = element.getAttribute("aria-labelledby");
      const labelledText = (labelledBy ?? "")
        .split(/\s+/u)
        .filter(Boolean)
        .map((id) => document.getElementById(id)?.textContent ?? "")
        .join(" ");
      const ownId = element.getAttribute("id");
      const labelText =
        ownId === null
          ? ""
          : ([...document.querySelectorAll("label[for]")].find(
              (label) => label.htmlFor === ownId,
            )?.textContent ?? "");
      return (
        [
          element.getAttribute("aria-label"),
          labelledText,
          labelText,
          element.getAttribute("alt"),
          element.getAttribute("title"),
          element.textContent,
        ].find((candidate) => (candidate ?? "").trim().length > 0) ?? ""
      ).trim();
    };
    const formControls = [
      ...document.querySelectorAll("input, select, textarea"),
    ].filter(
      (element) =>
        visible(element) &&
        element.getAttribute("type") !== "hidden" &&
        !element.hasAttribute("disabled"),
    );
    return {
      documentLanguage: document.documentElement.lang,
      titlePresent: document.title.trim().length > 0,
      mainLandmarkCount: document.querySelectorAll('main, [role="main"]')
        .length,
      duplicateIdCount: new Set(duplicateIds).size,
      namelessInteractiveControlCount: controls.filter(
        (element) => accessibleName(element).length === 0,
      ).length,
      imageWithoutAltCount: document.querySelectorAll("img:not([alt])").length,
      unlabelledFormControlCount: formControls.filter(
        (element) => accessibleName(element).length === 0,
      ).length,
      positiveTabIndexCount: [
        ...document.querySelectorAll("[tabindex]"),
      ].filter((element) => Number(element.getAttribute("tabindex")) > 0)
        .length,
      domNodeCount: document.getElementsByTagName("*").length,
    };
  });

const rootRendered = async (page) =>
  page.evaluate(() => {
    const root = document.getElementById("root");
    return (
      root !== null && root.childElementCount > 0 && document.body !== null
    );
  });

const main = async () => {
  const options = parseArguments();
  const validateResult = await loadResultValidator();
  const profileBytes = await readFile(options.profilePath);
  const profile = JSON.parse(profileBytes.toString("utf8"));
  validateProfile(profile);

  const browser = await chromium.launch({ headless: true });
  let result;
  try {
    const context = await browser.newContext({
      viewport: profile.viewports.desktop,
      reducedMotion: "reduce",
      forcedColors: "none",
    });
    const page = await context.newPage();
    for (let index = 0; index < profile.warmupReloads; index += 1) {
      await page.goto(options.editorOrigin, {
        waitUntil: "load",
        timeout: 15_000,
      });
    }

    const navigationDurationMs = [];
    const responseStartMs = [];
    for (let index = 0; index < profile.sampleCount; index += 1) {
      await page.goto(options.editorOrigin, {
        waitUntil: "load",
        timeout: 15_000,
      });
      const timing = await page.evaluate(() => {
        const entry = performance.getEntriesByType("navigation")[0];
        return entry instanceof PerformanceNavigationTiming
          ? { duration: entry.duration, responseStart: entry.responseStart }
          : null;
      });
      if (timing === null) throw new SmokeFailure("navigation_timing_missing");
      navigationDurationMs.push(rounded(timing.duration));
      responseStartMs.push(rounded(timing.responseStart));
    }
    await page.waitForLoadState("networkidle", { timeout: 15_000 });

    const audit = await documentAudit(page);
    await page.locator("body").click({ position: { x: 1, y: 1 } });
    await page.keyboard.press("Tab");
    const initialTabFocusedVisibleControl = await page.evaluate(() => {
      const active = document.activeElement;
      if (!(active instanceof HTMLElement) || active === document.body)
        return false;
      const rect = active.getBoundingClientRect();
      const style = window.getComputedStyle(active);
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        style.display !== "none" &&
        style.visibility !== "hidden"
      );
    });

    const reducedMotionRendered = await rootRendered(page);
    await page.emulateMedia({
      reducedMotion: "reduce",
      forcedColors: "active",
    });
    await page.reload({ waitUntil: "load", timeout: 15_000 });
    await page.waitForLoadState("networkidle", { timeout: 15_000 });
    const forcedColorsRendered = await rootRendered(page);

    await page.setViewportSize(profile.viewports.narrow);
    await page.emulateMedia({ reducedMotion: "reduce", forcedColors: "none" });
    await page.reload({ waitUntil: "load", timeout: 15_000 });
    await page.waitForLoadState("networkidle", { timeout: 15_000 });
    const horizontalOverflowPx = await page.evaluate(() =>
      Math.max(
        0,
        Math.ceil(
          Math.max(
            document.documentElement.scrollWidth,
            document.body?.scrollWidth ?? 0,
          ) - document.documentElement.clientWidth,
        ),
      ),
    );

    const navigationDurationP95Ms = percentile95(navigationDurationMs);
    const responseStartP95Ms = percentile95(responseStartMs);
    const performancePassed =
      navigationDurationP95Ms <= profile.thresholds.navigationDurationP95Ms &&
      responseStartP95Ms <= profile.thresholds.responseStartP95Ms &&
      audit.domNodeCount <= profile.thresholds.domNodeCountMaximum;
    const accessibilityPassed =
      audit.documentLanguage ===
        profile.accessibilitySmoke.requiredDocumentLanguage &&
      audit.titlePresent &&
      audit.mainLandmarkCount >=
        profile.accessibilitySmoke.requiredMainLandmarksMinimum &&
      audit.duplicateIdCount <= profile.accessibilitySmoke.duplicateIdMaximum &&
      audit.namelessInteractiveControlCount <=
        profile.accessibilitySmoke.namelessInteractiveControlMaximum &&
      audit.imageWithoutAltCount <=
        profile.accessibilitySmoke.imageWithoutAltMaximum &&
      audit.unlabelledFormControlCount <=
        profile.accessibilitySmoke.unlabelledFormControlMaximum &&
      audit.positiveTabIndexCount <=
        profile.accessibilitySmoke.positiveTabIndexMaximum &&
      initialTabFocusedVisibleControl &&
      reducedMotionRendered &&
      forcedColorsRendered;
    const responsivePassed =
      horizontalOverflowPx <=
      profile.thresholds.narrowHorizontalOverflowMaximumPx;
    const failureCodes = [];
    const advisoryCodes = [];
    if (!performancePassed)
      advisoryCodes.push("performance_threshold_exceeded");
    if (!accessibilityPassed) failureCodes.push("accessibility_smoke_failed");
    if (!responsivePassed) failureCodes.push("narrow_overflow_failed");

    result = {
      schemaVersion: RESULT_SCHEMA_VERSION,
      status: failureCodes.length === 0 ? "passed" : "failed",
      scope: profile.scope,
      profileSha256: sha256(profileBytes),
      environment: {
        platform: platform(),
        architecture: arch(),
        node: process.version,
        browser: browser.version(),
      },
      performance: {
        warmupReloads: profile.warmupReloads,
        sampleCount: profile.sampleCount,
        navigationDurationMs,
        navigationDurationP95Ms,
        responseStartMs,
        responseStartP95Ms,
        domNodeCount: audit.domNodeCount,
        thresholdsPassed: performancePassed,
        gatePolicy: profile.performanceThresholdPolicy,
      },
      accessibilitySmoke: {
        documentLanguage: audit.documentLanguage,
        titlePresent: audit.titlePresent,
        mainLandmarkCount: audit.mainLandmarkCount,
        duplicateIdCount: audit.duplicateIdCount,
        namelessInteractiveControlCount: audit.namelessInteractiveControlCount,
        imageWithoutAltCount: audit.imageWithoutAltCount,
        unlabelledFormControlCount: audit.unlabelledFormControlCount,
        positiveTabIndexCount: audit.positiveTabIndexCount,
        initialTabFocusedVisibleControl,
        reducedMotionRendered,
        forcedColorsRendered,
        checksPassed: accessibilityPassed,
      },
      responsiveSmoke: {
        narrowViewport: profile.viewports.narrow,
        horizontalOverflowPx,
        checksPassed: responsivePassed,
      },
      manualEvidence: profile.manualEvidence,
      claimBoundary: {
        wcagConformance: false,
        fullPerformanceProfile: false,
        productionReady: false,
        releaseCreated: false,
      },
      ...(failureCodes.length === 0 ? {} : { failureCodes }),
      ...(advisoryCodes.length === 0 ? {} : { advisoryCodes }),
    };
    await context.close();
  } finally {
    await browser.close();
  }

  if (!validateResult(result)) {
    throw new SmokeFailure("result_schema_validation_failed");
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
  if (result.status !== "passed") process.exitCode = 1;
};

await main().catch((error) => {
  const code =
    error instanceof SmokeFailure ? error.code : "package_browser_smoke_failed";
  process.stderr.write(
    `${JSON.stringify({
      schemaVersion: RESULT_SCHEMA_VERSION,
      status: "failed",
      scope: "local_internal_evaluation_editor_root",
      errorCode: code,
      manualEvidence: {
        creatorEndToEndVerification: "pending",
        keyboardZoomAndContrast: "pending",
        screenReader: "pending",
        liveProvider: "pending_external_credential_and_billing_acknowledgement",
      },
      claimBoundary: {
        wcagConformance: false,
        fullPerformanceProfile: false,
        productionReady: false,
        releaseCreated: false,
      },
    })}\n`,
  );
  process.exitCode = 1;
});
