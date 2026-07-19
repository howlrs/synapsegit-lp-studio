#![forbid(unsafe_code)]

mod ai_provider;
mod change_set;
mod storage;

use axum::body::Body;
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, HOST, ORIGIN,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use synapse_artifact::{
    ArtifactDecisionOptions, ArtifactDisposition, ArtifactLimits, ArtifactManifestEntry,
    ArtifactSourceAttribution, PendingArtifactProposal, RegularFileManifest,
    TrustedArtifactProjectConfig, artifact_manifest_sha256, begin_artifact_proposal,
    decide_artifact_proposal, review_context_sha256,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use ai_provider::{
    OpenAiConfig, ProviderAttribution, ProviderBinding, ProviderDescriptor, ProviderError,
    ProviderRequest, ProviderSelectionError, bind_provider, execute_provider, provider_descriptors,
};
use change_set::{
    AppliedChangeKind, AppliedChangeSet, ChangeSetError, ChangeSetV1, StaticCheck,
    StaticCheckStatus, count_matching_start_tags, parse_and_apply_change_set,
};
use storage::{
    ImportExcluded, ImportIncluded, MAX_DEPTH as STORAGE_MAX_DEPTH,
    MAX_FILE_BYTES as STORAGE_MAX_FILE_BYTES, MAX_FILES as STORAGE_MAX_FILES,
    MAX_PATH_BYTES as STORAGE_MAX_PATH_BYTES, MAX_TOTAL_BYTES as STORAGE_MAX_TOTAL_BYTES,
    ManagedStorage, StorageError, scan_import,
};

pub const API_VERSION: &str = "v1";
pub const SCHEMA_VERSION: &str = "1";

const SESSION_TTL_SECONDS: i64 = 30 * 60;
const APPROVAL_TTL_SECONDS: i64 = 5 * 60;
const IMPORT_PREVIEW_TTL_SECONDS: i64 = 10 * 60;
const MAX_INSTRUCTION_BYTES: usize = 2_000;
const MAX_RATIONALE_BYTES: usize = 2_000;
const MAX_PREVIEW_HTML_BYTES: usize = 2 * 1024 * 1024;
const TARGET_SCHEMA_VERSION: u8 = 1;
const TARGET_RESOLVER_VERSION: u8 = 1;
const MAX_TARGET_LABEL_LENGTH: usize = 256;
const MAX_TARGET_QUOTE_LENGTH: usize = 1_024;
const MAX_TARGET_CONTEXT_LENGTH: usize = 256;
const MAX_TARGET_ANCHOR_LENGTH: usize = 2_048;
const MAX_TARGET_CLASS_TOKEN_LENGTH: usize = 128;
const MAX_SESSIONS: usize = 32;
const MAX_PROJECTS: usize = 8;
const MAX_TARGETS_PER_PROJECT: usize = 32;
const MAX_CONTEXTS_PER_PROJECT: usize = 32;
const MAX_PENDING_APPROVALS: usize = 32;
const MAX_EXPORTS: usize = 16;
const MAX_RETAINED_EXPORT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CONTEXT_FILES: usize = 10;
const MAX_CONTEXT_FILE_BYTES: usize = 512 * 1024;
const MAX_CONTEXT_TOTAL_BYTES: usize = 2 * 1024 * 1024;

const BLANK_INDEX: &str = include_str!("../../../templates/blank/index.html");
const BLANK_STYLES: &str = include_str!("../../../templates/blank/styles.css");
const PREVIEW_TARGET_RUNTIME: &str = include_str!("preview_target_runtime.js");

#[derive(Clone)]
pub struct ServerConfig {
    pub editor_origin: String,
    pub editor_host: String,
    pub preview_origin: String,
    pub state_root: PathBuf,
    pub web_dist: PathBuf,
    pub import_root: Option<PathBuf>,
    openai: Option<OpenAiConfig>,
}

impl ServerConfig {
    pub fn new(
        editor_origin: impl Into<String>,
        editor_host: impl Into<String>,
        preview_origin: impl Into<String>,
        state_root: impl Into<PathBuf>,
        web_dist: impl Into<PathBuf>,
    ) -> Self {
        Self {
            editor_origin: editor_origin.into(),
            editor_host: editor_host.into(),
            preview_origin: preview_origin.into(),
            state_root: state_root.into(),
            web_dist: web_dist.into(),
            import_root: None,
            openai: None,
        }
    }

    pub fn with_import_root(mut self, import_root: Option<PathBuf>) -> Self {
        self.import_root = import_root;
        self
    }

    pub fn with_openai(mut self, api_key: String, model: String) -> Result<Self, std::io::Error> {
        self.openai =
            Some(OpenAiConfig::new(api_key, model).map_err(|message| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
            })?);
        Ok(self)
    }
}

#[derive(Clone)]
pub struct StudioState(Arc<InnerState>);

struct InnerState {
    config: ServerConfig,
    preview_port: u16,
    preview_secret: [u8; 32],
    storage: ManagedStorage,
    store: Mutex<Store>,
}

impl fmt::Debug for StudioState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StudioState(<redacted>)")
    }
}

impl StudioState {
    pub fn new(config: ServerConfig) -> std::io::Result<Self> {
        let preview_port = validate_preview_scope_origin(&config.preview_origin)?;
        let mut preview_secret = [0_u8; 32];
        getrandom::fill(&mut preview_secret)
            .map_err(|_| std::io::Error::other("could not initialize preview isolation"))?;
        std::fs::create_dir_all(&config.state_root)?;
        let (storage, persisted) =
            ManagedStorage::open(&config.state_root).map_err(storage_initialization_error)?;
        if persisted.len() > MAX_PROJECTS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "managed storage contains too many projects",
            ));
        }
        let mut store = Store::default();
        for persisted in persisted {
            let calculated_artifact_manifest_sha256 =
                manifest_digest(&persisted.files).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted artifact manifest does not validate",
                    )
                })?;
            if calculated_artifact_manifest_sha256 != persisted.artifact_manifest_sha256 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted artifact manifest digest does not match its files",
                ));
            }
            let mut targets = HashMap::new();
            for (stored_target_id, bytes) in persisted.targets {
                let target: TargetRecord = serde_json::from_slice(&bytes).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted Target metadata does not validate",
                    )
                })?;
                validate_target_shape(&target).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted Target metadata does not validate",
                    )
                })?;
                if target.target_id != stored_target_id
                    || serde_json::to_vec(&target).ok().as_deref() != Some(bytes.as_slice())
                    || targets.insert(target.target_id.clone(), target).is_some()
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted Target metadata is not canonical",
                    ));
                }
            }
            store.projects.insert(
                persisted.id.clone(),
                Project {
                    id: persisted.id,
                    display_name: persisted.display_name,
                    revision_id: persisted.revision_id,
                    accepted_files: persisted.files,
                    accepted_manifest_sha256: persisted.artifact_manifest_sha256,
                    targets,
                    contexts: HashMap::new(),
                    proposal: None,
                },
            );
        }
        Ok(Self(Arc::new(InnerState {
            config,
            preview_port,
            preview_secret,
            storage,
            store: Mutex::new(store),
        })))
    }

    fn store(&self) -> Result<MutexGuard<'_, Store>, ApiError> {
        self.0.store.lock().map_err(|_| ApiError::internal())
    }
}

#[derive(Default)]
struct Store {
    sessions: HashMap<[u8; 32], Session>,
    projects: HashMap<String, Project>,
    reviews: HashMap<String, String>,
    approvals: HashMap<[u8; 32], ApprovalGrant>,
    exports: HashMap<String, ExportArtifact>,
    import_previews: HashMap<String, ImportPreviewRecord>,
    active_attempts: HashMap<String, String>,
}

struct ImportPreviewRecord {
    manifest_sha256: String,
    session_id: String,
    expires_at: i64,
}

struct Session {
    id: String,
    expires_at: i64,
}

