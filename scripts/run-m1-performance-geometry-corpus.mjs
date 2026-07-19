import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { arch, platform, release, tmpdir } from "node:os";
import { basename, join, relative, resolve, sep } from "node:path";
import { chromium } from "@playwright/test";
import Ajv2020 from "ajv/dist/2020.js";

const RESULT_SCHEMA_VERSION =
  "synapsegit-lp-studio.performance-geometry-result/1";
const PROFILE_SCHEMA_VERSION =
  "synapsegit-lp-studio.performance-geometry-profile/1";
const repositoryRoot = resolve(import.meta.dirname, "..");
const defaultProfilePath = resolve(
  repositoryRoot,
  "docs/evidence/fixtures/m1-performance-geometry-profile.v1.json",
);
const resultSchemaPath = resolve(
  repositoryRoot,
  "docs/evidence/schemas/m1-performance-geometry-result.schema.v1.json",
);
const temporaryPrefixName = "synapsegit-lp-performance-geometry-";
const temporaryPrefix = join(tmpdir(), temporaryPrefixName);

class CorpusFailure extends Error {
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
    } else if (argument === "--profile") {
      const value = args[++index];
      if (value === undefined) throw new CorpusFailure("profile_path_required");
      options.profilePath = resolve(value);
    } else if (argument === "--output") {
      const value = args[++index];
      if (value === undefined) throw new CorpusFailure("output_path_required");
      options.outputPath = resolve(value);
    } else {
      throw new CorpusFailure("unknown_argument");
    }
  }
  return options;
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const aggregateManifest = (entries) =>
  sha256(Buffer.from(JSON.stringify(entries), "utf8"));
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
      "generatorVersion",
      "staticSite",
      "dom",
      "geometryMatrix",
      "measurement",
      "thresholds",
      "coverageBoundary",
    ]) ||
    profile.schemaVersion !== PROFILE_SCHEMA_VERSION ||
    profile.scope !== "synthetic_full_corpus_local_evaluation" ||
    profile.generatorVersion !==
      "synapsegit-lp-studio.performance-geometry-generator/1" ||
    !exactKeys(profile.staticSite, [
      "fileCount",
      "totalByteLength",
      "symlinksAllowed",
      "specialEntriesAllowed",
    ]) ||
    profile.staticSite.fileCount !== 500 ||
    profile.staticSite.totalByteLength !== 52_428_800 ||
    profile.staticSite.symlinksAllowed !== false ||
    profile.staticSite.specialEntriesAllowed !== false ||
    !exactKeys(profile.dom, [
      "nodeCount",
      "targetKinds",
      "responsiveBreakpointsCssPx",
      "nestedScrollContainers",
      "transformedTargetRequired",
    ]) ||
    profile.dom.nodeCount !== 10_000 ||
    !exactArray(profile.dom.targetKinds, [
      "page",
      "block",
      "element",
      "text",
      "point",
      "region",
    ]) ||
    !exactArray(profile.dom.responsiveBreakpointsCssPx, [400, 640, 960]) ||
    profile.dom.nestedScrollContainers !== 2 ||
    profile.dom.transformedTargetRequired !== true ||
    !exactKeys(profile.geometryMatrix, [
      "viewports",
      "deviceScaleFactors",
      "contentZoomFactors",
      "previewScales",
      "scrollCases",
    ]) ||
    !exactArray(profile.geometryMatrix.viewports, [
      { name: "desktop", width: 1280, height: 800 },
      { name: "tablet", width: 768, height: 900 },
      { name: "mobile", width: 375, height: 667 },
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
    !exactKeys(profile.measurement, [
      "warmupCaseCount",
      "sampleCount",
      "rawCaseRecordsRequired",
    ]) ||
    profile.measurement.warmupCaseCount !== 0 ||
    profile.measurement.sampleCount !== 72 ||
    profile.measurement.rawCaseRecordsRequired !== true ||
    !exactKeys(profile.thresholds, [
      "overlayMaximumErrorCssPx",
      "overlayFeedbackP95Ms",
      "performanceThresholdPolicy",
    ]) ||
    profile.thresholds.overlayMaximumErrorCssPx !== 2 ||
    profile.thresholds.overlayFeedbackP95Ms !== 100 ||
    profile.thresholds.performanceThresholdPolicy !==
      "advisory_on_uncharacterized_ci" ||
    !exactKeys(profile.coverageBoundary, [
      "hardGates",
      "notMeasuredByThisSyntheticCorpus",
    ]) ||
    !exactArray(profile.coverageBoundary.hardGates, [
      "exact 500 regular files and 50 MiB bounded scan",
      "exact 10000-element DOM",
      "six deterministic target kinds",
      "overlay projection at or below 2 CSS px across the complete matrix",
      "nested scroll, transform, viewport, zoom, preview-scale, and DPR cases",
    ]) ||
    !exactArray(profile.coverageBoundary.notMeasuredByThisSyntheticCorpus, [
      "LP Studio production bridge integration",
      "metadata autosave latency",
      "ChangeSet validation latency",
      "import, diff, and operation-queue cancellation",
      "manual keyboard, screen-reader, zoom, or contrast workflow",
      "production or cross-browser support",
    ])
  ) {
    throw new CorpusFailure("profile_invalid");
  }
};

