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

export const RECOVERY_CONTRACT_LIMITS = {
  maxPoints: 72,
  maxSnapshotFiles: 1_000,
} as const;

export const PROJECT_DISPLAY_NAME_LIMITS = {
  codePoints: 256,
  utf8Bytes: 1_024,
} as const;
export const PROJECT_METADATA_AUTOSAVE_DEBOUNCE_MS = 250 as const;

export type SchemaVersion = typeof SCHEMA_VERSION;
export type PreviewMode = "select" | "interact";
export type PreviewSource = "accepted" | "proposed";
export type ViewportPreset = "desktop" | "tablet" | "mobile" | "custom";
export type ArtifactDisposition = "adopted_unchanged" | "rejected" | "deferred";

export type AiProviderAvailability = "available" | "not_configured";
export type AiProviderModelSelection = "closed" | "open";
export type AiCredentialSourceId = "editor_session" | "server_configured";

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
  dataRetentionPolicy: string;
  trainingPolicy: string;
  policyNotice: string;
  models: AiProviderModelDescriptor[];
  modelSelection: AiProviderModelSelection;
  credentialSources: AiCredentialSourceId[];
}

export interface ProviderCredentialStatus {
  providerId: "openai";
  credentialSourceId: "editor_session";
  credentialBindingId: string;
  status: "configured_unverified";
  expiresAt: string;
}

