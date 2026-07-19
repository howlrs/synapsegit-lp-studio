export const SCHEMA_VERSION = "1" as const;
export const API_VERSION = "v1" as const;

export type SchemaVersion = typeof SCHEMA_VERSION;
export type PreviewMode = "select" | "interact";
export type PreviewSource = "accepted" | "proposed";
export type ViewportPreset = "desktop" | "tablet" | "mobile" | "custom";
export type ArtifactDisposition = "adopted_unchanged" | "rejected" | "deferred";

export interface BootstrapResponse {
  schemaVersion: SchemaVersion;
  apiVersion: typeof API_VERSION;
  session: {
    token: string;
    expiresAt: string;
  };
  editorOrigin: string;
  previewOrigin: string;
  capabilities: {
    targetKinds: ["element"];
    dispositions: ArtifactDisposition[];
    singleProposalPerProject: true;
    importAvailable: boolean;
    limits: ImportLimits;
  };
}

export interface ImportLimits {
  maxFiles: number;
  maxTotalBytes: number;
  maxFileBytes: number;
  maxPathBytes: number;
  maxDepth: number;
}

export interface ProjectFile {
  path: string;
  byteLength: number;
}

export interface Project {
  id: string;
  displayName: string;
  revisionId: string;
  acceptedManifestSha256: string;
  status: "ready";
  previewUrl: string;
  files: ProjectFile[];
}

export interface ProjectResponse {
  schemaVersion: SchemaVersion;
  project: Project;
}

export interface ProjectsResponse {
  schemaVersion: SchemaVersion;
  projects: Project[];
}

export interface ImportPreviewFile {
  path: string;
  byteLength: number;
  sha256: string;
}

export interface ImportPreviewExclusion {
  path: string;
  reason: string;
}

export interface ImportPreview {
  id: string;
  displayName: string;
  manifestSha256: string;
  totalBytes: number;
  entryPoint: string | null;
  included: ImportPreviewFile[];
  excluded: ImportPreviewExclusion[];
  warnings: string[];
}

export interface ImportPreviewResponse {
  schemaVersion: SchemaVersion;
  importPreview: ImportPreview;
}

export interface CreateImportPreviewRequest {
  schemaVersion: SchemaVersion;
}

export interface ConfirmImportRequest {
  schemaVersion: SchemaVersion;
  expectedManifestSha256: string;
}

export interface CreateProjectRequest {
  schemaVersion: SchemaVersion;
  template: "blank";
}

export interface CreateTargetRequest {
  schemaVersion: SchemaVersion;
  revisionId: string;
  kind: "element";
  elementId: string;
}

export interface Target {
  id: string;
  revisionId: string;
  kind: "element";
  elementId: string;
  label: string;
}

export interface TargetResponse {
  schemaVersion: SchemaVersion;
  target: Target;
}

export interface CreateContextRequest {
  schemaVersion: SchemaVersion;
  revisionId: string;
  targetId: string;
  instruction: string;
}

export interface ContextReview {
  id: string;
  revisionId: string;
  targetId: string;
  instruction: string;
  canonicalJson: string;
  sha256: string;
}

export interface ContextResponse {
  schemaVersion: SchemaVersion;
  context: ContextReview;
}

export interface CreateProposalRequest {
  schemaVersion: SchemaVersion;
  contextId: string;
  contextSha256: string;
}

export interface ProposalChange {
  path: string;
  kind: "created" | "modified" | "renamed" | "deleted";
}

export type ValidationStatus = "passed" | "warning" | "failed";

export interface ValidationCheck {
  id: string;
  label: string;
  status: ValidationStatus;
  message: string;
}

export interface ProposalValidation {
  status: ValidationStatus;
  checks: ValidationCheck[];
}

export interface Proposal {
  id: string;
  reviewId: string;
  baseRevisionId: string;
  status: "pending_review";
  summary: string;
  artifactManifestSha256: string;
  reviewContextSha256: string;
  sourceAttribution: "caller_supplied_ai_attributed";
  executionVerified: false;
  previewUrl: string;
  changes: ProposalChange[];
  unifiedDiff: string;
  validation: ProposalValidation;
}

export interface ProposalResponse {
  schemaVersion: SchemaVersion;
  proposal: Proposal;
}

export interface ApprovalRequest {
  schemaVersion: SchemaVersion;
  proposalId: string;
  expectedRevisionId: string;
  disposition: ArtifactDisposition;
  intentId: string;
}