const toolVersion = (command, args, failureCode) =>
  new Promise((resolveVersion, rejectVersion) => {
    execFile(
      command,
      args,
      {
        cwd: repositoryRoot,
        encoding: "utf8",
        timeout: 10_000,
        maxBuffer: 16_384,
      },
      (error, stdout) => {
        const version = stdout.trim().split("\n")[0];
        if (error !== null || version.length === 0) {
          rejectVersion(new CorpusFailure(failureCode));
          return;
        }
        resolveVersion(version);
      },
    );
  });

const parseOsReleaseValue = (value) => {
  const trimmed = value.trim();
  if (trimmed.startsWith('"') && trimmed.endsWith('"')) {
    return trimmed.slice(1, -1).replaceAll('\\"', '"').replaceAll("\\\\", "\\");
  }
  return trimmed;
};

const readOsIdentity = async () => {
  let bytes;
  try {
    bytes = await readFile("/etc/os-release");
  } catch {
    throw new CorpusFailure("os_release_read_failed");
  }
  const fields = new Map();
  for (const line of bytes.toString("utf8").split("\n")) {
    const separator = line.indexOf("=");
    if (separator <= 0) continue;
    fields.set(
      line.slice(0, separator),
      parseOsReleaseValue(line.slice(separator + 1)),
    );
  }
  return {
    osReleaseId: fields.get("ID") || "not_declared",
    osReleaseVersionId: fields.get("VERSION_ID") || "not_declared",
    osReleaseSha256: sha256(bytes),
    kernelRelease: release(),
  };
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
    throw new CorpusFailure("result_schema_invalid");
  }
};