export interface ProviderCredentialResponse {
  schemaVersion: SchemaVersion;
  credential: ProviderCredentialStatus | null;
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
    operatingMode?: "normal" | "read_only_recovery";
    recoveryPointCount?: number;
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

export type ActiveReviewStatus =
  "pending_review" | "reconciliation_required" | "failed";

export interface ProjectActiveReview {
  reviewId: string;
  proposalId: string;
  baseRevisionId: string;
  status: ActiveReviewStatus;
}

export interface ProjectHistoryEntry {
  reviewId: string;
  proposalId: string;
  baseRevisionId: string;
  resultingRevisionId: string;
  disposition: ArtifactDisposition;
  artifactManifestSha256: string;
  baseArtifactManifestSha256?: string;
  resultingAcceptedManifestSha256?: string;
  decisionReceiptSha256: string;
  recordedAt: string;
  publicNote?: string | null;
}

export interface Project {
  id: string;
  displayName: string;
  revisionId: string;
  acceptedManifestSha256: string;
  status: "ready";
  previewUrl: string;
  files: ProjectFile[];
  /** Absent only when talking to the pre-C7 local server. */
  activeReview?: ProjectActiveReview | null;
  /** Absent only when talking to the pre-C7 local server. */
  history?: ProjectHistoryEntry[];
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

export interface UpdateProjectMetadataRequest {
  schemaVersion: SchemaVersion;
  expectedDisplayName: string;
  displayName: string;
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
  credentialSourceId?: AiCredentialSourceId;
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
  credentialSourceId: string;
  credentialBindingId: string;
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

export interface CancelAiAttemptRequest {
  schemaVersion: SchemaVersion;
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
  /** Persisted review context added by the C7 resume contract. */
  target?: TargetV1;
  /** Proposed-workspace re-resolution added by the C7 resume contract. */
  targetResolution?: TargetResolverResultV1;
  /** Human request restored without exposing provider request/response data. */
  instruction?: string;
  /** A deferred predecessor is terminal; continuation always has a new ID. */
  derivedFromProposalId?: string | null;
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

export type ReviewStatus =
  | "pending_review"
  | "adopted"
  | "rejected"
  | "deferred"
  | "reconciliation_required"
  | "failed";

export interface ReviewDecision {
  proposalId: string;
  disposition: ArtifactDisposition;
  revisionId: string;
  artifactManifestSha256: string;
}

export interface Review {
  reviewId: string;
  projectId: string;
  proposalId: string;
  status: ReviewStatus;
  reconciliationRequired: boolean;
  proposal: Proposal | null;
  decision: ReviewDecision | null;
}

export interface ReviewResponse {
  schemaVersion: SchemaVersion;
  review: Review;
}

export interface CreateExportRequest {
  schemaVersion: SchemaVersion;
  revisionId: string;
}

export interface SchemaIdentityV1 {
  name: string;
  version: number;
}

export interface ExportOptionsV1 {
  entryPoint: string;
  basePathProfile: "relative_static_http";
  externalAssetPolicy: "report";
  artifactFormat: "zip_stored_v1";
}

export interface ExportFileManifestEntryV1 {
  path: string;
  mediaType: string;
  byteLength: number;
  sha256: string;
}

export interface ExportFileManifestV1 {
  schema: SchemaIdentityV1;
  sha256: string;
  totalByteLength: number;
  files: ExportFileManifestEntryV1[];
}

export type StaticHostingProfile =
  "offline_self_contained" | "standalone_static";

export interface ExportExternalReferenceV1 {
  sourcePath: string;
  sanitizedUrl: string;
  origin: string;
  userinfoRedacted: boolean;
  queryRedacted: boolean;
  fragmentRedacted: boolean;
}

export interface ExportNoticeV1 {
  code: string;
  message: string;
}

export interface ExportValidationReportV1 {
  profile: StaticHostingProfile;
  localReferenceCount: number;
  externalReferences: ExportExternalReferenceV1[];
  externalOrigins: string[];
  dynamicReferenceSources: string[];
  warnings: ExportNoticeV1[];
  limitations: ExportNoticeV1[];
}

export interface ExportArchiveIdentityV1 {
  sha256: string;
  byteLength: number;
}

interface ExportReceiptBase {
  id: string;
  revisionId: string;
  sha256: string;
  byteLength: number;
  downloadUrl: string;
}

interface LegacyExportReceiptDetails {
  receiptSha256?: never;
  sourceManifestSha256?: never;
  generatedAtUtc?: never;
  options?: never;
  fileManifest?: never;
  validation?: never;
  archive?: never;
}

interface DetailedExportReceiptDetails {
  receiptSha256: string;
  sourceManifestSha256: string;
  generatedAtUtc: string;
  options: ExportOptionsV1;
  fileManifest: ExportFileManifestV1;
  validation: ExportValidationReportV1;
  archive: ExportArchiveIdentityV1;
}

/** Detailed fields are all-or-none while the pre-C8 server remains readable. */
export type ExportReceipt = ExportReceiptBase &
  (LegacyExportReceiptDetails | DetailedExportReceiptDetails);

export interface ExportResponse {
  schemaVersion: SchemaVersion;
  export: ExportReceipt;
}

export interface CreatePublicationRequest {
  schemaVersion: SchemaVersion;
  revisionId: string;
  publicLabel: string;
  title: string;
  summary: string;
  publicDecisionNote?: string;
}

export interface PublicationFile {
  path: string;
  mediaType: string;
  sha256: string;
  byteLength: number;
  utf8: string;
}

export interface PublicationDraft {
  id: string;
  revisionId: string;
  sha256: string;
  byteLength: number;
  files: PublicationFile[];
  downloadUrl: string;
  networkWrites: false;
  remotePublication: "separate_human_action";
}

export interface PublicationResponse {
  schemaVersion: SchemaVersion;
  publication: PublicationDraft;
}

export type RetainedArtifactKind = "static_export" | "publication_draft";

export interface RetainedArtifactSummary {
  kind: RetainedArtifactKind;
  id: string;
  projectId: string;
  revisionId: string;
  sha256: string;
  payloadByteLength: number;
  cleanupImpact: string;
}

export interface FailedProposalRetentionSummary {
  proposalId: string;
  reviewId: string;
  status: "failed";
  payloadByteLength: number;
  cleanupImpact: string;
}

export interface ProjectRetentionSummary {
  projectId: string;
  displayName: string;
  revisionId: string;
  acceptedManifestSha256: string;
  acceptedFileByteLength: number;
  targetCount: number;
  conversationContextCount: number;
  conversationPersistence: "memory_only";
  failedProposal: FailedProposalRetentionSummary | null;
  terminalDecisionCount: number;
  artifacts: RetainedArtifactSummary[];
  projectDeletionImpact: string;
}

export interface RetentionInventory {
  automaticGc: false;
  telemetry: "absent";
  cleanupRequiresExplicitConfirmation: true;
  projects: ProjectRetentionSummary[];
}

export interface RetentionResponse {
  schemaVersion: SchemaVersion;
  retention: RetentionInventory;
}

export type RetentionCleanupScope =
  | "conversation_context"
  | "failed_proposal"
  | "static_export"
  | "publication_draft"
  | "project";

export type RetentionCleanupRequest =
  | {
      scope: "conversation_context";
      projectId: string;
      confirmation: string;
    }
  | {
      scope: "failed_proposal";
      projectId: string;
      proposalId: string;
      reviewId: string;
      confirmation: string;
    }
  | {
      scope: "static_export" | "publication_draft";
      projectId: string;
      artifactId: string;
      expectedSha256: string;
      confirmation: string;
    }
  | {
      scope: "project";
      projectId: string;
      expectedRevisionId: string;
      expectedManifestSha256: string;
      confirmation: string;
    };

export interface RetentionCleanupResponse {
  schemaVersion: SchemaVersion;
  removed: {
    scope: RetentionCleanupScope;
    id: string;
    payloadByteLength: number;
  };
  retention: RetentionInventory;
}

export interface RecoveryPoint {
  id: string;
  kind: "versioned_backup" | "last_accepted";
  projectId: string | null;
  revisionId: string | null;
  artifactManifestSha256: string | null;
  diagnostic: {
    verified: boolean;
    code: string;
    manifestSha256: string | null;
    fileCount: number;
    totalBytes: number;
  };
  exportUrl: string | null;
}

export interface RecoveryResponse {
  schemaVersion: SchemaVersion;
  recoveryPoints: RecoveryPoint[];
}

export type ApiErrorAcceptedState = "unchanged" | "reconciliation_required";
export type ApiErrorRecoveryAction =
  "retry" | "refresh" | "reconcile" | "correct_request" | "manual_recovery";

export interface ApiErrorDetail {
  acceptedState: ApiErrorAcceptedState;
  recoveryAction: ApiErrorRecoveryAction;
}

export interface ApiErrorResponse {
  schemaVersion: SchemaVersion;
  error: {
    code: string;
    message: string;
    requestId: string;
    operationId: string;
    retryable: boolean;
    detail: ApiErrorDetail;
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

export const isProjectDisplayName = (value: unknown): value is string =>
  isBoundedString(value, 1, PROJECT_DISPLAY_NAME_LIMITS.codePoints) &&
  isBoundedUtf8String(value, 1, PROJECT_DISPLAY_NAME_LIMITS.utf8Bytes) &&
  value.normalize("NFC") === value &&
  value.trim().length > 0 &&
  !containsAbsolutePath(value) &&
  ![...value].some((character) => /\p{Cc}/u.test(character));
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

const isArtifactDisposition = (value: unknown): value is ArtifactDisposition =>
  value === "adopted_unchanged" || value === "rejected" || value === "deferred";

const isUtcTimestamp = (value: unknown): value is string =>
  isString(value) &&
  /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/.test(value) &&
  !Number.isNaN(Date.parse(value));

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

const isProjectActiveReview = (value: unknown): value is ProjectActiveReview =>
  isRecord(value) &&
  hasExactKeys(value, ["reviewId", "proposalId", "baseRevisionId", "status"]) &&
  isBoundedId(value.reviewId) &&
  isBoundedId(value.proposalId) &&
  isBoundedId(value.baseRevisionId) &&
  (value.status === "pending_review" ||
    value.status === "reconciliation_required" ||
    value.status === "failed");

const isProjectHistoryEntry = (value: unknown): value is ProjectHistoryEntry =>
  isRecord(value) &&
  hasExactKeys(
    value,
    [
      "reviewId",
      "proposalId",
      "baseRevisionId",
      "resultingRevisionId",
      "disposition",
      "artifactManifestSha256",
      "decisionReceiptSha256",
      "recordedAt",
    ],
    [
      "baseArtifactManifestSha256",
      "resultingAcceptedManifestSha256",
      "publicNote",
    ],
  ) &&
  isBoundedId(value.reviewId) &&
  isBoundedId(value.proposalId) &&
  isBoundedId(value.baseRevisionId) &&
  isBoundedId(value.resultingRevisionId) &&
  isArtifactDisposition(value.disposition) &&
  isSha256(value.artifactManifestSha256) &&
  Object.hasOwn(value, "baseArtifactManifestSha256") ===
    Object.hasOwn(value, "resultingAcceptedManifestSha256") &&
  (!Object.hasOwn(value, "baseArtifactManifestSha256") ||
    isSha256(value.baseArtifactManifestSha256)) &&
  (!Object.hasOwn(value, "resultingAcceptedManifestSha256") ||
    isSha256(value.resultingAcceptedManifestSha256)) &&
  isSha256(value.decisionReceiptSha256) &&
  isUtcTimestamp(value.recordedAt) &&
  (!Object.hasOwn(value, "publicNote") ||
    value.publicNote === null ||
    isBoundedUtf8String(value.publicNote, 1, 2_000));

export const isProject = (value: unknown): value is Project => {
  if (
    !isRecord(value) ||
    !hasExactKeys(
      value,
      [
        "id",
        "displayName",
        "revisionId",
        "acceptedManifestSha256",
        "status",
        "previewUrl",
        "files",
      ],
      ["activeReview", "history"],
    ) ||
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
    isProjectDisplayName(value.displayName) &&
    isNonEmptyString(value.revisionId) &&
    isSha256(value.acceptedManifestSha256) &&
    value.status === "ready" &&
    isScopedPreviewUrl(value.previewUrl) &&
    (!Object.hasOwn(value, "activeReview") ||
      value.activeReview === null ||
      (isProjectActiveReview(value.activeReview) &&
        value.activeReview.baseRevisionId === value.revisionId)) &&
    (!Object.hasOwn(value, "history") ||
      (isBoundedExactArray(value.history, 0, 1_024, isProjectHistoryEntry) &&
        new Set(
          value.history.map((entry) => `${entry.reviewId}:${entry.proposalId}`),
        ).size === value.history.length))
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
      "dataRetentionPolicy",
      "trainingPolicy",
      "policyNotice",
      "models",
      "modelSelection",
      "credentialSources",
    ]) ||
    !isBoundedExactArray(value.models, 0, 32, isAiProviderModelDescriptor) ||
    !isBoundedExactArray(
      value.credentialSources,
      0,
      2,
      (source) => source === "editor_session" || source === "server_configured",
    )
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
    isBoundedString(value.dataRetentionPolicy, 1, 256) &&
    isBoundedString(value.trainingPolicy, 1, 256) &&
    isBoundedString(value.policyNotice, 1, 1024) &&
    (value.availability !== "available" ||
      value.modelSelection === "open" ||
      value.models.length > 0) &&
    (value.modelSelection === "closed" || value.modelSelection === "open") &&
    new Set(value.models.map((model) => model.id)).size === value.models.length
  );
};

export const isProviderCredentialResponse = (
  value: unknown,
): value is ProviderCredentialResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "credential"]) &&
  hasVersion(value) &&
  (value.credential === null ||
    (isRecord(value.credential) &&
      hasExactKeys(value.credential, [
        "providerId",
        "credentialSourceId",
        "credentialBindingId",
        "status",
        "expiresAt",
      ]) &&
      value.credential.providerId === "openai" &&
      value.credential.credentialSourceId === "editor_session" &&
      value.credential.status === "configured_unverified" &&
      isBoundedString(value.credential.credentialBindingId, 1, 128) &&
      isBoundedString(value.credential.expiresAt, 1, 64)));

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
    !hasExactKeys(
      capabilities,
      [
        "targetKinds",
        "dispositions",
        "singleProposalPerProject",
        "importAvailable",
        "aiProviders",
        "limits",
      ],
      ["operatingMode", "recoveryPointCount"],
    )
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
    (!Object.hasOwn(capabilities, "operatingMode") ||
      capabilities.operatingMode === "normal" ||
      capabilities.operatingMode === "read_only_recovery") &&
    (!Object.hasOwn(capabilities, "recoveryPointCount") ||
      isSafeIntegerRange(
        capabilities.recoveryPointCount,
        0,
        RECOVERY_CONTRACT_LIMITS.maxPoints,
      )) &&
    Object.hasOwn(capabilities, "operatingMode") ===
      Object.hasOwn(capabilities, "recoveryPointCount") &&
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

export const isUpdateProjectMetadataRequest = (
  value: unknown,
): value is UpdateProjectMetadataRequest =>
  isRecord(value) &&
  hasExactKeys(value, [
    "schemaVersion",
    "expectedDisplayName",
    "displayName",
  ]) &&
  hasVersion(value) &&
  isProjectDisplayName(value.expectedDisplayName) &&
  isProjectDisplayName(value.displayName);

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
  (!Object.hasOwn(value, "credentialSourceId") ||
    value.credentialSourceId === "editor_session" ||
    value.credentialSourceId === "server_configured") &&
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
  (hasExactKeys(value, [
    "providerId",
    "adapterVersion",
    "requestedModel",
    "external",
    "credentialSourceId",
    "credentialBindingId",
  ]) ||
    hasExactKeys(value, [
      "providerId",
      "adapterVersion",
      "requestedModel",
      "external",
    ])) &&
  isBoundedString(value.providerId, 1, AI_CONTRACT_LIMITS.providerIdLength) &&
  isBoundedString(
    value.adapterVersion,
    1,
    AI_CONTRACT_LIMITS.adapterVersionLength,
  ) &&
  isBoundedString(value.requestedModel, 1, AI_CONTRACT_LIMITS.modelIdLength) &&
  (!Object.hasOwn(value, "credentialSourceId") ||
    isBoundedString(value.credentialSourceId, 1, 64)) &&
  (!Object.hasOwn(value, "credentialBindingId") ||
    isBoundedString(value.credentialBindingId, 1, 128)) &&
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

const isProposal = (value: unknown): value is Proposal => {
  if (
    !isRecord(value) ||
    !hasExactKeys(
      value,
      [
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
      ],
      ["target", "targetResolution", "instruction", "derivedFromProposalId"],
    )
  ) {
    return false;
  }
  const hasResumeContext = ["target", "targetResolution", "instruction"].map(
    (key) => Object.hasOwn(value, key),
  );
  if (hasResumeContext.some(Boolean) && !hasResumeContext.every(Boolean)) {
    return false;
  }
  return (
    isNonEmptyString(value.id) &&
    isNonEmptyString(value.reviewId) &&
    isNonEmptyString(value.baseRevisionId) &&
    value.status === "pending_review" &&
    isString(value.summary) &&
    isSha256(value.artifactManifestSha256) &&
    isSha256(value.reviewContextSha256) &&
    isSha256(value.providerContextSha256) &&
    isSha256(value.changeSetSha256) &&
    isChangeSetV1(value.changeSet) &&
    value.changeSet.baseRevisionId === value.baseRevisionId &&
    isAiProviderAttributionV1(value.attribution) &&
    value.sourceAttribution === "caller_supplied_ai_attributed" &&
    value.executionVerified === false &&
    isScopedPreviewUrl(value.previewUrl) &&
    isExactArrayOf(value.changes, isProposalChange) &&
    isString(value.unifiedDiff) &&
    isProposalValidation(value.validation) &&
    (!hasResumeContext[0] ||
      (isTargetV1(value.target) &&
        value.target.captureRevisionId === value.baseRevisionId &&
        isTargetResolverResultV1(value.targetResolution) &&
        value.targetResolution.targetId === value.target.targetId &&
        value.targetResolution.captureRevisionId ===
          value.target.captureRevisionId &&
        isBoundedUtf8String(value.instruction, 1, 2_000))) &&
    (!Object.hasOwn(value, "derivedFromProposalId") ||
      value.derivedFromProposalId === null ||
      isBoundedId(value.derivedFromProposalId))
  );
};

export const isProposalResponse = (value: unknown): value is ProposalResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "proposal"]) &&
  hasVersion(value) &&
  isProposal(value.proposal);

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
    isArtifactDisposition(decision.disposition) &&
    decision.status === "committed" &&
    isNonEmptyString(decision.revisionId) &&
    isSha256(decision.artifactManifestSha256)
  );
};