struct Project {
    id: String,
    display_name: String,
    revision_id: String,
    accepted_files: BTreeMap<String, Vec<u8>>,
    accepted_manifest_sha256: String,
    targets: HashMap<String, TargetRecord>,
    contexts: HashMap<String, ContextRecord>,
    proposal: Option<ProposalRecord>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetCaptureSource {
    Accepted,
    Proposal,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetKind {
    Page,
    Block,
    Element,
    Text,
    Point,
    Region,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetViewport {
    css_width: f64,
    css_height: f64,
    scroll_x: f64,
    scroll_y: f64,
    device_pixel_ratio: f64,
    visual_viewport_scale: f64,
    preview_scale: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetDocument {
    css_width: f64,
    css_height: f64,
    layout_epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetPoint {
    x: f64,
    y: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetGeometry {
    document_css_pixel_rect: TargetRect,
    viewport_css_pixel_rect: TargetRect,
    viewport_normalized_rect: TargetRect,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetPointCapture {
    document_css_pixel: TargetPoint,
    viewport_normalized: TargetPoint,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ElementAnchor {
    tag_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    unique_element_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accessible_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dom_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class_tokens: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ancestor_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sibling_index: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TextAnchor {
    exact: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_offset: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_offset: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetLayoutMode {
    Flow,
    Flex,
    Grid,
    Positioned,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegionAnchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    containing_block: Option<ElementAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_visible_sibling: Option<ElementAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_visible_sibling: Option<ElementAnchor>,
    layout_mode: TargetLayoutMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetBlockSource {
    Semantic,
    Landmark,
    Heuristic,
    User,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetBlock {
    source: TargetBlockSource,
    level: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetRecord {
    schema_version: u8,
    target_id: String,
    capture_revision_id: String,
    capture_source: TargetCaptureSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_proposal_id: Option<String>,
    page_path: String,
    kind: TargetKind,
    label: String,
    viewport: TargetViewport,
    document: TargetDocument,
    #[serde(skip_serializing_if = "Option::is_none")]
    geometry: Option<TargetGeometry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    point: Option<TargetPointCapture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    element_anchor: Option<ElementAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text_anchor: Option<TextAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    region_anchor: Option<RegionAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    block: Option<TargetBlock>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetResolutionStatus {
    Resolved,
    Ambiguous,
    Detached,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetResolutionCandidate {
    candidate_id: String,
    score: f64,
    reasons: Vec<&'static str>,
    summary: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetResolution {
    schema_version: u8,
    resolver_version: u8,
    target_id: String,
    capture_revision_id: String,
    resolved_revision_id: String,
    status: TargetResolutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_candidate_id: Option<String>,
    candidates: Vec<TargetResolutionCandidate>,
}

struct ContextRecord {
    id: String,
    attempt_id: String,
    revision_id: String,
    target_id: String,
    target_resolution_id: String,
    instruction: String,
    provider: ProviderBinding,
    manifest: ContextManifestDto,
    canonical_json: String,
    sha256: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextManifestEntryDto {
    path: String,
    media_type: String,
    purpose: &'static str,
    source_byte_length: usize,
    included_byte_length: usize,
    start_line: usize,
    end_line: usize,
    sha256: String,
    estimated_tokens: usize,
    redacted: bool,
    truncated: bool,
    redactions: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextManifestDto {
    entries: Vec<ContextManifestEntryDto>,
    total_included_bytes: usize,
    estimated_tokens: usize,
    screenshot_included: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UntrustedSiteContent {
    path: String,
    media_type: String,
    source_sha256: String,
    included_sha256: String,
    content: String,
    quoted_untrusted_data: bool,
}

struct ProposalRecord {
    id: String,
    review_id: String,
    base_revision_id: String,
    artifact_manifest_sha256: String,
    review_context_sha256: String,
    provider_context_sha256: String,
    change_set_sha256: String,
    change_set: ChangeSetV1,
    attribution: ProviderAttribution,
    summary: String,
    changes: Vec<ProposalChangeDto>,
    unified_diff: String,
    validation: ProposalValidationDto,
    proposed_files: BTreeMap<String, Vec<u8>>,
    pending: PendingArtifactProposal,
    status: ProposalStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProposalStatus {
    PendingReview,
    Committed,
}

#[derive(Clone, Eq, PartialEq)]
struct ApprovalBinding {
    session_id: String,
    project_id: String,
    review_id: String,
    proposal_id: String,
    proposal_digest: String,
    expected_revision_id: String,
    disposition: DispositionDto,
    intent_id: String,
}

struct ApprovalGrant {
    binding: ApprovalBinding,
    expires_at: i64,
}

struct ExportArtifact {
    project_id: String,
    revision_id: String,
    sha256: String,
    bytes: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Versioned<T: Serialize> {
    schema_version: &'static str,
    #[serde(flatten)]
    payload: T,
}

impl<T: Serialize> Versioned<T> {
    fn new(payload: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            payload,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    schema_version: &'static str,
    status: &'static str,
    role: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapPayload {
    api_version: &'static str,
    session: SessionDto,
    editor_origin: String,
    preview_origin: String,
    capabilities: CapabilitiesDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionDto {
    token: String,
    expires_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilitiesDto {
    target_kinds: [&'static str; 6],
    dispositions: [&'static str; 3],
    single_proposal_per_project: bool,
    import_available: bool,
    ai_providers: Vec<ProviderDescriptor>,
    limits: StorageLimitsDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StorageLimitsDto {
    max_files: usize,
    max_total_bytes: usize,
    max_file_bytes: usize,
    max_path_bytes: usize,
    max_depth: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectDto {
    id: String,
    display_name: String,
    revision_id: String,
    status: &'static str,
    preview_url: String,
    accepted_manifest_sha256: String,
    files: Vec<ProjectFileDto>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectFileDto {
    path: String,
    byte_length: usize,
}

#[derive(Serialize)]
struct ProjectPayload {
    project: ProjectDto,
}

#[derive(Serialize)]
struct ProjectsPayload {
    projects: Vec<ProjectDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateProjectRequest {
    schema_version: String,
    template: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateImportPreviewRequest {
    schema_version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfirmImportRequest {
    schema_version: String,
    expected_manifest_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportPreviewDto {
    id: String,
    display_name: String,
    manifest_sha256: String,
    total_bytes: usize,
    entry_point: Option<String>,
    included: Vec<ImportIncluded>,
    excluded: Vec<ImportExcluded>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportPreviewPayload {
    import_preview: ImportPreviewDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateTargetRequest {
    schema_version: String,
    target: TargetRecord,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TargetPayload {
    target: TargetRecord,
    resolution: TargetResolution,
    resolution_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateContextRequest {
    schema_version: String,
    attempt_id: String,
    revision_id: String,
    target_id: String,
    resolution_id: String,
    provider_id: String,
    requested_model: String,
    instruction: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextDto {
    id: String,
    attempt_id: String,
    revision_id: String,
    target_id: String,
    target_resolution_id: String,
    instruction: String,
    provider_id: String,
    requested_model: String,
    provider: ProviderBinding,
    manifest: ContextManifestDto,
    canonical_json: String,
    sha256: String,
}

#[derive(Serialize)]
struct ContextPayload {
    context: ContextDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateProposalRequest {
    schema_version: String,
    context_id: String,
    context_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalDto {
    id: String,
    review_id: String,
    base_revision_id: String,
    status: &'static str,
    summary: String,
    artifact_manifest_sha256: String,
    review_context_sha256: String,
    provider_context_sha256: String,
    change_set_sha256: String,
    change_set: ChangeSetV1,
    attribution: ProviderAttribution,
    source_attribution: &'static str,
    execution_verified: bool,
    preview_url: String,
    changes: Vec<ProposalChangeDto>,
    unified_diff: String,
    validation: ProposalValidationDto,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalChangeDto {
    path: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    from_path: Option<String>,
}

#[derive(Clone, Serialize)]
struct ProposalValidationDto {
    status: &'static str,
    checks: Vec<ProposalValidationCheckDto>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalValidationCheckDto {
    id: String,
    label: String,
    status: &'static str,
    message: String,
    blocking: bool,
    destinations: Vec<String>,
}

#[derive(Serialize)]
struct ProposalPayload {
    proposal: ProposalDto,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DispositionDto {
    AdoptedUnchanged,
    Rejected,
    Deferred,
}

impl DispositionDto {
    fn artifact(self) -> ArtifactDisposition {
        match self {
            Self::AdoptedUnchanged => ArtifactDisposition::AdoptedUnchanged,
            Self::Rejected => ArtifactDisposition::Rejected,
            Self::Deferred => ArtifactDisposition::Deferred,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovalRequest {
    schema_version: String,
    proposal_id: String,
    expected_revision_id: String,
    disposition: DispositionDto,
    intent_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApprovalDto {
    token: String,
    expires_at: String,
    intent_id: String,
}

#[derive(Serialize)]
struct ApprovalPayload {
    approval: ApprovalDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DecisionRequest {
    schema_version: String,
    approval_token: String,
    proposal_id: String,
    expected_revision_id: String,
    disposition: DispositionDto,
    intent_id: String,
    rationale: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionDto {
    review_id: String,
    proposal_id: String,
    disposition: DispositionDto,
    status: &'static str,
    revision_id: String,
    artifact_manifest_sha256: String,
}

#[derive(Serialize)]
struct DecisionPayload {
    decision: DecisionDto,
    project: ProjectDto,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateExportRequest {
    schema_version: String,
    revision_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportDto {
    id: String,
    revision_id: String,
    sha256: String,
    byte_length: usize,
    download_url: String,
}

#[derive(Serialize)]
struct ExportPayload {
    #[serde(rename = "export")]
    receipt: ExportDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEnvelope {
    schema_version: &'static str,
    error: ErrorDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorDto {
    code: &'static str,
    message: &'static str,
    request_id: String,
    retryable: bool,
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    retryable: bool,
}

impl ApiError {
    const fn new(
        status: StatusCode,
        code: &'static str,
        message: &'static str,
        retryable: bool,
    ) -> Self {
        Self {
            status,
            code,
            message,
            retryable,
        }
    }

    const fn invalid() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request is invalid.",
            false,
        )
    }

    const fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication is required.",
            false,
        )
    }

    const fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "request_origin_rejected",
            "Request origin validation failed.",
            false,
        )
    }

    const fn not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "resource_not_found",
            "The requested resource was not found.",
            false,
        )
    }

    const fn conflict(code: &'static str) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            code,
            "The request conflicts with current project state.",
            false,
        )
    }

    const fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "The local server could not complete the request.",
            true,
        )
    }

    const fn capacity() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "local_capacity_reached",
            "The local server capacity has been reached.",
            false,
        )
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiError")
            .field("status", &self.status)
            .field("code", &self.code)
            .field("detail", &"<redacted>")
            .finish()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            schema_version: SCHEMA_VERSION,
            error: ErrorDto {
                code: self.code,
                message: self.message,
                request_id: opaque_id("req"),
                retryable: self.retryable,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

enum PreviewError {
    NotFound,
    Failure(ApiError),
}

impl PreviewError {
    const fn not_found() -> Self {
        Self::NotFound
    }
}

impl From<ApiError> for PreviewError {
    fn from(error: ApiError) -> Self {
        if error.status == StatusCode::NOT_FOUND {
            Self::NotFound
        } else {
            Self::Failure(error)
        }
    }
}

impl IntoResponse for PreviewError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => uniform_preview_not_found(),
            Self::Failure(error) => error.into_response(),
        }
    }
}

fn uniform_preview_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Not found",
    )
        .into_response()
}

fn storage_initialization_error(error: StorageError) -> std::io::Error {
    match error {
        StorageError::Io(error) => error,
        StorageError::Corrupt
        | StorageError::Drift
        | StorageError::UnsafeImport
        | StorageError::ImportLimit => std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "managed local state is invalid",
        ),
    }
}

fn validate_preview_scope_origin(origin: &str) -> std::io::Result<u16> {
    let port_text = origin.strip_prefix("http://localhost:").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "preview origin must be an explicit http://localhost port",
        )
    })?;
    if port_text.is_empty() || !port_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "preview origin must contain one explicit TCP port",
        ));
    }
    let port = port_text.parse::<u16>().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "preview origin contains an invalid TCP port",
        )
    })?;
    if matches!(port, 0 | 80) || origin != format!("http://localhost:{port}") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "preview origin must be canonical and use a bound TCP port",
        ));
    }
    Ok(port)
}

fn preview_frame_source(port: u16) -> String {
    format!("http://*.localhost:{port}")
}

fn length_delimited_hash_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn scoped_preview_label(
    state: &StudioState,
    session_id: &str,
    project_id: &str,
    snapshot_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    length_delimited_hash_part(&mut hasher, &state.0.preview_secret);
    length_delimited_hash_part(&mut hasher, session_id.as_bytes());
    length_delimited_hash_part(&mut hasher, project_id.as_bytes());
    length_delimited_hash_part(&mut hasher, snapshot_id.as_bytes());
    let digest = hasher.finalize();
    let label = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("pv-{label}")
}

fn scoped_preview_host(
    state: &StudioState,
    session_id: &str,
    project_id: &str,
    snapshot_id: &str,
) -> String {
    format!(
        "{}.localhost:{}",
        scoped_preview_label(state, session_id, project_id, snapshot_id),
        state.0.preview_port
    )
}

fn scoped_preview_url(
    state: &StudioState,
    session_id: &str,
    project_id: &str,
    snapshot_id: &str,
) -> String {
    format!(
        "http://{}/preview/{project_id}/{snapshot_id}/",
        scoped_preview_host(state, session_id, project_id, snapshot_id)
    )
}

fn preview_request_is_bound(
    state: &StudioState,
    store: &Store,
    headers: &HeaderMap,
    project_id: &str,
    snapshot_id: &str,
) -> bool {
    if headers.get_all(HOST).iter().count() != 1 {
        return false;
    }
    let Some(host) = headers.get(HOST).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let suffix = format!(".localhost:{}", state.0.preview_port);
    let Some(label) = host.strip_suffix(&suffix) else {
        return false;
    };
    let Some(hex) = label.strip_prefix("pv-") else {
        return false;
    };
    if hex.len() != 32
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return false;
    }
    store
        .sessions
        .values()
        .any(|session| scoped_preview_label(state, &session.id, project_id, snapshot_id) == label)
}

fn storage_api_error(error: StorageError) -> ApiError {
    match error {
        StorageError::Drift => ApiError::new(
            StatusCode::CONFLICT,
            "external_changes_detected",
            "The materialized site differs from its accepted revision.",
            false,
        ),
        StorageError::UnsafeImport => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "import_source_unsafe",
            "The import source contains an unsafe file or path.",
            false,
        ),
        StorageError::ImportLimit => ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "import_limit_exceeded",
            "The import source exceeds a local safety limit.",
            false,
        ),
        StorageError::Io(error) => {
            tracing::warn!(kind = ?error.kind(), "managed storage I/O failed");
            ApiError::internal()
        }
        StorageError::Corrupt => {
            tracing::warn!("managed storage validation failed");
            ApiError::internal()
        }
    }
}

fn provider_selection_api_error(error: ProviderSelectionError) -> ApiError {
    match error {
        ProviderSelectionError::UnsupportedProvider => ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider_unsupported",
            "The selected AI provider is not supported.",
            false,
        ),
        ProviderSelectionError::UnsupportedModel => ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider_model_unsupported",
            "The selected model is not available for this provider.",
            false,
        ),
        ProviderSelectionError::NotConfigured => ApiError::new(
            StatusCode::CONFLICT,
            "provider_not_configured",
            "The selected external AI provider is not configured on this local server.",
            false,
        ),
    }
}

fn provider_api_error(error: ProviderError) -> ApiError {
    let status = match error.code() {
        "provider_context_invalid" => StatusCode::CONFLICT,
        "fake_ai_input_unsupported" => StatusCode::UNPROCESSABLE_ENTITY,
        "provider_timeout" => StatusCode::GATEWAY_TIMEOUT,
        "provider_rate_limited" => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::BAD_GATEWAY,
    };
    ApiError::new(status, error.code(), error.message(), error.retryable)
}

fn change_set_api_error(error: ChangeSetError) -> ApiError {
    let status = match &error {
        ChangeSetError::StaleBase
        | ChangeSetError::PreconditionFailed
        | ChangeSetError::OperationConflict
        | ChangeSetError::RenameCycle => StatusCode::CONFLICT,
        ChangeSetError::LimitExceeded => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    ApiError::new(
        status,
        error.code(),
        "The AI ChangeSet failed bounded validation. Accepted files were not changed.",
        false,
    )
}

pub fn editor_router(state: StudioState) -> Router {
    let web_dist = state.0.config.web_dist.clone();
    let editor_csp = HeaderValue::from_str(&format!(
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-src {}; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'none'",
        preview_frame_source(state.0.preview_port)
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"));
    let spa =
        ServeDir::new(&web_dist).not_found_service(ServeFile::new(web_dist.join("index.html")));
    let api = Router::new()
        .route("/bootstrap", get(bootstrap))
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/{project_id}", get(get_project))
        .route("/imports/previews", post(create_import_preview))
        .route(
            "/imports/{preview_id}/confirm",
            post(confirm_import_preview),
        )
        .route("/projects/{project_id}/targets", post(create_target))
        .route("/projects/{project_id}/contexts", post(create_context))
        .route("/projects/{project_id}/proposals", post(create_proposal))
        .route("/reviews/{review_id}/approvals", post(create_approval))
        .route("/reviews/{review_id}/decisions", post(create_decision))
        .route("/projects/{project_id}/exports", post(create_export))
        .route("/exports/{export_id}/download", get(download_export))
        .fallback(api_not_found);

    Router::new()
        .route("/health", get(editor_health))
        .nest("/api/v1", api)
        .fallback_service(spa)
        .layer(SetResponseHeaderLayer::if_not_present(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy"),
            editor_csp,
        ))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}

pub fn preview_router(state: StudioState) -> Router {
    Router::new()
        .route("/health", get(preview_health))
        .route("/preview/{project_id}/{snapshot_id}/", get(preview_index))
        .route(
            "/preview/{project_id}/{snapshot_id}/{*path}",
            get(preview_file),
        )
        .fallback(preview_not_found)
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("clear-site-data"),
            HeaderValue::from_static("\"cache\", \"cookies\", \"storage\""),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-dns-prefetch-control"),
            HeaderValue::from_static("off"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static(
                "accelerometer=(), attribution-reporting=(), autoplay=(), bluetooth=(), browsing-topics=(), camera=(), clipboard-read=(), clipboard-write=(), display-capture=(), encrypted-media=(), fullscreen=(), gamepad=(), geolocation=(), gyroscope=(), hid=(), idle-detection=(), local-fonts=(), magnetometer=(), microphone=(), midi=(), payment=(), picture-in-picture=(), publickey-credentials-create=(), publickey-credentials-get=(), screen-wake-lock=(), serial=(), storage-access=(), usb=(), web-share=(), window-management=(), xr-spatial-tracking=()",
            ),
        ))
        .with_state(state)
}

async fn editor_health() -> Json<HealthResponse> {
    Json(HealthResponse {
        schema_version: SCHEMA_VERSION,
        status: "ok",
        role: "editor",
    })
}

async fn preview_health() -> Json<HealthResponse> {
    Json(HealthResponse {
        schema_version: SCHEMA_VERSION,
        status: "ok",
        role: "preview",
    })
}

async fn bootstrap(
    State(state): State<StudioState>,
    headers: HeaderMap,
) -> Result<Json<Versioned<BootstrapPayload>>, ApiError> {
    validate_bootstrap_headers(&state, &headers)?;
    let token = random_token(32)?;
    let expires_at = now_unix().saturating_add(SESSION_TTL_SECONDS);
    let session = Session {
        id: opaque_id("ses"),
        expires_at,
    };
    let mut store = state.store()?;
    sweep_expired_sessions(&mut store, now_unix());
    ensure_capacity(store.sessions.len(), MAX_SESSIONS)?;
    store.sessions.insert(token_hash(&token), session);
    Ok(Json(Versioned::new(BootstrapPayload {
        api_version: API_VERSION,
        session: SessionDto {
            token,
            expires_at: format_timestamp(expires_at)?,
        },
        editor_origin: state.0.config.editor_origin.clone(),
        preview_origin: state.0.config.preview_origin.clone(),
        capabilities: CapabilitiesDto {
            target_kinds: ["page", "block", "element", "text", "point", "region"],
            dispositions: ["adopted_unchanged", "rejected", "deferred"],
            single_proposal_per_project: true,
            import_available: state.0.config.import_root.is_some(),
            ai_providers: provider_descriptors(state.0.config.openai.as_ref()),
            limits: StorageLimitsDto {
                max_files: STORAGE_MAX_FILES,
                max_total_bytes: STORAGE_MAX_TOTAL_BYTES,
                max_file_bytes: STORAGE_MAX_FILE_BYTES,
                max_path_bytes: STORAGE_MAX_PATH_BYTES,
                max_depth: STORAGE_MAX_DEPTH,
            },
        },
    })))
}

async fn create_project(
    State(state): State<StudioState>,
    headers: HeaderMap,
    payload: Result<Json<CreateProjectRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ProjectPayload>>), ApiError> {
    let session_id = authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    if request.template != "blank" {
        return Err(ApiError::invalid());
    }
    let id = opaque_id("prj");
    let revision_id = opaque_id("rev");
    let files = blank_files();
    let manifest_sha256 = manifest_digest(&files)?;
    let project = Project {
        id: id.clone(),
        display_name: "Untitled landing page".into(),
        revision_id,
        accepted_files: files,
        accepted_manifest_sha256: manifest_sha256,
        targets: HashMap::new(),
        contexts: HashMap::new(),
        proposal: None,
    };
    let mut store = state.store()?;
    ensure_capacity(store.projects.len(), MAX_PROJECTS)?;
    state
        .0
        .storage
        .create_project(
            &project.id,
            &project.display_name,
            &project.revision_id,
            &project.accepted_manifest_sha256,
            &project.accepted_files,
        )
        .map_err(storage_api_error)?;
    let dto = project_dto(&state, &session_id, &project);
    store.projects.insert(id, project);
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ProjectPayload { project: dto })),
    ))
}

async fn list_projects(
    State(state): State<StudioState>,
    headers: HeaderMap,
) -> Result<Json<Versioned<ProjectsPayload>>, ApiError> {
    let session_id = authorize_read(&state, &headers)?;
    let store = state.store()?;
    let mut projects = store
        .projects
        .values()
        .map(|project| project_dto(&state, &session_id, project))
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(Json(Versioned::new(ProjectsPayload { projects })))
}

async fn create_import_preview(
    State(state): State<StudioState>,
    headers: HeaderMap,
    payload: Result<Json<CreateImportPreviewRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ImportPreviewPayload>>), ApiError> {
    let session_id = authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    let import_root = state
        .0
        .config
        .import_root
        .as_deref()
        .ok_or_else(|| ApiError::conflict("import_unavailable"))?;
    let scan = scan_import(import_root).map_err(storage_api_error)?;
    let id = opaque_id("imp");
    let dto = ImportPreviewDto {
        id: id.clone(),
        display_name: scan.display_name,
        manifest_sha256: scan.manifest_sha256.clone(),
        total_bytes: scan.total_bytes,
        entry_point: scan.entry_point,
        included: scan.included,
        excluded: scan.excluded,
        warnings: scan.warnings,
    };
    let mut store = state.store()?;
    store
        .import_previews
        .retain(|_, preview| preview.expires_at >= now_unix());
    ensure_capacity(store.import_previews.len(), MAX_PROJECTS)?;
    store.import_previews.insert(
        id,
        ImportPreviewRecord {
            manifest_sha256: scan.manifest_sha256,
            session_id,
            expires_at: now_unix().saturating_add(IMPORT_PREVIEW_TTL_SECONDS),
        },
    );
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ImportPreviewPayload {
            import_preview: dto,
        })),
    ))
}

async fn confirm_import_preview(
    State(state): State<StudioState>,
    Path(preview_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<ConfirmImportRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ProjectPayload>>), ApiError> {
    let session_id = authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    if request.expected_manifest_sha256.len() != 64
        || !request
            .expected_manifest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ApiError::invalid());
    }
    let preview_digest = {
        let mut store = state.store()?;
        let preview = store
            .import_previews
            .get(&preview_id)
            .ok_or_else(ApiError::not_found)?;
        if preview.session_id != session_id {
            return Err(ApiError::conflict("import_preview_binding_mismatch"));
        }
        if preview.expires_at < now_unix() {
            store.import_previews.remove(&preview_id);
            return Err(ApiError::conflict("import_preview_expired"));
        }
        preview.manifest_sha256.clone()
    };
    if preview_digest != request.expected_manifest_sha256 {
        return Err(ApiError::conflict("import_preview_binding_mismatch"));
    }
    let import_root = state
        .0
        .config
        .import_root
        .as_deref()
        .ok_or_else(|| ApiError::conflict("import_unavailable"))?;
    let scan = scan_import(import_root).map_err(storage_api_error)?;
    {
        let mut store = state.store()?;
        let preview = store
            .import_previews
            .get(&preview_id)
            .ok_or_else(ApiError::not_found)?;
        if preview.session_id != session_id || preview.manifest_sha256 != preview_digest {
            return Err(ApiError::conflict("import_preview_binding_mismatch"));
        }
        if preview.expires_at < now_unix() {
            store.import_previews.remove(&preview_id);
            return Err(ApiError::conflict("import_preview_expired"));
        }
    }
    if scan.manifest_sha256 != preview_digest {
        return Err(ApiError::conflict("import_source_changed"));
    }
    if scan.files.is_empty() || scan.entry_point.as_deref() != Some("index.html") {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "import_entry_point_missing",
            "The import source does not contain an HTML entry point.",
            false,
        ));
    }
    let id = opaque_id("prj");
    let revision_id = opaque_id("rev");
    let artifact_manifest_sha256 = manifest_digest(&scan.files)?;
    let project = Project {
        id: id.clone(),
        display_name: scan.display_name,
        revision_id,
        accepted_files: scan.files,
        accepted_manifest_sha256: artifact_manifest_sha256,
        targets: HashMap::new(),
        contexts: HashMap::new(),
        proposal: None,
    };
    let mut store = state.store()?;
    ensure_capacity(store.projects.len(), MAX_PROJECTS)?;
    let preview = store
        .import_previews
        .get(&preview_id)
        .ok_or_else(ApiError::not_found)?;
    if preview.session_id != session_id || preview.manifest_sha256 != preview_digest {
        return Err(ApiError::conflict("import_preview_binding_mismatch"));
    }
    if preview.expires_at < now_unix() {
        store.import_previews.remove(&preview_id);
        return Err(ApiError::conflict("import_preview_expired"));
    }
    state
        .0
        .storage
        .create_project(
            &project.id,
            &project.display_name,
            &project.revision_id,
            &project.accepted_manifest_sha256,
            &project.accepted_files,
        )
        .map_err(storage_api_error)?;
    store.import_previews.remove(&preview_id);
    let dto = project_dto(&state, &session_id, &project);
    store.projects.insert(id, project);
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ProjectPayload { project: dto })),
    ))
}

async fn get_project(
    State(state): State<StudioState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Versioned<ProjectPayload>>, ApiError> {
    let session_id = authorize_read(&state, &headers)?;
    let store = state.store()?;
    let project = store
        .projects
        .get(&project_id)
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(Versioned::new(ProjectPayload {
        project: project_dto(&state, &session_id, project),
    })))
}

async fn create_target(
    State(state): State<StudioState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<CreateTargetRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<TargetPayload>>), ApiError> {
    authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    let target = request.target;
    validate_target_shape(&target)?;
    let mut store = state.store()?;
    let project = store
        .projects
        .get_mut(&project_id)
        .ok_or_else(ApiError::not_found)?;
    require_revision(project, &target.capture_revision_id)?;
    state
        .0
        .storage
        .verify_project(
            &project.id,
            &project.revision_id,
            &project.accepted_manifest_sha256,
        )
        .map_err(storage_api_error)?;
    if project.targets.contains_key(&target.target_id) {
        return Err(ApiError::conflict("target_id_exists"));
    }
    ensure_capacity(project.targets.len(), MAX_TARGETS_PER_PROJECT)?;
    let resolution = resolve_target(project, &target)?;
    let resolution_id = target_resolution_id(&resolution)?;
    let canonical_target = serde_json::to_vec(&target).map_err(|_| ApiError::internal())?;
    state
        .0
        .storage
        .persist_target(&project.id, &target.target_id, &canonical_target)
        .map_err(storage_api_error)?;
    project
        .targets
        .insert(target.target_id.clone(), target.clone());
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(TargetPayload {
            target,
            resolution,
            resolution_id,
        })),
    ))
}

async fn create_context(
    State(state): State<StudioState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<CreateContextRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ContextPayload>>), ApiError> {
    authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    validate_intent(&request.attempt_id)?;
    validate_human_text(&request.instruction, MAX_INSTRUCTION_BYTES)?;
    let provider = bind_provider(
        state.0.config.openai.as_ref(),
        &request.provider_id,
        &request.requested_model,
    )
    .map_err(provider_selection_api_error)?;
    let mut store = state.store()?;
    let project = store
        .projects
        .get_mut(&project_id)
        .ok_or_else(ApiError::not_found)?;
    require_revision(project, &request.revision_id)?;
    state
        .0
        .storage
        .verify_project(
            &project.id,
            &project.revision_id,
            &project.accepted_manifest_sha256,
        )
        .map_err(storage_api_error)?;
    ensure_capacity(project.contexts.len(), MAX_CONTEXTS_PER_PROJECT)?;
    let target = project
        .targets
        .get(&request.target_id)
        .ok_or_else(ApiError::not_found)?;
    if target.capture_revision_id != request.revision_id {
        return Err(ApiError::conflict("target_revision_mismatch"));
    }
    let resolution = resolve_target(project, target)?;
    match resolution.status {
        TargetResolutionStatus::Resolved => {}
        TargetResolutionStatus::Ambiguous => {
            return Err(ApiError::conflict("target_ambiguous"));
        }
        TargetResolutionStatus::Detached => {
            return Err(ApiError::conflict("target_detached"));
        }
    }
    let resolution_id = target_resolution_id(&resolution)?;
    if resolution_id != request.resolution_id {
        return Err(ApiError::conflict("target_resolution_mismatch"));
    }
    let context_id = opaque_id("ctx");
    let assembled = assemble_provider_context(
        project,
        &context_id,
        &request.attempt_id,
        target,
        &resolution,
        &request.instruction,
        &provider,
    )?;
    let sha256 = review_context_sha256(assembled.canonical_json.as_bytes())
        .map_err(|_| ApiError::internal())?;
    let context = ContextRecord {
        id: context_id,
        attempt_id: request.attempt_id,
        revision_id: request.revision_id,
        target_id: request.target_id,
        target_resolution_id: resolution_id,
        instruction: assembled.instruction,
        provider,
        manifest: assembled.manifest,
        canonical_json: assembled.canonical_json,
        sha256,
    };
    let dto = ContextDto {
        id: context.id.clone(),
        attempt_id: context.attempt_id.clone(),
        revision_id: context.revision_id.clone(),
        target_id: context.target_id.clone(),
        target_resolution_id: context.target_resolution_id.clone(),
        instruction: context.instruction.clone(),
        provider_id: context.provider.provider_id.clone(),
        requested_model: context.provider.requested_model.clone(),
        provider: context.provider.clone(),
        manifest: context.manifest.clone(),
        canonical_json: context.canonical_json.clone(),
        sha256: context.sha256.clone(),
    };
    project.contexts.insert(context.id.clone(), context);
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ContextPayload { context: dto })),
    ))
}

fn claim_ai_attempt(
    active_attempts: &mut HashMap<String, String>,
    project_id: &str,
    attempt_id: &str,
) -> Result<(), ApiError> {
    match active_attempts.entry(project_id.to_owned()) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(attempt_id.to_owned());
            Ok(())
        }
        std::collections::hash_map::Entry::Occupied(_) => {
            Err(ApiError::conflict("ai_attempt_in_progress"))
        }
    }
}