const buildDomHtml = (nodeCount) => `<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>LP Studio synthetic geometry corpus</title>
  <style>
    * { box-sizing: border-box; }
    html, body { margin: 0; min-height: 1500px; }
    #fixture-root { width: 100%; transform-origin: 0 0; }
    #outer-scroll { width: min(92vw, 620px); height: 340px; overflow: auto; margin: 160px 24px 0; padding-top: 140px; border: 0; }
    #inner-scroll { width: 920px; height: 640px; overflow: auto; position: relative; }
    #spatial-plane { width: 1320px; height: 1040px; position: relative; }
    #element-target { position: absolute; left: 430px; top: 370px; width: 188px; height: 96px; transform: rotate(3deg) scale(1.02); transform-origin: center; }
    #point-target { position: absolute; left: 390px; top: 330px; width: 8px; height: 8px; }
    #region-target { position: absolute; left: 680px; top: 520px; width: 140px; height: 92px; }
    #corpus-nodes { contain: strict; width: 1px; height: 1px; overflow: hidden; }
    .corpus-node { width: 1px; height: 1px; }
    .corpus-node:nth-child(5n) { display: none; }
    @media (max-width: 960px) { #element-target { left: 390px; } }
    @media (max-width: 640px) { #element-target { left: 330px; top: 340px; } }
    @media (max-width: 400px) { #element-target { left: 280px; top: 310px; } }
  </style>
</head>
<body data-target-kind="page">
  <main id="fixture-root">
    <section id="outer-scroll" data-target-kind="block">
      <div id="inner-scroll">
        <div id="spatial-plane">
          <div id="point-target" data-target-kind="point"></div>
          <div id="element-target" data-target-kind="element"><span id="text-target" data-target-kind="text">Target</span></div>
          <div id="region-target" data-target-kind="region"></div>
        </div>
      </div>
    </section>
    <div id="corpus-nodes" aria-hidden="true"></div>
  </main>
  <script>
    (() => {
      const expected = ${nodeCount};
      const host = document.getElementById("corpus-nodes");
      while (document.getElementsByTagName("*").length < expected) {
        const node = document.createElement("div");
        node.className = "corpus-node";
        host.append(node);
      }
      document.body.dataset.ready = document.getElementsByTagName("*").length === expected ? "true" : "false";
    })();
  </script>
</body>
</html>`;

const padIndex = (html, byteLength) => {
  const bytes = Buffer.from(html, "utf8");
  const remaining = byteLength - bytes.byteLength;
  if (remaining < 8) throw new CorpusFailure("index_fixture_too_large");
  return Buffer.concat([
    bytes,
    Buffer.from(`\n<!--${"x".repeat(remaining - 8)}-->`, "utf8"),
  ]);
};

const generateStaticFixture = async (profile, fixtureRoot) => {
  const fileCount = profile.staticSite.fileCount;
  const totalByteLength = profile.staticSite.totalByteLength;
  const baseSize = Math.floor(totalByteLength / fileCount);
  const largerFileCount = totalByteLength - baseSize * fileCount;
  const sizeFor = (index) => baseSize + (index < largerFileCount ? 1 : 0);
  const domHtml = buildDomHtml(profile.dom.nodeCount);
  await mkdir(join(fixtureRoot, "assets"), { recursive: true, mode: 0o700 });
  await writeFile(
    join(fixtureRoot, "index.html"),
    padIndex(domHtml, sizeFor(0)),
    {
      flag: "wx",
      mode: 0o600,
    },
  );
  for (let start = 1; start < fileCount; start += 20) {
    const writes = [];
    for (
      let index = start;
      index < Math.min(fileCount, start + 20);
      index += 1
    ) {
      const name = `fixture-${String(index).padStart(3, "0")}.txt`;
      const bytes = Buffer.alloc(sizeFor(index), 65 + (index % 26));
      writes.push(
        writeFile(join(fixtureRoot, "assets", name), bytes, {
          flag: "wx",
          mode: 0o600,
        }),
      );
    }
    await Promise.all(writes);
  }
  return { domHtml };
};

const scanFixture = async (root) => {
  const entries = [];
  const visit = async (directory) => {
    const names = await readdir(directory);
    names.sort();
    for (const name of names) {
      const path = join(directory, name);
      const metadata = await lstat(path);
      if (metadata.isSymbolicLink())
        throw new CorpusFailure("fixture_symlink_found");
      if (metadata.isDirectory()) {
        await visit(path);
      } else if (metadata.isFile()) {
        const bytes = await readFile(path);
        entries.push({
          path: relative(root, path).split(sep).join("/"),
          byteLength: bytes.byteLength,
          sha256: sha256(bytes),
        });
      } else {
        throw new CorpusFailure("fixture_special_entry_found");
      }
    }
  };
  await visit(root);
  entries.sort((left, right) => left.path.localeCompare(right.path, "en"));
  return entries;
};

