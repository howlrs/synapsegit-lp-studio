export const SCHEMA_VERSION = "1" as const;
export const API_VERSION = "v1" as const;
export const TARGET_SCHEMA_VERSION = 1 as const;
export const TARGET_RESOLVER_VERSION = 1 as const;
export const CHANGE_SET_SCHEMA = "org.synapsegit-lp-studio.change-set" as const;
export const CHANGE_SET_VERSION = 1 as const;

export const AI_CONTRACT_LIMITS = {
  providerIdLength: 128,
  providerLabelLength: 256,
  modelIdLength: 256,
  adapterVersionLength: 128,
  providerRequestIdLength: 512,
  contextEntries: 32,
  contextRedactions: 32,
  mediaTypeLength: 256,
  changeOperations: 32,
  changeSummaryLength: 2_000,
  changeFileBytes: 2 * 1024 * 1024,
  changeTotalBytes: 8 * 1024 * 1024,
  validationDestinations: 32,
  streamDeltaLength: 16 * 1024,
} as const;

export const TARGET_CONTRACT_LIMITS = {
  idLength: 128,
  pagePathBytes: 512,
  pagePathDepth: 16,
  labelLength: 256,
  textQuoteLength: 1024,
  textContextLength: 256,
  accessibleNameLength: 512,
  anchorStringLength: 2048,
  classTokens: 32,
  classTokenLength: 128,
  anchorsPerRegion: 3,
  resolverCandidates: 8,
  resolverReasons: 8,
  cssPixels: 1_000_000,
  textOffset: 1_000_000,
  blockLevel: 64,
} as const;

export type SchemaVersion = typeof SCHEMA_VERSION;
export type PreviewMode = "select" | "interact";
export type PreviewSource = "accepted" | "proposed";
export type ViewportPreset = "desktop" | "tablet" | "mobile" | "custom";
export type ArtifactDisposition = "adopted_unchanged" | "rejected" | "deferred";

export type AiProviderAvailability = "available" | "not_configured";

export interface AiProviderModelDescriptor {
  id: string;
  label: string;
}

export interface AiProviderDescriptor {
  id: string;
  label: string;
  adapterVersion: string;
  external: boolean;
  availability: AiProviderAvailability;
  models: AiProviderModelDescriptor[];
}

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
    targetKinds: ["page", "block", "element", "text", "point", "region"];
    dispositions: ArtifactDisposition[];
    singleProposalPerProject: true;
    importAvailable: boolean;
    aiProviders: AiProviderDescriptor[];
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
  target: TargetV1;
}

export type Target = TargetV1;

export interface TargetResponse {
  schemaVersion: SchemaVersion;
  target: TargetV1;
  resolution: TargetResolverResultV1;
  resolutionId: string;
}

export type TargetSelection = Omit<TargetResponse, "schemaVersion">;

export type TargetKind =
  "page" | "block" | "element" | "text" | "point" | "region";