/// Clears an in-flight marker even when the request future is cancelled (for
/// example, because the browser disconnects during a provider call). Keeping
/// this synchronous makes `Drop` safe with the process-local `std::sync`
/// store. The compare-before-remove rule prevents an old guard from releasing
/// a later attempt.
struct ActiveAttemptGuard {
    state: StudioState,
    project_id: String,
    attempt_id: String,
    armed: bool,
}

impl ActiveAttemptGuard {
    fn new(state: StudioState, project_id: &str, attempt_id: &str) -> Self {
        Self {
            state,
            project_id: project_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            armed: true,
        }
    }

    fn release(&mut self, store: &mut Store) -> Result<(), ApiError> {
        if store.active_attempts.get(&self.project_id) != Some(&self.attempt_id) {
            return Err(ApiError::conflict("ai_attempt_not_active"));
        }
        store.active_attempts.remove(&self.project_id);
        self.armed = false;
        Ok(())
    }
}

impl Drop for ActiveAttemptGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut store) = self.state.0.store.lock()
            && store.active_attempts.get(&self.project_id) == Some(&self.attempt_id)
        {
            store.active_attempts.remove(&self.project_id);
        }
    }
}

async fn create_proposal(
    State(state): State<StudioState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<CreateProposalRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ProposalPayload>>), ApiError> {
    let session_id = authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    let provider_request = {
        let mut store = state.store()?;
        let project = store
            .projects
            .get(&project_id)
            .ok_or_else(ApiError::not_found)?;
        state
            .0
            .storage
            .verify_project(
                &project.id,
                &project.revision_id,
                &project.accepted_manifest_sha256,
            )
            .map_err(storage_api_error)?;
        if project.proposal.is_some() {
            return Err(ApiError::conflict("artifact_single_proposal_limit"));
        }
        let context = project
            .contexts
            .get(&request.context_id)
            .ok_or_else(ApiError::not_found)?;
        if context.sha256 != request.context_sha256
            || raw_sha256(context.canonical_json.as_bytes()) != context.sha256
        {
            return Err(ApiError::conflict("context_digest_mismatch"));
        }
        if context.revision_id != project.revision_id {
            return Err(ApiError::conflict("stale_base"));
        }
        if context.manifest.entries.iter().any(|entry| entry.redacted) {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "provider_context_redacted_source_unsupported",
                "Generation is blocked because ChangeSet v1 cannot safely round-trip redacted site bytes.",
                false,
            ));
        }
        let provider_request = ProviderRequest {
            attempt_id: context.attempt_id.clone(),
            base_revision_id: context.revision_id.clone(),
            provider_context_sha256: context.sha256.clone(),
            canonical_context_json: context.canonical_json.clone(),
            binding: context.provider.clone(),
        };
        claim_ai_attempt(
            &mut store.active_attempts,
            &project_id,
            &provider_request.attempt_id,
        )?;
        provider_request
    };

    let mut attempt_guard =
        ActiveAttemptGuard::new(state.clone(), &project_id, &provider_request.attempt_id);

    let provider_result = execute_provider(state.0.config.openai.as_ref(), &provider_request)
        .await
        .map_err(provider_api_error)?;

    let mut store = state.store()?;
    attempt_guard.release(&mut store)?;
    let project = store
        .projects
        .get_mut(&project_id)
        .ok_or_else(ApiError::not_found)?;
    state
        .0
        .storage
        .verify_project(
            &project.id,
            &project.revision_id,
            &project.accepted_manifest_sha256,
        )
        .map_err(storage_api_error)?;
    if project.proposal.is_some() {
        return Err(ApiError::conflict("artifact_single_proposal_limit"));
    }
    let context = project
        .contexts
        .get(&request.context_id)
        .ok_or_else(ApiError::not_found)?;
    if context.sha256 != request.context_sha256
        || context.sha256 != provider_request.provider_context_sha256
        || context.revision_id != project.revision_id
    {
        return Err(ApiError::conflict("stale_base"));
    }
    validate_provider_attribution(context, &provider_result.attribution)?;
    let mut applied = parse_and_apply_change_set(
        &provider_result.raw_change_set_json,
        &project.revision_id,
        &project.accepted_files,
    )
    .map_err(change_set_api_error)?;
    let change_set_json = applied.change_set.to_json().map_err(change_set_api_error)?;
    let change_set_sha256 = raw_sha256(change_set_json.as_bytes());
    let target = project
        .targets
        .get(&context.target_id)
        .ok_or_else(ApiError::not_found)?;
    applied.checks.push(proposal_target_reresolution_check(
        target,
        &applied.files,
        &project.revision_id,
    )?);
    let synapse_review_context = synapse_review_context_json(
        context,
        target,
        &change_set_sha256,
        &provider_result.attribution,
    )?;
    let synapse_review_context_sha256 = review_context_sha256(synapse_review_context.as_bytes())
        .map_err(|_| ApiError::internal())?;
    let accepted_manifest = manifest(&project.accepted_files)?;
    let proposed_manifest = manifest(&applied.files)?;
    let proposal_id = opaque_id("pro");
    let repository_path = state
        .0
        .storage
        .proposal_repository(&project.id, &proposal_id)
        .map_err(storage_api_error)?;
    state
        .0
        .storage
        .persist_proposal_workspace(&project.id, &proposal_id, &applied.files)
        .map_err(storage_api_error)?;
    let recorded_at = now_rfc3339()?;
    let grant_expires_at =
        canonical_timestamp(OffsetDateTime::now_utc().saturating_add(time::Duration::hours(1)));
    let trusted = TrustedArtifactProjectConfig::new(
        repository_path,
        project.id.trim_start_matches("prj_"),
        "Local creator",
        if provider_result.attribution.external {
            "External AI provider"
        } else {
            "Deterministic fake AI"
        },
        recorded_at,
        grant_expires_at,
    );
    let pending = begin_artifact_proposal(
        &trusted,
        &accepted_manifest,
        &proposed_manifest,
        synapse_review_context.as_bytes(),
        ArtifactSourceAttribution::CallerSuppliedAiAttributed,
    )
    .map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "synapsegit_proposal_failed",
            "SynapseGit could not record the proposal.",
            true,
        )
    })?;
    let receipt = pending.receipt();
    if receipt.review_context_sha256() != synapse_review_context_sha256
        || receipt.artifact_manifest_sha256() != artifact_manifest_sha256(&proposed_manifest)
        || receipt.execution_verified()
    {
        return Err(ApiError::internal());
    }
    let review_id = opaque_id("revw");
    let (changes, validation) = proposal_review_details(&applied);
    let summary = applied.change_set.summary.clone();
    let proposal = ProposalRecord {
        id: proposal_id.clone(),
        review_id: review_id.clone(),
        base_revision_id: project.revision_id.clone(),
        artifact_manifest_sha256: receipt.artifact_manifest_sha256().to_owned(),
        review_context_sha256: receipt.review_context_sha256().to_owned(),
        provider_context_sha256: context.sha256.clone(),
        change_set_sha256,
        change_set: applied.change_set,
        attribution: provider_result.attribution,
        summary,
        changes,
        unified_diff: applied.unified_diff,
        validation,
        proposed_files: applied.files,
        pending,
        status: ProposalStatus::PendingReview,
    };
    let dto = proposal_dto(&state, &session_id, project, &proposal);
    project.proposal = Some(proposal);
    store.reviews.insert(review_id, project_id);
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ProposalPayload { proposal: dto })),
    ))
}