const isReviewDecision = (value: unknown): value is ReviewDecision =>
  isRecord(value) &&
  hasExactKeys(value, [
    "proposalId",
    "disposition",
    "revisionId",
    "artifactManifestSha256",
  ]) &&
  isBoundedId(value.proposalId) &&
  isArtifactDisposition(value.disposition) &&
  isBoundedId(value.revisionId) &&
  isSha256(value.artifactManifestSha256);

export const isReviewResponse = (value: unknown): value is ReviewResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "review"]) ||
    !hasVersion(value) ||
    !isRecord(value.review) ||
    !hasExactKeys(value.review, [
      "reviewId",
      "projectId",
      "proposalId",
      "status",
      "reconciliationRequired",
      "proposal",
      "decision",
    ])
  ) {
    return false;
  }
  const review = value.review;
  if (
    !isBoundedId(review.reviewId) ||
    !isBoundedId(review.projectId) ||
    !isBoundedId(review.proposalId) ||
    ![
      "pending_review",
      "adopted",
      "rejected",
      "deferred",
      "reconciliation_required",
      "failed",
    ].includes(String(review.status)) ||
    typeof review.reconciliationRequired !== "boolean"
  ) {
    return false;
  }
  if (
    review.status === "pending_review" ||
    review.status === "reconciliation_required" ||
    review.status === "failed"
  ) {
    return (
      review.reconciliationRequired ===
        (review.status === "reconciliation_required") &&
      isProposal(review.proposal) &&
      review.proposal.reviewId === review.reviewId &&
      review.proposal.id === review.proposalId &&
      review.decision === null
    );
  }
  const expectedDisposition: ArtifactDisposition =
    review.status === "adopted"
      ? "adopted_unchanged"
      : review.status === "rejected"
        ? "rejected"
        : "deferred";
  return (
    review.reconciliationRequired === false &&
    review.proposal === null &&
    isReviewDecision(review.decision) &&
    review.decision.proposalId === review.proposalId &&
    review.decision.disposition === expectedDisposition
  );
};