export interface TargetRectV1 {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TargetPointCoordinatesV1 {
  x: number;
  y: number;
}

export interface TargetViewportV1 {
  cssWidth: number;
  cssHeight: number;
  scrollX: number;
  scrollY: number;
  devicePixelRatio: number;
  visualViewportScale: number;
  previewScale: number;
}

export interface TargetDocumentV1 {
  cssWidth: number;
  cssHeight: number;
  layoutEpoch: number;
}

export interface TargetGeometryV1 {
  documentCssPixelRect: TargetRectV1;
  viewportCssPixelRect: TargetRectV1;
  viewportNormalizedRect: TargetRectV1;
}

export interface TargetPointV1 {
  documentCssPixel: TargetPointCoordinatesV1;
  viewportNormalized: TargetPointCoordinatesV1;
}

/** Persistable element evidence. A render-local runtime node handle is omitted. */
export interface ElementAnchorV1 {
  tagName: string;
  uniqueElementId?: string;
  role?: string;
  accessibleName?: string;
  domPath?: string;
  classTokens?: string[];
  ancestorFingerprint?: string;
  siblingIndex?: number;
}

type TargetTextOffsetsV1 =
  | { startOffset?: never; endOffset?: never }
  | { startOffset: number; endOffset: number };

export type TargetTextAnchorV1 = {
  exact: string;
  prefix?: string;
  suffix?: string;
} & TargetTextOffsetsV1;

export interface TargetRegionAnchorV1 {
  containingBlock?: ElementAnchorV1;
  previousVisibleSibling?: ElementAnchorV1;
  nextVisibleSibling?: ElementAnchorV1;
  layoutMode: "flow" | "flex" | "grid" | "positioned" | "unknown";
}

export interface TargetBlockV1 {
  source: "semantic" | "landmark" | "heuristic" | "user";
  level: number;
}

type TargetCaptureBindingV1 =
  | { captureSource: "accepted"; captureProposalId?: never }
  | { captureSource: "proposal"; captureProposalId: string };

interface TargetCommonV1 {
  schemaVersion: typeof TARGET_SCHEMA_VERSION;
  targetId: string;
  captureRevisionId: string;
  pagePath: string;
  label: string;
  viewport: TargetViewportV1;
  document: TargetDocumentV1;
}

export type PageTargetV1 = TargetCommonV1 &
  TargetCaptureBindingV1 & {
    kind: "page";
  };

export type BlockTargetV1 = TargetCommonV1 &
  TargetCaptureBindingV1 & {
    kind: "block";
    elementAnchor: ElementAnchorV1;
    block: TargetBlockV1;
    geometry?: TargetGeometryV1;
  };

export type ElementTargetV1 = TargetCommonV1 &
  TargetCaptureBindingV1 & {
    kind: "element";
    elementAnchor: ElementAnchorV1;
    geometry?: TargetGeometryV1;
  };

export type TextTargetV1 = TargetCommonV1 &
  TargetCaptureBindingV1 & {
    kind: "text";
    elementAnchor: ElementAnchorV1;
    textAnchor: TargetTextAnchorV1;
    geometry?: TargetGeometryV1;
  };

export type PointTargetV1 = TargetCommonV1 &
  TargetCaptureBindingV1 & {
    kind: "point";
    point: TargetPointV1;
    regionAnchor: TargetRegionAnchorV1 & {
      containingBlock: ElementAnchorV1;
    };
  };

export type RegionTargetV1 = TargetCommonV1 &
  TargetCaptureBindingV1 & {
    kind: "region";
    geometry: TargetGeometryV1;
    regionAnchor: TargetRegionAnchorV1;
  };

export type TargetV1 =
  | PageTargetV1
  | BlockTargetV1
  | ElementTargetV1
  | TextTargetV1
  | PointTargetV1
  | RegionTargetV1;

export type TargetResolverSignalV1 =
  | "unique_id"
  | "semantic_fingerprint"
  | "text_quote"
  | "ancestor"
  | "sibling"
  | "dom_path"
  | "geometry";

export interface TargetResolverCandidateV1 {
  candidateId: string;
  score: number;
  reasons: TargetResolverSignalV1[];
  summary: string;
  elementAnchor?: ElementAnchorV1;
  geometry?: TargetGeometryV1;
}

interface TargetResolverResultCommonV1 {
  schemaVersion: typeof TARGET_SCHEMA_VERSION;
  resolverVersion: typeof TARGET_RESOLVER_VERSION;
  targetId: string;
  captureRevisionId: string;
  resolvedRevisionId: string;
  candidates: TargetResolverCandidateV1[];
}

export type TargetResolverResultV1 = TargetResolverResultCommonV1 &
  (
    | { status: "resolved"; selectedCandidateId: string }
    | {
        status: "ambiguous" | "detached";
        selectedCandidateId?: never;
      }
  );

export interface CreateContextRequest {
  schemaVersion: SchemaVersion;
  revisionId: string;
  targetId: string;
  resolutionId: string;
  attemptId: string;
  providerId: string;
  requestedModel: string;
  instruction: string;
}

export type ContextManifestPurpose = "entrypoint" | "dependency";

export interface ContextManifestEntryV1 {
  path: string;
  mediaType: string;
  purpose: ContextManifestPurpose;
  sourceByteLength: number;
  includedByteLength: number;
  startLine: number;
  endLine: number;
  sha256: string;
  estimatedTokens: number;
  redacted: boolean;
  truncated: boolean;
  redactions: string[];
}

export interface ContextManifestV1 {
  entries: ContextManifestEntryV1[];
  totalIncludedBytes: number;
  estimatedTokens: number;
  screenshotIncluded: false;
}

export interface ContextProviderBindingV1 {
  providerId: string;
  adapterVersion: string;
  requestedModel: string;
  external: boolean;
}

export interface ContextReview {
  id: string;
  revisionId: string;
  targetId: string;
  targetResolutionId: string;
  attemptId: string;
  providerId: string;
  requestedModel: string;
  provider: ContextProviderBindingV1;
  manifest: ContextManifestV1;
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
  fromPath?: string;
}

export interface CreateTextOperationV1 {
  op: "create_text";
  path: string;
  mediaType: string;
  content: string;
}

export interface ReplaceTextOperationV1 {
  op: "replace_text";
  path: string;
  expectedSha256: string;
  mediaType: string;
  content: string;
}

export interface RenameOperationV1 {
  op: "rename";
  from: string;
  to: string;
  expectedSha256: string;
}

export interface DeleteOperationV1 {
  op: "delete";
  path: string;
  expectedSha256: string;
}

export type ChangeOperationV1 =
  | CreateTextOperationV1
  | ReplaceTextOperationV1
  | RenameOperationV1
  | DeleteOperationV1;

export interface ChangeSetV1 {
  schema: typeof CHANGE_SET_SCHEMA;
  version: typeof CHANGE_SET_VERSION;
  baseRevisionId: string;
  summary: string;
  operations: ChangeOperationV1[];
}

export interface AiProviderUsageV1 {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
}

export interface AiProviderAttributionV1 {
  attemptId: string;
  providerRequestId: string;
  providerId: string;
  adapterVersion: string;
  requestedModel: string;
  reportedModel: string;
  external: boolean;
  usage?: AiProviderUsageV1;
}

export type AiAttemptStatus =
  | "queued"
  | "running"
  | "cancelled"
  | "timed_out"
  | "provider_failed"
  | "validation_failed"
  | "proposal_ready"
  | "completed";

export interface AiAttemptStatusV1 {
  schemaVersion: SchemaVersion;
  attemptId: string;
  status: AiAttemptStatus;
}

interface AiProviderStreamEventCommonV1 {
  schemaVersion: SchemaVersion;
  attemptId: string;
  sequence: number;
}

export type AiProviderStreamEventV1 = AiProviderStreamEventCommonV1 &
  (
    | { event: "started" }
    | { event: "text_delta"; text: string }
    | { event: "usage"; usage: AiProviderUsageV1 }
    | { event: "completed" }
  );

export type AiProviderOutputV1 =
  | { kind: "consultation"; text: string }
  | { kind: "change_set"; changeSet: ChangeSetV1 };

export interface AiProviderResultV1 {
  schemaVersion: SchemaVersion;
  attemptId: string;
  attribution: AiProviderAttributionV1;
  output: AiProviderOutputV1;
}

export type AiProviderErrorCodeV1 =
  | "not_configured"
  | "cancelled"
  | "timeout"
  | "rate_limited"
  | "unavailable"
  | "invalid_response"
  | "internal";

export interface AiProviderErrorV1 {
  schemaVersion: SchemaVersion;
  attemptId: string;
  code: AiProviderErrorCodeV1;
  message: string;
  retryable: boolean;
}

export type ValidationStatus = "passed" | "warning" | "failed";

export interface ValidationCheck {
  id: string;
  label: string;
  status: ValidationStatus;
  message: string;
  blocking: boolean;
  destinations: string[];
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
  providerContextSha256: string;
  changeSetSha256: string;
  changeSet: ChangeSetV1;
  attribution: AiProviderAttributionV1;
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

export type PreviewDiagnosticSeverity = "warning" | "error";
export type PreviewDiagnosticCode =
  "csp_blocked" | "site_error" | "unhandled_rejection";

/** Privacy-safe diagnostic envelope. Raw URLs, messages, and stacks are forbidden. */
export interface PreviewDiagnosticMessage {
  type: "synapsegit-lp.diagnostic";
  schemaVersion: SchemaVersion;
  channelId: string;
  projectId: string;
  snapshotId: string;
  revisionId: string;
  severity: PreviewDiagnosticSeverity;
  code: PreviewDiagnosticCode;
  sourceUnavailable: true;
}

/** Untrusted DOM observation. The server revalidates it before persistence. */
export interface PreviewTargetMessage {
  type: "synapsegit-lp.target-draft";
  schemaVersion: SchemaVersion;
  channelId: string;
  projectId: string;
  snapshotId: string;
  revisionId: string;
  target: TargetV1;
}

export interface PreviewStructureNode {
  runtimeNodeHandle: string;
  parentRuntimeNodeHandle?: string;
  kind: "block" | "element";
  label: string;
  tagName: string;
  depth: number;
}

export interface PreviewStructureMessage {
  type: "synapsegit-lp.structure";
  schemaVersion: SchemaVersion;
  channelId: string;
  projectId: string;
  snapshotId: string;
  revisionId: string;
  nodes: PreviewStructureNode[];
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
    | {
        action: "set_mode";
        mode: PreviewMode;
        targetKind: TargetKind;
        previewScale: number;
      }
    | { action: "clear_selection"; mode?: never }
    | { action: "request_structure" }
    | { action: "capture_page" }
    | {
        action: "capture_node";
        runtimeNodeHandle: string;
        targetKind: TargetKind;
      }
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
const isExplicitPort = (value: string): boolean =>
  /^[1-9][0-9]{0,4}$/.test(value) && Number(value) <= 65_535;

export const isPreviewScopeBaseOrigin = (value: unknown): value is string => {
  if (!isString(value) || !/^http:\/\/localhost:[1-9][0-9]{0,4}$/.test(value)) {
    return false;
  }
  try {
    const parsed = new URL(value);
    return (
      parsed.protocol === "http:" &&
      parsed.hostname === "localhost" &&
      isExplicitPort(parsed.port) &&
      parsed.origin === value &&
      parsed.username === "" &&
      parsed.password === "" &&
      parsed.pathname === "/" &&
      parsed.search === "" &&
      parsed.hash === ""
    );
  } catch {
    return false;
  }
};

const isCanonicalPreviewPath = (parsed: URL, rawUrl: string): boolean => {
  if (parsed.href !== rawUrl || !parsed.pathname.startsWith("/preview/")) {
    return false;
  }
  const path = parsed.pathname.slice("/preview/".length);
  if (path.length === 0 || path.includes("\\") || path.includes("//")) {
    return false;
  }
  const hasTrailingSlash = path.endsWith("/");
  const segments = (hasTrailingSlash ? path.slice(0, -1) : path).split("/");
  if (
    segments.length < 2 ||
    (segments.length === 2 && !hasTrailingSlash) ||
    (segments.length > 2 && hasTrailingSlash)
  ) {
    return false;
  }
  return segments.every((segment) => {
    if (segment.length === 0 || segment === "." || segment === "..") {
      return false;
    }
    try {
      const decoded = decodeURIComponent(segment);
      return (
        decoded.length > 0 &&
        decoded !== "." &&
        decoded !== ".." &&
        !decoded.includes("/") &&
        !decoded.includes("\\") &&
        !decoded.includes("\0")
      );
    } catch {
      return false;
    }
  });
};

export const isScopedPreviewUrl = (
  value: unknown,
  previewScopeBaseOrigin?: string,
): value is string => {
  if (!isString(value)) return false;
  try {
    const parsed = new URL(value);
    if (
      parsed.protocol !== "http:" ||
      !/^pv-[0-9a-f]{32}\.localhost$/.test(parsed.hostname) ||
      !isExplicitPort(parsed.port) ||
      parsed.username !== "" ||
      parsed.password !== "" ||
      parsed.search !== "" ||
      parsed.hash !== "" ||
      !isCanonicalPreviewPath(parsed, value)
    ) {
      return false;
    }
    if (previewScopeBaseOrigin === undefined) return true;
    if (!isPreviewScopeBaseOrigin(previewScopeBaseOrigin)) return false;
    const base = new URL(previewScopeBaseOrigin);
    return parsed.protocol === base.protocol && parsed.port === base.port;
  } catch {
    return false;
  }
};
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
const isContractRelativePath = (
  value: unknown,
  maximumBytes = 512,
): value is string =>
  isSafeRelativePath(value) &&
  value.normalize("NFC") === value &&
  new TextEncoder().encode(value).byteLength <= maximumBytes;
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

const isBoundedString = (
  value: unknown,
  minimumLength: number,
  maximumLength: number,
): value is string => {
  if (!isString(value) || value.includes("\0")) return false;
  let length = 0;
  for (const _character of value) {
    length += 1;
    if (length > maximumLength) return false;
  }
  return length >= minimumLength;
};

const isBoundedUtf8String = (
  value: unknown,
  minimumBytes: number,
  maximumBytes: number,
): value is string => {
  if (!isString(value) || value.includes("\0")) return false;
  const bytes = new TextEncoder().encode(value).byteLength;
  return bytes >= minimumBytes && bytes <= maximumBytes;
};

const isBoundedId = (value: unknown): value is string =>
  isBoundedString(value, 1, TARGET_CONTRACT_LIMITS.idLength);

const isFiniteRange = (
  value: unknown,
  minimum: number,
  maximum: number,
): value is number =>
  isFiniteNumber(value) && value >= minimum && value <= maximum;

const isSafeIntegerRange = (
  value: unknown,
  minimum: number,
  maximum: number,
): value is number =>
  isFiniteNumber(value) &&
  Number.isSafeInteger(value) &&
  value >= minimum &&
  value <= maximum;

const isBoundedExactArray = <T>(
  value: unknown,
  minimumLength: number,
  maximumLength: number,
  guard: (item: unknown) => item is T,
): value is T[] =>
  Array.isArray(value) &&
  value.length >= minimumLength &&
  value.length <= maximumLength &&
  isExactArrayOf(value, guard);

const isTargetPagePath = (value: unknown): value is string => {
  if (!isContractRelativePath(value, TARGET_CONTRACT_LIMITS.pagePathBytes)) {
    return false;
  }
  const segments = value.split("/");
  const fileName = segments.at(-1) ?? "";
  const extensionSeparator = fileName.lastIndexOf(".");
  return (
    segments.length <= TARGET_CONTRACT_LIMITS.pagePathDepth &&
    segments.every(
      (segment) =>
        !segment.endsWith(" ") &&
        !segment.endsWith(".") &&
        !/[<>:"|?*\u0000-\u001F\u007F-\u009F]/u.test(segment) &&
        !/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(segment),
    ) &&
    extensionSeparator > 0 &&
    /\.[Hh][Tt][Mm][Ll]?$/.test(fileName)
  );
};

const isTargetRectV1 = (
  value: unknown,
  normalized: boolean,
  nonZero: boolean,
): value is TargetRectV1 => {
  if (!isRecord(value) || !hasExactKeys(value, ["x", "y", "width", "height"])) {
    return false;
  }
  if (normalized) {
    if (
      !isFiniteRange(value.x, 0, 1) ||
      !isFiniteRange(value.y, 0, 1) ||
      !isFiniteRange(value.width, nonZero ? Number.MIN_VALUE : 0, 1) ||
      !isFiniteRange(value.height, nonZero ? Number.MIN_VALUE : 0, 1)
    ) {
      return false;
    }
    const roundingTolerance = Number.EPSILON * 8;
    return (
      value.x + value.width <= 1 + roundingTolerance &&
      value.y + value.height <= 1 + roundingTolerance
    );
  }
  return (
    isFiniteRange(
      value.x,
      -TARGET_CONTRACT_LIMITS.cssPixels,
      TARGET_CONTRACT_LIMITS.cssPixels,
    ) &&
    isFiniteRange(
      value.y,
      -TARGET_CONTRACT_LIMITS.cssPixels,
      TARGET_CONTRACT_LIMITS.cssPixels,
    ) &&
    isFiniteRange(
      value.width,
      nonZero ? Number.MIN_VALUE : 0,
      TARGET_CONTRACT_LIMITS.cssPixels,
    ) &&
    isFiniteRange(
      value.height,
      nonZero ? Number.MIN_VALUE : 0,
      TARGET_CONTRACT_LIMITS.cssPixels,
    )
  );
};

const isTargetViewportV1 = (value: unknown): value is TargetViewportV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "cssWidth",
    "cssHeight",
    "scrollX",
    "scrollY",
    "devicePixelRatio",
    "visualViewportScale",
    "previewScale",
  ]) &&
  isFiniteRange(
    value.cssWidth,
    Number.MIN_VALUE,
    TARGET_CONTRACT_LIMITS.cssPixels,
  ) &&
  isFiniteRange(
    value.cssHeight,
    Number.MIN_VALUE,
    TARGET_CONTRACT_LIMITS.cssPixels,
  ) &&
  isFiniteRange(value.scrollX, 0, TARGET_CONTRACT_LIMITS.cssPixels) &&
  isFiniteRange(value.scrollY, 0, TARGET_CONTRACT_LIMITS.cssPixels) &&
  isFiniteRange(value.devicePixelRatio, Number.MIN_VALUE, 16) &&
  isFiniteRange(value.visualViewportScale, Number.MIN_VALUE, 16) &&
  isFiniteRange(value.previewScale, Number.MIN_VALUE, 8);

const isTargetDocumentV1 = (value: unknown): value is TargetDocumentV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["cssWidth", "cssHeight", "layoutEpoch"]) &&
  isFiniteRange(value.cssWidth, 0, TARGET_CONTRACT_LIMITS.cssPixels) &&
  isFiniteRange(value.cssHeight, 0, TARGET_CONTRACT_LIMITS.cssPixels) &&
  isSafeIntegerRange(value.layoutEpoch, 0, Number.MAX_SAFE_INTEGER);

const isTargetGeometryV1 = (
  value: unknown,
  nonZero: boolean,
): value is TargetGeometryV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "documentCssPixelRect",
    "viewportCssPixelRect",
    "viewportNormalizedRect",
  ]) &&
  isTargetRectV1(value.documentCssPixelRect, false, nonZero) &&
  isTargetRectV1(value.viewportCssPixelRect, false, nonZero) &&
  isTargetRectV1(value.viewportNormalizedRect, true, nonZero);