export interface ApprovalResponse {
  schemaVersion: SchemaVersion;
  approval: {
    token: string;
    expiresAt: string;
    intentId: string;
  };
}

export interface DecisionRequest extends ApprovalRequest {
  approvalToken: string;
  rationale?: string;
}

export interface DecisionResponse {
  schemaVersion: SchemaVersion;
  decision: {
    reviewId: string;
    proposalId: string;
    disposition: ArtifactDisposition;
    status: "committed";
    revisionId: string;
    artifactManifestSha256: string;
  };
  project: Project;
}

export interface CreateExportRequest {
  schemaVersion: SchemaVersion;
  revisionId: string;
}

export interface ExportReceipt {
  id: string;
  revisionId: string;
  sha256: string;
  byteLength: number;
  downloadUrl: string;
}

export interface ExportResponse {
  schemaVersion: SchemaVersion;
  export: ExportReceipt;
}

export interface ApiErrorResponse {
  schemaVersion: SchemaVersion;
  error: {
    code: string;
    message: string;
    requestId: string;
    retryable: boolean;
  };
}

export interface PreviewRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** This payload is untrusted even after the envelope has been validated. */
export interface PreviewSelectionMessage {
  type: "synapsegit-lp.selection";
  schemaVersion: SchemaVersion;
  channelId: string;
  projectId: string;
  snapshotId: string;
  revisionId: string;
  elementId: string;
  rect: PreviewRect;
}

interface PreviewActionEnvelope {
  type: "synapsegit-lp.action";
  schemaVersion: SchemaVersion;
  channelId: string;
  projectId: string;
  snapshotId: string;
  revisionId: string;
}

export type PreviewActionMessage = PreviewActionEnvelope &
  (
    | { action: "set_mode"; mode: PreviewMode }
    | { action: "clear_selection"; mode?: never }
  );

type UnknownRecord = Record<string, unknown>;

const isRecord = (value: unknown): value is UnknownRecord =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const isString = (value: unknown): value is string => typeof value === "string";
const isNonEmptyString = (value: unknown): value is string =>
  isString(value) && value.length > 0;
const isSha256 = (value: unknown): value is string =>
  isString(value) && /^[0-9a-f]{64}$/.test(value);
const isFiniteNumber = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value);
const isNonNegativeInteger = (value: unknown): value is number =>
  isFiniteNumber(value) && Number.isInteger(value) && value >= 0;
