import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const repositoryRoot = resolve(import.meta.dirname, "..");
const supportPath = resolve(
  repositoryRoot,
  "docs/evidence/m1-support-matrix.template.v1.json",
);
const evidencePath = resolve(
  repositoryRoot,
  "docs/evidence/m1-evidence-manifest.template.v1.json",
);

class TemplateFailure extends Error {}

const readJson = async (path) => JSON.parse(await readFile(path, "utf8"));
const assert = (condition, message) => {
  if (!condition) throw new TemplateFailure(message);
};

const main = async () => {
  const [support, evidence] = await Promise.all([
    readJson(supportPath),
    readJson(evidencePath),
  ]);

  assert(
    support.schemaVersion === "synapsegit-lp-studio.support-matrix/1",
    "support matrix schemaVersion is not v1",
  );
  assert(
    support.recordKind === "template",
    "support matrix must remain a template",
  );
  assert(
    support.completionState === "incomplete",
    "support template must not claim completion",
  );
  assert(
    support.scope === "local_internal_evaluation_only",
    "support template scope is too broad",
  );
  assert(
    support.candidateEnvironment?.verificationStatus === "pending",
    "candidate support environment must remain pending in the template",
  );
  assert(
    Array.isArray(support.supportedClaims) &&
      support.supportedClaims.length === 0,
    "a template must not contain a verified support claim",
  );

  const legal = support.licenseAndBrand;
  assert(
    legal !== null && typeof legal === "object",
    "license status is missing",
  );
  assert(
    legal.lpStudioLicenseIdentifier ===
      "LicenseRef-SynapseGit-LP-Studio-Evaluation" &&
      legal.lpStudioLicenseTextStatus === "not_recorded" &&
      legal.thirdPartyNoticeStatus === "not_recorded" &&
      legal.templateAndAssetRightsStatus === "not_recorded",
    "license or asset rights must remain explicit and unresolved",
  );
  for (const field of [
    "synapseGitRightsHolderPermission",
    "productionPermission",
    "externalDeliveryPermission",
    "redistributionPermission",
    "brandPermission",
  ]) {
    assert(
      legal[field] === "not_recorded",
      `${field} must not be inferred in a template`,
    );
  }
  assert(
    legal.responsibleOwner === "unassigned",
    "the rights owner must remain explicit and unresolved",
  );

  assert(
    evidence.schemaVersion === "synapsegit-lp-studio.m1-evidence/1",
    "evidence manifest schemaVersion is not v1",
  );
  assert(
    evidence.recordKind === "template",
    "evidence manifest must remain a template",
  );
  assert(
    evidence.completionState === "incomplete",
    "evidence template must not claim completion",
  );
  assert(
    evidence.scope === "local_internal_evaluation_only",
    "evidence template scope is too broad",
  );

  const expectedCheckpoints = [5, 15, 10, 8, 8, 10, 9, 8, 7, 10, 7, 3].map(
    (weight, index) => ({ id: `C${index}`, weight }),
  );
  const actualCheckpoints = evidence.checkpoints?.map(({ id, weight }) => ({
    id,
    weight,
  }));
  assert(
    JSON.stringify(actualCheckpoints) === JSON.stringify(expectedCheckpoints),
    "evidence template must bind the exact C0 through C11 weight map",
  );
  assert(
    evidence.checkpoints.every(
      (checkpoint) =>
        checkpoint.status === "unverified" &&
        Array.isArray(checkpoint.evidence) &&
        checkpoint.evidence.length === 0,
    ),
    "template checkpoints must remain unverified and empty",
  );
  const checkpointWeight = evidence.checkpoints.reduce(
    (total, checkpoint) => total + checkpoint.weight,
    0,
  );
  assert(checkpointWeight === 100, "checkpoint weights must total 100");

  assert(
    JSON.stringify(evidence.automatedEvidenceContracts) ===
      JSON.stringify({
        packageBrowserSmoke: {
          profile:
            "docs/evidence/fixtures/m1-package-browser-smoke-profile.v1.json",
          resultSchema:
            "docs/evidence/schemas/m1-package-browser-smoke-result.schema.v1.json",
          command:
            "pnpm measure:package-browser -- --editor-origin http://127.0.0.1:<port>",
          resultStatus: "pending_result",
        },
        performanceGeometryCorpus: {
          profile:
            "docs/evidence/fixtures/m1-performance-geometry-profile.v1.json",
          resultSchema:
            "docs/evidence/schemas/m1-performance-geometry-result.schema.v1.json",
          command: "pnpm measure:performance-geometry",
          resultStatus: "pending_result",
        },
        productionIntegrationPerformance: {
          profile:
            "docs/evidence/fixtures/m1-production-integration-performance-profile.v1.json",
          resultSchema:
            "docs/evidence/schemas/m1-production-integration-performance-result.schema.v1.json",
          command:
            "pnpm measure:production-integration-performance -- --editor-origin http://127.0.0.1:<port> --change-set-probe <packaged-probe-path>",
          resultStatus: "pending_result",
        },
        safeObservability: {
          profile: "docs/evidence/fixtures/m1-safe-log-profile.v1.json",
          resultSchema:
            "docs/evidence/schemas/m1-safe-log-result.schema.v1.json",
          command:
            "pnpm verify:package -- --output-dir <outside-repository-directory>",
          resultStatus: "pending_result",
        },
      }),
    "automated evidence contracts must bind the four pinned profiles and remain pending",
  );

  assert(
    evidence.manualEvidence?.creator80PercentVerification?.status === "pending",
    "Creator verification must remain pending in the template",
  );
  assert(
    evidence.manualEvidence?.screenReader?.status === "pending",
    "screen-reader evidence must remain pending in the template",
  );
  assert(
    evidence.manualEvidence?.liveProvider?.status ===
      "pending_external_credential_and_billing_acknowledgement",
    "live-provider evidence must not be fabricated",
  );
  assert(
    evidence.licenseAndReleaseBoundary?.responsibleOwner === "unassigned" &&
      evidence.licenseAndReleaseBoundary?.licenseStatus === "unresolved" &&
      evidence.licenseAndReleaseBoundary?.brandStatus === "unresolved" &&
      evidence.licenseAndReleaseBoundary?.releaseCreated === false &&
      evidence.licenseAndReleaseBoundary?.tagCreated === false,
    "release and license template boundary is not fail-closed",
  );

  const serialized = JSON.stringify({ support, evidence });
  for (const forbidden of ["/home/", "/tmp/", "sk-", "Bearer "]) {
    assert(
      !serialized.includes(forbidden),
      "template contains a privacy canary",
    );
  }

  process.stdout.write(
    `${JSON.stringify({
      schemaVersion: "synapsegit-lp-studio.evidence-template-check/1",
      status: "passed",
      checkpointCount: evidence.checkpoints.length,
      checkpointWeight,
      completionState: evidence.completionState,
    })}\n`,
  );
};

await main().catch((error) => {
  const reason =
    error instanceof TemplateFailure
      ? error.message
      : "template_read_or_parse_failed";
  process.stderr.write(
    `${JSON.stringify({
      schemaVersion: "synapsegit-lp-studio.evidence-template-check/1",
      status: "failed",
      reason,
    })}\n`,
  );
  process.exitCode = 1;
});