const isTargetPointCoordinatesV1 = (
  value: unknown,
  normalized: boolean,
): value is TargetPointCoordinatesV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["x", "y"]) &&
  (normalized
    ? isFiniteRange(value.x, 0, 1) && isFiniteRange(value.y, 0, 1)
    : isFiniteRange(value.x, 0, TARGET_CONTRACT_LIMITS.cssPixels) &&
      isFiniteRange(value.y, 0, TARGET_CONTRACT_LIMITS.cssPixels));

const isTargetPointV1 = (value: unknown): value is TargetPointV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["documentCssPixel", "viewportNormalized"]) &&
  isTargetPointCoordinatesV1(value.documentCssPixel, false) &&
  isTargetPointCoordinatesV1(value.viewportNormalized, true);

export const isElementAnchorV1 = (value: unknown): value is ElementAnchorV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(
      value,
      ["tagName"],
      [
        "uniqueElementId",
        "role",
        "accessibleName",
        "domPath",
        "classTokens",
        "ancestorFingerprint",
        "siblingIndex",
      ],
    ) ||
    !isBoundedString(
      value.tagName,
      1,
      TARGET_CONTRACT_LIMITS.classTokenLength,
    ) ||
    !/^[A-Za-z][A-Za-z0-9-]*$/.test(value.tagName)
  ) {
    return false;
  }
  if (
    (Object.hasOwn(value, "uniqueElementId") &&
      !isBoundedString(
        value.uniqueElementId,
        1,
        TARGET_CONTRACT_LIMITS.anchorStringLength,
      )) ||
    (Object.hasOwn(value, "role") &&
      !isBoundedString(
        value.role,
        1,
        TARGET_CONTRACT_LIMITS.classTokenLength,
      )) ||
    (Object.hasOwn(value, "accessibleName") &&
      !isBoundedString(
        value.accessibleName,
        1,
        TARGET_CONTRACT_LIMITS.accessibleNameLength,
      )) ||
    (Object.hasOwn(value, "domPath") &&
      !isBoundedString(
        value.domPath,
        1,
        TARGET_CONTRACT_LIMITS.anchorStringLength,
      )) ||
    (Object.hasOwn(value, "ancestorFingerprint") &&
      !isBoundedString(
        value.ancestorFingerprint,
        1,
        TARGET_CONTRACT_LIMITS.anchorStringLength,
      )) ||
    (Object.hasOwn(value, "siblingIndex") &&
      !isSafeIntegerRange(
        value.siblingIndex,
        0,
        TARGET_CONTRACT_LIMITS.textOffset,
      ))
  ) {
    return false;
  }
  if (Object.hasOwn(value, "classTokens")) {
    if (
      !isBoundedExactArray(
        value.classTokens,
        1,
        TARGET_CONTRACT_LIMITS.classTokens,
        (token): token is string =>
          isBoundedString(token, 1, TARGET_CONTRACT_LIMITS.classTokenLength) &&
          !/\s/u.test(token),
      ) ||
      new Set(value.classTokens).size !== value.classTokens.length
    ) {
      return false;
    }
  }
  return true;
};