const isSchemaIdentityV1 = (value: unknown): value is SchemaIdentityV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["name", "version"]) &&
  isBoundedString(value.name, 1, 128) &&
  isSafeIntegerRange(value.version, 1, 65_535);

const isExportOptionsV1 = (value: unknown): value is ExportOptionsV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "entryPoint",
    "basePathProfile",
    "externalAssetPolicy",
    "artifactFormat",
  ]) &&
  isContractRelativePath(value.entryPoint) &&
  value.basePathProfile === "relative_static_http" &&
  value.externalAssetPolicy === "report" &&
  value.artifactFormat === "zip_stored_v1";

const isExportFileManifestEntryV1 = (
  value: unknown,
): value is ExportFileManifestEntryV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "mediaType", "byteLength", "sha256"]) &&
  isContractRelativePath(value.path) &&
  isBoundedString(value.mediaType, 1, 256) &&
  isNonNegativeInteger(value.byteLength) &&
  isSha256(value.sha256);

const isExportFileManifestV1 = (
  value: unknown,
): value is ExportFileManifestV1 => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schema", "sha256", "totalByteLength", "files"]) ||
    !isSchemaIdentityV1(value.schema) ||
    value.schema.name !== "org.synapsegit-lp-studio.export-file-manifest" ||
    value.schema.version !== 1 ||
    !isSha256(value.sha256) ||
    !isNonNegativeInteger(value.totalByteLength) ||
    !isBoundedExactArray(value.files, 1, 4_096, isExportFileManifestEntryV1)
  ) {
    return false;
  }
  const paths = value.files.map((file) => file.path);
  const total = value.files.reduce((sum, file) => sum + file.byteLength, 0);
  return (
    new Set(paths).size === paths.length &&
    total === value.totalByteLength &&
    Number.isSafeInteger(total)
  );
};