async fn create_approval(
    State(state): State<StudioState>,
    Path(review_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<ApprovalRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ApprovalPayload>>), ApiError> {
    let session_id = authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    validate_intent(&request.intent_id)?;
    let token = random_token(32)?;
    let expires_at = now_unix().saturating_add(APPROVAL_TTL_SECONDS);
    let mut store = state.store()?;
    sweep_expired_approvals(&mut store, now_unix());
    ensure_capacity(store.approvals.len(), MAX_PENDING_APPROVALS)?;
    let project_id = store
        .reviews
        .get(&review_id)
        .cloned()
        .ok_or_else(ApiError::not_found)?;
    let (verified_project_id, verified_revision_id, verified_manifest_sha256) = store
        .projects
        .get(&project_id)
        .map(|project| {
            (
                project.id.clone(),
                project.revision_id.clone(),
                project.accepted_manifest_sha256.clone(),
            )
        })
        .ok_or_else(ApiError::not_found)?;
    state
        .0
        .storage
        .verify_project(
            &verified_project_id,
            &verified_revision_id,
            &verified_manifest_sha256,
        )
        .map_err(storage_api_error)?;
    let project = store
        .projects
        .get(&project_id)
        .ok_or_else(ApiError::not_found)?;
    let proposal = project.proposal.as_ref().ok_or_else(ApiError::not_found)?;
    if proposal.status != ProposalStatus::PendingReview
        || proposal.id != request.proposal_id
        || proposal.base_revision_id != request.expected_revision_id
        || project.revision_id != request.expected_revision_id
    {
        return Err(ApiError::conflict("approval_binding_mismatch"));
    }
    if request.disposition == DispositionDto::AdoptedUnchanged
        && proposal
            .validation
            .checks
            .iter()
            .any(|check| check.blocking)
    {
        return Err(ApiError::conflict("proposal_blocked_by_validation"));
    }
    let binding = ApprovalBinding {
        session_id,
        project_id,
        review_id,
        proposal_id: request.proposal_id,
        proposal_digest: proposal.artifact_manifest_sha256.clone(),
        expected_revision_id: request.expected_revision_id,
        disposition: request.disposition,
        intent_id: request.intent_id.clone(),
    };
    store.approvals.insert(
        token_hash(&token),
        ApprovalGrant {
            binding,
            expires_at,
        },
    );
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ApprovalPayload {
            approval: ApprovalDto {
                token,
                expires_at: format_timestamp(expires_at)?,
                intent_id: request.intent_id,
            },
        })),
    ))
}

