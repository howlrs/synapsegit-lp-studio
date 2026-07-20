import { createHash, randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import {
  chmod,
  cp,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readlink,
  readdir,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from "node:path";
import { stripVTControlCharacters } from "node:util";
import Ajv2020 from "ajv/dist/2020.js";

const SCHEMA_VERSION = "synapsegit-lp-studio.local-package-evidence/1";
const AUTOMATED_EVIDENCE_SCHEMA_VERSION =
  "synapsegit-lp-studio.automated-evidence-record/1";
const SAFE_LOG_SCHEMA_VERSION = "synapsegit-lp-studio.safe-log-result/1";
const SUPPORTED_RUST_HOST = "x86_64-unknown-linux-gnu";
const SYNAPSEGIT_SOURCE_REVISION = "5352aa9412dfdd2ad6cfcf3746770d015af11b49";
const SYNAPSEGIT_LICENSE_SHA256 =
  "200d6c727d7b3b62c85e7672c5615d21e5081fd95b8935acc5424bc98305415e";
const repositoryRoot = resolve(import.meta.dirname, "..");
const packagePrefix = "synapsegit-lp-studio-package-smoke-";
const temporaryPrefix = join(tmpdir(), packagePrefix);
const runtimeCaptureMaximumBytes = 8 * 1024 * 1024;

const allowedEnvironment = (names) =>
  Object.fromEntries(
    names
      .filter((name) => process.env[name] !== undefined)
      .map((name) => [name, process.env[name]]),
  );
const sharedEnvironmentNames = [
  "PATH",
  "HOME",
  "USER",
  "LOGNAME",
  "SHELL",
  "TMPDIR",
  "TMP",
  "TEMP",
  "XDG_CACHE_HOME",
  "XDG_CONFIG_HOME",
  "LD_LIBRARY_PATH",
  "FONTCONFIG_FILE",
  "FONTCONFIG_PATH",
  "PLAYWRIGHT_BROWSERS_PATH",
];
const runtimeEnvironment = {
  ...allowedEnvironment(sharedEnvironmentNames),
  CI: "1",
  LANG: "C.UTF-8",
  LC_ALL: "C.UTF-8",
  TZ: "UTC",
};
const buildEnvironment = {
  ...runtimeEnvironment,
  ...allowedEnvironment([
    "CARGO_HOME",
    "RUSTUP_HOME",
    "PNPM_HOME",
    "COREPACK_HOME",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
  ]),
  NODE_ENV: "production",
};
const dependencyInstallEnvironment = { ...buildEnvironment };
delete dependencyInstallEnvironment.NODE_ENV;

class SmokeFailure extends Error {
  constructor(code, details = undefined) {
    super(code);
    this.name = "SmokeFailure";
    this.code = code;
    this.details = details;
  }
}

const activeChildren = new Set();
let interruptedSignal;
let temporaryRoot;
let runningServer;
let evidence;
let outputDirectory;
let allowDirty = false;
let sourceEvidence;
let failedBrowserSmoke;
let failedPerformanceGeometry;
let failedProductionIntegrationPerformance;

const parseArguments = () => {
  const args = process.argv.slice(2);
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--") {
      continue;
    } else if (argument === "--allow-dirty") {
      allowDirty = true;
    } else if (argument === "--output-dir") {
      const value = args[++index];
      if (value === undefined)
        throw new SmokeFailure("output_directory_required");
      outputDirectory = resolve(value);
    } else {
      throw new SmokeFailure("unknown_argument");
    }
  }
  if (outputDirectory !== undefined) {
    const fromRepository = relative(repositoryRoot, outputDirectory);
    if (
      outputDirectory === repositoryRoot ||
      (!fromRepository.startsWith(`..${sep}`) &&
        fromRepository !== ".." &&
        !isAbsolute(fromRepository))
    ) {
      throw new SmokeFailure("output_directory_inside_repository");
    }
  }
};

const registerChild = (child) => {
  activeChildren.add(child);
  const forget = () => activeChildren.delete(child);
  child.once("error", forget);
  child.once("exit", forget);
  return child;
};

const signalChild = (child, signal) => {
  if (child.exitCode !== null || child.signalCode !== null) return;
  try {
    if (process.platform === "linux" && child.pid !== undefined) {
      process.kill(-child.pid, signal);
    } else {
      child.kill(signal);
    }
  } catch {
    // The child may have exited between the state check and signal delivery.
  }
};

const interruptionCode = () =>
  interruptedSignal === "SIGINT"
    ? "smoke_interrupted_sigint"
    : "smoke_interrupted_sigterm";

const recordCleanupFailure = (code) => {
  process.exitCode = 1;
  const priorCodes = Array.isArray(evidence?.cleanupErrorCodes)
    ? evidence.cleanupErrorCodes
    : [];
  evidence =
    evidence?.status === "failed"
      ? { ...evidence, cleanupErrorCodes: [...priorCodes, code] }
      : {
          schemaVersion: SCHEMA_VERSION,
          status: "failed",
          errorCode: code,
          cleanupErrorCodes: [code],
        };
};

const throwIfInterrupted = () => {
  if (interruptedSignal !== undefined) {
    throw new SmokeFailure(interruptionCode());
  }
};

const interrupt = (signal) => {
  const repeated = interruptedSignal !== undefined;
  interruptedSignal ??= signal;
  for (const child of activeChildren) {
    signalChild(
      child,
      repeated ? "SIGKILL" : child === runningServer ? "SIGINT" : "SIGTERM",
    );
  }
};

const onSigint = () => interrupt("SIGINT");
const onSigterm = () => interrupt("SIGTERM");
process.on("SIGINT", onSigint);
process.on("SIGTERM", onSigterm);

const run = async (command, args, options = {}) =>
  new Promise((resolveRun, rejectRun) => {
    const child = registerChild(
      spawn(command, args, {
        cwd: options.cwd ?? repositoryRoot,
        env: options.env ?? process.env,
        stdio: options.capture ? ["ignore", "pipe", "pipe"] : "inherit",
        detached: process.platform === "linux",
        shell: false,
      }),
    );
    let stdout = "";
    let stderr = "";
    const captureLimit = options.captureLimit ?? 16_384;
    if (options.capture) {
      child.stdout.setEncoding("utf8");
      child.stderr.setEncoding("utf8");
      child.stdout.on("data", (chunk) => {
        stdout = `${stdout}${chunk}`.slice(-captureLimit);
      });
      child.stderr.on("data", (chunk) => {
        stderr = `${stderr}${chunk}`.slice(-captureLimit);
      });
    }
    child.once("error", () =>
      rejectRun(new SmokeFailure("process_spawn_failed")),
    );
    child.once("close", (code, signal) => {
      if (code === 0) {
        resolveRun({ stdout, stderr });
      } else {
        rejectRun(
          new SmokeFailure(
            signal === null
              ? `process_exit_${String(code)}`
              : "process_signalled",
            { stdout, stderr, exitCode: code, signal },
          ),
        );
      }
    });
  });

const runStep = async (failureCode, command, args, options = {}) => {
  throwIfInterrupted();
  try {
    const result = await run(command, args, options);
    throwIfInterrupted();
    return result;
  } catch {
    throwIfInterrupted();
    throw new SmokeFailure(failureCode);
  }
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

const jsonSha256 = (value) =>
  sha256(Buffer.from(JSON.stringify(value), "utf8"));

const createRuntimeCapture = () => {
  const streams = {
    stdout: { byteLength: 0, chunks: [], overflow: false },
    stderr: { byteLength: 0, chunks: [], overflow: false },
  };
  const append = (streamName) => (chunk) => {
    const stream = streams[streamName];
    const bytes = Buffer.from(chunk);
    stream.byteLength += bytes.byteLength;
    if (stream.byteLength > runtimeCaptureMaximumBytes) {
      stream.overflow = true;
      return;
    }
    stream.chunks.push(bytes);
  };
  const bytes = (streamName) => {
    const stream = streams[streamName];
    if (stream.overflow) {
      throw new SmokeFailure("runtime_log_capture_limit_exceeded");
    }
    return Buffer.concat(stream.chunks, stream.byteLength);
  };
  return {
    onStdout: append("stdout"),
    onStderr: append("stderr"),
    bytes,
  };
};

const validateSafeLogProfile = (profile) => {
  const exactArray = (value, expected) =>
    Array.isArray(value) && JSON.stringify(value) === JSON.stringify(expected);
  if (
    profile?.schemaVersion !== "synapsegit-lp-studio.safe-log-profile/1" ||
    profile.scope !== "packaged_local_fake_provider_flow" ||
    profile.maximumSupportedLogLevel !== "trace" ||
    profile.provider?.id !== "fake" ||
    profile.provider?.model !== "deterministic-v1" ||
    profile.provider?.external !== false ||
    profile.decisionDisposition !== "adopted_unchanged" ||
    !exactArray(profile.requiredRequestFields, [
      "operation_id",
      "correlation_id",
      "error_code",
      "duration_ms",
      "version",
    ]) ||
    !exactArray(profile.requiredAdapterPhases, [
      "proposal.publish.begin",
      "proposal.publish.observed",
      "decision.publish.begin",
      "decision.publish.observed",
    ]) ||
    !exactArray(profile.forbiddenCanaryClasses, [
      "prompt",
      "session_token",
      "approval_token",
      "file_body",
      "private_rationale",
      "absolute_path",
      "provider_raw_response",
    ]) ||
    profile.intentionallyFailedRequest?.errorCode !== "invalid_request" ||
    !exactArray(profile.requirementIds, [
      "NFR-OBS-001",
      "NFR-OBS-002",
      "NFR-OBS-004",
      "TEST-001",
      "TEST-005",
    ]) ||
    profile.pendingGates?.liveProvider !==
      "pending_external_credential_and_billing_acknowledgement" ||
    profile.pendingGates?.creatorVerification !== "pending" ||
    profile.pendingGates?.manualKeyboardZoomContrast !== "pending" ||
    profile.pendingGates?.screenReader !== "pending" ||
    profile.pendingGates?.license !== "unresolved" ||
    profile.pendingGates?.brand !== "unresolved" ||
    profile.pendingGates?.releaseCreated !== false
  ) {
    throw new SmokeFailure("safe_log_profile_invalid");
  }
};

const createSafeLogImportFixture = async (
  sourceSnapshotRoot,
  importRoot,
  canaries,
) => {
  const [blankIndex, blankStyles] = await Promise.all([
    readFile(resolve(sourceSnapshotRoot, "templates/blank/index.html"), "utf8"),
    readFile(resolve(sourceSnapshotRoot, "templates/blank/styles.css"), "utf8"),
  ]);
  if (!blankIndex.includes("<body>") || !blankIndex.includes("</body>")) {
    throw new SmokeFailure("safe_log_import_fixture_invalid");
  }
  const performanceFixture = `  <div id="production-performance-zoom-root">
  <section id="production-performance-outer-scroll" aria-label="Production performance outer scroll fixture">
    <div id="production-performance-inner-scroll">
      <div class="production-performance-plane">
        <button id="production-performance-target" data-lp-id="production-performance-target" type="button">Production integration geometry target</button>
      </div>
    </div>
  </section>
  <div class="production-performance-page-spacer" aria-hidden="true"></div>
  </div>`;
  const index = blankIndex
    .replace("<body>", `<body>\n${performanceFixture}`)
    .replace("</body>", `  <!-- ${canaries.providerRawResponse} -->\n</body>`);
  const styles = `${blankStyles}
body { min-height: 1400px; }
#production-performance-outer-scroll {
  width: min(720px, calc(100% - 32px));
  height: 260px;
  overflow: auto;
  margin: 24px auto;
  padding-top: 40px;
  border: 1px solid #456;
}
#production-performance-inner-scroll {
  width: 100%;
  height: 520px;
  overflow: auto;
  position: relative;
}
.production-performance-plane {
  width: 1200px;
  height: 700px;
  position: relative;
}
#production-performance-target {
  position: absolute;
  left: 80px;
  top: 80px;
  width: 120px;
  height: 72px;
}
.production-performance-page-spacer { height: 900px; }
/* ${canaries.fileBody} */
/* ${canaries.absolutePath} */
`;
  await mkdir(importRoot, { recursive: false, mode: 0o700 });
  await Promise.all([
    writeFile(join(importRoot, "index.html"), index, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    }),
    writeFile(join(importRoot, "styles.css"), styles, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    }),
  ]);
};