const isTargetTextAnchorV1 = (value: unknown): value is TargetTextAnchorV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(
      value,
      ["exact"],
      ["prefix", "suffix", "startOffset", "endOffset"],
    ) ||
    !isBoundedString(value.exact, 1, TARGET_CONTRACT_LIMITS.textQuoteLength) ||
    (Object.hasOwn(value, "prefix") &&
      !isBoundedString(
        value.prefix,
        1,
        TARGET_CONTRACT_LIMITS.textContextLength,
      )) ||
    (Object.hasOwn(value, "suffix") &&
      !isBoundedString(
        value.suffix,
        1,
        TARGET_CONTRACT_LIMITS.textContextLength,
      ))
  ) {
    return false;
  }
  const hasStartOffset = Object.hasOwn(value, "startOffset");
  const hasEndOffset = Object.hasOwn(value, "endOffset");
  if (hasStartOffset !== hasEndOffset) return false;
  if (!hasStartOffset) return true;
  return (
    isSafeIntegerRange(
      value.startOffset,
      0,
      TARGET_CONTRACT_LIMITS.textOffset,
    ) &&
    isSafeIntegerRange(value.endOffset, 0, TARGET_CONTRACT_LIMITS.textOffset) &&
    value.startOffset < value.endOffset &&
    value.endOffset - value.startOffset === value.exact.length
  );
};

const isTargetRegionAnchorV1 = (
  value: unknown,
  requireContainingBlock: boolean,
): value is TargetRegionAnchorV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(
      value,
      ["layoutMode"],
      ["containingBlock", "previousVisibleSibling", "nextVisibleSibling"],
    ) ||
    !isString(value.layoutMode) ||
    !["flow", "flex", "grid", "positioned", "unknown"].includes(
      value.layoutMode,
    )
  ) {
    return false;
  }
  const anchorKeys = [
    "containingBlock",
    "previousVisibleSibling",
    "nextVisibleSibling",
  ] as const;
  if (requireContainingBlock && !Object.hasOwn(value, "containingBlock")) {
    return false;
  }
  return anchorKeys.every(
    (key) => !Object.hasOwn(value, key) || isElementAnchorV1(value[key]),
  );
};

const isTargetBlockV1 = (value: unknown): value is TargetBlockV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["source", "level"]) &&
  isString(value.source) &&
  ["semantic", "landmark", "heuristic", "user"].includes(value.source) &&
  isSafeIntegerRange(value.level, 0, TARGET_CONTRACT_LIMITS.blockLevel);