const isSafeRelativePath = (value: unknown): value is string => {
  if (
    !isNonEmptyString(value) ||
    value.includes("\0") ||
    value.includes("\\")
  ) {
    return false;
  }
  if (
    value.startsWith("/") ||
    value.startsWith("\\") ||
    /^[A-Za-z]:[\\/]/.test(value)
  ) {
    return false;
  }
  const segments = value.split("/");
  return segments.every(
    (segment) => segment.length > 0 && segment !== "." && segment !== "..",
  );
};
const containsAbsolutePath = (value: string): boolean =>
  /(?:^|[\s("'=])\/(?!\/)/.test(value) ||
  /[A-Za-z]:[\\/]/.test(value) ||
  /(?:^|[\s("'=])\\\\/.test(value);
const isPathPrivateString = (value: unknown): value is string =>
  isString(value) && !value.includes("\0") && !containsAbsolutePath(value);
const isPathPrivateNonEmptyString = (value: unknown): value is string =>
  isNonEmptyString(value) &&
  !value.includes("\0") &&
  !containsAbsolutePath(value);
const isExactArrayOf = <T>(
  value: unknown,
  guard: (item: unknown) => item is T,
): value is T[] => {
  if (
    !Array.isArray(value) ||
    Reflect.ownKeys(value).length !== value.length + 1
  ) {
    return false;
  }
  for (let index = 0; index < value.length; index += 1) {
    if (!Object.hasOwn(value, index) || !guard(value[index])) return false;
  }
  return true;
};
const isStringArray = (value: unknown): value is string[] =>
  isExactArrayOf(value, isString);
const hasVersion = (value: UnknownRecord): boolean =>
  value.schemaVersion === SCHEMA_VERSION;
const hasExactKeys = (
  value: UnknownRecord,
  required: readonly string[],
  optional: readonly string[] = [],
): boolean => {
  const allowed = new Set([...required, ...optional]);
  return (
    required.every((key) => Object.hasOwn(value, key)) &&
    Reflect.ownKeys(value).every(
      (key) => typeof key === "string" && allowed.has(key),
    )
  );
};

export const isProject = (value: unknown): value is Project => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "id",
      "displayName",
      "revisionId",
      "acceptedManifestSha256",
      "status",
      "previewUrl",
      "files",
    ]) ||
    !isExactArrayOf(
      value.files,
      (file): file is ProjectFile =>
        isRecord(file) &&
        hasExactKeys(file, ["path", "byteLength"]) &&
        isSafeRelativePath(file.path) &&
        isNonNegativeInteger(file.byteLength),
    )
  ) {
    return false;
  }
  return (
    isNonEmptyString(value.id) &&
    isPathPrivateNonEmptyString(value.displayName) &&
    isNonEmptyString(value.revisionId) &&
    isSha256(value.acceptedManifestSha256) &&
    value.status === "ready" &&
    isNonEmptyString(value.previewUrl)
  );
};

export const isBootstrapResponse = (
  value: unknown,
): value is BootstrapResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "schemaVersion",
      "apiVersion",
      "session",
      "editorOrigin",
      "previewOrigin",
      "capabilities",
    ]) ||
    !hasVersion(value)
  ) {
    return false;
  }
  const session = value.session;
  const capabilities = value.capabilities;
  if (
    !isRecord(session) ||
    !hasExactKeys(session, ["token", "expiresAt"]) ||
    !isRecord(capabilities) ||
    !hasExactKeys(capabilities, [
      "targetKinds",
      "dispositions",
      "singleProposalPerProject",
      "importAvailable",
      "limits",
    ])
  ) {
    return false;
  }
  return (
    value.apiVersion === API_VERSION &&
    isNonEmptyString(session.token) &&
    isNonEmptyString(session.expiresAt) &&
    isNonEmptyString(value.editorOrigin) &&
    isNonEmptyString(value.previewOrigin) &&
    isExactArrayOf(capabilities.targetKinds, isString) &&
    capabilities.targetKinds.length === 1 &&
    capabilities.targetKinds[0] === "element" &&
    isStringArray(capabilities.dispositions) &&
    capabilities.dispositions.length > 0 &&
    new Set(capabilities.dispositions).size ===
      capabilities.dispositions.length &&
    capabilities.dispositions.every((item) =>
      ["adopted_unchanged", "rejected", "deferred"].includes(item),
    ) &&
    capabilities.singleProposalPerProject === true &&
    typeof capabilities.importAvailable === "boolean" &&
    isImportLimits(capabilities.limits)
  );
};

const isImportLimits = (value: unknown): value is ImportLimits =>
  isRecord(value) &&
  hasExactKeys(value, [
    "maxFiles",
    "maxTotalBytes",
    "maxFileBytes",
    "maxPathBytes",
    "maxDepth",
  ]) &&
  isNonNegativeInteger(value.maxFiles) &&
  value.maxFiles > 0 &&
  isNonNegativeInteger(value.maxTotalBytes) &&
  value.maxTotalBytes > 0 &&
  isNonNegativeInteger(value.maxFileBytes) &&
  value.maxFileBytes > 0 &&
  isNonNegativeInteger(value.maxPathBytes) &&
  value.maxPathBytes > 0 &&
  isNonNegativeInteger(value.maxDepth) &&
  value.maxDepth > 0;

export const isProjectResponse = (value: unknown): value is ProjectResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "project"]) &&
  hasVersion(value) &&
  isProject(value.project);

export const isProjectsResponse = (value: unknown): value is ProjectsResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "projects"]) &&
  hasVersion(value) &&
  isExactArrayOf(value.projects, isProject);

const isImportPreviewFile = (value: unknown): value is ImportPreviewFile =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "byteLength", "sha256"]) &&
  isSafeRelativePath(value.path) &&
  isNonNegativeInteger(value.byteLength) &&
  isSha256(value.sha256);

const isImportPreviewExclusion = (
  value: unknown,
): value is ImportPreviewExclusion =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "reason"]) &&
  isSafeRelativePath(value.path) &&
  isPathPrivateNonEmptyString(value.reason);