const geometryCaseId = ({
  viewport,
  deviceScaleFactor,
  contentZoomFactor,
  previewScale,
  scrollCase,
}) =>
  `${viewport}:dpr=${deviceScaleFactor}:zoom=${contentZoomFactor}:preview=${previewScale}:scroll=${scrollCase}`;

const expectedGeometryCaseIds = (profile) => {
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

const recomputeAggregates = (cases) => ({
  maximumErrorCssPx: Math.max(...cases.map((entry) => entry.overlayErrorCssPx)),
  overlayFeedbackP95Ms: percentile95(
    cases.map((entry) => entry.overlayFeedbackDurationMs),
  ),
});

const geometryCaseCoveragePassed = (profile, cases) =>
  exactArray(
    cases.map((entry) => entry.caseId),
    expectedGeometryCaseIds(profile),
  ) &&
  cases.every((entry) => {
    const viewport = profile.geometryMatrix.viewports.find(
      (candidate) => candidate.name === entry.viewport,
    );
    const scrollCase = profile.geometryMatrix.scrollCases.find(
      (candidate) => candidate.name === entry.scrollCase,
    );
    return (
      viewport !== undefined &&
      scrollCase !== undefined &&
      entry.viewportWidthCssPx === viewport.width &&
      entry.viewportHeightCssPx === viewport.height &&
      entry.windowScrollTopCssPx === scrollCase.windowTop &&
      entry.outerScrollTopCssPx === scrollCase.outerTop &&
      entry.innerScrollLeftCssPx === scrollCase.innerLeft
    );
  });

const measureGeometry = async (browser, profile, domHtml) => {
  const cases = [];
  const targetKinds = new Set();
  let measuredDomNodeCount = 0;
  for (const deviceScaleFactor of profile.geometryMatrix.deviceScaleFactors) {
    const context = await browser.newContext({
      deviceScaleFactor,
      reducedMotion: "reduce",
    });
    const page = await context.newPage();
    for (const viewport of profile.geometryMatrix.viewports) {
      await page.setViewportSize({
        width: viewport.width,
        height: viewport.height,
      });
      await page.setContent(
        '<!doctype html><html><body style="margin:0;min-height:1800px"><iframe id="fixture-frame" title="Synthetic geometry fixture" style="position:absolute;left:24px;top:140px;border:0;transform-origin:0 0"></iframe><div id="projected-overlay" style="position:fixed;box-sizing:border-box;pointer-events:none;border:1px solid transparent"></div></body></html>',
      );
      const iframe = page.locator("#fixture-frame");
      await iframe.evaluate(
        (element, dimensions) => {
          element.style.width = `${dimensions.width}px`;
          element.style.height = `${dimensions.height}px`;
        },
        {
          width: Math.max(320, viewport.width - 80),
          height: Math.max(420, viewport.height - 180),
        },
      );
      const frame = page
        .frames()
        .find((candidate) => candidate !== page.mainFrame());
      if (frame === undefined) throw new CorpusFailure("fixture_frame_missing");
      await frame.setContent(domHtml, { waitUntil: "load" });
      await frame
        .locator('body[data-ready="true"]')
        .waitFor({ timeout: 15_000 });
      measuredDomNodeCount = await frame.evaluate(
        () => document.getElementsByTagName("*").length,
      );
      const observedKinds = await frame.evaluate(() =>
        [...document.querySelectorAll("[data-target-kind]")].map((element) =>
          element.getAttribute("data-target-kind"),
        ),
      );
      for (const kind of observedKinds) targetKinds.add(kind);

      for (const contentZoom of profile.geometryMatrix.contentZoomFactors) {
        for (const previewScale of profile.geometryMatrix.previewScales) {
          await iframe.evaluate((element, scale) => {
            element.style.transform = `scale(${scale})`;
          }, previewScale);
          for (const scrollCase of profile.geometryMatrix.scrollCases) {
            await page.evaluate(
              (top) => window.scrollTo(0, top),
              scrollCase.windowTop,
            );
            const appliedNestedScroll = await frame.evaluate(
              ({ zoom, outerTop, innerLeft }) => {
                const root = document.getElementById("fixture-root");
                const outer = document.getElementById("outer-scroll");
                const inner = document.getElementById("inner-scroll");
                root.style.zoom = String(zoom);
                outer.scrollTop = outerTop;
                inner.scrollLeft = innerLeft;
                return {
                  outerTop: outer.scrollTop,
                  innerLeft: inner.scrollLeft,
                };
              },
              {
                zoom: contentZoom,
                outerTop: scrollCase.outerTop,
                innerLeft: scrollCase.innerLeft,
              },
            );
            await page.evaluate(
              () =>
                new Promise((resolveFrame) =>
                  requestAnimationFrame(resolveFrame),
                ),
            );
            const appliedWindowTop = await page.evaluate(() => window.scrollY);
            const [iframeBox, iframeSize, childRect, targetBox] =
              await Promise.all([
                iframe.boundingBox(),
                iframe.evaluate((element) => ({
                  width: element.clientWidth,
                  height: element.clientHeight,
                })),
                frame.locator("#element-target").evaluate((element) => {
                  const rect = element.getBoundingClientRect();
                  return {
                    left: rect.left,
                    top: rect.top,
                    width: rect.width,
                    height: rect.height,
                  };
                }),
                frame.locator("#element-target").boundingBox(),
              ]);
            if (iframeBox === null || targetBox === null) {
              throw new CorpusFailure("geometry_box_missing");
            }
            const scaleX = iframeBox.width / iframeSize.width;
            const scaleY = iframeBox.height / iframeSize.height;
            const projected = {
              left: iframeBox.x + childRect.left * scaleX,
              top: iframeBox.y + childRect.top * scaleY,
              width: childRect.width * scaleX,
              height: childRect.height * scaleY,
            };
            const feedbackDuration = await page.evaluate(
              (rect) =>
                new Promise((resolveFeedback) => {
                  const started = performance.now();
                  requestAnimationFrame(() => {
                    const overlay =
                      document.getElementById("projected-overlay");
                    Object.assign(overlay.style, {
                      left: `${rect.left}px`,
                      top: `${rect.top}px`,
                      width: `${rect.width}px`,
                      height: `${rect.height}px`,
                    });
                    requestAnimationFrame(() =>
                      resolveFeedback(performance.now() - started),
                    );
                  });
                }),
              projected,
            );
            const overlayBox = await page
              .locator("#projected-overlay")
              .boundingBox();
            if (overlayBox === null)
              throw new CorpusFailure("overlay_box_missing");
            const overlayErrorCssPx = Math.max(
              Math.abs(overlayBox.x - targetBox.x),
              Math.abs(overlayBox.y - targetBox.y),
              Math.abs(overlayBox.width - targetBox.width),
              Math.abs(overlayBox.height - targetBox.height),
            );
            cases.push({
              caseId: geometryCaseId({
                viewport: viewport.name,
                deviceScaleFactor,
                contentZoomFactor: contentZoom,
                previewScale,
                scrollCase: scrollCase.name,
              }),
              viewport: viewport.name,
              viewportWidthCssPx: viewport.width,
              viewportHeightCssPx: viewport.height,
              deviceScaleFactor,
              contentZoomFactor: contentZoom,
              previewScale,
              scrollCase: scrollCase.name,
              windowScrollTopCssPx: appliedWindowTop,
              outerScrollTopCssPx: appliedNestedScroll.outerTop,
              innerScrollLeftCssPx: appliedNestedScroll.innerLeft,
              overlayErrorCssPx,
              overlayFeedbackDurationMs: feedbackDuration,
            });
          }
        }
      }
    }
    await context.close();
  }
  return {
    cases,
    measuredDomNodeCount,
    targetKinds: [...targetKinds].sort(),
  };
};

let temporaryRoot;

const main = async () => {
  const options = parseArguments();
  const validateResult = await loadResultValidator();
  const profileBytes = await readFile(options.profilePath);
  const profile = JSON.parse(profileBytes.toString("utf8"));
  validateProfile(profile);
  if (platform() !== "linux" || arch() !== "x64") {
    throw new CorpusFailure("unsupported_corpus_host");
  }
  const [osIdentity, pnpmVersion, rustVersion, playwrightVersion] =
    await Promise.all([
      readOsIdentity(),
      toolVersion("pnpm", ["--version"], "pnpm_version_failed"),
      toolVersion("rustc", ["--version"], "rust_version_failed"),
      toolVersion(
        "pnpm",
        ["exec", "playwright", "--version"],
        "playwright_version_failed",
      ),
    ]);

  temporaryRoot = await mkdtemp(temporaryPrefix);
  if (
    !resolve(temporaryRoot).startsWith(
      `${resolve(tmpdir())}${sep}${temporaryPrefixName}`,
    ) ||
    basename(temporaryRoot).length <= temporaryPrefixName.length
  ) {
    throw new CorpusFailure("unsafe_temporary_root");
  }
  const fixtureRoot = join(temporaryRoot, "fixture");
  await mkdir(fixtureRoot, { mode: 0o700 });
  const { domHtml } = await generateStaticFixture(profile, fixtureRoot);
  const entries = await scanFixture(fixtureRoot);
  const fileCount = entries.length;
  const totalByteLength = entries.reduce(
    (total, entry) => total + entry.byteLength,
    0,
  );
  const boundedScanPassed =
    fileCount === profile.staticSite.fileCount &&
    totalByteLength === profile.staticSite.totalByteLength;
  if (!boundedScanPassed)
    throw new CorpusFailure("bounded_fixture_scan_failed");

  const browser = await chromium.launch({ headless: true });
  const browserVersion = browser.version();
  let measurement;
  try {
    measurement = await measureGeometry(browser, profile, domHtml);
  } finally {
    await browser.close();
  }
  const expectedCaseCount =
    profile.geometryMatrix.viewports.length *
    profile.geometryMatrix.deviceScaleFactors.length *
    profile.geometryMatrix.contentZoomFactors.length *
    profile.geometryMatrix.previewScales.length *
    profile.geometryMatrix.scrollCases.length;
  const caseCoveragePassed = geometryCaseCoveragePassed(
    profile,
    measurement.cases,
  );
  const aggregates = recomputeAggregates(measurement.cases);
  const maximumErrorCssPx = aggregates.maximumErrorCssPx;
  const geometryPassed =
    measurement.cases.length === expectedCaseCount &&
    measurement.cases.length === profile.measurement.sampleCount &&
    caseCoveragePassed &&
    maximumErrorCssPx <= profile.thresholds.overlayMaximumErrorCssPx;
  const targetKindsPassed = exactArray(
    profile.dom.targetKinds.filter((kind) =>
      measurement.targetKinds.includes(kind),
    ),
    profile.dom.targetKinds,
  );
  const measuredTargetKinds = profile.dom.targetKinds.filter((kind) =>
    measurement.targetKinds.includes(kind),
  );
  const domPassed = measurement.measuredDomNodeCount === profile.dom.nodeCount;
  const feedbackP95Ms = aggregates.overlayFeedbackP95Ms;
  const feedbackThresholdPassed =
    feedbackP95Ms <= profile.thresholds.overlayFeedbackP95Ms;
  const failureCodes = [];
  const advisoryCodes = [];
  if (!geometryPassed) failureCodes.push("geometry_matrix_failed");
  if (!targetKindsPassed) failureCodes.push("target_kind_fixture_failed");
  if (!domPassed) failureCodes.push("dom_node_count_failed");
  if (!feedbackThresholdPassed)
    advisoryCodes.push("overlay_feedback_reference_exceeded");

  const result = {
    schemaVersion: RESULT_SCHEMA_VERSION,
    status: failureCodes.length === 0 ? "passed" : "failed",
    scope: profile.scope,
    profileSha256: sha256(profileBytes),
    environment: {
      platform: platform(),
      architecture: arch(),
      ...osIdentity,
      node: process.version,
      pnpm: pnpmVersion,
      rust: rustVersion,
      playwright: playwrightVersion,
      browser: browserVersion,
    },
    fixture: {
      generatorVersion: profile.generatorVersion,
      fileCount,
      totalByteLength,
      manifestSha256: aggregateManifest(entries),
      indexSha256: entries.find((entry) => entry.path === "index.html").sha256,
      domNodeCount: measurement.measuredDomNodeCount,
      targetKinds: measuredTargetKinds,
      responsiveBreakpointsCssPx: profile.dom.responsiveBreakpointsCssPx,
      boundedScanPassed,
    },
    geometry: {
      caseCount: measurement.cases.length,
      expectedCaseCount,
      maximumErrorCssPx,
      thresholdCssPx: profile.thresholds.overlayMaximumErrorCssPx,
      checksPassed: geometryPassed && targetKindsPassed && domPassed,
      cases: measurement.cases,
    },
    performance: {
      warmupCaseCount: profile.measurement.warmupCaseCount,
      sampleCount: measurement.cases.length,
      rawCaseRecordsIncluded: true,
      overlayFeedbackSampleCount: measurement.cases.length,
      overlayFeedbackP95Ms: feedbackP95Ms,
      referenceThresholdMs: profile.thresholds.overlayFeedbackP95Ms,
      thresholdPassed: feedbackThresholdPassed,
      gatePolicy: profile.thresholds.performanceThresholdPolicy,
    },
    coverageBoundary: {
      syntheticCorpus: "measured",
      applicationIntegration: "not_measured",
      unmeasured: profile.coverageBoundary.notMeasuredByThisSyntheticCorpus,
    },
    claimBoundary: {
      fullApplicationPerformance: false,
      productionReady: false,
      crossBrowserSupport: false,
      releaseCreated: false,
    },
    ...(failureCodes.length === 0 ? {} : { failureCodes }),
    ...(advisoryCodes.length === 0 ? {} : { advisoryCodes }),
  };
  const verifiedAggregates = recomputeAggregates(result.geometry.cases);
  if (
    result.geometry.caseCount !== result.geometry.cases.length ||
    result.performance.sampleCount !== result.geometry.cases.length ||
    result.performance.overlayFeedbackSampleCount !==
      result.geometry.cases.length ||
    result.geometry.maximumErrorCssPx !==
      verifiedAggregates.maximumErrorCssPx ||
    result.performance.overlayFeedbackP95Ms !==
      verifiedAggregates.overlayFeedbackP95Ms
  ) {
    throw new CorpusFailure("raw_measurement_aggregate_mismatch");
  }
  if (!validateResult(result)) {
    throw new CorpusFailure("result_schema_validation_failed");
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

await main()
  .catch((error) => {
    const errorCode =
      error instanceof CorpusFailure
        ? error.code
        : "performance_geometry_corpus_failed";
    process.stderr.write(
      `${JSON.stringify({
        schemaVersion: RESULT_SCHEMA_VERSION,
        status: "failed",
        scope: "synthetic_full_corpus_local_evaluation",
        errorCode,
        claimBoundary: {
          fullApplicationPerformance: false,
          productionReady: false,
          crossBrowserSupport: false,
          releaseCreated: false,
        },
      })}\n`,
    );
    process.exitCode = 1;
  })
  .finally(async () => {
    if (temporaryRoot === undefined) return;
    const safePrefix = `${resolve(tmpdir())}${sep}${temporaryPrefixName}`;
    if (!resolve(temporaryRoot).startsWith(safePrefix)) {
      process.stderr.write(
        `${JSON.stringify({ schemaVersion: RESULT_SCHEMA_VERSION, status: "failed", errorCode: "temporary_cleanup_scope_rejected" })}\n`,
      );
      process.exitCode = 1;
      return;
    }
    await rm(temporaryRoot, { recursive: true, force: true });
  });