const TARGET_COMMON_KEYS = [
  "schemaVersion",
  "targetId",
  "captureRevisionId",
  "captureSource",
  "pagePath",
  "kind",
  "label",
  "viewport",
  "document",
] as const;

const hasValidTargetCommonFields = (value: UnknownRecord): boolean =>
  value.schemaVersion === TARGET_SCHEMA_VERSION &&
  isBoundedId(value.targetId) &&
  isBoundedId(value.captureRevisionId) &&
  isTargetPagePath(value.pagePath) &&
  isBoundedString(value.label, 1, TARGET_CONTRACT_LIMITS.labelLength) &&
  isTargetViewportV1(value.viewport) &&
  isTargetDocumentV1(value.document) &&
  ((value.captureSource === "accepted" &&
    !Object.hasOwn(value, "captureProposalId")) ||
    (value.captureSource === "proposal" &&
      Object.hasOwn(value, "captureProposalId") &&
      isBoundedId(value.captureProposalId)));

export const isTargetV1 = (value: unknown): value is TargetV1 => {
  if (!isRecord(value) || !hasValidTargetCommonFields(value)) return false;
  const captureProposalKey = ["captureProposalId"] as const;
  switch (value.kind) {
    case "page":
      return hasExactKeys(value, TARGET_COMMON_KEYS, captureProposalKey);
    case "block":
      return (
        hasExactKeys(
          value,
          [...TARGET_COMMON_KEYS, "elementAnchor", "block"],
          [...captureProposalKey, "geometry"],
        ) &&
        isElementAnchorV1(value.elementAnchor) &&
        isTargetBlockV1(value.block) &&
        (!Object.hasOwn(value, "geometry") ||
          isTargetGeometryV1(value.geometry, false))
      );
    case "element":
      return (
        hasExactKeys(
          value,
          [...TARGET_COMMON_KEYS, "elementAnchor"],
          [...captureProposalKey, "geometry"],
        ) &&
        isElementAnchorV1(value.elementAnchor) &&
        (!Object.hasOwn(value, "geometry") ||
          isTargetGeometryV1(value.geometry, false))
      );
    case "text":
      return (
        hasExactKeys(
          value,
          [...TARGET_COMMON_KEYS, "elementAnchor", "textAnchor"],
          [...captureProposalKey, "geometry"],
        ) &&
        isElementAnchorV1(value.elementAnchor) &&
        isTargetTextAnchorV1(value.textAnchor) &&
        (!Object.hasOwn(value, "geometry") ||
          isTargetGeometryV1(value.geometry, false))
      );
    case "point":
      return (
        hasExactKeys(
          value,
          [...TARGET_COMMON_KEYS, "point", "regionAnchor"],
          captureProposalKey,
        ) &&
        isTargetPointV1(value.point) &&
        isTargetRegionAnchorV1(value.regionAnchor, true)
      );
    case "region":
      return (
        hasExactKeys(
          value,
          [...TARGET_COMMON_KEYS, "geometry", "regionAnchor"],
          captureProposalKey,
        ) &&
        isTargetGeometryV1(value.geometry, true) &&
        isTargetRegionAnchorV1(value.regionAnchor, false)
      );
    default:
      return false;
  }
};

const TARGET_RESOLVER_SIGNALS = [
  "unique_id",
  "semantic_fingerprint",
  "text_quote",
  "ancestor",
  "sibling",
  "dom_path",
  "geometry",
] as const satisfies readonly TargetResolverSignalV1[];

const isTargetResolverSignalV1 = (
  value: unknown,
): value is TargetResolverSignalV1 =>
  isString(value) &&
  (TARGET_RESOLVER_SIGNALS as readonly string[]).includes(value);

const isTargetResolverCandidateV1 = (
  value: unknown,
): value is TargetResolverCandidateV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(
      value,
      ["candidateId", "score", "reasons", "summary"],
      ["elementAnchor", "geometry"],
    ) ||
    !isBoundedId(value.candidateId) ||
    !isFiniteRange(value.score, 0, 1) ||
    !isBoundedExactArray(
      value.reasons,
      1,
      TARGET_CONTRACT_LIMITS.resolverReasons,
      isTargetResolverSignalV1,
    ) ||
    new Set(value.reasons).size !== value.reasons.length ||
    !isBoundedString(value.summary, 1, TARGET_CONTRACT_LIMITS.labelLength)
  ) {
    return false;
  }
  return (
    (!Object.hasOwn(value, "elementAnchor") ||
      isElementAnchorV1(value.elementAnchor)) &&
    (!Object.hasOwn(value, "geometry") ||
      isTargetGeometryV1(value.geometry, false))
  );
};

export const isTargetResolverResultV1 = (
  value: unknown,
): value is TargetResolverResultV1 => {
  if (!isRecord(value)) return false;
  const resolved = value.status === "resolved";
  if (
    !hasExactKeys(value, [
      "schemaVersion",
      "resolverVersion",
      "targetId",
      "captureRevisionId",
      "resolvedRevisionId",
      "status",
      "candidates",
      ...(resolved ? ["selectedCandidateId"] : []),
    ]) ||
    value.schemaVersion !== TARGET_SCHEMA_VERSION ||
    value.resolverVersion !== TARGET_RESOLVER_VERSION ||
    !isBoundedId(value.targetId) ||
    !isBoundedId(value.captureRevisionId) ||
    !isBoundedId(value.resolvedRevisionId) ||
    !isString(value.status) ||
    !["resolved", "ambiguous", "detached"].includes(value.status) ||
    !isBoundedExactArray(
      value.candidates,
      value.status === "detached" ? 0 : 1,
      TARGET_CONTRACT_LIMITS.resolverCandidates,
      isTargetResolverCandidateV1,
    )
  ) {
    return false;
  }
  const candidateIds = value.candidates.map(
    (candidate) => candidate.candidateId,
  );
  if (new Set(candidateIds).size !== candidateIds.length) return false;
  return (
    !resolved ||
    (isBoundedId(value.selectedCandidateId) &&
      candidateIds.includes(value.selectedCandidateId))
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
    isScopedPreviewUrl(value.previewUrl)
  );
};

const isAiProviderModelDescriptor = (
  value: unknown,
): value is AiProviderModelDescriptor =>
  isRecord(value) &&
  hasExactKeys(value, ["id", "label"]) &&
  isBoundedString(value.id, 1, AI_CONTRACT_LIMITS.modelIdLength) &&
  isBoundedString(value.label, 1, AI_CONTRACT_LIMITS.providerLabelLength);

