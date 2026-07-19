import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import Ajv2020 from "ajv/dist/2020.js";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFile(resolve(root, path), "utf8");
const readJson = async (path) => JSON.parse(await read(path));

class ValidationFailure extends Error {}
const assert = (condition, message) => {
  if (!condition) throw new ValidationFailure(message);
};

const main = async () => {
  const [
    requirements,
    traceability,
    evidence,
    browserProfile,
    browserResultSchema,
    geometryProfile,
    geometryResultSchema,
    productionIntegrationProfile,
    productionIntegrationResultSchema,
    safeLogProfile,
    safeLogResultSchema,
    automatedEvidenceResultSchema,
  ] = await Promise.all([
    read("docs/detailed-requirements.md"),
    read("docs/requirements-traceability.md"),
    readJson("docs/evidence/m1-evidence-manifest.template.v1.json"),
    readJson("docs/evidence/fixtures/m1-package-browser-smoke-profile.v1.json"),
    readJson(
      "docs/evidence/schemas/m1-package-browser-smoke-result.schema.v1.json",
    ),
    readJson("docs/evidence/fixtures/m1-performance-geometry-profile.v1.json"),
    readJson(
      "docs/evidence/schemas/m1-performance-geometry-result.schema.v1.json",
    ),
    readJson(
      "docs/evidence/fixtures/m1-production-integration-performance-profile.v1.json",
    ),
    readJson(
      "docs/evidence/schemas/m1-production-integration-performance-result.schema.v1.json",
    ),
    readJson("docs/evidence/fixtures/m1-safe-log-profile.v1.json"),
    readJson("docs/evidence/schemas/m1-safe-log-result.schema.v1.json"),
    readJson(
      "docs/evidence/schemas/m1-automated-evidence-record.schema.v1.json",
    ),
  ]);

  const sourceRequirements = [
    ...requirements.matchAll(
      /^- \*\*([A-Z][A-Z0-9-]+)(?:\s+\/\s+([^*]+))?:\*\*/gm,
    ),
  ].map((match) => ({
    id: match[1],
    priority: (match[2] ?? "—").trim(),
  }));
  assert(sourceRequirements.length > 0, "no source requirements found");
  assert(
    new Set(sourceRequirements.map(({ id }) => id)).size ===
      sourceRequirements.length,
    "source requirement IDs are not unique",
  );

  const traceRows = [
    ...traceability.matchAll(
      /^\| \[([A-Z][A-Z0-9-]+)\]\([^)]+\) \| ([^|]+) \| ([^|]+) \| ([^|]+) \|/gm,
    ),
  ].map((match) => ({
    id: match[1],
    priority: match[2].trim(),
    checkpoint: match[3].trim(),
    evidence: match[4].trim(),
  }));
  assert(
    traceRows.length === sourceRequirements.length,
    `traceability has ${traceRows.length} rows for ${sourceRequirements.length} requirements`,
  );
  const rowById = new Map(traceRows.map((row) => [row.id, row]));
  assert(rowById.size === traceRows.length, "traceability IDs are not unique");
  const validCheckpoints = /^(?:C(?:[0-9]|1[01])(?:–C11)?|C0 \/ M2|M2)$/u;
  let p0Count = 0;
  for (const requirement of sourceRequirements) {
    const row = rowById.get(requirement.id);
    assert(row !== undefined, `${requirement.id} is missing from traceability`);
    assert(
      row.priority === requirement.priority,
      `${requirement.id} priority differs between source and traceability`,
    );
    assert(
      validCheckpoints.test(row.checkpoint),
      `${requirement.id} has an invalid checkpoint`,
    );
    assert(
      row.evidence.length > 0,
      `${requirement.id} has no planned evidence`,
    );
    if (requirement.priority === "P0") p0Count += 1;
  }

  const checkpointIds = evidence.checkpoints.map(({ id }) => id);
  const checkpointWeights = evidence.checkpoints.map(({ weight }) => weight);
  assert(
    JSON.stringify(checkpointIds) ===
      JSON.stringify(Array.from({ length: 12 }, (_, index) => `C${index}`)),
    "evidence template does not bind C0 through C11 exactly",
  );
  assert(
    JSON.stringify(checkpointWeights) ===
      JSON.stringify([5, 15, 10, 8, 8, 10, 9, 8, 7, 10, 7, 3]),
    "evidence template does not bind the exact checkpoint weights",
  );
  assert(
    evidence.manualEvidence.creator80PercentVerification.status === "pending" &&
      evidence.manualEvidence.keyboardZoomAndContrast.status === "pending" &&
      evidence.manualEvidence.screenReader.status === "pending" &&
      evidence.manualEvidence.liveProvider.status ===
        "pending_external_credential_and_billing_acknowledgement",
    "manual evidence was promoted without a recorded manual result",
  );
  assert(
    browserProfile.performanceThresholdPolicy ===
      "advisory_on_uncharacterized_ci" &&
      browserProfile.manualEvidence.creatorEndToEndVerification === "pending" &&
      browserProfile.manualEvidence.keyboardZoomAndContrast === "pending" &&
      browserProfile.manualEvidence.screenReader === "pending" &&
      browserProfile.manualEvidence.liveProvider ===
        "pending_external_credential_and_billing_acknowledgement",
    "browser profile must preserve manual evidence gates",
  );
  assert(
    geometryProfile.staticSite.fileCount === 500 &&
      geometryProfile.staticSite.totalByteLength === 52_428_800 &&
      geometryProfile.dom.nodeCount === 10_000 &&
      geometryProfile.geometryMatrix.viewports.length === 3 &&
      geometryProfile.geometryMatrix.deviceScaleFactors.length === 2 &&
      geometryProfile.geometryMatrix.contentZoomFactors.length === 3 &&
      geometryProfile.geometryMatrix.previewScales.length === 2 &&
      geometryProfile.geometryMatrix.scrollCases.length === 2 &&
      geometryProfile.measurement.warmupCaseCount === 0 &&
      geometryProfile.measurement.sampleCount === 72 &&
      geometryProfile.measurement.rawCaseRecordsRequired === true &&
      geometryProfile.thresholds.overlayMaximumErrorCssPx === 2 &&
      geometryProfile.thresholds.performanceThresholdPolicy ===
        "advisory_on_uncharacterized_ci",
    "performance/geometry profile no longer binds the complete 72-case corpus",
  );
  assert(
    safeLogProfile.maximumSupportedLogLevel === "trace" &&
      safeLogProfile.provider.id === "fake" &&
      safeLogProfile.provider.external === false &&
      safeLogProfile.forbiddenCanaryClasses.length === 7 &&
      safeLogProfile.pendingGates.liveProvider ===
        "pending_external_credential_and_billing_acknowledgement" &&
      safeLogProfile.pendingGates.creatorVerification === "pending" &&
      safeLogProfile.pendingGates.license === "unresolved" &&
      safeLogProfile.pendingGates.brand === "unresolved" &&
      safeLogProfile.pendingGates.releaseCreated === false,
    "safe-log profile no longer binds maximum-level privacy and pending gates",
  );
  assert(
    productionIntegrationProfile.geometryMatrix.viewports.length === 3 &&
      productionIntegrationProfile.geometryMatrix.deviceScaleFactors.length ===
        2 &&
      productionIntegrationProfile.geometryMatrix.contentZoomFactors.length ===
        3 &&
      productionIntegrationProfile.geometryMatrix.previewScales.length === 2 &&
      productionIntegrationProfile.geometryMatrix.scrollCases.length === 2 &&
      productionIntegrationProfile.geometryThresholds
        .overlayMaximumErrorCssPx === 2 &&
      productionIntegrationProfile.geometryThresholds.overlayFeedbackP95Ms ===
        100 &&
      productionIntegrationProfile.autosave.sampleCount === 20 &&
      productionIntegrationProfile.autosave.finalInputToCompletionP95Ms ===
        1000 &&
      productionIntegrationProfile.changeSet.changedTextFileCount === 10 &&
      productionIntegrationProfile.changeSet.totalChangedTextBytes ===
        2_097_152 &&
      productionIntegrationProfile.changeSet.validationAndApplicationP95Ms ===
        2000 &&
      productionIntegrationProfile.thresholdPolicy ===
        "advisory_on_uncharacterized_ci",
    "production integration profile no longer binds the complete application matrix",
  );

  const ajv = new Ajv2020({
    allErrors: true,
    strict: true,
    strictRequired: false,
  });
  assert(
    ajv.validateSchema(browserResultSchema),
    "browser result schema is invalid",
  );
  assert(
    ajv.validateSchema(geometryResultSchema),
    "performance/geometry result schema is invalid",
  );
  assert(
    ajv.validateSchema(productionIntegrationResultSchema),
    "production integration performance result schema is invalid",
  );
  assert(
    ajv.validateSchema(safeLogResultSchema),
    "safe-log result schema is invalid",
  );
  assert(
    ajv.validateSchema(automatedEvidenceResultSchema),
    "automated evidence result schema is invalid",
  );
  const validateBrowserResult = ajv.compile(browserResultSchema);
  assert(
    validateBrowserResult({
      schemaVersion: "synapsegit-lp-studio.package-browser-smoke-result/1",
      status: "failed",
      scope: "local_internal_evaluation_editor_root",
      errorCode: "synthetic_validation_failure",
      manualEvidence: browserProfile.manualEvidence,
      claimBoundary: {
        wcagConformance: false,
        fullPerformanceProfile: false,
        productionReady: false,
        releaseCreated: false,
      },
    }),
    "browser result schema rejects its fail-closed error envelope",
  );
  const passingBrowserResult = {
    schemaVersion: "synapsegit-lp-studio.package-browser-smoke-result/1",
    status: "passed",
    scope: "local_internal_evaluation_editor_root",
    profileSha256: "0".repeat(64),
    environment: {
      platform: "linux",
      architecture: "x64",
      node: process.version,
      browser: "Chromium synthetic",
    },
    performance: {
      warmupReloads: 1,
      sampleCount: 3,
      navigationDurationMs: [1, 1, 1],
      navigationDurationP95Ms: 1,
      responseStartMs: [1, 1, 1],
      responseStartP95Ms: 1,
      domNodeCount: 10,
      thresholdsPassed: true,
      gatePolicy: "advisory_on_uncharacterized_ci",
    },
    accessibilitySmoke: {
      documentLanguage: "ja",
      titlePresent: true,
      mainLandmarkCount: 1,
      duplicateIdCount: 0,
      namelessInteractiveControlCount: 0,
      imageWithoutAltCount: 0,
      unlabelledFormControlCount: 0,
      positiveTabIndexCount: 0,
      initialTabFocusedVisibleControl: true,
      reducedMotionRendered: true,
      forcedColorsRendered: true,
      checksPassed: true,
    },
    responsiveSmoke: {
      narrowViewport: { width: 320, height: 640 },
      horizontalOverflowPx: 0,
      checksPassed: true,
    },
    manualEvidence: browserProfile.manualEvidence,
    claimBoundary: {
      wcagConformance: false,
      fullPerformanceProfile: false,
      productionReady: false,
      releaseCreated: false,
    },
  };
  assert(
    validateBrowserResult(passingBrowserResult),
    "browser result schema rejects a conforming passing result",
  );
  const contradictoryBrowserResult = structuredClone(passingBrowserResult);
  contradictoryBrowserResult.accessibilitySmoke.checksPassed = false;
  assert(
    !validateBrowserResult(contradictoryBrowserResult),
    "browser result schema accepts passed with failed accessibility checks",
  );

  const validateGeometryResult = ajv.compile(geometryResultSchema);
  assert(
    validateGeometryResult({
      schemaVersion: "synapsegit-lp-studio.performance-geometry-result/1",
      status: "failed",
      scope: "synthetic_full_corpus_local_evaluation",
      errorCode: "synthetic_validation_failure",
      claimBoundary: {
        fullApplicationPerformance: false,
        productionReady: false,
        crossBrowserSupport: false,
        releaseCreated: false,
      },
    }),
    "performance/geometry schema rejects its fail-closed error envelope",
  );
  const syntheticGeometryCases = [];
  for (const deviceScaleFactor of geometryProfile.geometryMatrix
    .deviceScaleFactors) {
    for (const viewport of geometryProfile.geometryMatrix.viewports) {
      for (const contentZoomFactor of geometryProfile.geometryMatrix
        .contentZoomFactors) {
        for (const previewScale of geometryProfile.geometryMatrix
          .previewScales) {
          for (const scrollCase of geometryProfile.geometryMatrix.scrollCases) {
            syntheticGeometryCases.push({
              caseId: `${viewport.name}:dpr=${deviceScaleFactor}:zoom=${contentZoomFactor}:preview=${previewScale}:scroll=${scrollCase.name}`,
              viewport: viewport.name,
              viewportWidthCssPx: viewport.width,
              viewportHeightCssPx: viewport.height,
              deviceScaleFactor,
              contentZoomFactor,
              previewScale,
              scrollCase: scrollCase.name,
              windowScrollTopCssPx: scrollCase.windowTop,
              outerScrollTopCssPx: scrollCase.outerTop,
              innerScrollLeftCssPx: scrollCase.innerLeft,
              overlayErrorCssPx: 0,
              overlayFeedbackDurationMs: 1,
            });
          }
        }
      }
    }
  }
  assert(
    syntheticGeometryCases.length === 72,
    "synthetic schema test does not cover all 72 matrix cases",
  );
  const passingGeometryResult = {
    schemaVersion: "synapsegit-lp-studio.performance-geometry-result/1",
    status: "passed",
    scope: "synthetic_full_corpus_local_evaluation",
    profileSha256: "0".repeat(64),
    environment: {
      platform: "linux",
      architecture: "x64",
      osReleaseId: "synthetic",
      osReleaseVersionId: "1",
      osReleaseSha256: "3".repeat(64),
      kernelRelease: "synthetic-kernel",
      node: process.version,
      pnpm: "10.33.0",
      rust: "rustc 1.95.0 (synthetic)",
      playwright: "Version 1.61.1",
      browser: "Chromium synthetic",
    },
    fixture: {
      generatorVersion: "synapsegit-lp-studio.performance-geometry-generator/1",
      fileCount: 500,
      totalByteLength: 52_428_800,
      manifestSha256: "1".repeat(64),
      indexSha256: "2".repeat(64),
      domNodeCount: 10_000,
      targetKinds: ["page", "block", "element", "text", "point", "region"],
      responsiveBreakpointsCssPx: [400, 640, 960],
      boundedScanPassed: true,
    },
    geometry: {
      caseCount: 72,
      expectedCaseCount: 72,
      maximumErrorCssPx: 0,
      thresholdCssPx: 2,
      checksPassed: true,
      cases: syntheticGeometryCases,
    },
    performance: {
      warmupCaseCount: 0,
      sampleCount: 72,
      rawCaseRecordsIncluded: true,
      overlayFeedbackSampleCount: 72,
      overlayFeedbackP95Ms: 1,
      referenceThresholdMs: 100,
      thresholdPassed: true,
      gatePolicy: "advisory_on_uncharacterized_ci",
    },
    coverageBoundary: {
      syntheticCorpus: "measured",
      applicationIntegration: "not_measured",
      unmeasured:
        geometryProfile.coverageBoundary.notMeasuredByThisSyntheticCorpus,
    },
    claimBoundary: {
      fullApplicationPerformance: false,
      productionReady: false,
      crossBrowserSupport: false,
      releaseCreated: false,
    },
  };
  assert(
    validateGeometryResult(passingGeometryResult),
    "performance/geometry schema rejects a conforming passing result",
  );
  const contradictoryGeometryResult = structuredClone(passingGeometryResult);
  contradictoryGeometryResult.geometry.checksPassed = false;
  assert(
    !validateGeometryResult(contradictoryGeometryResult),
    "performance/geometry schema accepts passed with failed geometry checks",
  );
  const overLimitRawGeometryResult = structuredClone(passingGeometryResult);
  overLimitRawGeometryResult.geometry.cases[0].overlayErrorCssPx = 2.0001;
  assert(
    !validateGeometryResult(overLimitRawGeometryResult),
    "performance/geometry schema accepts a passed raw case above 2 CSS px",
  );

  const validateProductionIntegrationResult = ajv.compile(
    productionIntegrationResultSchema,
  );
  assert(
    validateProductionIntegrationResult({
      schemaVersion:
        "synapsegit-lp-studio.production-integration-performance-result/1",
      status: "failed",
      scope: "packaged_local_production_application_integration",
      errorCode: "synthetic_validation_failure",
      manualEvidence: productionIntegrationProfile.manualEvidence,
      claimBoundary: productionIntegrationProfile.claimBoundary,
    }),
    "production integration schema rejects its fail-closed error envelope",
  );
  const productionGeometryCases = syntheticGeometryCases.map((entry) => ({
    caseId: entry.caseId,
    viewport: entry.viewport,
    configuredViewportWidthCssPx: entry.viewportWidthCssPx,
    deviceScaleFactor: entry.deviceScaleFactor,
    contentZoomFactor: entry.contentZoomFactor,
    previewScale: entry.previewScale,
    scrollCase: entry.scrollCase,
    windowScrollTopCssPx: entry.windowScrollTopCssPx,
    outerScrollTopCssPx: entry.outerScrollTopCssPx,
    innerScrollLeftCssPx: entry.innerScrollLeftCssPx,
    overlayErrorCssPx: entry.overlayErrorCssPx,
    overlayFeedbackDurationMs: entry.overlayFeedbackDurationMs,
    renderedOuterFrameWidthCssPx: entry.viewportWidthCssPx,
    renderedIframeWidthCssPx: entry.viewportWidthCssPx,
    runtimeViewportWidthCssPx: entry.viewportWidthCssPx,
    runtimeViewportHeightCssPx: 640,
    runtimeDevicePixelRatio: entry.deviceScaleFactor,
    runtimePreviewScale: entry.previewScale,
    scopedPreviewOriginVerified: true,
    appBridgeTargetVerified: true,
    targetApiRoundTripVerified: true,
    actualOverlayRendered: true,
  }));
  const passingProductionIntegrationResult = {
    schemaVersion:
      "synapsegit-lp-studio.production-integration-performance-result/1",
    status: "passed",
    scope: "packaged_local_production_application_integration",
    profileSha256: "5".repeat(64),
    environment: {
      platform: "linux",
      architecture: "x64",
      node: process.version,
      browser: "Chromium synthetic",
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
      caseCount: 72,
    },
    geometry: {
      caseCount: 72,
      expectedCaseCount: 72,
      distinctRenderedViewportWidthCount: 3,
      rawCaseRecordsIncluded: true,
      maximumErrorCssPx: 0,
      thresholdCssPx: 2,
      overlayFeedbackP95Ms: 1,
      overlayFeedbackReferenceThresholdMs: 100,
      overlayFeedbackThresholdPassed: true,
      overlayFeedbackGatePolicy: "advisory_on_uncharacterized_ci",
      checksPassed: true,
      cases: productionGeometryCases,
    },
    autosave: {
      productionApplicationMeasured: true,
      finalInputEventMeasured: true,
      durableApiCompletionMeasured: true,
      warmupSampleCount: 2,
      sampleCount: 20,
      rawSamplesMs: Array.from({ length: 20 }, () => 1),
      p95Ms: 1,
      referenceThresholdMs: 1000,
      thresholdPassed: true,
      functionalChecksPassed: true,
      gatePolicy: "advisory_on_uncharacterized_ci",
    },
    changeSet: {
      productionParserMeasured: true,
      probeSchemaVersion: "synapsegit-lp-studio.change-set-performance-probe/1",
      changedTextFileCount: 10,
      totalChangedTextBytes: 2_097_152,
      operationCount: 10,
      changeSetSha256: "6".repeat(64),
      outputManifestSha256: "7".repeat(64),
      warmupSampleCount: 2,
      sampleCount: 20,
      rawSamplesMs: Array.from({ length: 20 }, () => 1),
      p95Ms: 1,
      referenceThresholdMs: 2000,
      thresholdPassed: true,
      functionalChecksPassed: true,
      gatePolicy: "advisory_on_uncharacterized_ci",
    },
    manualEvidence: productionIntegrationProfile.manualEvidence,
    claimBoundary: productionIntegrationProfile.claimBoundary,
  };
  assert(
    validateProductionIntegrationResult(passingProductionIntegrationResult),
    `production integration schema rejects a conforming passing result: ${JSON.stringify(validateProductionIntegrationResult.errors)}`,
  );
  const mismatchedProductionViewportWidth = structuredClone(
    passingProductionIntegrationResult,
  );
  mismatchedProductionViewportWidth.geometry.cases[0].renderedOuterFrameWidthCssPx = 375;
  assert(
    !validateProductionIntegrationResult(mismatchedProductionViewportWidth),
    "production integration schema accepts a configured/rendered viewport mismatch",
  );
  const mismatchedProductionRuntimeWidth = structuredClone(
    passingProductionIntegrationResult,
  );
  mismatchedProductionRuntimeWidth.geometry.cases[0].runtimeViewportWidthCssPx = 1279;
  assert(
    !validateProductionIntegrationResult(mismatchedProductionRuntimeWidth),
    "production integration schema accepts an iframe/runtime viewport mismatch",
  );
  const contradictoryProductionIntegrationResult = structuredClone(
    passingProductionIntegrationResult,
  );
  contradictoryProductionIntegrationResult.geometry.checksPassed = false;
  assert(
    !validateProductionIntegrationResult(
      contradictoryProductionIntegrationResult,
    ),
    "production integration schema accepts passed with failed geometry checks",
  );
  const overLimitProductionIntegrationResult = structuredClone(
    passingProductionIntegrationResult,
  );
  overLimitProductionIntegrationResult.geometry.cases[0].overlayErrorCssPx = 2.0001;
  assert(
    !validateProductionIntegrationResult(overLimitProductionIntegrationResult),
    "production integration schema accepts a raw case above 2 CSS px",
  );

  const validateSafeLogResult = ajv.compile(safeLogResultSchema);
  const passingSafeLogResult = {
    schemaVersion: "synapsegit-lp-studio.safe-log-result/1",
    status: "passed",
    scope: "packaged_local_fake_provider_flow",
    profileSha256: "0".repeat(64),
    maximumSupportedLogLevel: "trace",
    flow: {
      providerId: "fake",
      requestedModel: "deterministic-v1",
      externalProviderConfigured: false,
      fakeProviderAttributionExternal: false,
      decisionDisposition: "adopted_unchanged",
      importedFileBodyExercised: true,
      providerRawResponseExercised: true,
      intentionallyFailedRequestErrorCode: "invalid_request",
      requestOperationCount: 9,
    },
    logCapture: {
      stdoutByteLength: 1,
      stdoutSha256: "1".repeat(64),
      stderrByteLength: 1,
      stderrSha256: "2".repeat(64),
      rawLogsRetained: false,
    },
    requiredSafeFields: {
      fields: safeLogProfile.requiredRequestFields,
      requestRecordCount: 9,
      errorRecordCount: 1,
      checksPassed: true,
    },
    adapterCorrelation: {
      component: "synapsegit-adapter",
      phases: safeLogProfile.requiredAdapterPhases,
      correlatedOperationCount: 2,
      checksPassed: true,
    },
    privacyScan: {
      canaryChecks: safeLogProfile.forbiddenCanaryClasses.map(
        (canaryClass, index) => ({
          class: canaryClass,
          sha256: String(index).repeat(64),
          absent: true,
        }),
      ),
      absoluteRuntimePathCount: 8,
      authorizationSchemeAbsent: true,
      rawCanaryValuesRetained: false,
      checksPassed: true,
    },
    requirementIds: safeLogProfile.requirementIds,
    pendingGates: safeLogProfile.pendingGates,
    claimBoundary: {
      defaultLevelIncludedByMaximumLevel: true,
      safeAtMaximumSupportedLevel: true,
      liveProviderCovered: false,
      productionReady: false,
      releaseCreated: false,
    },
  };
  assert(
    validateSafeLogResult(passingSafeLogResult),
    "safe-log result schema rejects a conforming passing result",
  );
  const contradictorySafeLogResult = structuredClone(passingSafeLogResult);
  contradictorySafeLogResult.privacyScan.checksPassed = false;
  assert(
    !validateSafeLogResult(contradictorySafeLogResult),
    "safe-log result schema accepts passed with failed privacy scan",
  );

  const validateAutomatedEvidence = ajv.compile(automatedEvidenceResultSchema);
  const automatedCommand = {
    executable: "node",
    arguments: ["scripts/synthetic-check.mjs"],
    redactions: [],
  };
  const automatedResult = (id, profile = false) => ({
    id,
    status: "passed",
    coverageKind: "partial_automated_evidence",
    command: automatedCommand,
    ...(profile
      ? {
          profile: {
            path: "docs/evidence/fixtures/synthetic.v1.json",
            sha256: "3".repeat(64),
          },
        }
      : {}),
    result: {
      embeddedAt: "runtime.synthetic",
      sha256: "4".repeat(64),
    },
    requirementIds: ["TEST-005"],
  });
  const passingAutomatedEvidence = {
    schemaVersion: "synapsegit-lp-studio.automated-evidence-record/1",
    recordKind: "automated_result",
    status: "passed",
    scope: "local_internal_evaluation_only",
    sourceBinding: {
      revision: "0".repeat(40),
      clean: true,
      evaluationMode: "clean_tree",
      statusSha256: "1".repeat(64),
      contentFingerprintSha256: "2".repeat(64),
    },
    results: [
      automatedResult("package-browser-smoke", true),
      automatedResult("performance-geometry-corpus", true),
      automatedResult("production-integration-performance", true),
      automatedResult("safe-observability-fake-flow", true),
      automatedResult("local-evaluation-package"),
    ],
    pendingGates: {
      creatorVerification: "pending",
      liveProvider: "pending_external_credential_and_billing_acknowledgement",
      manualKeyboardZoomContrast: "pending",
      screenReader: "pending",
      applicationAutosaveAndChangeSetPerformance: "measured",
      boundedApplicationCancellation: "pending_unmeasured",
      license: "unresolved",
      brand: "unresolved",
      productionPermission: "not_recorded",
      externalDeliveryPermission: "not_recorded",
      redistributionPermission: "not_recorded",
      releaseCreated: false,
      tagCreated: false,
    },
    claimBoundary: {
      requirementCompletionInferred: false,
      allP0Complete: false,
      productionReady: false,
      releaseCreated: false,
      distributionPermissionInferred: false,
    },
  };
  assert(
    validateAutomatedEvidence(passingAutomatedEvidence),
    `automated evidence schema rejects a conforming result: ${JSON.stringify(validateAutomatedEvidence.errors)}`,
  );
  const overstatedProductionCoverage = structuredClone(
    passingAutomatedEvidence,
  );
  overstatedProductionCoverage.results.find(
    (entry) => entry.id === "production-integration-performance",
  ).coverageKind = "automated_evidence";
  assert(
    !validateAutomatedEvidence(overstatedProductionCoverage),
    "automated evidence schema accepts production timing evidence as complete coverage",
  );

  process.stdout.write(
    `${JSON.stringify({
      schemaVersion: "synapsegit-lp-studio.traceability-check/1",
      status: "passed",
      requirementCount: sourceRequirements.length,
      p0RequirementCount: p0Count,
      checkpointCount: checkpointIds.length,
      automatedProfileCount: 4,
      manualEvidenceStatus: "pending",
    })}\n`,
  );
};

await main().catch((error) => {
  process.stderr.write(
    `${JSON.stringify({
      schemaVersion: "synapsegit-lp-studio.traceability-check/1",
      status: "failed",
      reason:
        error instanceof ValidationFailure
          ? error.message
          : "traceability_validation_failed",
    })}\n`,
  );
  process.exitCode = 1;
});