const isExportNoticeV1 = (value: unknown): value is ExportNoticeV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["code", "message"]) &&
  isBoundedString(value.code, 1, 128) &&
  isBoundedString(value.message, 1, 2_048);

const sanitizedExternalIdentity = (
  sanitizedUrl: string,
  origin: string,
): boolean => {
  const protocolRelative = sanitizedUrl.startsWith("//");
  let parsed: URL;
  try {
    parsed = new URL(protocolRelative ? `https:${sanitizedUrl}` : sanitizedUrl);
  } catch {
    return false;
  }
  if (
    !["http:", "https:"].includes(parsed.protocol) ||
    parsed.username.length > 0 ||
    parsed.password.length > 0 ||
    parsed.search.length > 0 ||
    parsed.hash.length > 0
  ) {
    return false;
  }
  const expectedOrigin = protocolRelative
    ? parsed.origin.replace(/^https:/, "")
    : parsed.origin;
  return expectedOrigin === origin;
};

const isExportExternalReferenceV1 = (
  value: unknown,
): value is ExportExternalReferenceV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "sourcePath",
    "sanitizedUrl",
    "origin",
    "userinfoRedacted",
    "queryRedacted",
    "fragmentRedacted",
  ]) &&
  isContractRelativePath(value.sourcePath) &&
  isBoundedString(value.sanitizedUrl, 1, 4_096) &&
  isBoundedString(value.origin, 1, 512) &&
  sanitizedExternalIdentity(value.sanitizedUrl, value.origin) &&
  typeof value.userinfoRedacted === "boolean" &&
  typeof value.queryRedacted === "boolean" &&
  typeof value.fragmentRedacted === "boolean";