export const isAiProviderDescriptor = (
  value: unknown,
): value is AiProviderDescriptor => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "id",
      "label",
      "adapterVersion",
      "external",
      "availability",
      "models",
    ]) ||
    !isBoundedExactArray(value.models, 0, 32, isAiProviderModelDescriptor)
  ) {
    return false;
  }
  return (
    isBoundedString(value.id, 1, AI_CONTRACT_LIMITS.providerIdLength) &&
    isBoundedString(value.label, 1, AI_CONTRACT_LIMITS.providerLabelLength) &&
    isBoundedString(
      value.adapterVersion,
      1,
      AI_CONTRACT_LIMITS.adapterVersionLength,
    ) &&
    typeof value.external === "boolean" &&
    (value.availability === "available" ||
      value.availability === "not_configured") &&
    (value.availability !== "available" || value.models.length > 0) &&
    new Set(value.models.map((model) => model.id)).size === value.models.length
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
      "aiProviders",
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
    isPreviewScopeBaseOrigin(value.previewOrigin) &&
    isExactArrayOf(capabilities.targetKinds, isString) &&
    capabilities.targetKinds.length === 6 &&
    capabilities.targetKinds.every(
      (kind, index) =>
        kind ===
        (["page", "block", "element", "text", "point", "region"] as const)[
          index
        ],
    ) &&
    isStringArray(capabilities.dispositions) &&
    capabilities.dispositions.length > 0 &&
    new Set(capabilities.dispositions).size ===
      capabilities.dispositions.length &&
    capabilities.dispositions.every((item) =>
      ["adopted_unchanged", "rejected", "deferred"].includes(item),
    ) &&
    capabilities.singleProposalPerProject === true &&
    typeof capabilities.importAvailable === "boolean" &&
    isBoundedExactArray(
      capabilities.aiProviders,
      1,
      32,
      isAiProviderDescriptor,
    ) &&
    new Set(capabilities.aiProviders.map((provider) => provider.id)).size ===
      capabilities.aiProviders.length &&
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
  isContractRelativePath(value.path) &&
  isNonNegativeInteger(value.byteLength) &&
  isSha256(value.sha256);

const isImportPreviewExclusion = (
  value: unknown,
): value is ImportPreviewExclusion =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "reason"]) &&
  isContractRelativePath(value.path) &&
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
  return (
    isRecord(value) &&
    hasExactKeys(value, [
      "schemaVersion",
      "target",
      "resolution",
      "resolutionId",
    ]) &&
    hasVersion(value) &&
    isTargetV1(value.target) &&
    isTargetResolverResultV1(value.resolution) &&
    value.resolution.targetId === value.target.targetId &&
    value.resolution.captureRevisionId === value.target.captureRevisionId &&
    isBoundedId(value.resolutionId)
  );
};

export const isCreateContextRequest = (
  value: unknown,
): value is CreateContextRequest =>
  isRecord(value) &&
  hasExactKeys(value, [
    "schemaVersion",
    "revisionId",
    "targetId",
    "resolutionId",
    "attemptId",
    "providerId",
    "requestedModel",
    "instruction",
  ]) &&
  hasVersion(value) &&
  isBoundedId(value.revisionId) &&
  isBoundedId(value.targetId) &&
  isBoundedId(value.resolutionId) &&
  isBoundedId(value.attemptId) &&
  isBoundedString(value.providerId, 1, AI_CONTRACT_LIMITS.providerIdLength) &&
  isBoundedString(value.requestedModel, 1, AI_CONTRACT_LIMITS.modelIdLength) &&
  isBoundedUtf8String(value.instruction, 1, 2_000);

export const isContextManifestEntryV1 = (
  value: unknown,
): value is ContextManifestEntryV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "path",
    "mediaType",
    "purpose",
    "sourceByteLength",
    "includedByteLength",
    "startLine",
    "endLine",
    "sha256",
    "estimatedTokens",
    "redacted",
    "truncated",
    "redactions",
  ]) &&
  isContractRelativePath(value.path) &&
  isBoundedString(value.mediaType, 1, AI_CONTRACT_LIMITS.mediaTypeLength) &&
  (value.purpose === "entrypoint" || value.purpose === "dependency") &&
  isNonNegativeInteger(value.sourceByteLength) &&
  isNonNegativeInteger(value.includedByteLength) &&
  isSafeIntegerRange(value.startLine, 1, 1_000_000) &&
  isSafeIntegerRange(value.endLine, 0, 1_000_000) &&
  (value.includedByteLength === 0
    ? value.endLine === 0
    : value.endLine >= value.startLine) &&
  isSha256(value.sha256) &&
  isNonNegativeInteger(value.estimatedTokens) &&
  typeof value.redacted === "boolean" &&
  typeof value.truncated === "boolean" &&
  isBoundedExactArray(
    value.redactions,
    0,
    AI_CONTRACT_LIMITS.contextRedactions,
    (redaction): redaction is string => isBoundedString(redaction, 1, 128),
  ) &&
  new Set(value.redactions).size === value.redactions.length &&
  value.redacted === value.redactions.length > 0;

export const isContextManifestV1 = (
  value: unknown,
): value is ContextManifestV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "entries",
      "totalIncludedBytes",
      "estimatedTokens",
      "screenshotIncluded",
    ]) ||
    !isBoundedExactArray(
      value.entries,
      1,
      AI_CONTRACT_LIMITS.contextEntries,
      isContextManifestEntryV1,
    ) ||
    !isNonNegativeInteger(value.totalIncludedBytes) ||
    !isNonNegativeInteger(value.estimatedTokens) ||
    value.screenshotIncluded !== false
  ) {
    return false;
  }
  return (
    new Set(value.entries.map((entry) => entry.path)).size ===
      value.entries.length &&
    value.entries.reduce(
      (total, entry) => total + entry.includedByteLength,
      0,
    ) === value.totalIncludedBytes
  );
};

const isContextProviderBindingV1 = (
  value: unknown,
): value is ContextProviderBindingV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "providerId",
    "adapterVersion",
    "requestedModel",
    "external",
  ]) &&
  isBoundedString(value.providerId, 1, AI_CONTRACT_LIMITS.providerIdLength) &&
  isBoundedString(
    value.adapterVersion,
    1,
    AI_CONTRACT_LIMITS.adapterVersionLength,
  ) &&
  isBoundedString(value.requestedModel, 1, AI_CONTRACT_LIMITS.modelIdLength) &&
  typeof value.external === "boolean";

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
      "targetResolutionId",
      "attemptId",
      "providerId",
      "requestedModel",
      "provider",
      "manifest",
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
    isBoundedId(context.targetResolutionId) &&
    isBoundedId(context.attemptId) &&
    isBoundedString(
      context.providerId,
      1,
      AI_CONTRACT_LIMITS.providerIdLength,
    ) &&
    isBoundedString(
      context.requestedModel,
      1,
      AI_CONTRACT_LIMITS.modelIdLength,
    ) &&
    isContextProviderBindingV1(context.provider) &&
    context.provider.providerId === context.providerId &&
    context.provider.requestedModel === context.requestedModel &&
    isContextManifestV1(context.manifest) &&
    isBoundedUtf8String(context.instruction, 1, 2_000) &&
    isString(context.canonicalJson) &&
    isSha256(context.sha256)
  );
};