const runtimeApiRequest = async ({
  editorOrigin,
  path,
  method,
  token,
  body,
  expectedStatus,
  expectedRoute,
  expectedErrorCode = "none",
  operations,
}) => {
  const headers = {
    Accept: "application/json",
    Origin: editorOrigin,
    "Sec-Fetch-Site": "same-origin",
  };
  if (token !== undefined) headers.Authorization = `Bearer ${token}`;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  let response;
  try {
    response = await fetch(new URL(path, editorOrigin), {
      method,
      headers,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(15_000),
    });
  } catch {
    throw new SmokeFailure("safe_log_flow_request_failed");
  }
  const operationId = response.headers.get("x-operation-id");
  const correlationId = response.headers.get("x-request-id");
  const errorCode = response.headers.get("x-error-code") ?? "none";
  if (
    response.status !== expectedStatus ||
    !/^op_[0-9a-f]{32}$/u.test(operationId ?? "") ||
    !/^(?:op|req)_[0-9a-f]{32}$/u.test(correlationId ?? "") ||
    errorCode !== expectedErrorCode
  ) {
    throw new SmokeFailure("safe_log_flow_response_invalid");
  }
  let value;
  try {
    value = JSON.parse(await response.text());
  } catch {
    throw new SmokeFailure("safe_log_flow_response_invalid");
  }
  operations.push({
    operationId,
    correlationId,
    method,
    route: expectedRoute,
    status: expectedStatus,
    errorCode,
  });
  return value;
};

const runSafeLogFakeFlow = async (editorOrigin, canaries) => {
  const operations = [];
  const bootstrap = await runtimeApiRequest({
    editorOrigin,
    path: "/api/v1/bootstrap",
    method: "GET",
    expectedStatus: 200,
    expectedRoute: "/api/v1/bootstrap",
    operations,
  });
  const sessionToken = bootstrap?.session?.token;
  if (
    typeof sessionToken !== "string" ||
    sessionToken.length === 0 ||
    bootstrap?.capabilities?.importAvailable !== true
  ) {
    throw new SmokeFailure("safe_log_flow_bootstrap_invalid");
  }
  const importPreview = await runtimeApiRequest({
    editorOrigin,
    path: "/api/v1/imports/previews",
    method: "POST",
    token: sessionToken,
    body: { schemaVersion: "1" },
    expectedStatus: 201,
    expectedRoute: "/api/v1/imports/previews",
    operations,
  });
  const previewId = importPreview?.importPreview?.id;
  const importManifestSha256 = importPreview?.importPreview?.manifestSha256;
  if (
    typeof previewId !== "string" ||
    !/^[0-9a-f]{64}$/u.test(importManifestSha256 ?? "")
  ) {
    throw new SmokeFailure("safe_log_flow_import_preview_invalid");
  }
  const projectResponse = await runtimeApiRequest({
    editorOrigin,
    path: `/api/v1/imports/${encodeURIComponent(previewId)}/confirm`,
    method: "POST",
    token: sessionToken,
    body: {
      schemaVersion: "1",
      expectedManifestSha256: importManifestSha256,
    },
    expectedStatus: 201,
    expectedRoute: "/api/v1/imports/{preview_id}/confirm",
    operations,
  });
  const projectId = projectResponse?.project?.id;
  const revisionId = projectResponse?.project?.revisionId;
  if (typeof projectId !== "string" || typeof revisionId !== "string") {
    throw new SmokeFailure("safe_log_flow_project_invalid");
  }
  const targetId = `tgt_${randomUUID().replaceAll("-", "")}`;
  const targetResponse = await runtimeApiRequest({
    editorOrigin,
    path: `/api/v1/projects/${encodeURIComponent(projectId)}/targets`,
    method: "POST",
    token: sessionToken,
    body: {
      schemaVersion: "1",
      target: {
        schemaVersion: 1,
        targetId,
        captureRevisionId: revisionId,
        captureSource: "accepted",
        pagePath: "index.html",
        kind: "element",
        label: "Safe-log fake-flow heading",
        viewport: {
          cssWidth: 1280,
          cssHeight: 800,
          scrollX: 0,
          scrollY: 0,
          devicePixelRatio: 1,
          visualViewportScale: 1,
          previewScale: 1,
        },
        document: { cssWidth: 1280, cssHeight: 1600, layoutEpoch: 1 },
        elementAnchor: {
          tagName: "h1",
          uniqueElementId: "hero-heading",
          role: "heading",
          accessibleName: "まだ、白紙です。",
          domPath: "main/section#hero/h1#hero-heading",
        },
      },
    },
    expectedStatus: 201,
    expectedRoute: "/api/v1/projects/{project_id}/targets",
    operations,
  });
  if (
    targetResponse?.target?.targetId !== targetId ||
    targetResponse?.resolution?.status !== "resolved" ||
    typeof targetResponse?.resolutionId !== "string"
  ) {
    throw new SmokeFailure("safe_log_flow_target_invalid");
  }
  const attemptId = `safe-log-attempt-${randomUUID()}`;
  const contextResponse = await runtimeApiRequest({
    editorOrigin,
    path: `/api/v1/projects/${encodeURIComponent(projectId)}/contexts`,
    method: "POST",
    token: sessionToken,
    body: {
      schemaVersion: "1",
      revisionId,
      targetId,
      resolutionId: targetResponse.resolutionId,
      attemptId,
      providerId: "fake",
      requestedModel: "deterministic-v1",
      instruction: canaries.prompt,
    },
    expectedStatus: 201,
    expectedRoute: "/api/v1/projects/{project_id}/contexts",
    operations,
  });
  const context = contextResponse?.context;
  if (
    context?.attemptId !== attemptId ||
    context?.provider?.external !== false ||
    typeof context?.canonicalJson !== "string" ||
    !context.canonicalJson.includes(canaries.fileBody) ||
    !context.canonicalJson.includes(canaries.absolutePath) ||
    !context.canonicalJson.includes(canaries.providerRawResponse) ||
    !/^[0-9a-f]{64}$/u.test(context?.sha256 ?? "")
  ) {
    throw new SmokeFailure("safe_log_flow_context_invalid");
  }
  const proposalResponse = await runtimeApiRequest({
    editorOrigin,
    path: `/api/v1/projects/${encodeURIComponent(projectId)}/proposals`,
    method: "POST",
    token: sessionToken,
    body: {
      schemaVersion: "1",
      contextId: context.id,
      contextSha256: context.sha256,
    },
    expectedStatus: 201,
    expectedRoute: "/api/v1/projects/{project_id}/proposals",
    operations,
  });
  const proposal = proposalResponse?.proposal;
  const proposalOperationId = operations.at(-1).operationId;
  if (
    proposal?.attribution?.providerId !== "fake" ||
    proposal?.attribution?.external !== false ||
    proposal?.sourceAttribution !== "caller_supplied_ai_attributed" ||
    proposal?.executionVerified !== false ||
    !JSON.stringify(proposal?.changeSet).includes(canaries.providerRawResponse)
  ) {
    throw new SmokeFailure("safe_log_flow_proposal_invalid");
  }
  const intentId = `safe-log-intent-${randomUUID()}`;
  const approvalResponse = await runtimeApiRequest({
    editorOrigin,
    path: `/api/v1/reviews/${encodeURIComponent(proposal.reviewId)}/approvals`,
    method: "POST",
    token: sessionToken,
    body: {
      schemaVersion: "1",
      proposalId: proposal.id,
      expectedRevisionId: revisionId,
      disposition: "adopted_unchanged",
      intentId,
    },
    expectedStatus: 201,
    expectedRoute: "/api/v1/reviews/{review_id}/approvals",
    operations,
  });
  const approvalToken = approvalResponse?.approval?.token;
  if (
    typeof approvalToken !== "string" ||
    approvalToken.length === 0 ||
    approvalResponse?.approval?.intentId !== intentId
  ) {
    throw new SmokeFailure("safe_log_flow_approval_invalid");
  }
  const decisionResponse = await runtimeApiRequest({
    editorOrigin,
    path: `/api/v1/reviews/${encodeURIComponent(proposal.reviewId)}/decisions`,
    method: "POST",
    token: sessionToken,
    body: {
      schemaVersion: "1",
      approvalToken,
      proposalId: proposal.id,
      expectedRevisionId: revisionId,
      disposition: "adopted_unchanged",
      intentId,
      rationale: canaries.privateRationale,
    },
    expectedStatus: 200,
    expectedRoute: "/api/v1/reviews/{review_id}/decisions",
    operations,
  });
  const decisionOperationId = operations.at(-1).operationId;
  if (
    decisionResponse?.decision?.status !== "committed" ||
    decisionResponse?.decision?.disposition !== "adopted_unchanged"
  ) {
    throw new SmokeFailure("safe_log_flow_decision_invalid");
  }
  await runtimeApiRequest({
    editorOrigin,
    path: "/api/v1/projects",
    method: "POST",
    token: sessionToken,
    body: { schemaVersion: "1", template: "blank", unexpected: true },
    expectedStatus: 400,
    expectedRoute: "/api/v1/projects",
    expectedErrorCode: "invalid_request",
    operations,
  });
  return {
    operations,
    proposalOperationId,
    decisionOperationId,
    canaries: [
      { class: "prompt", value: canaries.prompt },
      { class: "session_token", value: sessionToken },
      { class: "approval_token", value: approvalToken },
      { class: "file_body", value: canaries.fileBody },
      { class: "private_rationale", value: canaries.privateRationale },
      { class: "absolute_path", value: canaries.absolutePath },
      {
        class: "provider_raw_response",
        value: canaries.providerRawResponse,
      },
    ],
  };
};

const parseLogField = (line, field) => {
  const match = line.match(
    new RegExp(`(?:^|\\s)${field}=(?:"([^"]*)"|([^\\s]+))`, "u"),
  );
  return match?.[1] ?? match?.[2];
};

const countBucket = (count) => {
  if (count === 0) return "0";
  if (count < 10) return "1_9";
  if (count < 100) return "10_99";
  return "100_plus";
};