const isExportValidationReportV1 = (
  value: unknown,
): value is ExportValidationReportV1 =>
  isRecord(value) &&
  hasExactKeys(value, [
    "profile",
    "localReferenceCount",
    "externalReferences",
    "externalOrigins",
    "dynamicReferenceSources",
    "warnings",
    "limitations",
  ]) &&
  (value.profile === "offline_self_contained" ||
    value.profile === "standalone_static") &&
  isNonNegativeInteger(value.localReferenceCount) &&
  isBoundedExactArray(
    value.externalReferences,
    0,
    4_096,
    isExportExternalReferenceV1,
  ) &&
  isBoundedExactArray(
    value.externalOrigins,
    0,
    4_096,
    (origin): origin is string => isBoundedString(origin, 1, 512),
  ) &&
  new Set(value.externalOrigins).size === value.externalOrigins.length &&
  isBoundedExactArray(
    value.dynamicReferenceSources,
    0,
    4_096,
    (path): path is string => isContractRelativePath(path),
  ) &&
  new Set(value.dynamicReferenceSources).size ===
    value.dynamicReferenceSources.length &&
  isBoundedExactArray(value.warnings, 0, 256, isExportNoticeV1) &&
  isBoundedExactArray(value.limitations, 1, 256, isExportNoticeV1) &&
  (value.profile !== "offline_self_contained" ||
    value.externalReferences.length === 0) &&
  (value.profile !== "standalone_static" ||
    value.externalReferences.length > 0);

const isExportArchiveIdentityV1 = (
  value: unknown,
): value is ExportArchiveIdentityV1 =>
  isRecord(value) &&
  hasExactKeys(value, ["sha256", "byteLength"]) &&
  isSha256(value.sha256) &&
  isNonNegativeInteger(value.byteLength);

export const isExportResponse = (value: unknown): value is ExportResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "export"]) ||
    !hasVersion(value) ||
    !isRecord(value.export) ||
    !hasExactKeys(
      value.export,
      ["id", "revisionId", "sha256", "byteLength", "downloadUrl"],
      [
        "receiptSha256",
        "sourceManifestSha256",
        "generatedAtUtc",
        "options",
        "fileManifest",
        "validation",
        "archive",
      ],
    )
  ) {
    return false;
  }
  const receipt = value.export;
  if (!(
    isNonEmptyString(receipt.id) &&
    isNonEmptyString(receipt.revisionId) &&
    isSha256(receipt.sha256) &&
    isNonNegativeInteger(receipt.byteLength) &&
    isNonEmptyString(receipt.downloadUrl)
  )) {
    return false;
  }
  const detailKeys = [
    "receiptSha256",
    "sourceManifestSha256",
    "generatedAtUtc",
    "options",
    "fileManifest",
    "validation",
    "archive",
  ];
  const detailPresence = detailKeys.map((key) => Object.hasOwn(receipt, key));
  if (!detailPresence.some(Boolean)) return true;
  if (
    !detailPresence.every(Boolean) ||
    !isSha256(receipt.receiptSha256) ||
    !isSha256(receipt.sourceManifestSha256) ||
    !isUtcTimestamp(receipt.generatedAtUtc) ||
    !isExportOptionsV1(receipt.options) ||
    !isExportFileManifestV1(receipt.fileManifest) ||
    !isExportValidationReportV1(receipt.validation) ||
    !isExportArchiveIdentityV1(receipt.archive)
  ) {
    return false;
  }
  const options = receipt.options;
  const fileManifest = receipt.fileManifest;
  const archive = receipt.archive;
  return (
    fileManifest.files.some((file) => file.path === options.entryPoint) &&
    archive.sha256 === receipt.sha256 &&
    archive.byteLength === receipt.byteLength
  );
};