const isCreateTextOperationV1 = (
  value: unknown,
): value is CreateTextOperationV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["op", "path", "mediaType", "content"]) &&
  value.op === "create_text" &&
  isContractRelativePath(value.path) &&
  isBoundedString(value.mediaType, 1, AI_CONTRACT_LIMITS.mediaTypeLength) &&
  isBoundedUtf8String(value.content, 0, AI_CONTRACT_LIMITS.changeFileBytes);

const isReplaceTextOperationV1 = (
  value: unknown,
): value is ReplaceTextOperationV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "op",
    "path",
    "expectedSha256",
    "mediaType",
    "content",
  ]) &&
  value.op === "replace_text" &&
  isContractRelativePath(value.path) &&
  isSha256(value.expectedSha256) &&
  isBoundedString(value.mediaType, 1, AI_CONTRACT_LIMITS.mediaTypeLength) &&
  isBoundedUtf8String(value.content, 0, AI_CONTRACT_LIMITS.changeFileBytes);

const isRenameOperationV1 = (value: unknown): value is RenameOperationV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["op", "from", "to", "expectedSha256"]) &&
  value.op === "rename" &&
  isContractRelativePath(value.from) &&
  isContractRelativePath(value.to) &&
  value.from !== value.to &&
  isSha256(value.expectedSha256);

const isDeleteOperationV1 = (value: unknown): value is DeleteOperationV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["op", "path", "expectedSha256"]) &&
  value.op === "delete" &&
  isContractRelativePath(value.path) &&
  isSha256(value.expectedSha256);

export const isChangeOperationV1 = (
  value: unknown,
): value is ChangeOperationV1 =>
  isCreateTextOperationV1(value) ||
  isReplaceTextOperationV1(value) ||
  isRenameOperationV1(value) ||
  isDeleteOperationV1(value);

export const isChangeSetV1 = (value: unknown): value is ChangeSetV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "schema",
      "version",
      "baseRevisionId",
      "summary",
      "operations",
    ]) ||
    value.schema !== CHANGE_SET_SCHEMA ||
    value.version !== CHANGE_SET_VERSION ||
    !isBoundedId(value.baseRevisionId) ||
    !isBoundedUtf8String(
      value.summary,
      1,
      AI_CONTRACT_LIMITS.changeSummaryLength,
    ) ||
    !isBoundedExactArray(
      value.operations,
      1,
      AI_CONTRACT_LIMITS.changeOperations,
      isChangeOperationV1,
    )
  ) {
    return false;
  }
  const totalContentBytes = value.operations.reduce((total, operation) => {
    if (operation.op !== "create_text" && operation.op !== "replace_text") {
      return total;
    }
    return total + new TextEncoder().encode(operation.content).byteLength;
  }, 0);
  return totalContentBytes <= AI_CONTRACT_LIMITS.changeTotalBytes;
};

export const isAiProviderUsageV1 = (
  value: unknown,
): value is AiProviderUsageV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [], ["inputTokens", "outputTokens", "totalTokens"])
  ) {
    return false;
  }
  const present = ["inputTokens", "outputTokens", "totalTokens"].filter((key) =>
    Object.hasOwn(value, key),
  );
  return present.every((key) => isNonNegativeInteger(value[key]));
};

export const isAiProviderAttributionV1 = (
  value: unknown,
): value is AiProviderAttributionV1 =>
  isRecord(value) &&
  hasExactKeys(
    value,
    [
      "attemptId",
      "providerRequestId",
      "providerId",
      "adapterVersion",
      "requestedModel",
      "reportedModel",
      "external",
    ],
    ["usage"],
  ) &&
  isBoundedId(value.attemptId) &&
  isBoundedString(
    value.providerRequestId,
    1,
    AI_CONTRACT_LIMITS.providerRequestIdLength,
  ) &&
  isBoundedString(value.providerId, 1, AI_CONTRACT_LIMITS.providerIdLength) &&
  isBoundedString(
    value.adapterVersion,
    1,
    AI_CONTRACT_LIMITS.adapterVersionLength,
  ) &&
  isBoundedString(value.requestedModel, 1, AI_CONTRACT_LIMITS.modelIdLength) &&
  isBoundedString(value.reportedModel, 1, AI_CONTRACT_LIMITS.modelIdLength) &&
  typeof value.external === "boolean" &&
  (!Object.hasOwn(value, "usage") || isAiProviderUsageV1(value.usage));

const isAiAttemptStatus = (value: unknown): value is AiAttemptStatus =>
  value === "queued" ||
  value === "running" ||
  value === "cancelled" ||
  value === "timed_out" ||
  value === "provider_failed" ||
  value === "validation_failed" ||
  value === "proposal_ready" ||
  value === "completed";

export const isAiAttemptStatusV1 = (
  value: unknown,
): value is AiAttemptStatusV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "attemptId", "status"]) &&
  hasVersion(value) &&
  isBoundedId(value.attemptId) &&
  isAiAttemptStatus(value.status);

export const isAiProviderStreamEventV1 = (
  value: unknown,
): value is AiProviderStreamEventV1 => {
  if (
    !isRecord(value) ||
    !hasVersion(value) ||
    !isBoundedId(value.attemptId) ||
    !isNonNegativeInteger(value.sequence)
  ) {
    return false;
  }
  if (value.event === "text_delta") {
    return (
      hasExactKeys(value, [
        "schemaVersion",
        "attemptId",
        "sequence",
        "event",
        "text",
      ]) && isBoundedString(value.text, 1, AI_CONTRACT_LIMITS.streamDeltaLength)
    );
  }
  if (value.event === "usage") {
    return (
      hasExactKeys(value, [
        "schemaVersion",
        "attemptId",
        "sequence",
        "event",
        "usage",
      ]) && isAiProviderUsageV1(value.usage)
    );
  }
  return (
    (value.event === "started" || value.event === "completed") &&
    hasExactKeys(value, ["schemaVersion", "attemptId", "sequence", "event"])
  );
};

const isAiProviderOutputV1 = (value: unknown): value is AiProviderOutputV1 =>
  isRecord(value) &&
  ((hasExactKeys(value, ["kind", "text"]) &&
    value.kind === "consultation" &&
    isBoundedUtf8String(value.text, 0, AI_CONTRACT_LIMITS.changeTotalBytes)) ||
    (hasExactKeys(value, ["kind", "changeSet"]) &&
      value.kind === "change_set" &&
      isChangeSetV1(value.changeSet)));

export const isAiProviderResultV1 = (
  value: unknown,
): value is AiProviderResultV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "schemaVersion",
    "attemptId",
    "attribution",
    "output",
  ]) &&
  hasVersion(value) &&
  isBoundedId(value.attemptId) &&
  isAiProviderAttributionV1(value.attribution) &&
  value.attribution.attemptId === value.attemptId &&
  isAiProviderOutputV1(value.output);

const isAiProviderErrorCodeV1 = (
  value: unknown,
): value is AiProviderErrorCodeV1 =>
  value === "not_configured" ||
  value === "cancelled" ||
  value === "timeout" ||
  value === "rate_limited" ||
  value === "unavailable" ||
  value === "invalid_response" ||
  value === "internal";