const waitForSafeLogOperations = async (capture, flow) => {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    const combined = Buffer.concat([
      capture.bytes("stdout"),
      Buffer.from("\n"),
      capture.bytes("stderr"),
    ]).toString("utf8");
    if (stripVTControlCharacters(combined) !== combined) {
      throw new SmokeFailure("runtime_log_ansi_control_sequence_rejected");
    }
    const requestLines = combined
      .split(/\r?\n/u)
      .filter((line) => line.includes("local request completed"));
    let complete = true;
    for (const [index, operation] of flow.operations.entries()) {
      const matchCount = requestLines.filter(
        (line) => parseLogField(line, "operation_id") === operation.operationId,
      ).length;
      if (matchCount > 1) {
        throw new SmokeFailure(
          `safe_log_request_correlation_duplicate_${String(index)}`,
        );
      }
      if (matchCount === 0) complete = false;
    }
    if (complete) return;
    await new Promise((resolveWait) => setTimeout(resolveWait, 25));
  }
  const stdout = capture.bytes("stdout");
  const stderr = capture.bytes("stderr");
  const combined = Buffer.concat([stdout, Buffer.from("\n"), stderr]).toString(
    "utf8",
  );
  if (stripVTControlCharacters(combined) !== combined) {
    throw new SmokeFailure("runtime_log_ansi_control_sequence_rejected");
  }
  const requestLines = combined
    .split(/\r?\n/u)
    .filter((line) => line.includes("local request completed"));
  const missingIndex = flow.operations.findIndex(
    (operation) =>
      !requestLines.some(
        (line) => parseLogField(line, "operation_id") === operation.operationId,
      ),
  );
  if (stdout.byteLength === 0 && stderr.byteLength === 0) {
    throw new SmokeFailure("safe_log_capture_empty");
  }
  if (requestLines.length === 0) {
    throw new SmokeFailure("safe_log_no_request_lines");
  }
  const missingOperation = flow.operations[Math.max(0, missingIndex)];
  const parsedOperationIds = requestLines
    .map((line) => parseLogField(line, "operation_id"))
    .filter((value) => value !== undefined);
  const bucketSuffix = `request_${countBucket(requestLines.length)}_parsed_${countBucket(parsedOperationIds.length)}`;
  if (
    missingOperation !== undefined &&
    combined.includes(missingOperation.operationId)
  ) {
    throw new SmokeFailure(
      `safe_log_expected_id_raw_present_but_parser_miss_${bucketSuffix}`,
    );
  }
  if (parsedOperationIds.length > 0) {
    throw new SmokeFailure(
      `safe_log_request_lines_parsed_but_id_mismatch_${bucketSuffix}`,
    );
  }
  throw new SmokeFailure(`safe_log_request_lines_unparsed_${bucketSuffix}`);
};

const validateSafeRuntimeLogs = ({
  capture,
  flow,
  profile,
  profileBytes,
  absoluteRuntimePaths,
}) => {
  const stdout = capture.bytes("stdout");
  const stderr = capture.bytes("stderr");
  const combined = Buffer.concat([stdout, Buffer.from("\n"), stderr]).toString(
    "utf8",
  );
  const forbiddenValues = [
    ...flow.canaries.map((entry) => entry.value),
    ...absoluteRuntimePaths,
    "Bearer ",
  ];
  if (
    forbiddenValues.some(
      (value) => value.length > 0 && combined.includes(value),
    )
  ) {
    throw new SmokeFailure("runtime_log_privacy_canary_exposed");
  }
  const lines = combined.split(/\r?\n/u);
  const requestLines = lines.filter((line) =>
    line.includes("local request completed"),
  );
  const expectedOperationIds = new Set(
    flow.operations.map((operation) => operation.operationId),
  );
  const scopedRequestLines = requestLines.filter((line) =>
    expectedOperationIds.has(parseLogField(line, "operation_id")),
  );
  if (expectedOperationIds.size !== flow.operations.length) {
    throw new SmokeFailure("safe_log_operation_id_not_unique");
  }
  for (const [index, operation] of flow.operations.entries()) {
    const matches = scopedRequestLines.filter(
      (line) => parseLogField(line, "operation_id") === operation.operationId,
    );
    if (matches.length === 0)
      throw new SmokeFailure(
        `safe_log_request_correlation_missing_${String(index)}`,
      );
    if (matches.length > 1)
      throw new SmokeFailure(
        `safe_log_request_correlation_duplicate_${String(index)}`,
      );
    const line = matches[0];
    if (
      parseLogField(line, "correlation_id") !== operation.correlationId ||
      parseLogField(line, "method") !== operation.method ||
      parseLogField(line, "route") !== operation.route ||
      parseLogField(line, "status") !== String(operation.status) ||
      parseLogField(line, "error_code") !== operation.errorCode ||
      !/^\d+$/u.test(parseLogField(line, "duration_ms") ?? "") ||
      !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/u.test(
        parseLogField(line, "version") ?? "",
      )
    ) {
      throw new SmokeFailure("safe_log_required_field_missing");
    }
  }
  if (scopedRequestLines.length !== flow.operations.length) {
    throw new SmokeFailure("safe_log_request_correlation_count_invalid");
  }
  const requiredAdapterPhases = new Map([
    [
      flow.proposalOperationId,
      ["proposal.publish.begin", "proposal.publish.observed"],
    ],
    [
      flow.decisionOperationId,
      ["decision.publish.begin", "decision.publish.observed"],
    ],
  ]);
  for (const [operationId, phases] of requiredAdapterPhases) {
    for (const phase of phases) {
      const matches = lines.filter(
        (line) =>
          parseLogField(line, "operation_id") === operationId &&
          parseLogField(line, "component") === "synapsegit-adapter" &&
          parseLogField(line, "phase") === phase,
      );
      if (matches.length !== 1) {
        throw new SmokeFailure("safe_log_adapter_correlation_invalid");
      }
    }
  }
  const canaryChecks = flow.canaries.map((entry) => ({
    class: entry.class,
    sha256: sha256(Buffer.from(entry.value, "utf8")),
    absent: true,
  }));
  const result = {
    schemaVersion: SAFE_LOG_SCHEMA_VERSION,
    status: "passed",
    scope: profile.scope,
    profileSha256: sha256(profileBytes),
    maximumSupportedLogLevel: profile.maximumSupportedLogLevel,
    flow: {
      providerId: "fake",
      requestedModel: "deterministic-v1",
      externalProviderConfigured: false,
      fakeProviderAttributionExternal: false,
      decisionDisposition: "adopted_unchanged",
      importedFileBodyExercised: true,
      providerRawResponseExercised: true,
      intentionallyFailedRequestErrorCode: "invalid_request",
      requestOperationCount: flow.operations.length,
    },
    logCapture: {
      stdoutByteLength: stdout.byteLength,
      stdoutSha256: sha256(stdout),
      stderrByteLength: stderr.byteLength,
      stderrSha256: sha256(stderr),
      rawLogsRetained: false,
    },
    requiredSafeFields: {
      fields: profile.requiredRequestFields,
      requestRecordCount: flow.operations.length,
      errorRecordCount: flow.operations.filter(
        (operation) => operation.errorCode !== "none",
      ).length,
      checksPassed: true,
    },
    adapterCorrelation: {
      component: "synapsegit-adapter",
      phases: profile.requiredAdapterPhases,
      correlatedOperationCount: requiredAdapterPhases.size,
      checksPassed: true,
    },
    privacyScan: {
      canaryChecks,
      absoluteRuntimePathCount: absoluteRuntimePaths.length,
      authorizationSchemeAbsent: true,
      rawCanaryValuesRetained: false,
      checksPassed: true,
    },
    requirementIds: profile.requirementIds,
    pendingGates: profile.pendingGates,
    claimBoundary: {
      defaultLevelIncludedByMaximumLevel: true,
      safeAtMaximumSupportedLevel: true,
      liveProviderCovered: false,
      productionReady: false,
      releaseCreated: false,
    },
  };
  return result;
};