async fn create_decision(
    State(state): State<StudioState>,
    Path(review_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Json<Versioned<DecisionPayload>>, ApiError> {
    let session_id = authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    validate_intent(&request.intent_id)?;
    if let Some(rationale) = request.rationale.as_deref() {
        validate_human_text(rationale, MAX_RATIONALE_BYTES)?;
    }
    let mut store = state.store()?;
    let project_id = store
        .reviews
        .get(&review_id)
        .cloned()
        .ok_or_else(ApiError::not_found)?;
    let proposal_digest = store
        .projects
        .get(&project_id)
        .and_then(|project| project.proposal.as_ref())
        .map(|proposal| proposal.artifact_manifest_sha256.clone())
        .ok_or_else(ApiError::not_found)?;
    let expected_binding = ApprovalBinding {
        session_id: session_id.clone(),
        project_id: project_id.clone(),
        review_id: review_id.clone(),
        proposal_id: request.proposal_id.clone(),
        proposal_digest,
        expected_revision_id: request.expected_revision_id.clone(),
        disposition: request.disposition,
        intent_id: request.intent_id.clone(),
    };
    validate_approval(
        &mut store.approvals,
        &request.approval_token,
        &expected_binding,
        now_unix(),
    )?;
    let (verified_project_id, verified_revision_id, verified_manifest_sha256) = {
        let project = store
            .projects
            .get(&project_id)
            .ok_or_else(ApiError::not_found)?;
        if project.revision_id != request.expected_revision_id {
            return Err(ApiError::conflict("revision_mismatch"));
        }
        let proposal = project.proposal.as_ref().ok_or_else(ApiError::not_found)?;
        if proposal.status != ProposalStatus::PendingReview
            || proposal.id != request.proposal_id
            || proposal.review_id != review_id
        {
            return Err(ApiError::conflict("decision_binding_mismatch"));
        }
        (
            project.id.clone(),
            project.revision_id.clone(),
            project.accepted_manifest_sha256.clone(),
        )
    };
    state
        .0
        .storage
        .verify_project(
            &verified_project_id,
            &verified_revision_id,
            &verified_manifest_sha256,
        )
        .map_err(storage_api_error)?;
    consume_approval(
        &mut store.approvals,
        &request.approval_token,
        &expected_binding,
        now_unix(),
    )?;

    let project = store
        .projects
        .get_mut(&project_id)
        .ok_or_else(ApiError::not_found)?;
    let proposal = project.proposal.as_mut().ok_or_else(ApiError::not_found)?;
    debug_assert_eq!(project.revision_id, request.expected_revision_id);
    debug_assert_eq!(proposal.status, ProposalStatus::PendingReview);
    debug_assert_eq!(proposal.id, request.proposal_id);
    debug_assert_eq!(proposal.review_id, review_id);
    let receipt = decide_artifact_proposal(
        &mut proposal.pending,
        &ArtifactDecisionOptions {
            disposition: request.disposition.artifact(),
            private_rationale: request.rationale,
        },
    )
    .map_err(|_| {
        ApiError::new(
            StatusCode::CONFLICT,
            "synapsegit_decision_failed",
            "SynapseGit could not commit the decision.",
            false,
        )
    })?;
    if receipt.reviewed_artifact_manifest_sha256()
        != match request.disposition {
            DispositionDto::AdoptedUnchanged => proposal.artifact_manifest_sha256.as_str(),
            DispositionDto::Rejected | DispositionDto::Deferred => {
                project.accepted_manifest_sha256.as_str()
            }
        }
    {
        return Err(ApiError::internal());
    }
    if request.disposition == DispositionDto::AdoptedUnchanged {
        let revision_id = opaque_id("rev");
        state
            .0
            .storage
            .commit_revision(
                &project.id,
                &revision_id,
                &proposal.artifact_manifest_sha256,
                &proposal.proposed_files,
            )
            .map_err(storage_api_error)?;
        project.accepted_files = proposal.proposed_files.clone();
        project.accepted_manifest_sha256 = proposal.artifact_manifest_sha256.clone();
        project.revision_id = revision_id;
    }
    proposal.status = ProposalStatus::Committed;
    let revision_id = project.revision_id.clone();
    let manifest_sha256 = project.accepted_manifest_sha256.clone();
    let project_dto = project_dto(&state, &session_id, project);
    Ok(Json(Versioned::new(DecisionPayload {
        decision: DecisionDto {
            review_id,
            proposal_id: request.proposal_id,
            disposition: request.disposition,
            status: "committed",
            revision_id,
            artifact_manifest_sha256: manifest_sha256,
        },
        project: project_dto,
    })))
}

async fn create_export(
    State(state): State<StudioState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    payload: Result<Json<CreateExportRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Versioned<ExportPayload>>), ApiError> {
    authorize_mutation(&state, &headers)?;
    let Json(request) = valid_json(payload)?;
    require_schema(&request.schema_version)?;
    let mut store = state.store()?;
    ensure_capacity(store.exports.len(), MAX_EXPORTS)?;
    let (revision_id, accepted_files) = {
        let project = store
            .projects
            .get(&project_id)
            .ok_or_else(ApiError::not_found)?;
        require_revision(project, &request.revision_id)?;
        state
            .0
            .storage
            .verify_project(
                &project.id,
                &project.revision_id,
                &project.accepted_manifest_sha256,
            )
            .map_err(storage_api_error)?;
        (project.revision_id.clone(), project.accepted_files.clone())
    };
    let zip = deterministic_zip(&accepted_files)?;
    let retained_bytes = store
        .exports
        .values()
        .try_fold(0_usize, |total, artifact| {
            total.checked_add(artifact.bytes.len())
        })
        .ok_or_else(ApiError::capacity)?;
    ensure_byte_capacity(retained_bytes, zip.len(), MAX_RETAINED_EXPORT_BYTES)?;
    let sha256 = raw_sha256(&zip);
    let export_id = opaque_id("exp");
    let dto = ExportDto {
        id: export_id.clone(),
        revision_id,
        sha256: sha256.clone(),
        byte_length: zip.len(),
        download_url: format!("/api/v1/exports/{export_id}/download"),
    };
    store.exports.insert(
        export_id,
        ExportArtifact {
            project_id,
            revision_id: request.revision_id,
            sha256,
            bytes: zip,
        },
    );
    Ok((
        StatusCode::CREATED,
        Json(Versioned::new(ExportPayload { receipt: dto })),
    ))
}

async fn download_export(
    State(state): State<StudioState>,
    Path(export_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    authorize_read(&state, &headers)?;
    let store = state.store()?;
    let artifact = store
        .exports
        .get(&export_id)
        .ok_or_else(ApiError::not_found)?;
    let mut response = Response::new(Body::from(artifact.bytes.clone()));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=landing-page.zip"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-sha256",
        HeaderValue::from_str(&artifact.sha256).map_err(|_| ApiError::internal())?,
    );
    response.headers_mut().insert(
        "x-project-binding",
        HeaderValue::from_str(&raw_sha256(artifact.project_id.as_bytes()))
            .map_err(|_| ApiError::internal())?,
    );
    response.headers_mut().insert(
        "x-revision-binding",
        HeaderValue::from_str(&raw_sha256(artifact.revision_id.as_bytes()))
            .map_err(|_| ApiError::internal())?,
    );
    Ok(response)
}

async fn preview_index(
    state: State<StudioState>,
    Path((project_id, snapshot_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, PreviewError> {
    serve_preview(state, headers, project_id, snapshot_id, "index.html".into()).await
}

async fn preview_file(
    state: State<StudioState>,
    Path((project_id, snapshot_id, path)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Response, PreviewError> {
    serve_preview(state, headers, project_id, snapshot_id, path).await
}

async fn serve_preview(
    State(state): State<StudioState>,
    headers: HeaderMap,
    project_id: String,
    snapshot_id: String,
    path: String,
) -> Result<Response, PreviewError> {
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(PreviewError::not_found());
    }
    let (bytes, revision_id) = {
        let mut store = state.store()?;
        sweep_expired_sessions(&mut store, now_unix());
        if !preview_request_is_bound(&state, &store, &headers, &project_id, &snapshot_id) {
            return Err(PreviewError::not_found());
        }
        let project = store
            .projects
            .get(&project_id)
            .ok_or_else(ApiError::not_found)?;
        let (files, revision_id) = if snapshot_id == project.revision_id {
            (&project.accepted_files, project.revision_id.clone())
        } else if let Some(proposal) = project.proposal.as_ref().filter(|proposal| {
            proposal.id == snapshot_id && proposal.status == ProposalStatus::PendingReview
        }) {
            (&proposal.proposed_files, proposal.base_revision_id.clone())
        } else {
            return Err(PreviewError::not_found());
        };
        (
            files.get(&path).cloned().ok_or_else(ApiError::not_found)?,
            revision_id,
        )
    };
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let mut response_bytes = bytes;
    let mut nonce = None;
    if mime.essence_str() == "text/html" {
        if response_bytes.len() > MAX_PREVIEW_HTML_BYTES {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "preview_too_large",
                "The preview document is too large.",
                false,
            )
            .into());
        }
        let script_nonce = random_token(16)?;
        response_bytes = inject_preview_bridge(
            &response_bytes,
            &script_nonce,
            &state.0.config.editor_origin,
            &project_id,
            &snapshot_id,
            &revision_id,
        )?;
        nonce = Some(script_nonce);
    }
    let mut response = Response::new(Body::from(response_bytes));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).map_err(|_| ApiError::internal())?,
    );
    if let Some(nonce) = nonce {
        let csp = format!(
            "default-src 'none'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; script-src 'self' 'nonce-{nonce}'; worker-src 'none'; frame-src 'none'; connect-src 'none'; webrtc 'block'; frame-ancestors {}; object-src 'none'; base-uri 'none'; form-action 'none'",
            state.0.config.editor_origin
        );
        response.headers_mut().insert(
            "content-security-policy",
            HeaderValue::from_str(&csp).map_err(|_| ApiError::internal())?,
        );
    }
    Ok(response)
}

async fn api_not_found() -> ApiError {
    ApiError::not_found()
}

async fn preview_not_found() -> Response {
    uniform_preview_not_found()
}

fn validate_bootstrap_headers(state: &StudioState, headers: &HeaderMap) -> Result<(), ApiError> {
    require_host(state, headers)?;
    require_fetch_site(headers)?;
    if let Some(origin) = headers.get(ORIGIN)
        && origin.as_bytes() != state.0.config.editor_origin.as_bytes()
    {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

fn authorize_mutation(state: &StudioState, headers: &HeaderMap) -> Result<String, ApiError> {
    require_host(state, headers)?;
    if headers
        .get(ORIGIN)
        .is_none_or(|value| value.as_bytes() != state.0.config.editor_origin.as_bytes())
    {
        return Err(ApiError::forbidden());
    }
    require_fetch_site(headers)?;
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if content_type
        .split(';')
        .next()
        .is_none_or(|value| value.trim() != "application/json")
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_content_type_required",
            "A JSON request body is required.",
            false,
        ));
    }
    authenticate(state, headers)
}

fn authorize_read(state: &StudioState, headers: &HeaderMap) -> Result<String, ApiError> {
    require_host(state, headers)?;
    if let Some(origin) = headers.get(ORIGIN)
        && origin.as_bytes() != state.0.config.editor_origin.as_bytes()
    {
        return Err(ApiError::forbidden());
    }
    authenticate(state, headers)
}

fn require_host(state: &StudioState, headers: &HeaderMap) -> Result<(), ApiError> {
    if headers
        .get(HOST)
        .is_none_or(|value| value.as_bytes() != state.0.config.editor_host.as_bytes())
    {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

fn require_fetch_site(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers
        .get("sec-fetch-site")
        .is_none_or(|value| value.as_bytes() != b"same-origin")
    {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

fn authenticate(state: &StudioState, headers: &HeaderMap) -> Result<String, ApiError> {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(ApiError::unauthorized)?;
    let token = authorization
        .strip_prefix("Bearer ")
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(ApiError::unauthorized)?;
    let hash = token_hash(token);
    let mut store = state.store()?;
    let Some(session) = store.sessions.get(&hash) else {
        return Err(ApiError::unauthorized());
    };
    if session.expires_at < now_unix() {
        store.sessions.remove(&hash);
        return Err(ApiError::unauthorized());
    }
    Ok(session.id.clone())
}

fn valid_json<T>(payload: Result<Json<T>, JsonRejection>) -> Result<Json<T>, ApiError> {
    payload.map_err(|_| ApiError::invalid())
}

fn require_schema(schema: &str) -> Result<(), ApiError> {
    if schema == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "schema_version_unsupported",
            "The schema version is not supported.",
            false,
        ))
    }
}

fn require_revision(project: &Project, revision_id: &str) -> Result<(), ApiError> {
    if project.revision_id == revision_id {
        Ok(())
    } else {
        Err(ApiError::conflict("revision_mismatch"))
    }
}

fn ensure_capacity(current: usize, maximum: usize) -> Result<(), ApiError> {
    if current < maximum {
        Ok(())
    } else {
        Err(ApiError::capacity())
    }
}

fn ensure_byte_capacity(current: usize, additional: usize, maximum: usize) -> Result<(), ApiError> {
    if current
        .checked_add(additional)
        .is_some_and(|total| total <= maximum)
    {
        Ok(())
    } else {
        Err(ApiError::capacity())
    }
}

fn sweep_expired_sessions(store: &mut Store, now: i64) {
    store
        .sessions
        .retain(|_, session| session.expires_at >= now);
}

fn sweep_expired_approvals(store: &mut Store, now: i64) {
    store
        .approvals
        .retain(|_, approval| approval.expires_at >= now);
}

fn validate_human_text(value: &str, max_bytes: usize) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        Err(ApiError::invalid())
    } else {
        Ok(())
    }
}

fn validate_intent(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        Err(ApiError::invalid())
    } else {
        Ok(())
    }
}

fn selectable_text(element_id: &str) -> Option<&'static str> {
    match element_id {
        "hero-heading" | "hero-title" | "hero" | "page" => Some("まだ、白紙です。"),
        "hero-copy" | "next" => Some("伝えたいことを選び、AIとの対話から最初の一歩をつくります。"),
        "hero-cta" => Some("構想を始める"),
        _ => None,
    }
}

fn validate_target_shape(target: &TargetRecord) -> Result<(), ApiError> {
    if target.schema_version != TARGET_SCHEMA_VERSION
        || !is_opaque_identifier(&target.target_id, "tgt_")
        || !is_opaque_identifier(&target.capture_revision_id, "rev_")
        || !is_safe_page_path(&target.page_path)
    {
        return Err(ApiError::invalid());
    }
    match target.capture_source {
        TargetCaptureSource::Accepted if target.capture_proposal_id.is_none() => {}
        TargetCaptureSource::Proposal
            if target
                .capture_proposal_id
                .as_deref()
                .is_some_and(|value| is_opaque_identifier(value, "pro_")) => {}
        _ => return Err(ApiError::invalid()),
    }
    validate_target_text(&target.label, MAX_TARGET_LABEL_LENGTH, false)?;
    validate_viewport(&target.viewport)?;
    validate_document_capture(&target.document)?;
    if let Some(geometry) = &target.geometry {
        validate_geometry(geometry)?;
    }
    if let Some(point) = &target.point {
        validate_point_capture(point)?;
    }
    if let Some(anchor) = &target.element_anchor {
        validate_element_anchor(anchor)?;
    }
    if let Some(anchor) = &target.text_anchor {
        validate_text_anchor(anchor)?;
    }
    if let Some(anchor) = &target.region_anchor {
        validate_region_anchor(anchor)?;
    }
    if target.block.as_ref().is_some_and(|block| block.level > 64) {
        return Err(ApiError::invalid());
    }

    let specific_shape_is_valid = match target.kind {
        TargetKind::Page => {
            target.geometry.is_none()
                && target.point.is_none()
                && target.element_anchor.is_none()
                && target.text_anchor.is_none()
                && target.region_anchor.is_none()
                && target.block.is_none()
        }
        TargetKind::Block => {
            target.element_anchor.is_some()
                && target.block.is_some()
                && target.point.is_none()
                && target.text_anchor.is_none()
                && target.region_anchor.is_none()
        }
        TargetKind::Element => {
            target.element_anchor.is_some()
                && target.point.is_none()
                && target.text_anchor.is_none()
                && target.region_anchor.is_none()
                && target.block.is_none()
        }
        TargetKind::Text => {
            target.element_anchor.is_some()
                && target.text_anchor.is_some()
                && target.point.is_none()
                && target.region_anchor.is_none()
                && target.block.is_none()
        }
        TargetKind::Point => {
            target.point.is_some()
                && target
                    .region_anchor
                    .as_ref()
                    .and_then(|anchor| anchor.containing_block.as_ref())
                    .is_some()
                && target.geometry.is_none()
                && target.element_anchor.is_none()
                && target.text_anchor.is_none()
                && target.block.is_none()
        }
        TargetKind::Region => {
            target.geometry.as_ref().is_some_and(|geometry| {
                non_zero_geometry(geometry)
                    && geometry.viewport_css_pixel_rect.width >= 8.0
                    && geometry.viewport_css_pixel_rect.height >= 8.0
                    && geometry.viewport_normalized_rect.width > 0.0
                    && geometry.viewport_normalized_rect.height > 0.0
            }) && target.region_anchor.is_some()
                && target.point.is_none()
                && target.element_anchor.is_none()
                && target.text_anchor.is_none()
                && target.block.is_none()
        }
    };
    if specific_shape_is_valid {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}

fn is_opaque_identifier(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn is_safe_page_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= STORAGE_MAX_PATH_BYTES
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
        && value
            .rsplit_once('.')
            .is_some_and(|(_, extension)| matches!(extension, "html" | "htm"))
}

fn validate_target_text(value: &str, max_length: usize, allow_empty: bool) -> Result<(), ApiError> {
    let lower = value.to_ascii_lowercase();
    let secret_like = ["ghp_", "github_pat_", "akia", "bearer ", "sk-"]
        .iter()
        .any(|prefix| contains_credential_shaped_token(&lower, prefix));
    if (!allow_empty && value.trim().is_empty())
        || value.chars().count() > max_length
        || secret_like
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        Err(ApiError::invalid())
    } else {
        Ok(())
    }
}