const isPublicationFile = (value: unknown): value is PublicationFile =>
  isRecord(value) &&
  hasExactKeys(value, ["path", "mediaType", "sha256", "byteLength", "utf8"]) &&
  isContractRelativePath(value.path) &&
  isBoundedString(value.mediaType, 1, 256) &&
  isSha256(value.sha256) &&
  isNonNegativeInteger(value.byteLength) &&
  isString(value.utf8) &&
  new TextEncoder().encode(value.utf8).byteLength === value.byteLength;

export const isPublicationResponse = (
  value: unknown,
): value is PublicationResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "publication"]) ||
    !hasVersion(value) ||
    !isRecord(value.publication) ||
    !hasExactKeys(value.publication, [
      "id",
      "revisionId",
      "sha256",
      "byteLength",
      "files",
      "downloadUrl",
      "networkWrites",
      "remotePublication",
    ])
  ) {
    return false;
  }
  const publication = value.publication;
  if (
    !isBoundedId(publication.id) ||
    !isBoundedId(publication.revisionId) ||
    !isSha256(publication.sha256) ||
    !isNonNegativeInteger(publication.byteLength) ||
    !isBoundedExactArray(publication.files, 1, 64, isPublicationFile) ||
    !isNonEmptyString(publication.downloadUrl) ||
    publication.networkWrites !== false ||
    publication.remotePublication !== "separate_human_action"
  ) {
    return false;
  }
  const paths = publication.files.map((file) => file.path);
  return (
    new Set(paths).size === paths.length &&
    ["projection.json", "story.md", "index.html", "manifest.json"].every(
      (required) => paths.includes(required),
    )
  );
};

const isRetainedArtifactSummary = (
  value: unknown,
): value is RetainedArtifactSummary =>
  isRecord(value) &&
  hasExactKeys(value, [
    "kind",
    "id",
    "projectId",
    "revisionId",
    "sha256",
    "payloadByteLength",
    "cleanupImpact",
  ]) &&
  (value.kind === "static_export" || value.kind === "publication_draft") &&
  isBoundedId(value.id) &&
  isBoundedId(value.projectId) &&
  isBoundedId(value.revisionId) &&
  isSha256(value.sha256) &&
  isNonNegativeInteger(value.payloadByteLength) &&
  isBoundedString(value.cleanupImpact, 1, 1_024);

const isFailedProposalRetentionSummary = (
  value: unknown,
): value is FailedProposalRetentionSummary =>
  isRecord(value) &&
  hasExactKeys(value, [
    "proposalId",
    "reviewId",
    "status",
    "payloadByteLength",
    "cleanupImpact",
  ]) &&
  isBoundedId(value.proposalId) &&
  isBoundedId(value.reviewId) &&
  value.status === "failed" &&
  isNonNegativeInteger(value.payloadByteLength) &&
  isBoundedString(value.cleanupImpact, 1, 1_024);

const isProjectRetentionSummary = (
  value: unknown,
): value is ProjectRetentionSummary => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "projectId",
      "displayName",
      "revisionId",
      "acceptedManifestSha256",
      "acceptedFileByteLength",
      "targetCount",
      "conversationContextCount",
      "conversationPersistence",
      "failedProposal",
      "terminalDecisionCount",
      "artifacts",
      "projectDeletionImpact",
    ]) ||
    !isBoundedId(value.projectId) ||
    !isBoundedString(value.displayName, 1, 256) ||
    !isBoundedId(value.revisionId) ||
    !isSha256(value.acceptedManifestSha256) ||
    !isNonNegativeInteger(value.acceptedFileByteLength) ||
    !isSafeIntegerRange(value.targetCount, 0, 32) ||
    !isSafeIntegerRange(value.conversationContextCount, 0, 32) ||
    value.conversationPersistence !== "memory_only" ||
    (value.failedProposal !== null &&
      !isFailedProposalRetentionSummary(value.failedProposal)) ||
    !isSafeIntegerRange(value.terminalDecisionCount, 0, 1_024) ||
    !isBoundedExactArray(value.artifacts, 0, 32, isRetainedArtifactSummary) ||
    !isBoundedString(value.projectDeletionImpact, 1, 2_048)
  ) {
    return false;
  }
  return (
    value.artifacts.every(
      (artifact) => artifact.projectId === value.projectId,
    ) &&
    new Set(
      value.artifacts.map((artifact) => `${artifact.kind}:${artifact.id}`),
    ).size === value.artifacts.length
  );
};