const automatedEvidenceRecord = ({
  source,
  browserSmoke,
  performanceGeometry,
  productionIntegrationPerformance,
  safeObservability,
  artifactManifestSha256,
}) => ({
  schemaVersion: AUTOMATED_EVIDENCE_SCHEMA_VERSION,
  recordKind: "automated_result",
  status: "passed",
  scope: "local_internal_evaluation_only",
  sourceBinding: {
    revision: source.revision,
    clean: source.clean,
    evaluationMode: source.evaluationMode,
    statusSha256: source.statusSha256,
    contentFingerprintSha256: source.contentFingerprintSha256,
  },
  results: [
    {
      id: "package-browser-smoke",
      status: "passed",
      coverageKind: "partial_automated_evidence",
      command: {
        executable: "node",
        arguments: [
          "scripts/run-m1-package-browser-smoke.mjs",
          "--editor-origin",
          "<ephemeral-loopback-editor-origin>",
        ],
        redactions: ["ephemeral_loopback_editor_origin"],
      },
      profile: {
        path: "docs/evidence/fixtures/m1-package-browser-smoke-profile.v1.json",
        sha256: browserSmoke.profileSha256,
      },
      result: {
        embeddedAt: "runtime.browserSmoke",
        sha256: jsonSha256(browserSmoke),
      },
      requirementIds: ["NFR-A11Y-006", "NFR-A11Y-007", "TEST-005"],
    },
    {
      id: "performance-geometry-corpus",
      status: "passed",
      coverageKind: "partial_automated_evidence",
      command: {
        executable: "node",
        arguments: ["scripts/run-m1-performance-geometry-corpus.mjs"],
        redactions: [],
      },
      profile: {
        path: "docs/evidence/fixtures/m1-performance-geometry-profile.v1.json",
        sha256: performanceGeometry.profileSha256,
      },
      result: {
        embeddedAt: "acceptance.performanceGeometry",
        sha256: jsonSha256(performanceGeometry),
      },
      requirementIds: [
        "NFR-PERF-001",
        "NFR-PERF-002",
        "NFR-PERF-003",
        "NFR-PERF-004",
        "TEST-005",
      ],
    },
    {
      id: "production-integration-performance",
      status: "passed",
      coverageKind: "partial_automated_evidence",
      command: {
        executable: "node",
        arguments: [
          "scripts/run-m1-production-integration-performance.mjs",
          "--editor-origin",
          "<ephemeral-loopback-editor-origin>",
          "--change-set-probe",
          "<packaged-change-set-probe>",
        ],
        redactions: [
          "ephemeral_loopback_editor_origin",
          "ephemeral_absolute_probe_path",
        ],
      },
      profile: {
        path: "docs/evidence/fixtures/m1-production-integration-performance-profile.v1.json",
        sha256: productionIntegrationPerformance.profileSha256,
      },
      result: {
        embeddedAt: "acceptance.productionIntegrationPerformance",
        sha256: jsonSha256(productionIntegrationPerformance),
      },
      requirementIds: [
        "FR-PROJ-003",
        "NFR-PERF-001",
        "NFR-PERF-002",
        "NFR-PERF-005",
        "NFR-PERF-006",
        "TEST-005",
      ],
    },
    {
      id: "safe-observability-fake-flow",
      status: "passed",
      coverageKind: "automated_evidence",
      command: {
        executable: "pnpm",
        arguments: [
          "verify:package",
          "--",
          "--output-dir",
          "<outside-repository-output-directory>",
        ],
        redactions: ["ephemeral_absolute_output_directory"],
      },
      profile: {
        path: "docs/evidence/fixtures/m1-safe-log-profile.v1.json",
        sha256: safeObservability.profileSha256,
      },
      result: {
        embeddedAt: "runtime.safeObservability",
        sha256: jsonSha256(safeObservability),
      },
      requirementIds: safeObservability.requirementIds,
    },
    {
      id: "local-evaluation-package",
      status: "passed",
      coverageKind: "automated_evidence",
      command: {
        executable: "pnpm",
        arguments: [
          "verify:package",
          "--",
          "--output-dir",
          "<outside-repository-output-directory>",
        ],
        redactions: ["ephemeral_absolute_output_directory"],
      },
      result: {
        embeddedAt: "artifact.manifestSha256",
        sha256: artifactManifestSha256,
      },
      requirementIds: ["TEST-001", "TEST-005"],
    },
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
});

const artifactEntries = async (root) => {
  const entries = [];
  const visit = async (directory) => {
    const names = await readdir(directory);
    names.sort();
    for (const name of names) {
      const path = join(directory, name);
      const metadata = await lstat(path);
      if (metadata.isSymbolicLink()) {
        throw new SmokeFailure("package_symlink_rejected");
      }
      if (metadata.isDirectory()) {
        await visit(path);
        continue;
      }
      if (!metadata.isFile()) {
        throw new SmokeFailure("package_special_entry_rejected");
      }
      const bytes = await readFile(path);
      entries.push({
        path: relative(root, path).split(sep).join("/"),
        byteLength: bytes.byteLength,
        sha256: sha256(bytes),
        mode: metadata.mode & 0o777,
      });
    }
  };
  await visit(root);
  entries.sort((left, right) => left.path.localeCompare(right.path, "en"));
  return entries;
};

const aggregateManifest = (entries) =>
  sha256(Buffer.from(JSON.stringify(entries), "utf8"));

const readSourceFingerprint = async () => {
  const listed = await runStep(
    "source_file_list_failed",
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    { capture: true, captureLimit: 16 * 1024 * 1024 },
  );
  const paths = [...new Set(listed.stdout.split("\0").filter(Boolean))].sort(
    (left, right) => left.localeCompare(right, "en"),
  );
  const entries = [];
  for (const path of paths) {
    const absolute = resolve(repositoryRoot, path);
    const relativePath = relative(repositoryRoot, absolute);
    if (
      relativePath.length === 0 ||
      relativePath.startsWith(`..${sep}`) ||
      isAbsolute(relativePath)
    ) {
      throw new SmokeFailure("source_path_scope_rejected");
    }
    let metadata;
    try {
      metadata = await lstat(absolute);
    } catch (error) {
      if (error?.code === "ENOENT") {
        entries.push({ path, kind: "missing" });
        continue;
      }
      throw new SmokeFailure("source_fingerprint_read_failed");
    }
    if (metadata.isFile()) {
      const bytes = await readFile(absolute);
      entries.push({
        path,
        kind: "file",
        mode: metadata.mode & 0o777,
        byteLength: bytes.byteLength,
        sha256: sha256(bytes),
      });
    } else if (metadata.isSymbolicLink()) {
      await readlink(absolute);
      throw new SmokeFailure("source_symlink_rejected");
    } else {
      throw new SmokeFailure("source_special_entry_rejected");
    }
  }
  return {
    entries,
    fileCount: entries.length,
    sha256: aggregateManifest(entries),
  };
};

const materializeSourceSnapshot = async (fingerprint, snapshotRoot) => {
  const viteEnvironmentFiles = new Set([
    "apps/web/.env",
    "apps/web/.env.local",
    "apps/web/.env.production",
    "apps/web/.env.production.local",
  ]);
  await mkdir(snapshotRoot, { recursive: true, mode: 0o700 });
  for (const entry of fingerprint.entries) {
    if (viteEnvironmentFiles.has(entry.path)) {
      throw new SmokeFailure("vite_environment_file_rejected");
    }
    if (entry.kind === "missing") continue;
    if (entry.kind !== "file") {
      throw new SmokeFailure("source_snapshot_entry_rejected");
    }
    const source = resolve(repositoryRoot, entry.path);
    const destination = resolve(snapshotRoot, entry.path);
    await mkdir(dirname(destination), { recursive: true, mode: 0o700 });
    await copyFile(source, destination);
    await chmod(destination, entry.mode);
    const copiedBytes = await readFile(destination);
    if (
      copiedBytes.byteLength !== entry.byteLength ||
      sha256(copiedBytes) !== entry.sha256
    ) {
      throw new SmokeFailure("source_changed_during_snapshot");
    }
  }
};

const verifySourceSnapshot = async (fingerprint, snapshotRoot) => {
  const observedEntries = [];
  const visit = async (directory) => {
    const names = await readdir(directory);
    names.sort();
    for (const name of names) {
      const path = join(directory, name);
      const metadata = await lstat(path);
      const relativePath = relative(snapshotRoot, path).split(sep).join("/");
      if (name === "node_modules") {
        if (!metadata.isDirectory()) {
          throw new SmokeFailure("snapshot_dependency_tree_invalid");
        }
        continue;
      }
      if (metadata.isSymbolicLink()) {
        throw new SmokeFailure("snapshot_source_symlink_found");
      }
      if (metadata.isDirectory()) {
        await visit(path);
        continue;
      }
      if (!metadata.isFile()) {
        throw new SmokeFailure("snapshot_source_special_entry_found");
      }
      const bytes = await readFile(path);
      observedEntries.push({
        path: relativePath,
        kind: "file",
        mode: metadata.mode & 0o777,
        byteLength: bytes.byteLength,
        sha256: sha256(bytes),
      });
    }
  };
  await visit(snapshotRoot);
  observedEntries.sort((left, right) =>
    left.path.localeCompare(right.path, "en"),
  );
  const expectedEntries = fingerprint.entries.filter(
    (entry) => entry.kind === "file",
  );
  if (JSON.stringify(observedEntries) !== JSON.stringify(expectedEntries)) {
    throw new SmokeFailure("snapshot_source_integrity_failed");
  }
  return {
    fileCount: observedEntries.length,
    contentFingerprintSha256: aggregateManifest(observedEntries),
  };
};

const renderThirdPartyNotices = (cargoMetadata, pnpmLicenses) => {
  const nodeById = new Map(
    cargoMetadata.resolve.nodes.map((node) => [node.id, node]),
  );
  const reachablePackageIds = new Set();
  const pendingPackageIds = [...cargoMetadata.workspace_members];
  while (pendingPackageIds.length > 0) {
    const id = pendingPackageIds.pop();
    if (reachablePackageIds.has(id)) continue;
    reachablePackageIds.add(id);
    const node = nodeById.get(id);
    if (node === undefined) continue;
    for (const dependency of node.deps) {
      if (dependency.dep_kinds.some((kind) => kind.kind !== "dev")) {
        pendingPackageIds.push(dependency.pkg);
      }
    }
  }
  const cargoPackages = cargoMetadata.packages
    .filter(
      (entry) => entry.source !== null && reachablePackageIds.has(entry.id),
    )
    .map((entry) => ({
      name: entry.name,
      version: entry.version,
      license: entry.license ?? "NOT_DECLARED_IN_PACKAGE_METADATA",
      source:
        typeof entry.source === "string" && entry.source.startsWith("git+")
          ? "git"
          : "registry",
    }))
    .sort((left, right) =>
      `${left.name}@${left.version}`.localeCompare(
        `${right.name}@${right.version}`,
        "en",
      ),
    );
  const webPackages = Object.entries(pnpmLicenses)
    .flatMap(([license, entries]) =>
      entries.flatMap((entry) =>
        entry.versions.map((version) => ({
          name: entry.name,
          version,
          license: entry.license ?? license,
        })),
      ),
    )
    .sort((left, right) =>
      `${left.name}@${left.version}`.localeCompare(
        `${right.name}@${right.version}`,
        "en",
      ),
    );
  const lines = [
    "# Third-party notices and inventory",
    "",
    "This internal evaluation package contains compiled third-party software.",
    "The conservative inventory below is derived from the Linux x86-64 Cargo",
    "normal/build resolution and pnpm production package metadata. A resolved",
    "package is not a claim that every byte was linked into the artifact. The",
    "expressions are not a legal interpretation and do not replace corresponding",
    "upstream license texts or notices.",
    "",
    "`NOT_DECLARED_IN_PACKAGE_METADATA` means this build metadata did not provide",
    "a license expression. It must not be read as a permission or a prohibition.",
    "Redistribution permission for this package has not been recorded.",
    "The exact SynapseGit v0.4.0 license is retained separately at",
    "`licenses/SynapseGit-v0.4.0-LICENSE`.",
    "",
    "## Rust normal/build dependency inventory",
    "",
    "| Package | Version | Declared license expression | Source kind |",
    "| --- | --- | --- | --- |",
    ...cargoPackages.map(
      (entry) =>
        `| ${entry.name} | ${entry.version} | ${entry.license} | ${entry.source} |`,
    ),
    "",
    "## Web runtime dependency inventory",
    "",
    "| Package | Version | Declared license expression |",
    "| --- | --- | --- |",
    ...webPackages.map(
      (entry) => `| ${entry.name} | ${entry.version} | ${entry.license} |`,
    ),
    "",
  ];
  return {
    text: lines.join("\n"),
    cargoPackageCount: cargoPackages.length,
    webPackageCount: webPackages.length,
    undeclaredCargoLicenseCount: cargoPackages.filter(
      (entry) => entry.license === "NOT_DECLARED_IN_PACKAGE_METADATA",
    ).length,
  };
};

const readPinnedSynapseGitLicense = async (cargoMetadata) => {
  const expectedSource = `git+https://github.com/howlrs/synapsegit?rev=${SYNAPSEGIT_SOURCE_REVISION}#${SYNAPSEGIT_SOURCE_REVISION}`;
  const artifactPackage = cargoMetadata.packages.find(
    (entry) =>
      entry.name === "synapse-artifact" && entry.source === expectedSource,
  );
  if (artifactPackage === undefined) {
    throw new SmokeFailure("synapsegit_license_source_missing");
  }
  const checkoutRoot = resolve(dirname(artifactPackage.manifest_path), "../..");
  let licenseBytes;
  try {
    licenseBytes = await readFile(join(checkoutRoot, "LICENSE"));
  } catch {
    throw new SmokeFailure("synapsegit_license_read_failed");
  }
  if (sha256(licenseBytes) !== SYNAPSEGIT_LICENSE_SHA256) {
    throw new SmokeFailure("synapsegit_license_hash_mismatch");
  }
  return licenseBytes;
};

const packageReadme = `# SynapseGit LP Studio local evaluation package

This directory is a Linux x86-64 GNU local-evaluation artifact. It is not a
production release or a supported distribution.

Set an absolute, private state directory and start the packaged launcher:

~~~sh
LP_STUDIO_STATE_ROOT=/absolute/private/state ./bin/start-lp-studio
~~~

Open the \`editorOrigin\` printed in the \`LP_STUDIO_READY\` line. The separate
\`previewOrigin\` is an isolated Preview listener, not the Editor entry point.
Stop the server with Ctrl+C. Re-running the same command exercises restart with
the retained state root.

\`PACKAGE-MANIFEST.json\` records every payload file's byte length, mode, and
SHA-256 digest. The manifest explicitly records the unresolved license,
redistribution, brand, and release boundary. See \`LICENSE\` and
\`THIRD-PARTY-NOTICES.md\` before any use beyond this local evaluation scope.
The exact SynapseGit v0.4.0 source license is retained at
\`licenses/SynapseGit-v0.4.0-LICENSE\`.

No OpenAI credential is included. Live-provider use is optional, externally
billed, and not part of the automated package smoke.

\`bin/synapsegit-lp-application-performance-probe\` is an evidence-only fixed
fixture executable. It invokes the same production ChangeSet parser as the
local server for exactly ten text files totaling 2 MiB; it does not accept or
retain project content.
`;

const packageLauncher = `#!/bin/sh
set -eu

launcher_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
package_directory=$(CDPATH= cd -- "$launcher_directory/.." && pwd)

if [ -z "\${LP_STUDIO_STATE_ROOT:-}" ]; then
  echo "LP_STUDIO_STATE_ROOT must be an absolute private directory" >&2
  exit 64
fi

case "$LP_STUDIO_STATE_ROOT" in
  /*) ;;
  *)
    echo "LP_STUDIO_STATE_ROOT must be absolute" >&2
    exit 64
    ;;
esac

LP_STUDIO_WEB_DIST="$package_directory/web"
export LP_STUDIO_WEB_DIST
exec "$package_directory/bin/synapsegit-lp-local-server" "$@"
`;

const parseReadyLine = (line) => {
  const prefix = "LP_STUDIO_READY ";
  if (!line.startsWith(prefix)) return null;
  let value;
  try {
    value = JSON.parse(line.slice(prefix.length));
  } catch {
    return null;
  }
  if (
    typeof value.editorOrigin !== "string" ||
    typeof value.previewOrigin !== "string"
  ) {
    return null;
  }
  let editor;
  let preview;
  try {
    editor = new URL(value.editorOrigin);
    preview = new URL(value.previewOrigin);
  } catch {
    return null;
  }
  if (
    editor.protocol !== "http:" ||
    preview.protocol !== "http:" ||
    editor.hostname !== "127.0.0.1" ||
    preview.hostname !== "127.0.0.1" ||
    editor.port.length === 0 ||
    preview.port.length === 0 ||
    editor.origin === preview.origin
  ) {
    return null;
  }
  return { editorOrigin: editor.origin, previewOrigin: preview.origin };
};

const waitForReady = async (server) =>
  new Promise((resolveReady, rejectReady) => {
    let buffered = "";
    const timeout = setTimeout(() => {
      cleanup();
      rejectReady(new SmokeFailure("server_ready_timeout"));
    }, 30_000);
    const onExit = () => {
      cleanup();
      rejectReady(new SmokeFailure("server_exited_before_ready"));
    };
    const cleanup = () => {
      clearTimeout(timeout);
      server.stdout.removeListener("data", onReadyData);
      server.removeListener("exit", onExit);
    };
    const onReadyData = (chunk) => {
      buffered += Buffer.from(chunk).toString("utf8");
      if (buffered.length > 64 * 1024) {
        cleanup();
        rejectReady(new SmokeFailure("server_ready_output_limit_exceeded"));
        return;
      }
      const lines = buffered.split(/\r?\n/u);
      buffered = lines.pop() ?? "";
      for (const line of lines) {
        const origins = parseReadyLine(line);
        if (origins === null) continue;
        cleanup();
        resolveReady(origins);
        return;
      }
    };
    server.stdout.on("data", onReadyData);
    server.once("exit", onExit);
  });

const readHealth = async (origin, expectedRole) => {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    throwIfInterrupted();
    try {
      const response = await fetch(`${origin}/health`, {
        cache: "no-store",
        signal: AbortSignal.timeout(1_000),
      });
      if (response.ok) {
        const value = await response.json();
        if (
          value?.schemaVersion === "1" &&
          value?.status === "ok" &&
          value?.role === expectedRole
        ) {
          return { role: expectedRole, status: "ok", schemaVersion: "1" };
        }
      }
    } catch {
      // The listener may not yet have entered its accept loop.
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 50));
  }
  throw new SmokeFailure(`${expectedRole}_health_timeout`);
};