fn contains_credential_shaped_token(value: &str, prefix: &str) -> bool {
    value.match_indices(prefix).any(|(index, _)| {
        value[index + prefix.len()..]
            .chars()
            .take_while(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            })
            .take(12)
            .count()
            == 12
    })
}

fn finite_between(value: f64, minimum: f64, maximum: f64) -> bool {
    value.is_finite() && value >= minimum && value <= maximum
}

fn validate_viewport(viewport: &TargetViewport) -> Result<(), ApiError> {
    if finite_between(viewport.css_width, f64::MIN_POSITIVE, 1_000_000.0)
        && finite_between(viewport.css_height, f64::MIN_POSITIVE, 1_000_000.0)
        && finite_between(viewport.scroll_x, 0.0, 1_000_000.0)
        && finite_between(viewport.scroll_y, 0.0, 1_000_000.0)
        && finite_between(viewport.device_pixel_ratio, f64::MIN_POSITIVE, 16.0)
        && finite_between(viewport.visual_viewport_scale, f64::MIN_POSITIVE, 16.0)
        && finite_between(viewport.preview_scale, f64::MIN_POSITIVE, 8.0)
    {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}

fn validate_document_capture(document: &TargetDocument) -> Result<(), ApiError> {
    if finite_between(document.css_width, 0.0, 1_000_000.0)
        && finite_between(document.css_height, 0.0, 1_000_000.0)
        && document.layout_epoch <= 9_007_199_254_740_991
    {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}

fn validate_rect(rect: &TargetRect, normalized: bool) -> Result<(), ApiError> {
    let (minimum, maximum) = if normalized {
        (0.0, 1.0)
    } else {
        (-1_000_000.0, 1_000_000.0)
    };
    if finite_between(rect.x, minimum, maximum)
        && finite_between(rect.y, minimum, maximum)
        && finite_between(rect.width, 0.0, maximum)
        && finite_between(rect.height, 0.0, maximum)
        && (!normalized || rect.x + rect.width <= 1.000_001)
        && (!normalized || rect.y + rect.height <= 1.000_001)
    {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}

fn validate_geometry(geometry: &TargetGeometry) -> Result<(), ApiError> {
    validate_rect(&geometry.document_css_pixel_rect, false)?;
    validate_rect(&geometry.viewport_css_pixel_rect, false)?;
    validate_rect(&geometry.viewport_normalized_rect, true)
}

fn non_zero_geometry(geometry: &TargetGeometry) -> bool {
    geometry.document_css_pixel_rect.width > 0.0
        && geometry.document_css_pixel_rect.height > 0.0
        && geometry.viewport_css_pixel_rect.width > 0.0
        && geometry.viewport_css_pixel_rect.height > 0.0
}

fn validate_point_capture(point: &TargetPointCapture) -> Result<(), ApiError> {
    if finite_between(point.document_css_pixel.x, 0.0, 1_000_000.0)
        && finite_between(point.document_css_pixel.y, 0.0, 1_000_000.0)
        && finite_between(point.viewport_normalized.x, 0.0, 1.0)
        && finite_between(point.viewport_normalized.y, 0.0, 1.0)
    {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}

fn validate_element_anchor(anchor: &ElementAnchor) -> Result<(), ApiError> {
    if anchor.tag_name.is_empty()
        || anchor.tag_name.chars().count() > MAX_TARGET_CLASS_TOKEN_LENGTH
        || !anchor.tag_name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || (index > 0 && (byte.is_ascii_digit() || byte == b'-'))
        })
    {
        return Err(ApiError::invalid());
    }
    for value in [
        anchor.unique_element_id.as_deref(),
        anchor.dom_path.as_deref(),
        anchor.ancestor_fingerprint.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_target_text(value, MAX_TARGET_ANCHOR_LENGTH, false)?;
    }
    if let Some(role) = &anchor.role {
        validate_target_text(role, MAX_TARGET_CLASS_TOKEN_LENGTH, false)?;
    }
    if let Some(name) = &anchor.accessible_name {
        validate_target_text(name, 512, false)?;
    }
    if let Some(tokens) = &anchor.class_tokens {
        if tokens.is_empty() || tokens.len() > 32 {
            return Err(ApiError::invalid());
        }
        for token in tokens {
            validate_target_text(token, MAX_TARGET_CLASS_TOKEN_LENGTH, false)?;
            if token.chars().any(char::is_whitespace) {
                return Err(ApiError::invalid());
            }
        }
        let unique = tokens.iter().collect::<std::collections::HashSet<_>>();
        if unique.len() != tokens.len() {
            return Err(ApiError::invalid());
        }
    }
    if anchor.sibling_index.is_some_and(|index| index > 1_000_000) {
        return Err(ApiError::invalid());
    }
    Ok(())
}

fn validate_text_anchor(anchor: &TextAnchor) -> Result<(), ApiError> {
    validate_target_text(&anchor.exact, MAX_TARGET_QUOTE_LENGTH, false)?;
    for value in [anchor.prefix.as_deref(), anchor.suffix.as_deref()]
        .into_iter()
        .flatten()
    {
        validate_target_text(value, MAX_TARGET_CONTEXT_LENGTH, false)?;
    }
    match (anchor.start_offset, anchor.end_offset) {
        (None, None) => Ok(()),
        (Some(start), Some(end)) => {
            let utf16_length = anchor.exact.encode_utf16().count() as u32;
            if start < end && end <= 1_000_000 && end - start == utf16_length {
                Ok(())
            } else {
                Err(ApiError::invalid())
            }
        }
        _ => Err(ApiError::invalid()),
    }
}

fn validate_region_anchor(anchor: &RegionAnchor) -> Result<(), ApiError> {
    for candidate in [
        anchor.containing_block.as_ref(),
        anchor.previous_visible_sibling.as_ref(),
        anchor.next_visible_sibling.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_element_anchor(candidate)?;
    }
    Ok(())
}

fn target_source_files<'a>(
    project: &'a Project,
    target: &TargetRecord,
) -> Result<&'a BTreeMap<String, Vec<u8>>, ApiError> {
    if target.capture_revision_id != project.revision_id {
        return Err(ApiError::conflict("target_revision_mismatch"));
    }
    match target.capture_source {
        TargetCaptureSource::Accepted if target.capture_proposal_id.is_none() => {
            Ok(&project.accepted_files)
        }
        TargetCaptureSource::Proposal => {
            let proposal_id = target
                .capture_proposal_id
                .as_deref()
                .ok_or_else(ApiError::invalid)?;
            project
                .proposal
                .as_ref()
                .filter(|proposal| {
                    proposal.id == proposal_id
                        && proposal.base_revision_id == project.revision_id
                        && proposal.status == ProposalStatus::PendingReview
                })
                .map(|proposal| &proposal.proposed_files)
                .ok_or_else(|| ApiError::conflict("target_source_proposal_stale"))
        }
        TargetCaptureSource::Accepted => Err(ApiError::invalid()),
    }
}

fn target_anchor(target: &TargetRecord) -> Option<&ElementAnchor> {
    match target.kind {
        TargetKind::Block | TargetKind::Element | TargetKind::Text => {
            target.element_anchor.as_ref()
        }
        TargetKind::Point | TargetKind::Region => target
            .region_anchor
            .as_ref()
            .and_then(|anchor| anchor.containing_block.as_ref()),
        TargetKind::Page => None,
    }
}

fn count_anchor_occurrences(html: &str, anchor: &ElementAnchor) -> (usize, bool) {
    let Some(identifier) = anchor.unique_element_id.as_deref() else {
        return (0, false);
    };
    let (identifier_count, matching_tag_count) =
        count_matching_start_tags(html, &anchor.tag_name, identifier);
    (identifier_count, matching_tag_count == 1)
}

fn resolve_target(project: &Project, target: &TargetRecord) -> Result<TargetResolution, ApiError> {
    let files = target_source_files(project, target)?;
    if files
        .get(&target.page_path)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .is_none()
    {
        return Err(ApiError::conflict("target_page_detached"));
    }
    resolve_target_against_files(target, files, &project.revision_id)
}

fn resolve_target_against_files(
    target: &TargetRecord,
    files: &BTreeMap<String, Vec<u8>>,
    resolved_revision_id: &str,
) -> Result<TargetResolution, ApiError> {
    let Some(html) = files
        .get(&target.page_path)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
    else {
        return Ok(TargetResolution {
            schema_version: TARGET_SCHEMA_VERSION,
            resolver_version: TARGET_RESOLVER_VERSION,
            target_id: target.target_id.clone(),
            capture_revision_id: target.capture_revision_id.clone(),
            resolved_revision_id: resolved_revision_id.to_owned(),
            status: TargetResolutionStatus::Detached,
            selected_candidate_id: None,
            candidates: Vec::new(),
        });
    };
    let (status, candidates) = if matches!(target.kind, TargetKind::Page) {
        (
            TargetResolutionStatus::Resolved,
            vec![TargetResolutionCandidate {
                candidate_id: "candidate-1".into(),
                score: 1.0,
                reasons: vec!["semantic_fingerprint", "dom_path"],
                summary: target.label.clone(),
            }],
        )
    } else {
        let anchor = target_anchor(target).ok_or_else(ApiError::invalid)?;
        let (identifier_count, tag_match) = count_anchor_occurrences(html, anchor);
        if identifier_count == 1 && tag_match {
            let mut score = 0.92;
            let mut reasons = vec!["unique_id", "semantic_fingerprint"];
            if target
                .text_anchor
                .as_ref()
                .is_some_and(|text| html.contains(&text.exact))
            {
                score = 0.98;
                reasons.push("text_quote");
            }
            (
                TargetResolutionStatus::Resolved,
                vec![TargetResolutionCandidate {
                    candidate_id: "candidate-1".into(),
                    score,
                    reasons,
                    summary: target.label.clone(),
                }],
            )
        } else if identifier_count > 1 {
            (
                TargetResolutionStatus::Ambiguous,
                (0..identifier_count.min(5))
                    .map(|index| TargetResolutionCandidate {
                        candidate_id: format!("candidate-{}", index + 1),
                        score: 0.72,
                        reasons: vec!["unique_id", "geometry"],
                        summary: target.label.clone(),
                    })
                    .collect(),
            )
        } else {
            let quote_matches = target
                .text_anchor
                .as_ref()
                .map(|text| html.matches(&text.exact).count())
                .unwrap_or(0);
            let name_matches = anchor
                .accessible_name
                .as_ref()
                .map(|name| html.matches(name).count())
                .unwrap_or(0);
            if quote_matches > 0 || name_matches > 0 {
                (
                    TargetResolutionStatus::Ambiguous,
                    vec![TargetResolutionCandidate {
                        candidate_id: "candidate-1".into(),
                        score: 0.65,
                        reasons: vec!["text_quote"],
                        summary: target.label.clone(),
                    }],
                )
            } else {
                (TargetResolutionStatus::Detached, Vec::new())
            }
        }
    };
    let selected_candidate_id =
        matches!(status, TargetResolutionStatus::Resolved).then(|| "candidate-1".to_owned());
    Ok(TargetResolution {
        schema_version: TARGET_SCHEMA_VERSION,
        resolver_version: TARGET_RESOLVER_VERSION,
        target_id: target.target_id.clone(),
        capture_revision_id: target.capture_revision_id.clone(),
        resolved_revision_id: resolved_revision_id.to_owned(),
        status,
        selected_candidate_id,
        candidates,
    })
}

fn proposal_target_reresolution_check(
    target: &TargetRecord,
    proposed_files: &BTreeMap<String, Vec<u8>>,
    resolved_revision_id: &str,
) -> Result<StaticCheck, ApiError> {
    let resolution = resolve_target_against_files(target, proposed_files, resolved_revision_id)?;
    match resolution.status {
        TargetResolutionStatus::Resolved => Ok(StaticCheck {
            id: "target-reresolution".into(),
            status: StaticCheckStatus::Passed,
            message: "The Target resolves uniquely in the fully applied Proposed files.".into(),
        }),
        TargetResolutionStatus::Ambiguous => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "proposal_target_ambiguous",
            "The AI ChangeSet made the selected Target ambiguous. No Proposal was recorded.",
            false,
        )),
        TargetResolutionStatus::Detached => Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "proposal_target_detached",
            "The AI ChangeSet detached the selected Target. No Proposal was recorded.",
            false,
        )),
    }
}

fn target_resolution_id(resolution: &TargetResolution) -> Result<String, ApiError> {
    let bytes = serde_json::to_vec(resolution).map_err(|_| ApiError::internal())?;
    let digest = raw_sha256(&bytes);
    Ok(format!("res_{}", &digest[..32]))
}

struct AssembledContext {
    instruction: String,
    manifest: ContextManifestDto,
    canonical_json: String,
}