export const isImportPreviewResponse = (
  value: unknown,
): value is ImportPreviewResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "importPreview"]) ||
    !hasVersion(value) ||
    !isRecord(value.importPreview) ||
    !hasExactKeys(value.importPreview, [
      "id",
      "displayName",
      "manifestSha256",
      "totalBytes",
      "entryPoint",
      "included",
      "excluded",
      "warnings",
    ])
  ) {
    return false;
  }
  const preview = value.importPreview;
  return (
    isNonEmptyString(preview.id) &&
    isPathPrivateNonEmptyString(preview.displayName) &&
    isSha256(preview.manifestSha256) &&
    isNonNegativeInteger(preview.totalBytes) &&
    (preview.entryPoint === null || isSafeRelativePath(preview.entryPoint)) &&
    isExactArrayOf(preview.included, isImportPreviewFile) &&
    isExactArrayOf(preview.excluded, isImportPreviewExclusion) &&
    isExactArrayOf(preview.warnings, isPathPrivateString)
  );
};

export const isTargetResponse = (value: unknown): value is TargetResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "target"]) ||
    !hasVersion(value) ||
    !isRecord(value.target) ||
    !hasExactKeys(value.target, [
      "id",
      "revisionId",
      "kind",
      "elementId",
      "label",
    ])
  ) {
    return false;
  }
  const target = value.target;
  return (
    isNonEmptyString(target.id) &&
    isNonEmptyString(target.revisionId) &&
    target.kind === "element" &&
    isNonEmptyString(target.elementId) &&
    isNonEmptyString(target.label)
  );
};

export const isContextResponse = (value: unknown): value is ContextResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "context"]) ||
    !hasVersion(value) ||
    !isRecord(value.context) ||
    !hasExactKeys(value.context, [
      "id",
      "revisionId",
      "targetId",
      "instruction",
      "canonicalJson",
      "sha256",
    ])
  ) {
    return false;
  }
  const context = value.context;
  return (
    isNonEmptyString(context.id) &&
    isNonEmptyString(context.revisionId) &&
    isNonEmptyString(context.targetId) &&
    isString(context.instruction) &&
    isString(context.canonicalJson) &&
    isSha256(context.sha256)
  );
};

const isProposalChange = (value: unknown): value is ProposalChange =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "kind"]) &&
  isNonEmptyString(value.path) &&
  (value.kind === "created" ||
    value.kind === "modified" ||
    value.kind === "renamed" ||
    value.kind === "deleted");

const isValidationCheck = (value: unknown): value is ValidationCheck =>
  isRecord(value) &&
  hasExactKeys(value, ["id", "label", "status", "message"]) &&
  isNonEmptyString(value.id) &&
  isNonEmptyString(value.label) &&
  (value.status === "passed" ||
    value.status === "warning" ||
    value.status === "failed") &&
  isString(value.message);

const isProposalValidation = (value: unknown): value is ProposalValidation =>
  isRecord(value) &&
  hasExactKeys(value, ["status", "checks"]) &&
  (value.status === "passed" ||
    value.status === "warning" ||
    value.status === "failed") &&
  isExactArrayOf(value.checks, isValidationCheck);

export const isProposalResponse = (
  value: unknown,
): value is ProposalResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "proposal"]) ||
    !hasVersion(value) ||
    !isRecord(value.proposal) ||
    !hasExactKeys(value.proposal, [
      "id",
      "reviewId",
      "baseRevisionId",
      "status",
      "summary",
      "artifactManifestSha256",
      "reviewContextSha256",
      "sourceAttribution",
      "executionVerified",
      "previewUrl",
      "changes",
      "unifiedDiff",
      "validation",
    ])
  ) {
    return false;
  }
  const proposal = value.proposal;
  return (
    isNonEmptyString(proposal.id) &&
    isNonEmptyString(proposal.reviewId) &&
    isNonEmptyString(proposal.baseRevisionId) &&
    proposal.status === "pending_review" &&
    isString(proposal.summary) &&
    isSha256(proposal.artifactManifestSha256) &&
    isSha256(proposal.reviewContextSha256) &&
    proposal.sourceAttribution === "caller_supplied_ai_attributed" &&
    proposal.executionVerified === false &&
    isNonEmptyString(proposal.previewUrl) &&
    isExactArrayOf(proposal.changes, isProposalChange) &&
    isString(proposal.unifiedDiff) &&
    isProposalValidation(proposal.validation)
  );
};