const readEditorRoot = async (origin) => {
  let response;
  try {
    response = await fetch(`${origin}/`, {
      cache: "no-store",
      signal: AbortSignal.timeout(2_000),
    });
  } catch {
    throw new SmokeFailure("editor_root_fetch_failed");
  }
  const contentType = response.headers.get("content-type") ?? "";
  const bytes = Buffer.from(await response.arrayBuffer());
  if (
    !response.ok ||
    !contentType.toLowerCase().startsWith("text/html") ||
    bytes.byteLength === 0 ||
    !bytes.toString("utf8").includes('id="root"')
  ) {
    throw new SmokeFailure("editor_root_invalid");
  }
  return {
    status: response.status,
    contentType: "text/html",
    byteLength: bytes.byteLength,
    sha256: sha256(bytes),
  };
};

const waitWithTimeout = async (promise, milliseconds) => {
  let timeout;
  try {
    return await Promise.race([
      promise,
      new Promise((resolveTimeout) => {
        timeout = setTimeout(() => resolveTimeout(null), milliseconds);
      }),
    ]);
  } finally {
    clearTimeout(timeout);
  }
};

const stopServer = async (server) => {
  if (server.exitCode !== null || server.signalCode !== null) {
    return { graceful: server.exitCode === 0, exitCode: server.exitCode };
  }
  const exited = new Promise((resolveExit) => {
    server.once("exit", (code, signal) => resolveExit({ code, signal }));
  });
  signalChild(server, "SIGINT");
  const result = await waitWithTimeout(exited, 5_000);
  if (result !== null) {
    return { graceful: result.code === 0, exitCode: result.code };
  }
  signalChild(server, "SIGKILL");
  const killed = await waitWithTimeout(exited, 5_000);
  if (killed === null) throw new SmokeFailure("server_sigkill_timeout");
  return { graceful: false, exitCode: killed.code };
};

const startPackagedServer = async (
  launcher,
  packageRoot,
  stateRoot,
  webDirectory,
  importRoot,
  capture,
) => {
  throwIfInterrupted();
  const server = registerChild(
    spawn(launcher, [], {
      cwd: packageRoot,
      env: {
        ...runtimeEnvironment,
        LP_STUDIO_EDITOR_PORT: "0",
        LP_STUDIO_PREVIEW_PORT: "0",
        LP_STUDIO_STATE_ROOT: stateRoot,
        LP_STUDIO_IMPORT_ROOT: importRoot,
        LP_STUDIO_WEB_DIST: webDirectory,
        RUST_LOG: "trace",
      },
      stdio: ["ignore", "pipe", "pipe"],
      detached: true,
      shell: false,
    }),
  );
  runningServer = server;
  server.stdout.on("data", capture.onStdout);
  server.stderr.on("data", capture.onStderr);
  const closed = new Promise((resolveClosed) => {
    server.once("close", () => resolveClosed(true));
  });
  const origins = await waitForReady(server);
  server.stdout.resume();
  return { server, closed, origins };
};

const stopPackagedServer = async ({ server, closed }) => {
  const stop = await stopServer(server);
  const stdioClosed = await waitWithTimeout(closed, 5_000);
  if (stdioClosed === null)
    throw new SmokeFailure("server_stdio_close_timeout");
  runningServer = undefined;
  if (!stop.graceful) throw new SmokeFailure("server_did_not_stop_gracefully");
  return stop;
};

const toolVersion = async (failureCode, command, args, options = {}) =>
  (
    await runStep(failureCode, command, args, {
      ...options,
      capture: true,
    })
  ).stdout
    .trim()
    .split("\n")[0];

const percentile95 = (values) => {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * 0.95) - 1)];
};

const geometryCaseId = ({
  viewport,
  deviceScaleFactor,
  contentZoomFactor,
  previewScale,
  scrollCase,
}) =>
  `${viewport}:dpr=${deviceScaleFactor}:zoom=${contentZoomFactor}:preview=${previewScale}:scroll=${scrollCase}`;

const validatePerformanceGeometrySemantics = (
  result,
  profile,
  profileBytes,
) => {
  if (result.status !== "passed") return true;
  const cases = result.geometry?.cases;
  if (!Array.isArray(cases) || cases.length === 0) return false;
  const expectedCases = [];
  for (const deviceScaleFactor of profile.geometryMatrix.deviceScaleFactors) {
    for (const viewport of profile.geometryMatrix.viewports) {
      for (const contentZoomFactor of profile.geometryMatrix
        .contentZoomFactors) {
        for (const previewScale of profile.geometryMatrix.previewScales) {
          for (const scrollCase of profile.geometryMatrix.scrollCases) {
            expectedCases.push({
              caseId: geometryCaseId({
                viewport: viewport.name,
                deviceScaleFactor,
                contentZoomFactor,
                previewScale,
                scrollCase: scrollCase.name,
              }),
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
            });
          }
        }
      }
    }
  }
  const identityFields = (entry) => ({
    caseId: entry.caseId,
    viewport: entry.viewport,
    viewportWidthCssPx: entry.viewportWidthCssPx,
    viewportHeightCssPx: entry.viewportHeightCssPx,
    deviceScaleFactor: entry.deviceScaleFactor,
    contentZoomFactor: entry.contentZoomFactor,
    previewScale: entry.previewScale,
    scrollCase: entry.scrollCase,
    windowScrollTopCssPx: entry.windowScrollTopCssPx,
    outerScrollTopCssPx: entry.outerScrollTopCssPx,
    innerScrollLeftCssPx: entry.innerScrollLeftCssPx,
  });
  if (
    JSON.stringify(cases.map(identityFields)) !==
    JSON.stringify(expectedCases.map(identityFields))
  ) {
    return false;
  }
  const maximumErrorCssPx = Math.max(
    ...cases.map((entry) => entry.overlayErrorCssPx),
  );
  const overlayFeedbackP95Ms = percentile95(
    cases.map((entry) => entry.overlayFeedbackDurationMs),
  );
  return (
    result.profileSha256 === sha256(profileBytes) &&
    result.fixture.fileCount === profile.staticSite.fileCount &&
    result.fixture.totalByteLength === profile.staticSite.totalByteLength &&
    result.fixture.domNodeCount === profile.dom.nodeCount &&
    result.fixture.boundedScanPassed === true &&
    result.geometry.caseCount === expectedCases.length &&
    result.geometry.expectedCaseCount === expectedCases.length &&
    result.geometry.maximumErrorCssPx === maximumErrorCssPx &&
    result.geometry.thresholdCssPx ===
      profile.thresholds.overlayMaximumErrorCssPx &&
    result.geometry.checksPassed === true &&
    maximumErrorCssPx <= profile.thresholds.overlayMaximumErrorCssPx &&
    result.performance.warmupCaseCount ===
      profile.measurement.warmupCaseCount &&
    result.performance.sampleCount === profile.measurement.sampleCount &&
    result.performance.rawCaseRecordsIncluded === true &&
    result.performance.overlayFeedbackSampleCount === cases.length &&
    result.performance.overlayFeedbackP95Ms === overlayFeedbackP95Ms &&
    result.performance.referenceThresholdMs ===
      profile.thresholds.overlayFeedbackP95Ms &&
    result.performance.thresholdPassed ===
      overlayFeedbackP95Ms <= profile.thresholds.overlayFeedbackP95Ms
  );
};