fn assemble_provider_context(
    project: &Project,
    context_id: &str,
    attempt_id: &str,
    target: &TargetRecord,
    resolution: &TargetResolution,
    instruction: &str,
    provider: &ProviderBinding,
) -> Result<AssembledContext, ApiError> {
    let selected_element_id = context_element_id(target);
    let selected_text = target
        .text_anchor
        .as_ref()
        .map(|anchor| anchor.exact.as_str())
        .or_else(|| selectable_text(&selected_element_id))
        .unwrap_or(&target.label);
    let (instruction, instruction_redactions) = redact_sensitive_text(instruction);
    let (selected_text, selected_text_redactions) = redact_sensitive_text(selected_text);
    let (manifest, untrusted_site_content) = assemble_context_files(project, target)?;
    let mut target_redactions = Vec::new();
    let target_value = redact_json_strings(
        canonical_safe_number_projection(
            serde_json::to_value(target).map_err(|_| ApiError::internal())?,
        )?,
        &mut target_redactions,
    );
    let mut resolution_redactions = Vec::new();
    let resolution_value = redact_json_strings(
        canonical_safe_number_projection(
            serde_json::to_value(resolution).map_err(|_| ApiError::internal())?,
        )?,
        &mut resolution_redactions,
    );
    let mut root = BTreeMap::<String, Value>::new();
    root.insert("attemptId".into(), json!(attempt_id));
    root.insert("baseRevisionId".into(), json!(project.revision_id));
    root.insert("contextId".into(), json!(context_id));
    root.insert("instruction".into(), json!(instruction));
    root.insert(
        "instructionRedactions".into(),
        json!(instruction_redactions),
    );
    root.insert(
        "manifest".into(),
        serde_json::to_value(&manifest).map_err(|_| ApiError::internal())?,
    );
    root.insert("mode".into(), json!("change"));
    root.insert(
        "numericEncoding".into(),
        json!("serde-json-shortest-decimal-string-v1"),
    );
    root.insert(
        "outputContract".into(),
        json!({
            "kind": "change_set",
            "schema": "org.synapsegit-lp-studio.change-set",
            "version": 1,
        }),
    );
    root.insert("projectId".into(), json!(project.id));
    root.insert(
        "provider".into(),
        serde_json::to_value(provider).map_err(|_| ApiError::internal())?,
    );
    root.insert(
        "providerContractVersion".into(),
        json!(ai_provider::PROVIDER_CONTRACT_VERSION),
    );
    root.insert(
        "providerSystemInstruction".into(),
        json!(ai_provider::PROVIDER_SYSTEM_INSTRUCTION),
    );
    root.insert(
        "schema".into(),
        json!("org.synapsegit-lp-studio.ai-context"),
    );
    root.insert("selectedElementId".into(), json!(selected_element_id));
    root.insert("selectedText".into(), json!(selected_text));
    root.insert(
        "selectedTextRedactions".into(),
        json!(selected_text_redactions),
    );
    root.insert("target".into(), target_value);
    root.insert("targetId".into(), json!(target.target_id));
    root.insert("targetRedactions".into(), json!(target_redactions));
    root.insert("targetResolution".into(), resolution_value);
    root.insert(
        "targetResolutionId".into(),
        json!(target_resolution_id(resolution)?),
    );
    root.insert(
        "targetResolutionRedactions".into(),
        json!(resolution_redactions),
    );
    root.insert(
        "untrustedSiteContent".into(),
        serde_json::to_value(untrusted_site_content).map_err(|_| ApiError::internal())?,
    );
    root.insert("version".into(), json!(1));
    Ok(AssembledContext {
        instruction,
        manifest,
        canonical_json: serde_json::to_string(&root).map_err(|_| ApiError::internal())?,
    })
}

fn context_element_id(target: &TargetRecord) -> String {
    target_anchor(target)
        .and_then(|anchor| anchor.unique_element_id.clone())
        .or_else(|| {
            target
                .region_anchor
                .as_ref()
                .and_then(|anchor| anchor.containing_block.as_ref())
                .and_then(|anchor| anchor.unique_element_id.clone())
        })
        .unwrap_or_else(|| match target.kind {
            TargetKind::Page => "page".into(),
            TargetKind::Block => "selected-block".into(),
            TargetKind::Element => "selected-element".into(),
            TargetKind::Text => "selected-text".into(),
            TargetKind::Point => "selected-point".into(),
            TargetKind::Region => "selected-region".into(),
        })
}

fn assemble_context_files(
    project: &Project,
    target: &TargetRecord,
) -> Result<(ContextManifestDto, Vec<UntrustedSiteContent>), ApiError> {
    let entry_path = context_entry_path(&target.page_path, &project.accepted_files)
        .ok_or_else(ApiError::invalid)?;
    let mut paths = vec![entry_path.clone()];
    paths.extend(
        project
            .accepted_files
            .keys()
            .filter(|path| **path != entry_path && context_media_type(path).is_some())
            .take(MAX_CONTEXT_FILES.saturating_sub(1))
            .cloned(),
    );
    let mut entries = Vec::new();
    let mut content_parts = Vec::new();
    let mut total_included_bytes = 0usize;
    for (index, path) in paths.into_iter().enumerate() {
        let bytes = project
            .accepted_files
            .get(&path)
            .ok_or_else(ApiError::invalid)?;
        if bytes.len() > MAX_CONTEXT_FILE_BYTES
            || total_included_bytes.saturating_add(bytes.len()) > MAX_CONTEXT_TOTAL_BYTES
        {
            if index == 0 {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ai_context_entrypoint_too_large",
                    "The selected page is too large for the bounded AI context.",
                    false,
                ));
            }
            continue;
        }
        let source = match std::str::from_utf8(bytes) {
            Ok(source) => source,
            Err(_) if index > 0 => continue,
            Err(_) => {
                return Err(ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "ai_context_entrypoint_not_utf8",
                    "The selected page is not UTF-8 text.",
                    false,
                ));
            }
        };
        let media_type = context_media_type(&path).ok_or_else(ApiError::invalid)?;
        let source_sha256 = raw_sha256(bytes);
        let (content, redactions) = redact_sensitive_text(source);
        let included_byte_length = content.len();
        if total_included_bytes.saturating_add(included_byte_length) > MAX_CONTEXT_TOTAL_BYTES {
            if index == 0 {
                return Err(ApiError::capacity());
            }
            continue;
        }
        total_included_bytes += included_byte_length;
        let end_line = content.bytes().filter(|byte| *byte == b'\n').count()
            + usize::from(!content.is_empty());
        let included_sha256 = raw_sha256(content.as_bytes());
        entries.push(ContextManifestEntryDto {
            path: path.clone(),
            media_type: media_type.into(),
            purpose: if index == 0 {
                "entrypoint"
            } else {
                "dependency"
            },
            source_byte_length: bytes.len(),
            included_byte_length,
            start_line: 1,
            end_line,
            sha256: included_sha256.clone(),
            estimated_tokens: included_byte_length.div_ceil(4),
            redacted: !redactions.is_empty(),
            truncated: false,
            redactions,
        });
        content_parts.push(UntrustedSiteContent {
            path,
            media_type: media_type.into(),
            source_sha256,
            included_sha256,
            content,
            quoted_untrusted_data: true,
        });
    }
    let estimated_tokens = total_included_bytes.div_ceil(4);
    Ok((
        ContextManifestDto {
            entries,
            total_included_bytes,
            estimated_tokens,
            screenshot_included: false,
        },
        content_parts,
    ))
}

fn context_entry_path(page_path: &str, files: &BTreeMap<String, Vec<u8>>) -> Option<String> {
    let path = page_path.trim_start_matches('/');
    let candidates = if path.is_empty() {
        vec!["index.html".into()]
    } else if path.ends_with('/') {
        vec![format!("{path}index.html")]
    } else {
        vec![path.into(), format!("{path}/index.html")]
    };
    candidates
        .into_iter()
        .find(|candidate| files.contains_key(candidate))
}

fn context_media_type(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, extension)| extension)?;
    match extension {
        "html" | "htm" => Some("text/html"),
        "css" => Some("text/css"),
        "js" | "mjs" => Some("application/javascript"),
        "json" => Some("application/json"),
        "svg" => Some("image/svg+xml"),
        "md" | "markdown" => Some("text/markdown"),
        "txt" => Some("text/plain"),
        _ => None,
    }
}

fn redact_json_strings(value: Value, redactions: &mut Vec<String>) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| redact_json_strings(value, redactions))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, redact_json_strings(value, redactions)))
                .collect(),
        ),
        Value::String(value) => {
            let (value, found) = redact_sensitive_text(&value);
            redactions.extend(found);
            redactions.sort();
            redactions.dedup();
            Value::String(value)
        }
        value => value,
    }
}

fn redact_sensitive_text(value: &str) -> (String, Vec<String>) {
    let mut redactions = Vec::new();
    let mut output = value.to_owned();
    for (prefix, minimum_suffix_length, label) in [
        ("sk-", 5, "openai_key"),
        ("bearer ", 5, "bearer_token"),
        ("ghp_", 8, "github_token"),
        ("github_pat_", 8, "github_token"),
        ("gho_", 8, "github_token"),
        ("ghu_", 8, "github_token"),
        ("ghs_", 8, "github_token"),
        ("ghr_", 8, "github_token"),
        ("akia", 16, "aws_access_key"),
        ("asia", 16, "aws_access_key"),
        ("/home/", 1, "absolute_path"),
        ("/users/", 1, "absolute_path"),
        ("c:\\users\\", 1, "absolute_path"),
    ] {
        let (next, replaced) = redact_prefixed_tokens(&output, prefix, minimum_suffix_length);
        if replaced {
            redactions.push(label.into());
            output = next;
        }
    }
    let sensitive_names = [
        "api_key",
        "apikey",
        "api-key",
        "password",
        "passwd",
        "client_secret",
        "client-secret",
        "authorization",
        "access_key",
        "access-key",
        "access_key_id",
        "access-key-id",
        "secret_access_key",
        "secret-access-key",
        "access_token",
        "access-token",
        "auth_token",
        "auth-token",
        "private_key",
        "private-key",
        "github_token",
        "github-token",
        "token",
    ];
    let mut lines = String::with_capacity(output.len());
    for line in output.split_inclusive('\n') {
        let lower = line.to_ascii_lowercase();
        let sensitive = sensitive_names.iter().any(|name| lower.contains(name));
        let separator = [lower.find('='), lower.find(':')]
            .into_iter()
            .flatten()
            .min();
        if sensitive && let Some(separator) = separator {
            lines.push_str(&line[..=separator]);
            lines.push_str("[LP_STUDIO_REDACTED]");
            if line.ends_with('\n') {
                lines.push('\n');
            }
            redactions.push("credential_assignment".into());
        } else {
            lines.push_str(line);
        }
    }
    redactions.sort();
    redactions.dedup();
    (lines, redactions)
}

fn redact_prefixed_tokens(
    value: &str,
    prefix: &str,
    minimum_suffix_length: usize,
) -> (String, bool) {
    let mut remaining = value;
    let mut output = String::with_capacity(value.len());
    let mut replaced = false;
    let lower_prefix = prefix.to_ascii_lowercase();
    while let Some(offset) = remaining.to_ascii_lowercase().find(&lower_prefix) {
        output.push_str(&remaining[..offset]);
        let token = &remaining[offset..];
        let end = token
            .char_indices()
            .skip(prefix.chars().count())
            .find(|(_, character)| {
                character.is_whitespace()
                    || matches!(
                        *character,
                        '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
                    )
            })
            .map(|(index, _)| index)
            .unwrap_or(token.len());
        if end < prefix.len().saturating_add(minimum_suffix_length) {
            output.push_str(&token[..prefix.len()]);
            remaining = &remaining[offset + prefix.len()..];
            continue;
        }
        output.push_str("[LP_STUDIO_REDACTED]");
        remaining = &token[end..];
        replaced = true;
    }
    output.push_str(remaining);
    (output, replaced)
}

/// Project JSON fractions into application-defined decimal strings before the
/// review context crosses SynapseGit's strict canonical boundary. Integer
/// tokens stay integers. Negative zero is normalized so equal measurements do
/// not acquire different review digests. This local profile is intentionally
/// versioned in the context until SynapseGit exposes a public fixed-point
/// helper (upstream issue #29).
fn canonical_safe_number_projection(value: Value) -> Result<Value, ApiError> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(canonical_safe_number_projection)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(values) => values
            .into_iter()
            .map(|(key, value)| Ok((key, canonical_safe_number_projection(value)?)))
            .collect::<Result<serde_json::Map<_, _>, ApiError>>()
            .map(Value::Object),
        Value::Number(number) if number.is_f64() => {
            let value = number
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(ApiError::invalid)?;
            Ok(Value::String(if value == 0.0 {
                "0".into()
            } else {
                number.to_string()
            }))
        }
        value => Ok(value),
    }
}

fn blank_files() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("index.html".into(), BLANK_INDEX.as_bytes().to_vec()),
        ("styles.css".into(), BLANK_STYLES.as_bytes().to_vec()),
    ])
}

fn manifest(files: &BTreeMap<String, Vec<u8>>) -> Result<RegularFileManifest, ApiError> {
    RegularFileManifest::from_entries(
        files
            .iter()
            .map(|(path, bytes)| ArtifactManifestEntry::regular_file(path, bytes.clone())),
        ArtifactLimits {
            max_files: STORAGE_MAX_FILES,
            max_file_bytes: STORAGE_MAX_FILE_BYTES as u64,
            max_total_bytes: STORAGE_MAX_TOTAL_BYTES as u64,
            max_path_bytes: STORAGE_MAX_PATH_BYTES,
            max_depth: STORAGE_MAX_DEPTH,
        },
    )
    .map_err(|_| ApiError::internal())
}

fn manifest_digest(files: &BTreeMap<String, Vec<u8>>) -> Result<String, ApiError> {
    Ok(artifact_manifest_sha256(&manifest(files)?))
}

fn validate_provider_attribution(
    context: &ContextRecord,
    attribution: &ProviderAttribution,
) -> Result<(), ApiError> {
    if attribution.attempt_id != context.attempt_id
        || attribution.provider_id != context.provider.provider_id
        || attribution.adapter_version != context.provider.adapter_version
        || attribution.requested_model != context.provider.requested_model
        || attribution.external != context.provider.external
        || attribution.provider_request_id.is_empty()
        || attribution.provider_request_id.len() > 256
        || attribution.reported_model.is_empty()
        || attribution.reported_model.len() > 128
    {
        Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "provider_attribution_mismatch",
            "The AI provider result was not bound to the reviewed attempt.",
            false,
        ))
    } else {
        Ok(())
    }
}