export const isApprovalResponse = (
  value: unknown,
): value is ApprovalResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "approval"]) ||
    !hasVersion(value) ||
    !isRecord(value.approval) ||
    !hasExactKeys(value.approval, ["token", "expiresAt", "intentId"])
  ) {
    return false;
  }
  return (
    isNonEmptyString(value.approval.token) &&
    isNonEmptyString(value.approval.expiresAt) &&
    isNonEmptyString(value.approval.intentId)
  );
};

export const isDecisionResponse = (
  value: unknown,
): value is DecisionResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "decision", "project"]) ||
    !hasVersion(value) ||
    !isRecord(value.decision) ||
    !hasExactKeys(value.decision, [
      "reviewId",
      "proposalId",
      "disposition",
      "status",
      "revisionId",
      "artifactManifestSha256",
    ]) ||
    !isProject(value.project)
  ) {
    return false;
  }
  const decision = value.decision;
  return (
    isNonEmptyString(decision.reviewId) &&
    isNonEmptyString(decision.proposalId) &&
    (decision.disposition === "adopted_unchanged" ||
      decision.disposition === "rejected" ||
      decision.disposition === "deferred") &&
    decision.status === "committed" &&
    isNonEmptyString(decision.revisionId) &&
    isSha256(decision.artifactManifestSha256)
  );
};

export const isExportResponse = (value: unknown): value is ExportResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "export"]) ||
    !hasVersion(value) ||
    !isRecord(value.export) ||
    !hasExactKeys(value.export, [
      "id",
      "revisionId",
      "sha256",
      "byteLength",
      "downloadUrl",
    ])
  ) {
    return false;
  }
  const receipt = value.export;
  return (
    isNonEmptyString(receipt.id) &&
    isNonEmptyString(receipt.revisionId) &&
    isSha256(receipt.sha256) &&
    isNonNegativeInteger(receipt.byteLength) &&
    isNonEmptyString(receipt.downloadUrl)
  );
};

export const isApiErrorResponse = (
  value: unknown,
): value is ApiErrorResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "error"]) ||
    !hasVersion(value) ||
    !isRecord(value.error) ||
    !hasExactKeys(value.error, ["code", "message", "requestId", "retryable"])
  ) {
    return false;
  }
  const error = value.error;
  return (
    isNonEmptyString(error.code) &&
    isNonEmptyString(error.message) &&
    isNonEmptyString(error.requestId) &&
    typeof error.retryable === "boolean"
  );
};

export const isPreviewSelectionMessage = (
  value: unknown,
): value is PreviewSelectionMessage => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "type",
      "schemaVersion",
      "channelId",
      "projectId",
      "snapshotId",
      "revisionId",
      "elementId",
      "rect",
    ]) ||
    !isRecord(value.rect) ||
    !hasExactKeys(value.rect, ["x", "y", "width", "height"])
  ) {
    return false;
  }
  return (
    value.type === "synapsegit-lp.selection" &&
    value.schemaVersion === SCHEMA_VERSION &&
    isNonEmptyString(value.channelId) &&
    isNonEmptyString(value.projectId) &&
    isNonEmptyString(value.snapshotId) &&
    isNonEmptyString(value.revisionId) &&
    isNonEmptyString(value.elementId) &&
    isFiniteNumber(value.rect.x) &&
    isFiniteNumber(value.rect.y) &&
    isFiniteNumber(value.rect.width) &&
    value.rect.width >= 0 &&
    isFiniteNumber(value.rect.height) &&
    value.rect.height >= 0
  );
};

export const isPreviewActionMessage = (
  value: unknown,
): value is PreviewActionMessage => {
  if (!isRecord(value)) return false;
  const commonKeys = [
    "type",
    "schemaVersion",
    "channelId",
    "action",
    "projectId",
    "snapshotId",
    "revisionId",
  ] as const;
  const hasValidEnvelope =
    value.type === "synapsegit-lp.action" &&
    value.schemaVersion === SCHEMA_VERSION &&
    isNonEmptyString(value.channelId) &&
    isNonEmptyString(value.projectId) &&
    isNonEmptyString(value.snapshotId) &&
    isNonEmptyString(value.revisionId);
  if (!hasValidEnvelope) return false;
  if (value.action === "set_mode") {
    return (
      hasExactKeys(value, [...commonKeys, "mode"]) &&
      (value.mode === "select" || value.mode === "interact")
    );
  }
  return value.action === "clear_selection" && hasExactKeys(value, commonKeys);
};