export const isAiProviderErrorV1 = (
  value: unknown,
): value is AiProviderErrorV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "schemaVersion",
    "attemptId",
    "code",
    "message",
    "retryable",
  ]) &&
  hasVersion(value) &&
  isBoundedId(value.attemptId) &&
  isAiProviderErrorCodeV1(value.code) &&
  isBoundedString(value.message, 1, 2_048) &&
  typeof value.retryable === "boolean";

const isProposalChange = (value: unknown): value is ProposalChange =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "kind"], ["fromPath"]) &&
  isContractRelativePath(value.path) &&
  ((value.kind === "renamed" && isContractRelativePath(value.fromPath)) ||
    ((value.kind === "created" ||
      value.kind === "modified" ||
      value.kind === "deleted") &&
      !Object.hasOwn(value, "fromPath")));

const isValidationCheck = (value: unknown): value is ValidationCheck =>
  isRecord(value) &&
  hasExactKeys(value, [
    "id",
    "label",
    "status",
    "message",
    "blocking",
    "destinations",
  ]) &&
  isNonEmptyString(value.id) &&
  isNonEmptyString(value.label) &&
  (value.status === "passed" ||
    value.status === "warning" ||
    value.status === "failed") &&
  isString(value.message) &&
  typeof value.blocking === "boolean" &&
  isBoundedExactArray(
    value.destinations,
    0,
    AI_CONTRACT_LIMITS.validationDestinations,
    (destination): destination is string =>
      isBoundedString(destination, 1, 2_048),
  ) &&
  new Set(value.destinations).size === value.destinations.length;

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
      "providerContextSha256",
      "changeSetSha256",
      "changeSet",
      "attribution",
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
    isSha256(proposal.providerContextSha256) &&
    isSha256(proposal.changeSetSha256) &&
    isChangeSetV1(proposal.changeSet) &&
    proposal.changeSet.baseRevisionId === proposal.baseRevisionId &&
    isAiProviderAttributionV1(proposal.attribution) &&
    proposal.sourceAttribution === "caller_supplied_ai_attributed" &&
    proposal.executionVerified === false &&
    isScopedPreviewUrl(proposal.previewUrl) &&
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

export const isPreviewDiagnosticMessage = (
  value: unknown,
): value is PreviewDiagnosticMessage =>
  isRecord(value) &&
  hasExactKeys(value, [
    "type",
    "schemaVersion",
    "channelId",
    "projectId",
    "snapshotId",
    "revisionId",
    "severity",
    "code",
    "sourceUnavailable",
  ]) &&
  value.type === "synapsegit-lp.diagnostic" &&
  value.schemaVersion === SCHEMA_VERSION &&
  isNonEmptyString(value.channelId) &&
  isNonEmptyString(value.projectId) &&
  isNonEmptyString(value.snapshotId) &&
  isNonEmptyString(value.revisionId) &&
  (value.severity === "warning" || value.severity === "error") &&
  (value.code === "csp_blocked" ||
    value.code === "site_error" ||
    value.code === "unhandled_rejection") &&
  value.sourceUnavailable === true;

export const isPreviewTargetMessage = (
  value: unknown,
): value is PreviewTargetMessage =>
  isRecord(value) &&
  hasExactKeys(value, [
    "type",
    "schemaVersion",
    "channelId",
    "projectId",
    "snapshotId",
    "revisionId",
    "target",
  ]) &&
  value.type === "synapsegit-lp.target-draft" &&
  value.schemaVersion === SCHEMA_VERSION &&
  isNonEmptyString(value.channelId) &&
  isNonEmptyString(value.projectId) &&
  isNonEmptyString(value.snapshotId) &&
  isNonEmptyString(value.revisionId) &&
  isTargetV1(value.target) &&
  value.target.captureRevisionId === value.revisionId &&
  ((value.target.captureSource === "accepted" &&
    value.snapshotId === value.revisionId) ||
    (value.target.captureSource === "proposal" &&
      value.target.captureProposalId === value.snapshotId &&
      value.snapshotId !== value.revisionId));

const isPreviewStructureNode = (
  value: unknown,
): value is PreviewStructureNode =>
  isRecord(value) &&
  hasExactKeys(
    value,
    ["runtimeNodeHandle", "kind", "label", "tagName", "depth"],
    ["parentRuntimeNodeHandle"],
  ) &&
  isBoundedString(value.runtimeNodeHandle, 1, 128) &&
  (!Object.hasOwn(value, "parentRuntimeNodeHandle") ||
    isBoundedString(value.parentRuntimeNodeHandle, 1, 128)) &&
  (value.kind === "block" || value.kind === "element") &&
  isBoundedString(value.label, 1, TARGET_CONTRACT_LIMITS.labelLength) &&
  isBoundedString(value.tagName, 1, TARGET_CONTRACT_LIMITS.classTokenLength) &&
  /^[a-z][a-z0-9-]*$/u.test(value.tagName) &&
  isSafeIntegerRange(value.depth, 0, TARGET_CONTRACT_LIMITS.blockLevel);

export const isPreviewStructureMessage = (
  value: unknown,
): value is PreviewStructureMessage => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "type",
      "schemaVersion",
      "channelId",
      "projectId",
      "snapshotId",
      "revisionId",
      "nodes",
    ]) ||
    value.type !== "synapsegit-lp.structure" ||
    value.schemaVersion !== SCHEMA_VERSION ||
    !isNonEmptyString(value.channelId) ||
    !isNonEmptyString(value.projectId) ||
    !isNonEmptyString(value.snapshotId) ||
    !isNonEmptyString(value.revisionId) ||
    !isBoundedExactArray(value.nodes, 0, 200, isPreviewStructureNode)
  ) {
    return false;
  }
  const handles = value.nodes.map((node) => node.runtimeNodeHandle);
  if (new Set(handles).size !== handles.length) return false;
  const known = new Set(handles);
  return value.nodes.every(
    (node) =>
      node.parentRuntimeNodeHandle === undefined ||
      (node.parentRuntimeNodeHandle !== node.runtimeNodeHandle &&
        known.has(node.parentRuntimeNodeHandle)),
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
      hasExactKeys(value, [
        ...commonKeys,
        "mode",
        "targetKind",
        "previewScale",
      ]) &&
      (value.mode === "select" || value.mode === "interact") &&
      isString(value.targetKind) &&
      ["page", "block", "element", "text", "point", "region"].includes(
        value.targetKind,
      ) &&
      isFiniteRange(value.previewScale, Number.MIN_VALUE, 8)
    );
  }
  if (
    value.action === "clear_selection" ||
    value.action === "request_structure" ||
    value.action === "capture_page"
  ) {
    return hasExactKeys(value, commonKeys);
  }
  return (
    value.action === "capture_node" &&
    hasExactKeys(value, [...commonKeys, "runtimeNodeHandle", "targetKind"]) &&
    isBoundedString(value.runtimeNodeHandle, 1, 128) &&
    isString(value.targetKind) &&
    ["page", "block", "element", "text", "point", "region"].includes(
      value.targetKind,
    )
  );
};