const validateProductionIntegrationPerformanceSemantics = (
  result,
  profile,
  profileBytes,
) => {
  if (result.status !== "passed") return true;
  const cases = result.geometry?.cases;
  const autosaveSamples = result.autosave?.rawSamplesMs;
  const changeSetSamples = result.changeSet?.rawSamplesMs;
  if (
    !Array.isArray(cases) ||
    !Array.isArray(autosaveSamples) ||
    !Array.isArray(changeSetSamples)
  ) {
    return false;
  }
  const expectedCaseIds = [];
  const viewportByName = new Map(
    profile.geometryMatrix.viewports.map((entry) => [entry.name, entry]),
  );
  const scrollByName = new Map(
    profile.geometryMatrix.scrollCases.map((entry) => [entry.name, entry]),
  );
  for (const deviceScaleFactor of profile.geometryMatrix.deviceScaleFactors) {
    for (const viewport of profile.geometryMatrix.viewports) {
      for (const contentZoomFactor of profile.geometryMatrix
        .contentZoomFactors) {
        for (const previewScale of profile.geometryMatrix.previewScales) {
          for (const scrollCase of profile.geometryMatrix.scrollCases) {
            expectedCaseIds.push(
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
  const maximumErrorCssPx = Math.max(
    ...cases.map((entry) => entry.overlayErrorCssPx),
  );
  const distinctRenderedViewportWidthCount = new Set(
    cases.map((entry) => entry.renderedIframeWidthCssPx),
  ).size;
  const overlayFeedbackP95Ms = percentile95(
    cases.map((entry) => entry.overlayFeedbackDurationMs),
  );
  const autosaveP95Ms = percentile95(autosaveSamples);
  const changeSetP95Ms = percentile95(changeSetSamples);
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
  const serialized = JSON.stringify(result);
  return (
    result.profileSha256 === sha256(profileBytes) &&
    JSON.stringify(cases.map((entry) => entry.caseId)) ===
      JSON.stringify(expectedCaseIds) &&
    new Set(expectedCaseIds).size === expectedCaseIds.length &&
    cases.every((entry) => {
      const viewport = viewportByName.get(entry.viewport);
      const scrollCase = scrollByName.get(entry.scrollCase);
      return (
        viewport?.width === entry.configuredViewportWidthCssPx &&
        scrollCase?.windowTop === entry.windowScrollTopCssPx &&
        scrollCase?.outerTop === entry.outerScrollTopCssPx &&
        scrollCase?.innerLeft === entry.innerScrollLeftCssPx &&
        entry.caseId ===
          geometryCaseId({
            viewport: entry.viewport,
            deviceScaleFactor: entry.deviceScaleFactor,
            contentZoomFactor: entry.contentZoomFactor,
            previewScale: entry.previewScale,
            scrollCase: entry.scrollCase,
          }) &&
        entry.scopedPreviewOriginVerified === true &&
        entry.appBridgeTargetVerified === true &&
        entry.targetApiRoundTripVerified === true &&
        entry.actualOverlayRendered === true &&
        entry.runtimeDevicePixelRatio === entry.deviceScaleFactor &&
        entry.runtimePreviewScale === entry.previewScale &&
        entry.runtimeViewportWidthCssPx === entry.renderedIframeWidthCssPx &&
        entry.renderedOuterFrameWidthCssPx ===
          entry.configuredViewportWidthCssPx &&
        entry.renderedOuterFrameWidthCssPx - entry.renderedIframeWidthCssPx >=
          0 &&
        entry.renderedOuterFrameWidthCssPx - entry.renderedIframeWidthCssPx <= 2
      );
    }) &&
    result.productionIntegration.measurement === "measured" &&
    result.productionIntegration.status === "pass" &&
    result.productionIntegration.packagedApplication === true &&
    result.productionIntegration.scopedPreviewIframe === true &&
    result.productionIntegration.previewTargetRuntime === true &&
    result.productionIntegration.boundBridgeRoundTrip === true &&
    result.productionIntegration.targetApiRoundTrip === true &&
    result.productionIntegration.actualRuntimeOverlay === true &&
    result.productionIntegration.caseCount === expectedCaseIds.length &&
    result.geometry.caseCount === expectedCaseIds.length &&
    result.geometry.expectedCaseCount === expectedCaseIds.length &&
    result.geometry.distinctRenderedViewportWidthCount ===
      distinctRenderedViewportWidthCount &&
    distinctRenderedViewportWidthCount === 3 &&
    result.geometry.rawCaseRecordsIncluded === true &&
    result.geometry.maximumErrorCssPx === maximumErrorCssPx &&
    maximumErrorCssPx <= profile.geometryThresholds.overlayMaximumErrorCssPx &&
    result.geometry.thresholdCssPx ===
      profile.geometryThresholds.overlayMaximumErrorCssPx &&
    result.geometry.overlayFeedbackP95Ms === overlayFeedbackP95Ms &&
    result.geometry.overlayFeedbackReferenceThresholdMs ===
      profile.geometryThresholds.overlayFeedbackP95Ms &&
    result.geometry.overlayFeedbackThresholdPassed ===
      overlayFeedbackP95Ms <= profile.geometryThresholds.overlayFeedbackP95Ms &&
    result.geometry.overlayFeedbackGatePolicy === profile.thresholdPolicy &&
    result.geometry.checksPassed === true &&
    result.autosave.warmupSampleCount === profile.autosave.warmupSampleCount &&
    result.autosave.sampleCount === profile.autosave.sampleCount &&
    autosaveSamples.length === profile.autosave.sampleCount &&
    result.autosave.p95Ms === autosaveP95Ms &&
    result.autosave.referenceThresholdMs ===
      profile.autosave.finalInputToCompletionP95Ms &&
    result.autosave.thresholdPassed ===
      autosaveP95Ms <= profile.autosave.finalInputToCompletionP95Ms &&
    result.autosave.functionalChecksPassed === true &&
    result.changeSet.probeSchemaVersion ===
      profile.changeSet.probeSchemaVersion &&
    result.changeSet.changedTextFileCount ===
      profile.changeSet.changedTextFileCount &&
    result.changeSet.totalChangedTextBytes ===
      profile.changeSet.totalChangedTextBytes &&
    result.changeSet.operationCount ===
      profile.changeSet.changedTextFileCount &&
    result.changeSet.warmupSampleCount ===
      profile.changeSet.warmupSampleCount &&
    result.changeSet.sampleCount === profile.changeSet.sampleCount &&
    changeSetSamples.length === profile.changeSet.sampleCount &&
    result.changeSet.p95Ms === changeSetP95Ms &&
    result.changeSet.referenceThresholdMs ===
      profile.changeSet.validationAndApplicationP95Ms &&
    result.changeSet.thresholdPassed ===
      changeSetP95Ms <= profile.changeSet.validationAndApplicationP95Ms &&
    result.changeSet.functionalChecksPassed === true &&
    JSON.stringify(result.advisoryCodes ?? []) ===
      JSON.stringify(expectedAdvisoryCodes) &&
    !["/home/", "/tmp/", "Bearer ", "sessionToken", "projectId"].some(
      (canary) => serialized.includes(canary),
    )
  );
};

const parseLastJsonLine = (processResult, failureCode) => {
  const lines = `${processResult.stdout}\n${processResult.stderr}`
    .trim()
    .split("\n")
    .reverse();
  for (const line of lines) {
    try {
      return JSON.parse(line);
    } catch {
      // A launcher may emit a non-JSON diagnostic before the result.
    }
  }
  throw new SmokeFailure(failureCode);
};

const loadSchemaValidator = async (path, failureCode) => {
  try {
    const schema = JSON.parse(await readFile(path, "utf8"));
    return new Ajv2020({
      allErrors: true,
      strict: true,
      strictRequired: false,
    }).compile(schema);
  } catch {
    throw new SmokeFailure(failureCode);
  }
};

try {
  parseArguments();
  if (process.platform !== "linux" || process.arch !== "x64") {
    throw new SmokeFailure("unsupported_smoke_host");
  }
  const rustVerbose = await runStep(
    "rust_host_detection_failed",
    "rustc",
    ["-vV"],
    { capture: true },
  );
  const rustHostTriple = rustVerbose.stdout
    .split("\n")
    .find((line) => line.startsWith("host: "))
    ?.slice("host: ".length);
  if (rustHostTriple !== SUPPORTED_RUST_HOST) {
    throw new SmokeFailure("unsupported_rust_host");
  }

  if (outputDirectory !== undefined) {
    try {
      await lstat(outputDirectory);
      throw new SmokeFailure("output_directory_exists");
    } catch (error) {
      if (error instanceof SmokeFailure) throw error;
      if (error?.code !== "ENOENT") {
        throw new SmokeFailure("output_directory_check_failed");
      }
    }
    try {
      const parent = await lstat(dirname(outputDirectory));
      if (!parent.isDirectory()) {
        throw new SmokeFailure("output_parent_not_directory");
      }
    } catch (error) {
      if (error instanceof SmokeFailure) throw error;
      throw new SmokeFailure("output_parent_missing");
    }
  }

  const [sourceRevision, status, sourceFingerprint] = await Promise.all([
    toolVersion("source_revision_failed", "git", ["rev-parse", "HEAD"]),
    runStep(
      "source_tree_status_failed",
      "git",
      ["status", "--porcelain=v1", "--untracked-files=all"],
      { capture: true, captureLimit: 16 * 1024 * 1024 },
    ),
    readSourceFingerprint(),
  ]);
  if (!/^[0-9a-f]{40,64}$/.test(sourceRevision)) {
    throw new SmokeFailure("source_revision_invalid");
  }
  const sourceStatusBytes = Buffer.from(status.stdout, "utf8");
  const sourceClean = sourceStatusBytes.byteLength === 0;
  if (!sourceClean && !allowDirty) {
    throw new SmokeFailure("source_tree_dirty_requires_allow_dirty");
  }
  const sourceChangedEntryCount = sourceClean
    ? 0
    : status.stdout.trimEnd().split("\n").length;
  sourceEvidence = {
    revision: sourceRevision,
    clean: sourceClean,
    evaluationMode: sourceClean ? "clean_tree" : "explicit_dirty_tree",
    dirtyEvaluationExplicitlyAllowed: !sourceClean && allowDirty,
    changedEntryCount: sourceChangedEntryCount,
    statusSha256: sha256(sourceStatusBytes),
    sourceFileCount: sourceFingerprint.fileCount,
    contentFingerprintSha256: sourceFingerprint.sha256,
    buildInputProfile: "git_tracked_and_nonignored_untracked_snapshot",
    ignoredSourceIncluded: false,
    sourceSymlinksAllowed: false,
  };

  temporaryRoot = await mkdtemp(temporaryPrefix);
  const expectedParent = `${resolve(tmpdir())}${sep}`;
  if (
    !resolve(temporaryRoot).startsWith(`${expectedParent}${packagePrefix}`) ||
    basename(temporaryRoot).length <= packagePrefix.length
  ) {
    throw new SmokeFailure("unsafe_temporary_root");
  }

  const packageRoot = join(temporaryRoot, "package");
  const sourceSnapshotRoot = join(temporaryRoot, "source");
  const binaryDirectory = join(packageRoot, "bin");
  const licenseDirectory = join(packageRoot, "licenses");
  const webDirectory = join(packageRoot, "web");
  const cargoTarget = join(temporaryRoot, "cargo-target");
  const stateRoot = join(temporaryRoot, "state");
  const safeLogImportRoot = join(temporaryRoot, "safe-log-import");
  const safeLogCanaries = {
    prompt: `LP_SAFE_LOG_PROMPT_${randomUUID()}`,
    fileBody: `LP_SAFE_LOG_FILE_BODY_${randomUUID()}`,
    privateRationale: `LP_SAFE_LOG_PRIVATE_RATIONALE_${randomUUID()}`,
    absolutePath: join(
      safeLogImportRoot,
      `private-origin-${randomUUID()}.html`,
    ),
    providerRawResponse: `LP_SAFE_LOG_PROVIDER_RAW_${randomUUID()}`,
  };
  await materializeSourceSnapshot(sourceFingerprint, sourceSnapshotRoot);
  await createSafeLogImportFixture(
    sourceSnapshotRoot,
    safeLogImportRoot,
    safeLogCanaries,
  );
  await runStep(
    "snapshot_dependency_install_failed",
    "pnpm",
    [
      "--dir",
      sourceSnapshotRoot,
      "install",
      "--offline",
      "--frozen-lockfile",
      "--ignore-scripts",
      "--prod=false",
    ],
    {
      capture: true,
      env: dependencyInstallEnvironment,
      captureLimit: 1024 * 1024,
    },
  );
  await Promise.all([
    mkdir(binaryDirectory, { recursive: true }),
    mkdir(licenseDirectory, { recursive: true }),
    mkdir(webDirectory, { recursive: true }),
    mkdir(stateRoot, { recursive: true, mode: 0o700 }),
  ]);
  await chmod(stateRoot, 0o700);

  await runStep(
    "web_build_failed",
    "pnpm",
    [
      "--dir",
      join(sourceSnapshotRoot, "apps/web"),
      "exec",
      "vite",
      "build",
      "--outDir",
      webDirectory,
      "--emptyOutDir",
    ],
    { capture: true, env: buildEnvironment },
  );
  await runStep(
    "local_server_build_failed",
    "cargo",
    [
      "build",
      "--manifest-path",
      join(sourceSnapshotRoot, "Cargo.toml"),
      "-p",
      "synapsegit-lp-local-server",
      "--release",
      "--locked",
    ],
    {
      env: { ...buildEnvironment, CARGO_TARGET_DIR: cargoTarget },
      capture: true,
    },
  );

  const binaryName = "synapsegit-lp-local-server";
  const packagedBinary = join(binaryDirectory, binaryName);
  const performanceProbeName = "synapsegit-lp-application-performance-probe";
  const packagedPerformanceProbe = join(binaryDirectory, performanceProbeName);
  await Promise.all([
    copyFile(join(cargoTarget, "release", binaryName), packagedBinary),
    copyFile(
      join(cargoTarget, "release", performanceProbeName),
      packagedPerformanceProbe,
    ),
  ]);
  await Promise.all([
    chmod(packagedBinary, 0o700),
    chmod(packagedPerformanceProbe, 0o700),
  ]);

  const [cargoMetadataResult, pnpmLicensesResult] = await Promise.all([
    runStep(
      "cargo_dependency_inventory_failed",
      "cargo",
      [
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--manifest-path",
        join(sourceSnapshotRoot, "Cargo.toml"),
        "--filter-platform",
        SUPPORTED_RUST_HOST,
      ],
      {
        env: buildEnvironment,
        capture: true,
        captureLimit: 16 * 1024 * 1024,
      },
    ),
    runStep(
      "web_dependency_inventory_failed",
      "pnpm",
      ["--dir", sourceSnapshotRoot, "licenses", "list", "--json", "--prod"],
      { env: buildEnvironment, capture: true, captureLimit: 1024 * 1024 },
    ),
  ]);
  let cargoMetadata;
  let pnpmLicenses;
  let dependencyNotices;
  try {
    cargoMetadata = JSON.parse(cargoMetadataResult.stdout);
    pnpmLicenses = JSON.parse(pnpmLicensesResult.stdout);
    dependencyNotices = renderThirdPartyNotices(cargoMetadata, pnpmLicenses);
  } catch {
    throw new SmokeFailure("dependency_inventory_invalid");
  }
  const synapseGitLicense = await readPinnedSynapseGitLicense(cargoMetadata);
  const preExecutionSnapshotIntegrity = await verifySourceSnapshot(
    sourceFingerprint,
    sourceSnapshotRoot,
  );
  const packageLicenseNotice = await readFile(
    join(sourceSnapshotRoot, "LICENSE"),
    "utf8",
  );

  const launcher = join(binaryDirectory, "start-lp-studio");
  await Promise.all([
    writeFile(join(packageRoot, "README.md"), packageReadme, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    }),
    writeFile(join(packageRoot, "LICENSE"), packageLicenseNotice, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o600,
    }),
    writeFile(
      join(licenseDirectory, "SynapseGit-v0.4.0-LICENSE"),
      synapseGitLicense,
      { flag: "wx", mode: 0o600 },
    ),
    writeFile(
      join(packageRoot, "THIRD-PARTY-NOTICES.md"),
      dependencyNotices.text,
      { encoding: "utf8", flag: "wx", mode: 0o600 },
    ),
    writeFile(launcher, packageLauncher, {
      encoding: "utf8",
      flag: "wx",
      mode: 0o700,
    }),
  ]);
  await chmod(launcher, 0o700);

  const payloadEntries = await artifactEntries(packageRoot);
  const payloadManifestSha256 = aggregateManifest(payloadEntries);
  await writeFile(
    join(packageRoot, "PACKAGE-MANIFEST.json"),
    `${JSON.stringify(
      {
        schemaVersion: "synapsegit-lp-studio.local-package-manifest/1",
        scope: "local_internal_evaluation_only",
        licenseIdentifier: "LicenseRef-SynapseGit-LP-Studio-Evaluation",
        licenseStatus: "unresolved",
        distributionPermission: "not_recorded",
        brandPermission: "not_recorded",
        releaseCreated: false,
        launcher: "bin/start-lp-studio",
        legalNotice: "LICENSE",
        thirdPartyNotices: "THIRD-PARTY-NOTICES.md",
        sourceBinding:
          "git_tracked_and_nonignored_untracked_snapshot_without_source_symlinks",
        entries: payloadEntries,
        payloadManifestSha256,
      },
      null,
      2,
    )}\n`,
    { encoding: "utf8", flag: "wx", mode: 0o600 },
  );
  const completeEntries = await artifactEntries(packageRoot);
  const artifactManifestSha256 = aggregateManifest(completeEntries);
  let safeLogProfile;
  let safeLogProfileBytes;
  try {
    safeLogProfileBytes = await readFile(
      resolve(
        sourceSnapshotRoot,
        "docs/evidence/fixtures/m1-safe-log-profile.v1.json",
      ),
    );
    safeLogProfile = JSON.parse(safeLogProfileBytes.toString("utf8"));
    validateSafeLogProfile(safeLogProfile);
  } catch (error) {
    if (error instanceof SmokeFailure) throw error;
    throw new SmokeFailure("safe_log_profile_invalid");
  }
  const productionRuntimeCapture = createRuntimeCapture();

  const firstCycle = await startPackagedServer(
    launcher,
    packageRoot,
    stateRoot,
    webDirectory,
    safeLogImportRoot,
    productionRuntimeCapture,
  );
  const [firstEditorRoot, firstEditorHealth, firstPreviewHealth] =
    await Promise.all([
      readEditorRoot(firstCycle.origins.editorOrigin),
      readHealth(firstCycle.origins.editorOrigin, "editor"),
      readHealth(firstCycle.origins.previewOrigin, "preview"),
    ]);
  let browserSmokeProcess;
  let browserSmokeProcessFailed = false;
  try {
    browserSmokeProcess = await run(
      process.execPath,
      [
        resolve(sourceSnapshotRoot, "scripts/run-m1-package-browser-smoke.mjs"),
        "--editor-origin",
        firstCycle.origins.editorOrigin,
      ],
      { capture: true, env: runtimeEnvironment, captureLimit: 1024 * 1024 },
    );
  } catch (error) {
    throwIfInterrupted();
    if (!(error instanceof SmokeFailure) || error.details === undefined) {
      throw new SmokeFailure("package_browser_smoke_failed");
    }
    browserSmokeProcess = error.details;
    browserSmokeProcessFailed = true;
  }
  const browserSmoke = parseLastJsonLine(
    browserSmokeProcess,
    "package_browser_smoke_result_invalid",
  );
  const validateBrowserSmoke = await loadSchemaValidator(
    resolve(
      sourceSnapshotRoot,
      "docs/evidence/schemas/m1-package-browser-smoke-result.schema.v1.json",
    ),
    "package_browser_smoke_schema_invalid",
  );
  const browserSmokeSchemaValid = validateBrowserSmoke(browserSmoke);
  if (browserSmokeSchemaValid && browserSmoke.status === "failed") {
    failedBrowserSmoke = browserSmoke;
  }
  if (
    !browserSmokeSchemaValid ||
    browserSmokeProcessFailed ||
    browserSmoke.schemaVersion !==
      "synapsegit-lp-studio.package-browser-smoke-result/1" ||
    browserSmoke.status !== "passed"
  ) {
    throw new SmokeFailure("package_browser_smoke_result_failed");
  }
  let productionIntegrationProcess;
  let productionIntegrationProcessFailed = false;
  try {
    productionIntegrationProcess = await run(
      process.execPath,
      [
        resolve(
          sourceSnapshotRoot,
          "scripts/run-m1-production-integration-performance.mjs",
        ),
        "--editor-origin",
        firstCycle.origins.editorOrigin,
        "--change-set-probe",
        packagedPerformanceProbe,
      ],
      { capture: true, env: runtimeEnvironment, captureLimit: 1024 * 1024 },
    );
  } catch (error) {
    throwIfInterrupted();
    if (!(error instanceof SmokeFailure) || error.details === undefined) {
      throw new SmokeFailure("production_integration_performance_failed");
    }
    productionIntegrationProcess = error.details;
    productionIntegrationProcessFailed = true;
  }
  const productionIntegrationPerformance = parseLastJsonLine(
    productionIntegrationProcess,
    "production_integration_performance_result_invalid",
  );
  const validateProductionIntegrationPerformance = await loadSchemaValidator(
    resolve(
      sourceSnapshotRoot,
      "docs/evidence/schemas/m1-production-integration-performance-result.schema.v1.json",
    ),
    "production_integration_performance_schema_invalid",
  );
  let productionIntegrationProfile;
  let productionIntegrationProfileBytes;
  try {
    productionIntegrationProfileBytes = await readFile(
      resolve(
        sourceSnapshotRoot,
        "docs/evidence/fixtures/m1-production-integration-performance-profile.v1.json",
      ),
    );
    productionIntegrationProfile = JSON.parse(
      productionIntegrationProfileBytes.toString("utf8"),
    );
  } catch {
    throw new SmokeFailure(
      "production_integration_performance_profile_invalid",
    );
  }
  const productionIntegrationSchemaValid =
    validateProductionIntegrationPerformance(productionIntegrationPerformance);
  const productionIntegrationSemanticsValid =
    productionIntegrationSchemaValid &&
    validateProductionIntegrationPerformanceSemantics(
      productionIntegrationPerformance,
      productionIntegrationProfile,
      productionIntegrationProfileBytes,
    );
  if (
    productionIntegrationSchemaValid &&
    productionIntegrationPerformance.status === "failed"
  ) {
    failedProductionIntegrationPerformance = productionIntegrationPerformance;
  }
  if (
    !productionIntegrationSchemaValid ||
    !productionIntegrationSemanticsValid ||
    productionIntegrationProcessFailed ||
    productionIntegrationPerformance.schemaVersion !==
      "synapsegit-lp-studio.production-integration-performance-result/1" ||
    productionIntegrationPerformance.status !== "passed"
  ) {
    throw new SmokeFailure("production_integration_performance_result_failed");
  }
  const firstStop = await stopPackagedServer(firstCycle);

  let performanceGeometryProcess;
  let performanceGeometryProcessFailed = false;
  try {
    performanceGeometryProcess = await run(
      process.execPath,
      [
        resolve(
          sourceSnapshotRoot,
          "scripts/run-m1-performance-geometry-corpus.mjs",
        ),
      ],
      { capture: true, env: runtimeEnvironment, captureLimit: 1024 * 1024 },
    );
  } catch (error) {
    throwIfInterrupted();
    if (!(error instanceof SmokeFailure) || error.details === undefined) {
      throw new SmokeFailure("performance_geometry_corpus_failed");
    }
    performanceGeometryProcess = error.details;
    performanceGeometryProcessFailed = true;
  }
  const performanceGeometry = parseLastJsonLine(
    performanceGeometryProcess,
    "performance_geometry_result_invalid",
  );
  const validatePerformanceGeometry = await loadSchemaValidator(
    resolve(
      sourceSnapshotRoot,
      "docs/evidence/schemas/m1-performance-geometry-result.schema.v1.json",
    ),
    "performance_geometry_schema_invalid",
  );
  let performanceGeometryProfile;
  let performanceGeometryProfileBytes;
  try {
    performanceGeometryProfileBytes = await readFile(
      resolve(
        sourceSnapshotRoot,
        "docs/evidence/fixtures/m1-performance-geometry-profile.v1.json",
      ),
    );
    performanceGeometryProfile = JSON.parse(
      performanceGeometryProfileBytes.toString("utf8"),
    );
  } catch {
    throw new SmokeFailure("performance_geometry_profile_invalid");
  }
  const performanceGeometrySchemaValid =
    validatePerformanceGeometry(performanceGeometry);
  const performanceGeometrySemanticsValid =
    performanceGeometrySchemaValid &&
    validatePerformanceGeometrySemantics(
      performanceGeometry,
      performanceGeometryProfile,
      performanceGeometryProfileBytes,
    );
  if (
    performanceGeometrySchemaValid &&
    performanceGeometry.status === "failed"
  ) {
    failedPerformanceGeometry = performanceGeometry;
  }
  if (
    !performanceGeometrySchemaValid ||
    !performanceGeometrySemanticsValid ||
    performanceGeometryProcessFailed ||
    performanceGeometry.schemaVersion !==
      "synapsegit-lp-studio.performance-geometry-result/1" ||
    performanceGeometry.status !== "passed"
  ) {
    throw new SmokeFailure("performance_geometry_result_failed");
  }

  const safeLogRuntimeCapture = createRuntimeCapture();
  const secondCycle = await startPackagedServer(
    launcher,
    packageRoot,
    stateRoot,
    webDirectory,
    safeLogImportRoot,
    safeLogRuntimeCapture,
  );
  const [secondEditorRoot, secondEditorHealth, secondPreviewHealth] =
    await Promise.all([
      readEditorRoot(secondCycle.origins.editorOrigin),
      readHealth(secondCycle.origins.editorOrigin, "editor"),
      readHealth(secondCycle.origins.previewOrigin, "preview"),
    ]);
  let safeObservability;
  let secondStop;
  try {
    const safeLogFlow = await runSafeLogFakeFlow(
      secondCycle.origins.editorOrigin,
      safeLogCanaries,
    );
    await waitForSafeLogOperations(safeLogRuntimeCapture, safeLogFlow);
    safeObservability = validateSafeRuntimeLogs({
      capture: safeLogRuntimeCapture,
      flow: safeLogFlow,
      profile: safeLogProfile,
      profileBytes: safeLogProfileBytes,
      absoluteRuntimePaths: [
        temporaryRoot,
        packageRoot,
        sourceSnapshotRoot,
        binaryDirectory,
        webDirectory,
        cargoTarget,
        stateRoot,
        safeLogImportRoot,
      ],
    });
    const validateSafeObservability = await loadSchemaValidator(
      resolve(
        sourceSnapshotRoot,
        "docs/evidence/schemas/m1-safe-log-result.schema.v1.json",
      ),
      "safe_log_result_schema_invalid",
    );
    if (!validateSafeObservability(safeObservability)) {
      throw new SmokeFailure("safe_log_result_schema_invalid");
    }
  } finally {
    secondStop = await stopPackagedServer(secondCycle);
  }
  if (firstEditorRoot.sha256 !== secondEditorRoot.sha256) {
    throw new SmokeFailure("editor_root_changed_after_restart");
  }

  const byteLength = completeEntries.reduce(
    (total, entry) => total + entry.byteLength,
    0,
  );
  const [pnpmVersion, rustVersion, playwrightVersion] = await Promise.all([
    toolVersion("pnpm_version_failed", "pnpm", ["--version"], {
      env: runtimeEnvironment,
    }),
    toolVersion("rust_version_failed", "rustc", ["--version"], {
      env: runtimeEnvironment,
    }),
    toolVersion(
      "playwright_version_failed",
      "pnpm",
      ["--dir", sourceSnapshotRoot, "exec", "playwright", "--version"],
      {
        env: runtimeEnvironment,
        cwd: sourceSnapshotRoot,
      },
    ),
  ]);
  if (
    performanceGeometry.environment.node !== process.version ||
    performanceGeometry.environment.pnpm !== pnpmVersion ||
    performanceGeometry.environment.rust !== rustVersion ||
    performanceGeometry.environment.playwright !== playwrightVersion ||
    productionIntegrationPerformance.environment.node !== process.version ||
    productionIntegrationPerformance.environment.browser !==
      performanceGeometry.environment.browser
  ) {
    throw new SmokeFailure("performance_geometry_toolchain_mismatch");
  }
  const postExecutionSnapshotIntegrity = await verifySourceSnapshot(
    sourceFingerprint,
    sourceSnapshotRoot,
  );
  if (
    JSON.stringify(postExecutionSnapshotIntegrity) !==
    JSON.stringify(preExecutionSnapshotIntegrity)
  ) {
    throw new SmokeFailure("snapshot_source_changed_during_execution");
  }
  const [finalSourceRevision, finalStatus, finalSourceFingerprint] =
    await Promise.all([
      toolVersion("final_source_revision_failed", "git", ["rev-parse", "HEAD"]),
      runStep(
        "final_source_tree_status_failed",
        "git",
        ["status", "--porcelain=v1", "--untracked-files=all"],
        { capture: true, captureLimit: 16 * 1024 * 1024 },
      ),
      readSourceFingerprint(),
    ]);
  if (
    finalSourceRevision !== sourceRevision ||
    finalStatus.stdout !== status.stdout ||
    finalSourceFingerprint.fileCount !== sourceFingerprint.fileCount ||
    finalSourceFingerprint.sha256 !== sourceFingerprint.sha256
  ) {
    throw new SmokeFailure("source_tree_changed_during_evaluation");
  }
  const actualAutomatedEvidence = automatedEvidenceRecord({
    source: sourceEvidence,
    browserSmoke,
    performanceGeometry,
    productionIntegrationPerformance,
    safeObservability,
    artifactManifestSha256,
  });
  const validateAutomatedEvidence = await loadSchemaValidator(
    resolve(
      sourceSnapshotRoot,
      "docs/evidence/schemas/m1-automated-evidence-record.schema.v1.json",
    ),
    "automated_evidence_schema_invalid",
  );
  if (!validateAutomatedEvidence(actualAutomatedEvidence)) {
    throw new SmokeFailure("automated_evidence_schema_invalid");
  }
  throwIfInterrupted();
  evidence = {
    schemaVersion: SCHEMA_VERSION,
    status: "passed",
    scope: "local_internal_evaluation_only",
    source: {
      ...sourceEvidence,
      snapshotIntegrity: {
        dependencyTreePolicy: "node_modules_excluded_and_lock_bound",
        verifiedBeforeRuntimeEvidence: preExecutionSnapshotIntegrity,
        verifiedAfterRuntimeEvidence: postExecutionSnapshotIntegrity,
      },
    },
    artifact: {
      fileCount: completeEntries.length,
      byteLength,
      manifestSha256: artifactManifestSha256,
      payloadManifestSha256,
      packageManifestSha256: completeEntries.find(
        (entry) => entry.path === "PACKAGE-MANIFEST.json",
      ).sha256,
      retained: outputDirectory !== undefined,
    },
    runtime: {
      launcher: "bin/start-lp-studio",
      firstStart: {
        editorRoot: firstEditorRoot,
        editorHealth: firstEditorHealth,
        previewHealth: firstPreviewHealth,
        gracefulStop: firstStop.graceful,
      },
      restart: {
        sameStateRoot: true,
        editorRoot: secondEditorRoot,
        editorHealth: secondEditorHealth,
        previewHealth: secondPreviewHealth,
        gracefulStop: secondStop.graceful,
      },
      browserSmoke,
      safeObservability,
    },
    acceptance: {
      performanceGeometry,
      productionIntegrationPerformance,
    },
    dependencyInventory: {
      cargoPackageCount: dependencyNotices.cargoPackageCount,
      webPackageCount: dependencyNotices.webPackageCount,
      undeclaredCargoLicenseCount:
        dependencyNotices.undeclaredCargoLicenseCount,
      noticePath: "THIRD-PARTY-NOTICES.md",
    },
    toolchain: {
      node: process.version,
      pnpm: pnpmVersion,
      rust: rustVersion,
      rustHostTriple,
      playwright: playwrightVersion,
    },
    automatedEvidence: actualAutomatedEvidence,
    claimBoundary: {
      productionReady: false,
      releaseCreated: false,
      distributionPermission: "not_recorded",
      brandPermission: "not_recorded",
    },
  };

  if (outputDirectory !== undefined) {
    try {
      await mkdir(outputDirectory, { mode: 0o700 });
      const retainedPackage = join(outputDirectory, "package");
      try {
        await rename(packageRoot, retainedPackage);
      } catch (error) {
        if (error?.code !== "EXDEV") throw error;
        await cp(packageRoot, retainedPackage, {
          recursive: true,
          errorOnExist: true,
          force: false,
        });
        await rm(packageRoot, { recursive: true, force: false });
      }
      const retainedEntries = await artifactEntries(retainedPackage);
      if (
        aggregateManifest(retainedEntries) !== artifactManifestSha256 ||
        JSON.stringify(retainedEntries) !== JSON.stringify(completeEntries)
      ) {
        throw new SmokeFailure("retained_artifact_verification_failed");
      }
      const evidenceBytes = Buffer.from(
        `${JSON.stringify(evidence, null, 2)}\n`,
      );
      await writeFile(
        join(outputDirectory, "LOCAL-EVALUATION-EVIDENCE.json"),
        evidenceBytes,
        { flag: "wx", mode: 0o600 },
      );
      await writeFile(
        join(outputDirectory, "SHA256SUMS"),
        `${evidence.artifact.packageManifestSha256}  package/PACKAGE-MANIFEST.json\n${sha256(evidenceBytes)}  LOCAL-EVALUATION-EVIDENCE.json\n`,
        { encoding: "utf8", flag: "wx", mode: 0o600 },
      );
    } catch (error) {
      if (error instanceof SmokeFailure) throw error;
      throw new SmokeFailure("artifact_retention_failed");
    }
  }
} catch (error) {
  const code =
    interruptedSignal !== undefined
      ? interruptionCode()
      : error instanceof SmokeFailure
        ? error.code
        : "local_package_smoke_failed";
  const failedAcceptance = {
    ...(failedPerformanceGeometry === undefined
      ? {}
      : { performanceGeometry: failedPerformanceGeometry }),
    ...(failedProductionIntegrationPerformance === undefined
      ? {}
      : {
          productionIntegrationPerformance:
            failedProductionIntegrationPerformance,
        }),
  };
  evidence = {
    schemaVersion: SCHEMA_VERSION,
    status: "failed",
    errorCode: code,
    ...(sourceEvidence === undefined ? {} : { source: sourceEvidence }),
    ...(failedBrowserSmoke === undefined
      ? {}
      : { runtime: { browserSmoke: failedBrowserSmoke } }),
    ...(Object.keys(failedAcceptance).length === 0
      ? {}
      : { acceptance: failedAcceptance }),
  };
  process.exitCode =
    interruptedSignal === "SIGINT"
      ? 130
      : interruptedSignal === "SIGTERM"
        ? 143
        : 1;
} finally {
  let serverCleanupSafe = true;
  if (runningServer !== undefined) {
    try {
      await stopServer(runningServer);
    } catch {
      serverCleanupSafe = false;
      recordCleanupFailure("server_cleanup_failed");
    }
  }
  if (temporaryRoot !== undefined) {
    const resolvedTemporary = resolve(temporaryRoot);
    const safePrefix = `${resolve(tmpdir())}${sep}${packagePrefix}`;
    if (!resolvedTemporary.startsWith(safePrefix)) {
      recordCleanupFailure("temporary_cleanup_scope_rejected");
    } else if (serverCleanupSafe) {
      try {
        await rm(resolvedTemporary, { recursive: true, force: true });
      } catch {
        recordCleanupFailure("temporary_cleanup_failed");
      }
    } else {
      recordCleanupFailure("temporary_cleanup_skipped_for_live_server");
    }
  }
}

process.removeListener("SIGINT", onSigint);
process.removeListener("SIGTERM", onSigterm);
if (interruptedSignal !== undefined) {
  process.exitCode = interruptedSignal === "SIGINT" ? 130 : 143;
  evidence = {
    schemaVersion: SCHEMA_VERSION,
    status: "failed",
    errorCode: interruptionCode(),
  };
}

process.stdout.write(`${JSON.stringify(evidence)}\n`);