const isRetentionInventory = (value: unknown): value is RetentionInventory =>
  isRecord(value) &&
  hasExactKeys(value, [
    "automaticGc",
    "telemetry",
    "cleanupRequiresExplicitConfirmation",
    "projects",
  ]) &&
  value.automaticGc === false &&
  value.telemetry === "absent" &&
  value.cleanupRequiresExplicitConfirmation === true &&
  isBoundedExactArray(value.projects, 0, 8, isProjectRetentionSummary) &&
  new Set(value.projects.map((project) => project.projectId)).size ===
    value.projects.length;

export const isRetentionResponse = (
  value: unknown,
): value is RetentionResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "retention"]) &&
  hasVersion(value) &&
  isRetentionInventory(value.retention);

export const isRetentionCleanupResponse = (
  value: unknown,
): value is RetentionCleanupResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "removed", "retention"]) &&
  hasVersion(value) &&
  isRecord(value.removed) &&
  hasExactKeys(value.removed, ["scope", "id", "payloadByteLength"]) &&
  [
    "conversation_context",
    "failed_proposal",
    "static_export",
    "publication_draft",
    "project",
  ].includes(String(value.removed.scope)) &&
  isBoundedId(value.removed.id) &&
  isNonNegativeInteger(value.removed.payloadByteLength) &&
  isRetentionInventory(value.retention);

const isRecoveryPoint = (value: unknown): value is RecoveryPoint => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, [
      "id",
      "kind",
      "projectId",
      "revisionId",
      "artifactManifestSha256",
      "diagnostic",
      "exportUrl",
    ]) ||
    !isBoundedId(value.id) ||
    (value.kind !== "versioned_backup" && value.kind !== "last_accepted") ||
    (value.projectId !== null && !isBoundedId(value.projectId)) ||
    (value.revisionId !== null && !isBoundedId(value.revisionId)) ||
    (value.artifactManifestSha256 !== null &&
      !isSha256(value.artifactManifestSha256)) ||
    !isRecord(value.diagnostic) ||
    !hasExactKeys(value.diagnostic, [
      "verified",
      "code",
      "manifestSha256",
      "fileCount",
      "totalBytes",
    ]) ||
    typeof value.diagnostic.verified !== "boolean" ||
    !isBoundedString(value.diagnostic.code, 1, 128) ||
    (value.diagnostic.manifestSha256 !== null &&
      !isSha256(value.diagnostic.manifestSha256)) ||
    !isSafeIntegerRange(
      value.diagnostic.fileCount,
      0,
      RECOVERY_CONTRACT_LIMITS.maxSnapshotFiles,
    ) ||
    !isNonNegativeInteger(value.diagnostic.totalBytes) ||
    (value.exportUrl !== null &&
      (!isNonEmptyString(value.exportUrl) ||
        !value.exportUrl.startsWith("/api/v1/recovery/") ||
        !value.exportUrl.endsWith("/export")))
  ) {
    return false;
  }
  return (
    value.diagnostic.verified === (value.exportUrl !== null) &&
    (!value.diagnostic.verified ||
      (value.projectId !== null &&
        value.revisionId !== null &&
        value.artifactManifestSha256 !== null &&
        value.diagnostic.manifestSha256 !== null))
  );
};

export const isRecoveryResponse = (value: unknown): value is RecoveryResponse =>
  isRecord(value) &&
  hasExactKeys(value, ["schemaVersion", "recoveryPoints"]) &&
  hasVersion(value) &&
  isBoundedExactArray(
    value.recoveryPoints,
    0,
    RECOVERY_CONTRACT_LIMITS.maxPoints,
    isRecoveryPoint,
  ) &&
  new Set(value.recoveryPoints.map((point) => point.id)).size ===
    value.recoveryPoints.length;

export const isApiErrorResponse = (
  value: unknown,
): value is ApiErrorResponse => {
  if (
    !isRecord(value) ||
    !hasExactKeys(value, ["schemaVersion", "error"]) ||
    !hasVersion(value) ||
    !isRecord(value.error) ||
    !hasExactKeys(value.error, [
      "code",
      "message",
      "requestId",
      "operationId",
      "retryable",
      "detail",
    ])
  ) {
    return false;
  }
  const error = value.error;
  return (
    isNonEmptyString(error.code) &&
    isNonEmptyString(error.message) &&
    isBoundedId(error.requestId) &&
    isBoundedId(error.operationId) &&
    typeof error.retryable === "boolean" &&
    isRecord(error.detail) &&
    hasExactKeys(error.detail, ["acceptedState", "recoveryAction"]) &&
    (error.detail.acceptedState === "unchanged" ||
      error.detail.acceptedState === "reconciliation_required") &&
    (error.detail.recoveryAction === "retry" ||
      error.detail.recoveryAction === "refresh" ||
      error.detail.recoveryAction === "reconcile" ||
      error.detail.recoveryAction === "correct_request" ||
      error.detail.recoveryAction === "manual_recovery")
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