fn synapse_review_context_json(
    context: &ContextRecord,
    target: &TargetRecord,
    change_set_sha256: &str,
    attribution: &ProviderAttribution,
) -> Result<String, ApiError> {
    let target_digest = raw_sha256(&serde_json::to_vec(target).map_err(|_| ApiError::internal())?);
    let mut root = BTreeMap::<String, Value>::new();
    root.insert("baseRevisionId".into(), json!(context.revision_id));
    root.insert("changeSetSha256".into(), json!(change_set_sha256));
    root.insert("executionVerified".into(), json!(false));
    root.insert(
        "provenanceTarget".into(),
        json!({
            "kind": target_kind_name(target.kind),
            "pageLabel": target.page_path,
            "scope": target_kind_name(target.kind),
        }),
    );
    root.insert(
        "provider".into(),
        json!({
            "adapterVersion": attribution.adapter_version,
            "providerId": attribution.provider_id,
            "providerRequestId": attribution.provider_request_id,
            "reportedModel": attribution.reported_model,
            "requestedModel": attribution.requested_model,
        }),
    );
    root.insert("providerContextSha256".into(), json!(context.sha256));
    root.insert(
        "schema".into(),
        json!("org.synapsegit-lp-studio.synapse-review-context"),
    );
    root.insert(
        "sourceAttribution".into(),
        json!("caller_supplied_ai_attributed"),
    );
    root.insert("targetDigest".into(), json!(target_digest));
    root.insert("version".into(), json!(1));
    serde_json::to_string(&root).map_err(|_| ApiError::internal())
}

fn target_kind_name(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Page => "page",
        TargetKind::Block => "block",
        TargetKind::Element => "element",
        TargetKind::Text => "text",
        TargetKind::Point => "point",
        TargetKind::Region => "region",
    }
}

fn proposal_review_details(
    applied: &AppliedChangeSet,
) -> (Vec<ProposalChangeDto>, ProposalValidationDto) {
    let changes = applied
        .changes
        .iter()
        .map(|change| ProposalChangeDto {
            path: change.path.clone(),
            kind: match change.kind {
                AppliedChangeKind::Created => "created",
                AppliedChangeKind::Modified => "modified",
                AppliedChangeKind::Renamed => "renamed",
                AppliedChangeKind::Deleted => "deleted",
            },
            from_path: change.from_path.clone(),
        })
        .collect();
    let mut checks = applied
        .checks
        .iter()
        .map(|check| {
            let blocking = check.status == StaticCheckStatus::BlockingWarning;
            let destinations = if blocking {
                applied
                    .blocking_warnings
                    .iter()
                    .map(|warning| format!("{} → {}", warning.path, warning.destination))
                    .collect()
            } else {
                Vec::new()
            };
            ProposalValidationCheckDto {
                id: check.id.clone(),
                label: validation_check_label(&check.id).into(),
                status: if blocking { "warning" } else { "passed" },
                message: check.message.clone(),
                blocking,
                destinations,
            }
        })
        .collect::<Vec<_>>();
    checks.push(ProposalValidationCheckDto {
        id: "synapsegit-recorded".into(),
        label: "SynapseGit proposal".into(),
        status: "passed",
        message: "SynapseGit recorded the isolated caller-supplied AI proposal.".into(),
        blocking: false,
        destinations: Vec::new(),
    });
    let blocking = checks.iter().any(|check| check.blocking);
    (
        changes,
        ProposalValidationDto {
            status: if blocking { "warning" } else { "passed" },
            checks,
        },
    )
}

fn validation_check_label(id: &str) -> &'static str {
    match id {
        "protocol-schema" => "Strict ChangeSet v1",
        "base-revision" => "Accepted base binding",
        "operation-graph" => "Operation graph",
        "file-preconditions" => "File hash preconditions",
        "isolated-apply" => "Isolated atomic apply",
        "static-syntax" => "Static syntax",
        "entry-point" => "Entry point",
        "local-references" => "Local references",
        "export-deny-list" => "Export deny-list",
        "active-behavior" => "Active behavior review",
        "target-reresolution" => "Proposed Target re-resolution",
        _ => "Bounded validation",
    }
}

fn project_dto(state: &StudioState, session_id: &str, project: &Project) -> ProjectDto {
    ProjectDto {
        id: project.id.clone(),
        display_name: project.display_name.clone(),
        revision_id: project.revision_id.clone(),
        status: "ready",
        preview_url: scoped_preview_url(state, session_id, &project.id, &project.revision_id),
        accepted_manifest_sha256: project.accepted_manifest_sha256.clone(),
        files: project
            .accepted_files
            .iter()
            .map(|(path, bytes)| ProjectFileDto {
                path: path.clone(),
                byte_length: bytes.len(),
            })
            .collect(),
    }
}

fn proposal_dto(
    state: &StudioState,
    session_id: &str,
    project: &Project,
    proposal: &ProposalRecord,
) -> ProposalDto {
    ProposalDto {
        id: proposal.id.clone(),
        review_id: proposal.review_id.clone(),
        base_revision_id: proposal.base_revision_id.clone(),
        status: "pending_review",
        summary: proposal.summary.clone(),
        artifact_manifest_sha256: proposal.artifact_manifest_sha256.clone(),
        review_context_sha256: proposal.review_context_sha256.clone(),
        provider_context_sha256: proposal.provider_context_sha256.clone(),
        change_set_sha256: proposal.change_set_sha256.clone(),
        change_set: proposal.change_set.clone(),
        attribution: proposal.attribution.clone(),
        source_attribution: "caller_supplied_ai_attributed",
        execution_verified: false,
        preview_url: scoped_preview_url(state, session_id, &project.id, &proposal.id),
        changes: proposal.changes.clone(),
        unified_diff: proposal.unified_diff.clone(),
        validation: proposal.validation.clone(),
    }
}

fn consume_approval(
    approvals: &mut HashMap<[u8; 32], ApprovalGrant>,
    token: &str,
    expected: &ApprovalBinding,
    now: i64,
) -> Result<(), ApiError> {
    let hash = validate_approval(approvals, token, expected, now)?;
    approvals.remove(&hash);
    Ok(())
}

fn validate_approval(
    approvals: &mut HashMap<[u8; 32], ApprovalGrant>,
    token: &str,
    expected: &ApprovalBinding,
    now: i64,
) -> Result<[u8; 32], ApiError> {
    if token.is_empty() || token.len() > 128 {
        return Err(ApiError::conflict("approval_invalid"));
    }
    let hash = token_hash(token);
    let Some(grant) = approvals.get(&hash) else {
        return Err(ApiError::conflict("approval_invalid_or_consumed"));
    };
    if grant.expires_at < now {
        approvals.remove(&hash);
        return Err(ApiError::conflict("approval_expired"));
    }
    if &grant.binding != expected {
        return Err(ApiError::conflict("approval_binding_mismatch"));
    }
    Ok(hash)
}

fn inject_preview_bridge(
    html: &[u8],
    nonce: &str,
    editor_origin: &str,
    project_id: &str,
    snapshot_id: &str,
    revision_id: &str,
) -> Result<Vec<u8>, ApiError> {
    let html = std::str::from_utf8(html).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "preview_html_invalid",
            "The preview document is invalid.",
            false,
        )
    })?;
    let origin = serde_json::to_string(editor_origin).map_err(|_| ApiError::internal())?;
    let project = serde_json::to_string(project_id).map_err(|_| ApiError::internal())?;
    let snapshot = serde_json::to_string(snapshot_id).map_err(|_| ApiError::internal())?;
    let revision = serde_json::to_string(revision_id).map_err(|_| ApiError::internal())?;
    let mut script = format!(
        r##"<script nonce="{nonce}">(()=>{{"use strict";window.name="";const O={origin},P={project},S={snapshot},R={revision},V=location.origin,B="/preview/"+encodeURIComponent(P)+"/"+encodeURIComponent(S)+"/",A=V+B,startsWith=Function.call.bind(String.prototype.startsWith),cancel=Function.call.bind(Event.prototype.preventDefault),seen=new Set(),pending=[];let mode="interact",channel="";const valid=s=>typeof s==="string"&&s.length>0&&s.length<=128,send=code=>{{try{{parent.postMessage({{type:"synapsegit-lp.diagnostic",schemaVersion:"1",channelId:channel,projectId:P,snapshotId:S,revisionId:R,severity:code==="csp_blocked"?"warning":"error",code,sourceUnavailable:true}},O);}}catch{{}}}},diagnose=code=>{{if(seen.has(code))return;seen.add(code);channel?send(code):pending.push(code);}},deny=()=>{{diagnose("csp_blocked");throw new DOMException("Blocked","SecurityError");}},lock=(owner,name)=>{{try{{Object.defineProperty(owner,name,{{value:deny,writable:false,configurable:false}});return true;}}catch{{return false;}}}};["open","write","writeln"].forEach(name=>{{const prototypeSafe=lock(Document.prototype,name),documentSafe=lock(document,name);if(!prototypeSafe||!documentSafe)diagnose("csp_blocked");}});const getter=(owner,name)=>{{try{{const value=Object.getOwnPropertyDescriptor(owner,name)?.get;return typeof value==="function"?Function.call.bind(value):null;}}catch{{return null;}}}},nav=window.navigation,getDestination=typeof NavigateEvent==="function"?getter(NavigateEvent.prototype,"destination"):null,getUrl=typeof NavigationDestination==="function"?getter(NavigationDestination.prototype,"url"):null;if(!nav||typeof nav.addEventListener!=="function")diagnose("csp_blocked");else nav.addEventListener("navigate",e=>{{let destination="";try{{destination=getDestination&&getUrl?getUrl(getDestination(e)):"";}}catch{{}}if(typeof destination!=="string"||!startsWith(destination,A)){{try{{cancel(e);}}catch{{}}diagnose("csp_blocked");}}}});addEventListener("securitypolicyviolation",()=>diagnose("csp_blocked"));addEventListener("error",()=>diagnose("site_error"),true);addEventListener("unhandledrejection",()=>diagnose("unhandled_rejection"));addEventListener("message",e=>{{const m=e.data;if(e.origin!==O||e.source!==parent||!m||m.type!=="synapsegit-lp.action"||m.schemaVersion!=="1"||m.projectId!==P||m.snapshotId!==S||m.revisionId!==R||!valid(m.channelId))return;if(m.action==="set_mode"&&(m.mode==="select"||m.mode==="interact")){{channel=m.channelId;mode=m.mode;pending.splice(0).forEach(send);}}else if(m.action==="clear_selection"&&channel===m.channelId){{document.querySelectorAll("[data-lp-selected]").forEach(n=>n.removeAttribute("data-lp-selected"));}}}});addEventListener("click",e=>{{if(mode!=="select"||!channel)return;const n=e.target instanceof Element?e.target.closest("[data-lp-id]"):null;if(!n)return;e.preventDefault();e.stopPropagation();const id=n.getAttribute("data-lp-id"),r=n.getBoundingClientRect();if(!id||id.length>128)return;parent.postMessage({{type:"synapsegit-lp.selection",schemaVersion:"1",channelId:channel,projectId:P,snapshotId:S,revisionId:R,elementId:id,rect:{{x:r.x,y:r.y,width:r.width,height:r.height}}}},O);}},true);}})();</script>"##
    );
    script.push_str(&format!(
        r#"<script nonce="{nonce}">(()=>{{"use strict";const blocked=()=>{{dispatchEvent(new Event("securitypolicyviolation"));throw new DOMException("Blocked","SecurityError");}};for(const name of ["RTCPeerConnection","webkitRTCPeerConnection"]){{try{{Object.defineProperty(window,name,{{value:blocked,writable:false,configurable:false}});}}catch{{dispatchEvent(new Event("securitypolicyviolation"));}}}}}})();</script>"#
    ));
    script.push_str(&format!(
        r#"<script nonce="{nonce}">({runtime})({origin},{project},{snapshot},{revision});</script>"#,
        runtime = PREVIEW_TARGET_RUNTIME.trim().trim_end_matches(';')
    ));
    let mut output = String::with_capacity(html.len() + script.len());
    let insertion = preview_bridge_insertion(html);
    output.push_str(&html[..insertion]);
    output.push_str(&script);
    output.push_str(&html[insertion..]);
    Ok(output.into_bytes())
}

fn preview_bridge_insertion(html: &str) -> usize {
    let bytes = html.as_bytes();
    let mut offset = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    while bytes
        .get(offset)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        offset += 1;
    }
    const DOCTYPE: &str = "<!doctype html>";
    if html[offset..]
        .get(..DOCTYPE.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(DOCTYPE))
    {
        offset + DOCTYPE.len()
    } else {
        0
    }
}

fn deterministic_zip(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, ApiError> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    for (path, bytes) in files {
        writer
            .start_file(path, options)
            .map_err(|_| ApiError::internal())?;
        writer.write_all(bytes).map_err(|_| ApiError::internal())?;
    }
    writer
        .finish()
        .map(Cursor::into_inner)
        .map_err(|_| ApiError::internal())
}

fn opaque_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn random_token(bytes: usize) -> Result<String, ApiError> {
    let mut value = vec![0_u8; bytes];
    getrandom::fill(&mut value).map_err(|_| ApiError::internal())?;
    Ok(URL_SAFE_NO_PAD.encode(value))
}

fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn raw_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn now_rfc3339() -> Result<String, ApiError> {
    Ok(canonical_timestamp(OffsetDateTime::now_utc()))
}

fn canonical_timestamp(value: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
        value.year(),
        value.month() as u8,
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.nanosecond()
    )
}

fn format_timestamp(seconds: i64) -> Result<String, ApiError> {
    OffsetDateTime::from_unix_timestamp(seconds)
        .map_err(|_| ApiError::internal())?
        .format(&Rfc3339)
        .map_err(|_| ApiError::internal())
}

#[cfg(test)]
mod tests;
