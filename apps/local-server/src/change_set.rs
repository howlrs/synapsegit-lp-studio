use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use unicase::UniCase;
use unicode_normalization::UnicodeNormalization;
use url::Url;

const CHANGE_SET_SCHEMA: &str = "org.synapsegit-lp-studio.change-set";
const CHANGE_SET_VERSION: u8 = 1;
const REDACTION_PLACEHOLDER: &str = "LP_STUDIO_REDACTED";
const MAX_OPERATIONS: usize = 32;
const MAX_PATH_BYTES: usize = 512;
const MAX_PATH_DEPTH: usize = 16;
const MAX_SUMMARY_BYTES: usize = 2_000;
const MAX_GENERATED_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_GENERATED_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_UNIFIED_DIFF_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ChangeSetV1 {
    pub(super) schema: String,
    pub(super) version: u8,
    pub(super) base_revision_id: String,
    pub(super) summary: String,
    pub(super) operations: Vec<ChangeOperationV1>,
}

impl ChangeSetV1 {
    // Kept as the typed, deterministic construction boundary for provider
    // adapters. The parser path remains authoritative even when an adapter
    // currently returns raw JSON.
    #[allow(dead_code)]
    pub(super) fn new(
        base_revision_id: impl Into<String>,
        summary: impl Into<String>,
        operations: Vec<ChangeOperationV1>,
    ) -> Self {
        Self {
            schema: CHANGE_SET_SCHEMA.to_owned(),
            version: CHANGE_SET_VERSION,
            base_revision_id: base_revision_id.into(),
            summary: summary.into(),
            operations,
        }
    }

    /// Serialization is deterministic because this contract contains no
    /// unordered maps and every field has a fixed declaration order.
    pub(super) fn to_json(&self) -> Result<String, ChangeSetError> {
        serde_json::to_string(self).map_err(|_| ChangeSetError::InvalidProtocol)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(super) enum ChangeOperationV1 {
    #[serde(rename = "create_text")]
    CreateText {
        path: String,
        #[serde(rename = "mediaType")]
        media_type: String,
        content: String,
    },
    #[serde(rename = "replace_text")]
    ReplaceText {
        path: String,
        #[serde(rename = "expectedSha256")]
        expected_sha256: String,
        #[serde(rename = "mediaType")]
        media_type: String,
        content: String,
    },
    #[serde(rename = "rename")]
    Rename {
        from: String,
        to: String,
        #[serde(rename = "expectedSha256")]
        expected_sha256: String,
    },
    #[serde(rename = "delete")]
    Delete {
        path: String,
        #[serde(rename = "expectedSha256")]
        expected_sha256: String,
    },
}

impl ChangeOperationV1 {
    #[allow(dead_code)]
    pub(super) fn create_text(
        path: impl Into<String>,
        media_type: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::CreateText {
            path: path.into(),
            media_type: media_type.into(),
            content: content.into(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn replace_text(
        path: impl Into<String>,
        expected_sha256: impl Into<String>,
        media_type: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::ReplaceText {
            path: path.into(),
            expected_sha256: expected_sha256.into(),
            media_type: media_type.into(),
            content: content.into(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn rename(
        from: impl Into<String>,
        to: impl Into<String>,
        expected_sha256: impl Into<String>,
    ) -> Self {
        Self::Rename {
            from: from.into(),
            to: to.into(),
            expected_sha256: expected_sha256.into(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn delete(path: impl Into<String>, expected_sha256: impl Into<String>) -> Self {
        Self::Delete {
            path: path.into(),
            expected_sha256: expected_sha256.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AppliedChangeKind {
    Created,
    Modified,
    Renamed,
    Deleted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AppliedFileChange {
    pub(super) path: String,
    pub(super) kind: AppliedChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) from_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) before_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) after_sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StaticCheckStatus {
    Passed,
    AdvisoryWarning,
    BlockingWarning,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StaticCheck {
    pub(super) id: String,
    pub(super) status: StaticCheckStatus,
    pub(super) message: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BlockingWarning {
    pub(super) code: String,
    pub(super) path: String,
    pub(super) destination: String,
    pub(super) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AppliedChangeSet {
    pub(super) change_set: ChangeSetV1,
    pub(super) files: BTreeMap<String, Vec<u8>>,
    pub(super) changes: Vec<AppliedFileChange>,
    pub(super) unified_diff: String,
    pub(super) checks: Vec<StaticCheck>,
    pub(super) blocking_warnings: Vec<BlockingWarning>,
}

type AppliedFilesAndChanges = (BTreeMap<String, Vec<u8>>, Vec<AppliedFileChange>);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub(super) enum ChangeSetError {
    #[error("the ChangeSet JSON or envelope is invalid")]
    InvalidProtocol,
    #[error("the ChangeSet base revision is stale")]
    StaleBase,
    #[error("the ChangeSet exceeds a configured limit")]
    LimitExceeded,
    #[error("a ChangeSet path is unsafe")]
    UnsafePath,
    #[error("a ChangeSet path is reserved")]
    ReservedPath,
    #[error("a media type does not match its text file extension")]
    UnsupportedMediaType,
    #[error("a SHA-256 precondition is malformed")]
    InvalidHash,
    #[error("ChangeSet operations conflict")]
    OperationConflict,
    #[error("the ChangeSet contains a rename cycle")]
    RenameCycle,
    #[error("a file precondition does not match the Accepted revision")]
    PreconditionFailed,
    #[error("the resulting site does not contain index.html")]
    MissingEntryPoint,
    #[error("a generated text file has invalid static syntax")]
    InvalidSyntax,
    #[error("the resulting site contains a missing or unsafe local reference")]
    InvalidLocalReference,
    #[error("generated content contains a context-redaction placeholder")]
    ReservedPlaceholder,
}

impl ChangeSetError {
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidProtocol => "change_set_invalid_protocol",
            Self::StaleBase => "change_set_stale_base",
            Self::LimitExceeded => "change_set_limit_exceeded",
            Self::UnsafePath => "change_set_unsafe_path",
            Self::ReservedPath => "change_set_reserved_path",
            Self::UnsupportedMediaType => "change_set_unsupported_media_type",
            Self::InvalidHash => "change_set_invalid_hash",
            Self::OperationConflict => "change_set_operation_conflict",
            Self::RenameCycle => "change_set_rename_cycle",
            Self::PreconditionFailed => "change_set_precondition_failed",
            Self::MissingEntryPoint => "change_set_missing_entry_point",
            Self::InvalidSyntax => "change_set_invalid_syntax",
            Self::InvalidLocalReference => "change_set_invalid_local_reference",
            Self::ReservedPlaceholder => "change_set_reserved_placeholder",
        }
    }
}

pub(super) fn parse_and_apply_change_set(
    raw: &str,
    expected_base_revision_id: &str,
    accepted: &BTreeMap<String, Vec<u8>>,
) -> Result<AppliedChangeSet, ChangeSetError> {
    let change_set =
        serde_json::from_str::<ChangeSetV1>(raw).map_err(|_| ChangeSetError::InvalidProtocol)?;
    validate_envelope(&change_set, expected_base_revision_id)?;
    validate_accepted_paths(accepted)?;
    validate_operations(&change_set.operations)?;
    validate_operation_graph(&change_set.operations)?;
    validate_preconditions(&change_set.operations, accepted)?;

    let (files, changes) = apply_to_clone(&change_set.operations, accepted)?;
    validate_final_case_collisions(&files)?;
    let html_recovery_warning_count = validate_static_syntax(&files)?;
    if !files.contains_key("index.html") {
        return Err(ChangeSetError::MissingEntryPoint);
    }
    let incomplete_reference_sources = validate_local_references(&files)?;
    validate_export_deny_list(&files)?;

    let removes_existing_path = accepted.keys().any(|path| !files.contains_key(path));
    let introduced_incomplete_reference_sources = incomplete_reference_sources
        .into_iter()
        .filter(|path| removes_existing_path || accepted.get(path) != files.get(path))
        .collect::<BTreeSet<_>>();
    let blocking_warnings = active_behavior_warnings(accepted, &files)
        .into_iter()
        .chain(
            introduced_incomplete_reference_sources
                .iter()
                .map(|path| BlockingWarning {
                    code: "static_reference_analysis_incomplete".to_owned(),
                    path: path.clone(),
                    destination: "bounded-static-reference-analysis".to_owned(),
                    message: "The proposal introduces reference syntax that the bounded static analyzer cannot exhaustively classify.".to_owned(),
                }),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let checks = ordered_checks(
        &files,
        html_recovery_warning_count,
        &blocking_warnings,
        &introduced_incomplete_reference_sources,
    );
    let unified_diff = build_unified_diff(accepted, &files, &changes);

    Ok(AppliedChangeSet {
        change_set,
        files,
        changes,
        unified_diff,
        checks,
        blocking_warnings,
    })
}

fn validate_envelope(
    change_set: &ChangeSetV1,
    expected_base_revision_id: &str,
) -> Result<(), ChangeSetError> {
    if change_set.schema != CHANGE_SET_SCHEMA
        || change_set.version != CHANGE_SET_VERSION
        || change_set.base_revision_id.is_empty()
        || change_set.base_revision_id.len() > 128
        || change_set.summary.trim().is_empty()
        || change_set.summary.len() > MAX_SUMMARY_BYTES
        || change_set.summary.chars().any(is_disallowed_control)
        || change_set.operations.is_empty()
    {
        return Err(ChangeSetError::InvalidProtocol);
    }
    if change_set.base_revision_id != expected_base_revision_id {
        return Err(ChangeSetError::StaleBase);
    }
    if change_set.operations.len() > MAX_OPERATIONS {
        return Err(ChangeSetError::LimitExceeded);
    }
    Ok(())
}

fn validate_accepted_paths(accepted: &BTreeMap<String, Vec<u8>>) -> Result<(), ChangeSetError> {
    let mut folded = BTreeSet::new();
    for path in accepted.keys() {
        validate_path(path)?;
        if !folded.insert(folded_path(path)) {
            return Err(ChangeSetError::OperationConflict);
        }
    }
    Ok(())
}

fn validate_operations(operations: &[ChangeOperationV1]) -> Result<(), ChangeSetError> {
    let mut generated_total = 0_usize;
    for operation in operations {
        match operation {
            ChangeOperationV1::CreateText {
                path,
                media_type,
                content,
            }
            | ChangeOperationV1::ReplaceText {
                path,
                media_type,
                content,
                ..
            } => {
                validate_path(path)?;
                validate_text_media_type(path, media_type)?;
                if content.contains(REDACTION_PLACEHOLDER) {
                    return Err(ChangeSetError::ReservedPlaceholder);
                }
                if content.len() > MAX_GENERATED_FILE_BYTES {
                    return Err(ChangeSetError::LimitExceeded);
                }
                generated_total = generated_total
                    .checked_add(content.len())
                    .ok_or(ChangeSetError::LimitExceeded)?;
                if generated_total > MAX_GENERATED_TOTAL_BYTES {
                    return Err(ChangeSetError::LimitExceeded);
                }
            }
            ChangeOperationV1::Rename {
                from,
                to,
                expected_sha256,
            } => {
                validate_path(from)?;
                validate_path(to)?;
                validate_hash(expected_sha256)?;
                if from == to
                    || !matches!(
                        (extension(from), extension(to)),
                        (Some(from_extension), Some(to_extension))
                            if from_extension.eq_ignore_ascii_case(to_extension)
                    )
                {
                    return Err(ChangeSetError::OperationConflict);
                }
            }
            ChangeOperationV1::Delete {
                path,
                expected_sha256,
            } => {
                validate_path(path)?;
                validate_hash(expected_sha256)?;
            }
        }
        if let ChangeOperationV1::ReplaceText {
            expected_sha256, ..
        } = operation
        {
            validate_hash(expected_sha256)?;
        }
    }
    Ok(())
}

fn validate_operation_graph(operations: &[ChangeOperationV1]) -> Result<(), ChangeSetError> {
    let rename_edges = operations
        .iter()
        .filter_map(|operation| match operation {
            ChangeOperationV1::Rename { from, to, .. } => {
                Some((folded_path(from), folded_path(to)))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    if contains_rename_cycle(&rename_edges) {
        return Err(ChangeSetError::RenameCycle);
    }

    let mut owners = HashMap::<String, usize>::new();
    for (index, operation) in operations.iter().enumerate() {
        for path in operation_claims(operation) {
            let folded = folded_path(path);
            if owners
                .insert(folded, index)
                .is_some_and(|owner| owner != index)
            {
                return Err(ChangeSetError::OperationConflict);
            }
        }
    }
    Ok(())
}

fn contains_rename_cycle(edges: &BTreeMap<String, String>) -> bool {
    for start in edges.keys() {
        let mut visited = BTreeSet::new();
        let mut cursor = start;
        while let Some(next) = edges.get(cursor) {
            if !visited.insert(cursor.clone()) {
                return true;
            }
            cursor = next;
        }
    }
    false
}

fn operation_claims(operation: &ChangeOperationV1) -> Vec<&str> {
    match operation {
        ChangeOperationV1::CreateText { path, .. }
        | ChangeOperationV1::ReplaceText { path, .. }
        | ChangeOperationV1::Delete { path, .. } => vec![path],
        ChangeOperationV1::Rename { from, to, .. } => vec![from, to],
    }
}

fn validate_preconditions(
    operations: &[ChangeOperationV1],
    accepted: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ChangeSetError> {
    let accepted_folded = accepted
        .keys()
        .map(|path| (folded_path(path), path))
        .collect::<BTreeMap<_, _>>();
    for operation in operations {
        match operation {
            ChangeOperationV1::CreateText { path, .. } => {
                if accepted_folded.contains_key(&folded_path(path)) {
                    return Err(ChangeSetError::PreconditionFailed);
                }
            }
            ChangeOperationV1::ReplaceText {
                path,
                expected_sha256,
                ..
            }
            | ChangeOperationV1::Delete {
                path,
                expected_sha256,
            } => require_file_hash(accepted, path, expected_sha256)?,
            ChangeOperationV1::Rename {
                from,
                to,
                expected_sha256,
            } => {
                require_file_hash(accepted, from, expected_sha256)?;
                if accepted_folded.contains_key(&folded_path(to)) {
                    return Err(ChangeSetError::PreconditionFailed);
                }
            }
        }
    }
    Ok(())
}

fn require_file_hash(
    files: &BTreeMap<String, Vec<u8>>,
    path: &str,
    expected_sha256: &str,
) -> Result<(), ChangeSetError> {
    let bytes = files.get(path).ok_or(ChangeSetError::PreconditionFailed)?;
    if sha256(bytes) == expected_sha256 {
        Ok(())
    } else {
        Err(ChangeSetError::PreconditionFailed)
    }
}

fn apply_to_clone(
    operations: &[ChangeOperationV1],
    accepted: &BTreeMap<String, Vec<u8>>,
) -> Result<AppliedFilesAndChanges, ChangeSetError> {
    let mut files = accepted.clone();
    let mut changes = Vec::with_capacity(operations.len());
    for operation in operations {
        match operation {
            ChangeOperationV1::CreateText { path, content, .. } => {
                let bytes = content.as_bytes().to_vec();
                let after_sha256 = sha256(&bytes);
                if files.insert(path.clone(), bytes).is_some() {
                    return Err(ChangeSetError::PreconditionFailed);
                }
                changes.push(AppliedFileChange {
                    path: path.clone(),
                    kind: AppliedChangeKind::Created,
                    from_path: None,
                    before_sha256: None,
                    after_sha256: Some(after_sha256),
                });
            }
            ChangeOperationV1::ReplaceText { path, content, .. } => {
                let before = files.get(path).ok_or(ChangeSetError::PreconditionFailed)?;
                let before_sha256 = sha256(before);
                let bytes = content.as_bytes().to_vec();
                let after_sha256 = sha256(&bytes);
                files.insert(path.clone(), bytes);
                changes.push(AppliedFileChange {
                    path: path.clone(),
                    kind: AppliedChangeKind::Modified,
                    from_path: None,
                    before_sha256: Some(before_sha256),
                    after_sha256: Some(after_sha256),
                });
            }
            ChangeOperationV1::Rename { from, to, .. } => {
                let bytes = files
                    .remove(from)
                    .ok_or(ChangeSetError::PreconditionFailed)?;
                let digest = sha256(&bytes);
                if files.insert(to.clone(), bytes).is_some() {
                    return Err(ChangeSetError::PreconditionFailed);
                }
                changes.push(AppliedFileChange {
                    path: to.clone(),
                    kind: AppliedChangeKind::Renamed,
                    from_path: Some(from.clone()),
                    before_sha256: Some(digest.clone()),
                    after_sha256: Some(digest),
                });
            }
            ChangeOperationV1::Delete { path, .. } => {
                let before = files
                    .remove(path)
                    .ok_or(ChangeSetError::PreconditionFailed)?;
                changes.push(AppliedFileChange {
                    path: path.clone(),
                    kind: AppliedChangeKind::Deleted,
                    from_path: None,
                    before_sha256: Some(sha256(&before)),
                    after_sha256: None,
                });
            }
        }
    }
    Ok((files, changes))
}

fn validate_final_case_collisions(files: &BTreeMap<String, Vec<u8>>) -> Result<(), ChangeSetError> {
    let mut folded = BTreeSet::new();
    if files.keys().all(|path| folded.insert(folded_path(path))) {
        Ok(())
    } else {
        Err(ChangeSetError::OperationConflict)
    }
}

fn validate_static_syntax(files: &BTreeMap<String, Vec<u8>>) -> Result<usize, ChangeSetError> {
    super::static_syntax::validate_resulting_site(files).map_err(|_| ChangeSetError::InvalidSyntax)
}

fn validate_local_references(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeSet<String>, ChangeSetError> {
    let mut incomplete_sources = BTreeSet::new();
    for (source_path, bytes) in files {
        let scan = super::static_export::scan_references(source_path, bytes)
            .map_err(|_| ChangeSetError::InvalidLocalReference)?;
        if scan.analysis_incomplete {
            incomplete_sources.insert(source_path.clone());
        }
        for reference in scan.references {
            let Some(resolved) = resolve_local_reference(source_path, &reference)? else {
                continue;
            };
            if !files.contains_key(&resolved)
                && !files.contains_key(&format!("{resolved}/index.html"))
            {
                return Err(ChangeSetError::InvalidLocalReference);
            }
        }
    }
    Ok(incomplete_sources)
}

fn validate_export_deny_list(files: &BTreeMap<String, Vec<u8>>) -> Result<(), ChangeSetError> {
    for path in files.keys() {
        if reserved_path(path) {
            return Err(ChangeSetError::ReservedPath);
        }
    }
    Ok(())
}

fn ordered_checks(
    files: &BTreeMap<String, Vec<u8>>,
    html_recovery_warning_count: usize,
    warnings: &[BlockingWarning],
    introduced_incomplete_reference_sources: &BTreeSet<String>,
) -> Vec<StaticCheck> {
    let passed = |id: &str, message: &str| StaticCheck {
        id: id.to_owned(),
        status: StaticCheckStatus::Passed,
        message: message.to_owned(),
    };
    let mut checks = vec![
        passed(
            "protocol-schema",
            "ChangeSet v1 schema is strict and supported.",
        ),
        passed(
            "base-revision",
            "The ChangeSet is bound to the current Accepted revision.",
        ),
        passed(
            "operation-graph",
            "Operation paths and dependencies do not conflict.",
        ),
        passed(
            "file-preconditions",
            "Every required file SHA-256 precondition matches.",
        ),
        passed(
            "isolated-apply",
            "All operations were applied to an isolated in-memory clone.",
        ),
        StaticCheck {
            id: "static-syntax".to_owned(),
            status: if html_recovery_warning_count == 0 {
                StaticCheckStatus::Passed
            } else {
                StaticCheckStatus::AdvisoryWarning
            },
            message: if html_recovery_warning_count == 0 {
                "The resulting site passed strict JSON, CSS, JavaScript, XML, and SVG parser checks; HTML5 parsing required no recovery."
                    .to_owned()
            } else {
                format!(
                    "HTML5 parsing reported a bounded {html_recovery_warning_count} recoverable parse error(s); strict JSON, CSS, JavaScript, XML, and SVG parser checks still passed."
                )
            },
        },
        passed("entry-point", "The resulting site retains index.html."),
        StaticCheck {
            id: "local-references".to_owned(),
            status: if introduced_incomplete_reference_sources.is_empty() {
                StaticCheckStatus::Passed
            } else {
                StaticCheckStatus::BlockingWarning
            },
            message: if introduced_incomplete_reference_sources.is_empty() {
                "Bounded static local references introduced or changed by this proposal resolve inside the site."
                    .to_owned()
            } else {
                format!(
                    "{} changed file(s) contain reference syntax that the bounded analyzer cannot exhaustively classify.",
                    introduced_incomplete_reference_sources.len()
                )
            },
        },
        passed(
            "export-deny-list",
            "The resulting manifest contains no reserved Studio path.",
        ),
    ];
    checks.push(accessibility_smoke_check(files));
    checks.push(StaticCheck {
        id: "active-behavior".to_owned(),
        status: if warnings.is_empty() {
            StaticCheckStatus::Passed
        } else {
            StaticCheckStatus::BlockingWarning
        },
        message: if warnings.is_empty() {
            "No new active or externally connected behavior was detected.".to_owned()
        } else {
            format!(
                "{} blocking review item(s) require explicit Human review.",
                warnings.len()
            )
        },
    });
    checks
}

#[derive(Default)]
struct AccessibilitySmokeSummary {
    html_documents: usize,
    documents_without_language: usize,
    duplicate_ids: usize,
    images_without_alt: usize,
    form_controls_without_name: usize,
}

fn accessibility_smoke_check(files: &BTreeMap<String, Vec<u8>>) -> StaticCheck {
    let mut summary = AccessibilitySmokeSummary::default();
    for (path, bytes) in files {
        if text_media_type_for_path(path) != Some("text/html") {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        summary.html_documents += 1;
        let tags = scan_markup_tags(text, true);
        let mut ids = BTreeMap::<String, usize>::new();
        let mut labelled_ids = BTreeSet::<String>::new();
        for tag in &tags {
            if tag.closing {
                continue;
            }
            let attributes = tag.attributes.iter().cloned().collect::<BTreeMap<_, _>>();
            if let Some(id) = attributes.get("id").filter(|id| !id.trim().is_empty()) {
                *ids.entry(id.clone()).or_default() += 1;
            }
            if tag.name == "label"
                && let Some(labelled) = attributes
                    .get("for")
                    .filter(|labelled| !labelled.trim().is_empty())
            {
                labelled_ids.insert(labelled.clone());
            }
        }
        summary.duplicate_ids += ids
            .values()
            .map(|count| count.saturating_sub(1))
            .sum::<usize>();
        let document_has_language = tags.iter().any(|tag| {
            !tag.closing
                && tag.name == "html"
                && tag
                    .attributes
                    .iter()
                    .any(|(name, value)| name == "lang" && !value.trim().is_empty())
        });
        if !document_has_language {
            summary.documents_without_language += 1;
        }
        for tag in tags {
            if tag.closing {
                continue;
            }
            let attributes = tag.attributes.into_iter().collect::<BTreeMap<_, _>>();
            if tag.name == "img" && !attributes.contains_key("alt") {
                summary.images_without_alt += 1;
            }
            if matches!(tag.name.as_str(), "input" | "select" | "textarea") {
                let hidden_input = tag.name == "input"
                    && attributes
                        .get("type")
                        .is_some_and(|value| value.eq_ignore_ascii_case("hidden"));
                let named_directly =
                    ["aria-label", "aria-labelledby", "title"]
                        .iter()
                        .any(|name| {
                            attributes
                                .get(*name)
                                .is_some_and(|value| !value.trim().is_empty())
                        });
                let named_by_label = attributes
                    .get("id")
                    .is_some_and(|id| labelled_ids.contains(id));
                if !hidden_input && !named_directly && !named_by_label {
                    summary.form_controls_without_name += 1;
                }
            }
        }
    }
    let issue_count = summary.documents_without_language
        + summary.duplicate_ids
        + summary.images_without_alt
        + summary.form_controls_without_name;
    StaticCheck {
        id: "accessibility-smoke".to_owned(),
        status: if issue_count == 0 {
            StaticCheckStatus::Passed
        } else {
            StaticCheckStatus::AdvisoryWarning
        },
        message: if issue_count == 0 {
            format!(
                "Bounded accessibility smoke found no missing language, duplicate id, image alt, or form-name issue across {} HTML document(s); this is not WCAG conformance.",
                summary.html_documents
            )
        } else {
            format!(
                "Bounded accessibility smoke found {issue_count} issue(s): {} document language, {} duplicate id, {} image alt, {} form-name; this is not WCAG conformance.",
                summary.documents_without_language,
                summary.duplicate_ids,
                summary.images_without_alt,
                summary.form_controls_without_name
            )
        },
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BehaviorFeature {
    code: String,
    path: String,
    destination: String,
    fingerprint: String,
}

impl BehaviorFeature {
    fn witnessed(
        code: &str,
        path: &str,
        destination: impl Into<String>,
        fingerprint: impl Into<String>,
    ) -> Self {
        Self {
            code: code.to_owned(),
            path: path.to_owned(),
            destination: destination.into(),
            fingerprint: fingerprint.into(),
        }
    }
}

fn active_behavior_warnings(
    accepted: &BTreeMap<String, Vec<u8>>,
    proposed: &BTreeMap<String, Vec<u8>>,
) -> Vec<BlockingWarning> {
    let before = behavior_features(accepted);
    let proposed = behavior_features(proposed);
    proposed
        .difference(&before)
        // A passive external reference (for example a stylesheet, font, image,
        // or ordinary hyperlink) is normal published-LP content. It remains
        // visible in the generated diff, but must not be treated as executable
        // behavior that prevents an explicit Human Decision.
        .filter(|feature| feature.code != "new_external_origin")
        .map(|feature| BlockingWarning {
            code: feature.code.clone(),
            path: feature.path.clone(),
            destination: feature.destination.clone(),
            message: behavior_message(&feature.code).to_owned(),
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn behavior_message(code: &str) -> &'static str {
    match code {
        "new_external_origin" => "The proposal introduces a new external network origin.",
        "new_form_action" => "The proposal introduces a form submission destination.",
        "new_script" => "The proposal introduces executable script content.",
        "changed_script_content" => {
            "The proposal creates or changes JavaScript content that may become executable."
        }
        "new_inline_event_handler" => {
            "The proposal introduces inline event-handler script behavior."
        }
        "new_iframe" => "The proposal introduces embedded frame content.",
        "new_download" => "The proposal introduces download behavior.",
        "new_analytics" => "The proposal introduces analytics-like behavior.",
        "new_cookie_behavior" => "The proposal introduces cookie behavior.",
        _ => "The proposal introduces active behavior.",
    }
}

fn behavior_features(files: &BTreeMap<String, Vec<u8>>) -> BTreeSet<BehaviorFeature> {
    let mut features = BTreeSet::new();
    for (path, bytes) in files {
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let media_type = text_media_type_for_path(path);
        if media_type == Some("application/javascript") {
            features.insert(BehaviorFeature::witnessed(
                "changed_script_content",
                path,
                path.clone(),
                sha256(bytes),
            ));
        }
        let mut raw_origin_occurrences = BTreeMap::<String, usize>::new();
        for origin in external_origins(text) {
            let occurrence = next_occurrence(&mut raw_origin_occurrences, &origin);
            features.insert(BehaviorFeature::witnessed(
                "new_external_origin",
                path,
                origin,
                format!("raw:{occurrence}"),
            ));
        }
        let mut static_origin_occurrences = BTreeMap::<String, usize>::new();
        if let Ok(scan) = super::static_export::scan_references(path, bytes) {
            for origin in scan
                .references
                .iter()
                .filter_map(|reference| external_reference_origin(reference))
            {
                let occurrence = next_occurrence(&mut static_origin_occurrences, &origin);
                features.insert(BehaviorFeature::witnessed(
                    "new_external_origin",
                    path,
                    origin,
                    format!("static:{occurrence}"),
                ));
            }
        }
        let lower = text.to_ascii_lowercase();
        for marker in [
            "google-analytics",
            "googletagmanager",
            "gtag(",
            "analytics(",
            "plausible.io",
            "segment.com",
            "mixpanel",
            "matomo",
            "fbq(",
        ] {
            for occurrence in 1..=lower.matches(marker).count() {
                features.insert(BehaviorFeature::witnessed(
                    "new_analytics",
                    path,
                    marker,
                    occurrence.to_string(),
                ));
            }
        }
        for marker in ["document.cookie", "cookie="] {
            for occurrence in 1..=lower.matches(marker).count() {
                features.insert(BehaviorFeature::witnessed(
                    "new_cookie_behavior",
                    path,
                    marker,
                    occurrence.to_string(),
                ));
            }
        }
        if !matches!(
            media_type,
            Some("text/html" | "image/svg+xml" | "application/xml")
        ) {
            continue;
        }
        let mut script_occurrences = BTreeMap::<String, usize>::new();
        let mut event_occurrences = BTreeMap::<String, usize>::new();
        let mut form_occurrences = BTreeMap::<String, usize>::new();
        let mut iframe_occurrences = BTreeMap::<String, usize>::new();
        let mut download_occurrences = BTreeMap::<String, usize>::new();
        for tag in scan_markup_tags(text, media_type == Some("text/html")) {
            if tag.closing {
                continue;
            }
            for (name, value) in &tag.attributes {
                if is_inline_event_handler_attribute(name) && !value.trim().is_empty() {
                    let occurrence_key = format!("{}:{name}", tag.name);
                    let occurrence = next_occurrence(&mut event_occurrences, &occurrence_key);
                    features.insert(BehaviorFeature::witnessed(
                        "new_inline_event_handler",
                        path,
                        format!("{}[{name}]", tag.name),
                        format!("{occurrence}:{}", sha256(value.as_bytes())),
                    ));
                }
            }
            let attributes = tag.attributes.iter().cloned().collect::<BTreeMap<_, _>>();
            let attributes_sha256 = canonical_attributes_sha256(&tag.attributes);
            match tag.name.as_str() {
                "form" => {
                    if let Some(action) = attributes.get("action") {
                        let occurrence = next_occurrence(&mut form_occurrences, action);
                        features.insert(BehaviorFeature::witnessed(
                            "new_form_action",
                            path,
                            action.clone(),
                            format!("{occurrence}:{attributes_sha256}"),
                        ));
                    }
                }
                "script" => {
                    let destination = attributes
                        .get("src")
                        .cloned()
                        .unwrap_or_else(|| "inline-script".to_owned());
                    let occurrence = next_occurrence(&mut script_occurrences, &destination);
                    let fingerprint = if destination == "inline-script" {
                        format!(
                            "{occurrence}:{attributes_sha256}:{}",
                            tag.body_sha256.as_deref().unwrap_or("missing-body")
                        )
                    } else {
                        format!("{occurrence}:{attributes_sha256}")
                    };
                    features.insert(BehaviorFeature::witnessed(
                        "new_script",
                        path,
                        destination.clone(),
                        fingerprint,
                    ));
                    if let Some(source) = attributes.get("src")
                        && let Ok(Some(resolved)) = resolve_local_reference(path, source)
                        && let Some(script_bytes) = files.get(&resolved)
                    {
                        features.insert(BehaviorFeature::witnessed(
                            "changed_script_content",
                            &resolved,
                            source.clone(),
                            sha256(script_bytes),
                        ));
                    }
                }
                "iframe" => {
                    let destination = attributes
                        .get("src")
                        .cloned()
                        .unwrap_or_else(|| "inline-frame".to_owned());
                    let occurrence = next_occurrence(&mut iframe_occurrences, &destination);
                    features.insert(BehaviorFeature::witnessed(
                        "new_iframe",
                        path,
                        destination,
                        format!("{occurrence}:{attributes_sha256}"),
                    ));
                }
                _ => {}
            }
            if attributes.contains_key("download") {
                let destination = attributes
                    .get("href")
                    .cloned()
                    .unwrap_or_else(|| "download-attribute".to_owned());
                let occurrence = next_occurrence(&mut download_occurrences, &destination);
                features.insert(BehaviorFeature::witnessed(
                    "new_download",
                    path,
                    destination,
                    format!("{occurrence}:{attributes_sha256}"),
                ));
            }
        }
    }
    features
}

fn canonical_attributes_sha256(attributes: &[(String, String)]) -> String {
    let mut attributes = attributes.to_vec();
    attributes.sort();
    let mut canonical = Vec::new();
    for (name, value) in attributes {
        canonical.extend_from_slice(&(name.len() as u64).to_be_bytes());
        canonical.extend_from_slice(name.as_bytes());
        canonical.extend_from_slice(&(value.len() as u64).to_be_bytes());
        canonical.extend_from_slice(value.as_bytes());
    }
    sha256(&canonical)
}

fn external_reference_origin(reference: &str) -> Option<String> {
    let reference = reference.trim();
    let protocol_relative = reference.starts_with("//");
    let parse_value = if protocol_relative {
        format!("https:{reference}")
    } else if reference
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http:"))
        || reference
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https:"))
    {
        reference.to_owned()
    } else {
        return None;
    };
    let url = Url::parse(&parse_value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }
    let origin = url.origin().ascii_serialization();
    Some(if protocol_relative {
        origin.strip_prefix("https:").unwrap_or(&origin).to_owned()
    } else {
        origin
    })
}

fn next_occurrence(occurrences: &mut BTreeMap<String, usize>, key: &str) -> usize {
    let occurrence = occurrences.entry(key.to_owned()).or_default();
    *occurrence = occurrence.saturating_add(1);
    *occurrence
}

fn is_inline_event_handler_attribute(name: &str) -> bool {
    name.len() > 2
        && name.starts_with("on")
        && name[2..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn external_origins(text: &str) -> Vec<String> {
    let mut origins = Vec::new();
    let lower = text.to_ascii_lowercase();
    for scheme in ["http://", "https://"] {
        let mut offset = 0;
        while let Some(relative) = lower[offset..].find(scheme) {
            let start = offset + relative;
            let tail = &text[start..];
            let end = external_url_end(tail);
            if let Ok(url) = Url::parse(&tail[..end]) {
                origins.push(url.origin().ascii_serialization());
            }
            offset = start + scheme.len();
        }
    }
    let mut offset = 0;
    while let Some(relative) = lower[offset..].find("//") {
        let start = offset + relative;
        let preceding = text[..start].chars().next_back();
        let has_url_boundary = preceding.is_none_or(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, ':' | '/' | '_' | '-' | '.')
        });
        if has_url_boundary {
            let tail = &text[start..];
            let end = external_url_end(tail);
            if let Ok(url) = Url::parse(&format!("https:{}", &tail[..end])) {
                let origin = url.origin().ascii_serialization();
                if let Some(protocol_relative) = origin.strip_prefix("https:") {
                    origins.push(protocol_relative.to_owned());
                }
            }
        }
        offset = start + 2;
    }
    origins
}

fn external_url_end(value: &str) -> usize {
    value
        .find(|character: char| {
            character.is_whitespace()
                || matches!(character, '"' | '\'' | '<' | '>' | ')' | ']' | '}')
        })
        .unwrap_or(value.len())
}

#[derive(Clone, Debug)]
struct MarkupTag {
    name: String,
    closing: bool,
    attributes: Vec<(String, String)>,
    body_sha256: Option<String>,
}

fn scan_markup_tags(text: &str, html_mode: bool) -> Vec<MarkupTag> {
    let bytes = text.as_bytes();
    let mut tags = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(relative_start) = text[cursor..].find('<') else {
            break;
        };
        let start = cursor + relative_start;
        if text[start..].starts_with("<!--") {
            cursor = text[start + 4..]
                .find("-->")
                .map(|end| start + 4 + end + 3)
                .unwrap_or(text.len());
            continue;
        }
        let Some(end) = markup_tag_end(text, start + 1) else {
            break;
        };
        let body = text[start + 1..end].trim();
        if let Some(mut tag) = parse_tag_body(body) {
            if !tag.closing && html_mode && tag.name == "plaintext" {
                tags.push(tag);
                cursor = text.len();
                continue;
            }
            if !tag.closing && behavior_raw_text_element(&tag.name, html_mode) {
                let body_start = end + 1;
                if let Some((body_end, next_cursor)) =
                    raw_text_element_bounds(text, body_start, &tag.name)
                {
                    if tag.name == "script" {
                        tag.body_sha256 = Some(sha256(&text.as_bytes()[body_start..body_end]));
                    }
                    tags.push(tag);
                    cursor = next_cursor;
                    continue;
                }
                if tag.name == "script" {
                    let body_end = text.len();
                    tag.body_sha256 = Some(sha256(&text.as_bytes()[body_start..body_end]));
                }
                tags.push(tag);
                cursor = text.len();
                continue;
            }
            tags.push(tag);
        }
        cursor = end + 1;
    }
    tags
}

fn behavior_raw_text_element(name: &str, html_mode: bool) -> bool {
    matches!(name, "style" | "script")
        || (html_mode
            && matches!(
                name,
                "title" | "textarea" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript"
            ))
}

fn raw_text_element_bounds(
    text: &str,
    body_start: usize,
    tag_name: &str,
) -> Option<(usize, usize)> {
    let closing_prefix = format!("</{tag_name}");
    let lower_tail = text[body_start..].to_ascii_lowercase();
    let mut search_start = 0;
    while let Some(relative_start) = lower_tail[search_start..].find(&closing_prefix) {
        let close_start = body_start + search_start + relative_start;
        let prefix_end = close_start + closing_prefix.len();
        let boundary = text.as_bytes().get(prefix_end).copied();
        if !boundary.is_none_or(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>')) {
            search_start += relative_start + closing_prefix.len();
            continue;
        }
        let close_end = markup_tag_end(text, close_start + 1)?;
        let close_body = text[close_start + 1..close_end].trim();
        let closing_body_prefix = &closing_prefix[1..];
        if !close_body
            .get(..closing_body_prefix.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(closing_body_prefix))
        {
            return None;
        }
        let trailing = close_body
            .get(closing_body_prefix.len()..)
            .unwrap_or_default()
            .trim();
        if !trailing.is_empty() && trailing != "/" {
            return None;
        }
        return Some((close_start, close_end + 1));
    }
    None
}

fn markup_tag_end(text: &str, body_start: usize) -> Option<usize> {
    let mut quote = None;
    for (relative, character) in text[body_start..].char_indices() {
        match quote {
            Some(expected) if character == expected => quote = None,
            Some(_) => {}
            None if matches!(character, '\'' | '"') => quote = Some(character),
            None if character == '>' => return Some(body_start + relative),
            None => {}
        }
    }
    None
}

pub(super) fn count_matching_start_tags(
    html: &str,
    tag_name: &str,
    identifier: &str,
) -> (usize, usize) {
    let mut identifier_count = 0;
    let mut matching_tag_count = 0;
    for tag in scan_markup_tags(html, true) {
        let has_identifier = !tag.closing
            && tag.attributes.iter().any(|(name, value)| {
                matches!(name.as_str(), "id" | "data-lp-id" | "data-studio-block")
                    && value == identifier
            });
        if has_identifier {
            identifier_count += 1;
            if tag.name.eq_ignore_ascii_case(tag_name) {
                matching_tag_count += 1;
            }
        }
    }
    (identifier_count, matching_tag_count)
}

fn parse_tag_body(body: &str) -> Option<MarkupTag> {
    if body.is_empty() || body.starts_with(['!', '?']) {
        return None;
    }
    let closing = body.starts_with('/');
    let body = body.strip_prefix('/').unwrap_or(body).trim_start();
    let name_end = body
        .find(|character: char| character.is_whitespace() || matches!(character, '/' | '>'))
        .unwrap_or(body.len());
    let name = body[..name_end].to_ascii_lowercase();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b':'))
    {
        return None;
    }
    Some(MarkupTag {
        name,
        closing,
        attributes: if closing {
            Vec::new()
        } else {
            parse_attributes(&body[name_end..])
        },
        body_sha256: None,
    })
}

fn parse_attributes(mut input: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    while !input.is_empty() {
        input = input.trim_start_matches(|character: char| {
            character.is_whitespace() || matches!(character, '/')
        });
        if input.is_empty() {
            break;
        }
        let name_end = input
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '=' | '/' | '>')
            })
            .unwrap_or(input.len());
        if name_end == 0 {
            break;
        }
        let name = input[..name_end].to_ascii_lowercase();
        input = &input[name_end..];
        input = input.trim_start();
        let mut value = String::new();
        if let Some(rest) = input.strip_prefix('=') {
            input = rest.trim_start();
            if let Some(quote) = input
                .chars()
                .next()
                .filter(|value| matches!(value, '\'' | '"'))
            {
                input = &input[quote.len_utf8()..];
                if let Some(end) = input.find(quote) {
                    value = input[..end].to_owned();
                    input = &input[end + quote.len_utf8()..];
                } else {
                    value = input.to_owned();
                    input = "";
                }
            } else {
                let end = input
                    .find(|character: char| {
                        character.is_whitespace() || matches!(character, '/' | '>')
                    })
                    .unwrap_or(input.len());
                value = input[..end].to_owned();
                input = &input[end..];
            }
        }
        attributes.push((name, value));
    }
    attributes
}

fn resolve_local_reference(
    source_path: &str,
    raw_reference: &str,
) -> Result<Option<String>, ChangeSetError> {
    let reference = raw_reference.trim();
    if reference.is_empty()
        || reference.starts_with('#')
        || reference.starts_with("//")
        || ["http:", "https:", "data:", "mailto:", "tel:", "blob:"]
            .iter()
            .any(|scheme| reference.to_ascii_lowercase().starts_with(scheme))
    {
        return Ok(None);
    }
    if reference.to_ascii_lowercase().starts_with("javascript:")
        || reference.to_ascii_lowercase().starts_with("file:")
        || reference.contains('\\')
    {
        return Err(ChangeSetError::InvalidLocalReference);
    }
    let without_suffix = reference.split(['?', '#']).next().unwrap_or_default();
    let decoded = percent_decode(without_suffix)?;
    let rooted = decoded.starts_with('/');
    let mut segments = if rooted {
        Vec::new()
    } else {
        source_path
            .rsplit_once('/')
            .map(|(directory, _)| directory.split('/').map(ToOwned::to_owned).collect())
            .unwrap_or_default()
    };
    for segment in decoded.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err(ChangeSetError::InvalidLocalReference);
                }
            }
            value => segments.push(value.to_owned()),
        }
    }
    let resolved = segments.join("/").nfc().collect::<String>();
    if resolved.is_empty() {
        Ok(Some("index.html".to_owned()))
    } else {
        validate_path(&resolved).map_err(|_| ChangeSetError::InvalidLocalReference)?;
        Ok(Some(resolved))
    }
}

fn percent_decode(value: &str) -> Result<String, ChangeSetError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes
                .get(index + 1)
                .and_then(|byte| hex_value(*byte))
                .ok_or(ChangeSetError::InvalidLocalReference)?;
            let low = bytes
                .get(index + 2)
                .and_then(|byte| hex_value(*byte))
                .ok_or(ChangeSetError::InvalidLocalReference)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ChangeSetError::InvalidLocalReference)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn build_unified_diff(
    accepted: &BTreeMap<String, Vec<u8>>,
    proposed: &BTreeMap<String, Vec<u8>>,
    changes: &[AppliedFileChange],
) -> String {
    let mut output = String::new();
    for change in changes {
        let (before_path, after_path) = match change.kind {
            AppliedChangeKind::Created => (None, Some(change.path.as_str())),
            AppliedChangeKind::Renamed => (change.from_path.as_deref(), Some(change.path.as_str())),
            AppliedChangeKind::Deleted => (Some(change.path.as_str()), None),
            AppliedChangeKind::Modified => (Some(change.path.as_str()), Some(change.path.as_str())),
        };
        push_bounded(
            &mut output,
            &format!(
                "--- {}\n+++ {}\n@@ before={} after={} @@\n",
                before_path.map_or("/dev/null".to_owned(), |path| format!("a/{path}")),
                after_path.map_or("/dev/null".to_owned(), |path| format!("b/{path}")),
                change.before_sha256.as_deref().unwrap_or("none"),
                change.after_sha256.as_deref().unwrap_or("none")
            ),
        );
        if let Some(path) = before_path
            && let Some(bytes) = accepted.get(path)
        {
            append_diff_content(&mut output, '-', bytes);
        }
        if let Some(path) = after_path
            && let Some(bytes) = proposed.get(path)
        {
            append_diff_content(&mut output, '+', bytes);
        }
        push_bounded(&mut output, "\n");
        if output.len() >= MAX_UNIFIED_DIFF_BYTES {
            break;
        }
    }
    if output.len() >= MAX_UNIFIED_DIFF_BYTES {
        truncate_utf8(&mut output, MAX_UNIFIED_DIFF_BYTES.saturating_sub(24));
        output.push_str("\n... diff truncated ...\n");
    }
    output
}

fn append_diff_content(output: &mut String, marker: char, bytes: &[u8]) {
    let Ok(text) = std::str::from_utf8(bytes) else {
        push_bounded(output, &format!("{marker}[binary content omitted]\n"));
        return;
    };
    for line in text.lines() {
        push_bounded(output, &format!("{marker}{line}\n"));
        if output.len() >= MAX_UNIFIED_DIFF_BYTES {
            return;
        }
    }
}

fn push_bounded(output: &mut String, value: &str) {
    let remaining = MAX_UNIFIED_DIFF_BYTES.saturating_sub(output.len());
    if remaining == 0 {
        return;
    }
    if value.len() <= remaining {
        output.push_str(value);
    } else {
        let mut end = remaining;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        output.push_str(&value[..end]);
    }
}

fn truncate_utf8(value: &mut String, maximum: usize) {
    let mut end = maximum.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn validate_path(path: &str) -> Result<(), ChangeSetError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.starts_with('/')
        || path.contains('\\')
        || path.nfc().collect::<String>() != path
        || path.chars().any(char::is_control)
    {
        return Err(ChangeSetError::UnsafePath);
    }
    let segments = path.split('/').collect::<Vec<_>>();
    if segments.len() > MAX_PATH_DEPTH {
        return Err(ChangeSetError::UnsafePath);
    }
    for segment in segments {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || segment.ends_with([' ', '.'])
            || segment
                .chars()
                .any(|character| "<>:\"|?*".contains(character))
            || windows_reserved(segment)
        {
            return Err(ChangeSetError::UnsafePath);
        }
    }
    if reserved_path(path) {
        return Err(ChangeSetError::ReservedPath);
    }
    Ok(())
}

fn reserved_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    let segments = lower.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| matches!(*segment, ".studio" | ".git" | ".ssh" | ".aws"))
    {
        return true;
    }
    let name = segments.last().copied().unwrap_or_default();
    name == ".env"
        || name.starts_with(".env.")
        || matches!(name, ".envrc" | ".netrc" | ".npmrc" | ".pypirc")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || matches!(
            name,
            "id_rsa" | "id_ed25519" | "credentials" | "credentials.json"
        )
}

fn windows_reserved(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || stem
            .strip_prefix("com")
            .or_else(|| stem.strip_prefix("lpt"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn validate_text_media_type(path: &str, media_type: &str) -> Result<(), ChangeSetError> {
    if text_media_type_for_path(path) == Some(media_type) {
        Ok(())
    } else {
        Err(ChangeSetError::UnsupportedMediaType)
    }
}

fn text_media_type_for_path(path: &str) -> Option<&'static str> {
    let extension = extension(path)?;
    if matches_ignore_ascii_case(extension, &["html", "htm"]) {
        Some("text/html")
    } else if extension.eq_ignore_ascii_case("css") {
        Some("text/css")
    } else if matches_ignore_ascii_case(extension, &["js", "mjs", "cjs"]) {
        Some("application/javascript")
    } else if matches_ignore_ascii_case(extension, &["json", "map"]) {
        Some("application/json")
    } else if extension.eq_ignore_ascii_case("webmanifest") {
        Some("application/manifest+json")
    } else if extension.eq_ignore_ascii_case("svg") {
        Some("image/svg+xml")
    } else if extension.eq_ignore_ascii_case("xml") {
        Some("application/xml")
    } else if extension.eq_ignore_ascii_case("txt") {
        Some("text/plain")
    } else if extension.eq_ignore_ascii_case("csv") {
        Some("text/csv")
    } else {
        None
    }
}

fn matches_ignore_ascii_case(value: &str, choices: &[&str]) -> bool {
    choices
        .iter()
        .any(|choice| value.eq_ignore_ascii_case(choice))
}

fn extension(path: &str) -> Option<&str> {
    path.rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| !extension.is_empty())
}

fn validate_hash(value: &str) -> Result<(), ChangeSetError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ChangeSetError::InvalidHash)
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn folded_path(path: &str) -> String {
    UniCase::new(path.to_owned()).to_folded_case()
}

fn is_disallowed_control(character: char) -> bool {
    character.is_control() && !matches!(character, '\n' | '\r' | '\t')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BASE: &str = "rev_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn accepted_fixture() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (
                "index.html".to_owned(),
                br#"<!doctype html><html lang="en"><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>Before</h1></body></html>"#
                    .to_vec(),
            ),
            ("styles/base.css".to_owned(), b"h1 { color: navy; }\n".to_vec()),
            ("rename.txt".to_owned(), b"rename me\n".to_vec()),
            ("delete.txt".to_owned(), b"delete me\n".to_vec()),
        ])
    }

    fn valid_all_operations() -> ChangeSetV1 {
        let accepted = accepted_fixture();
        ChangeSetV1::new(
            BASE,
            "Apply every ChangeSet v1 operation",
            vec![
                ChangeOperationV1::create_text("created.txt", "text/plain", "created\n"),
                ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    r#"<!doctype html><html lang="en"><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>After</h1></body></html>"#,
                ),
                ChangeOperationV1::rename(
                    "rename.txt",
                    "renamed.txt",
                    sha256(&accepted["rename.txt"]),
                ),
                ChangeOperationV1::delete("delete.txt", sha256(&accepted["delete.txt"])),
            ],
        )
    }

    fn apply(change_set: &ChangeSetV1) -> Result<AppliedChangeSet, ChangeSetError> {
        parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted_fixture())
    }

    #[test]
    fn all_four_operations_apply_atomically_and_serialize_deterministically() {
        let change_set = valid_all_operations();
        assert_eq!(change_set.to_json().unwrap(), change_set.to_json().unwrap());
        let applied = apply(&change_set).unwrap();

        assert_eq!(applied.change_set, change_set);
        assert_eq!(applied.files["created.txt"], b"created\n");
        assert!(applied.files.contains_key("renamed.txt"));
        assert!(!applied.files.contains_key("rename.txt"));
        assert!(!applied.files.contains_key("delete.txt"));
        assert!(
            std::str::from_utf8(&applied.files["index.html"])
                .unwrap()
                .contains("After")
        );
        assert_eq!(
            applied
                .changes
                .iter()
                .map(|change| change.kind)
                .collect::<Vec<_>>(),
            vec![
                AppliedChangeKind::Created,
                AppliedChangeKind::Modified,
                AppliedChangeKind::Renamed,
                AppliedChangeKind::Deleted
            ]
        );
        assert!(applied.unified_diff.contains("+++ b/index.html"));
        assert!(applied.unified_diff.len() <= MAX_UNIFIED_DIFF_BYTES);
        assert!(applied.blocking_warnings.is_empty());
        assert!(
            applied
                .checks
                .iter()
                .all(|check| check.status == StaticCheckStatus::Passed)
        );
        assert_eq!(
            applied
                .checks
                .iter()
                .map(|check| check.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "protocol-schema",
                "base-revision",
                "operation-graph",
                "file-preconditions",
                "isolated-apply",
                "static-syntax",
                "entry-point",
                "local-references",
                "export-deny-list",
                "accessibility-smoke",
                "active-behavior"
            ]
        );
    }

    #[test]
    fn accessibility_smoke_is_visible_bounded_and_non_blocking() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><html><body><img src="hero.png"><label for="named">Named</label><input id="named"><input id="duplicate"><textarea id="duplicate"></textarea></body></html>"#
                .to_vec(),
        )]);
        let check = accessibility_smoke_check(&files);
        assert_eq!(check.id, "accessibility-smoke");
        assert_eq!(check.status, StaticCheckStatus::AdvisoryWarning);
        assert!(check.message.contains("5 issue(s)"));
        assert!(check.message.contains("not WCAG conformance"));
    }

    #[test]
    fn html5_recovery_is_reported_as_a_redacted_advisory() {
        let accepted = accepted_fixture();
        let change_set = ChangeSetV1::new(
            BASE,
            "HTML5 recovery advisory",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<!doctype html><html lang="en"><head><title>LP</title></head><body><main><strong>recover-me-secret</main></strong></body></html>"#,
            )],
        );

        let applied = apply(&change_set).unwrap();
        let check = applied
            .checks
            .iter()
            .find(|check| check.id == "static-syntax")
            .unwrap();
        assert_eq!(check.status, StaticCheckStatus::AdvisoryWarning);
        assert!(check.message.contains("HTML5"));
        assert!(check.message.contains("bounded"));
        assert!(!check.message.contains("index.html"));
        assert!(!check.message.contains("recover-me-secret"));
        assert!(applied.blocking_warnings.is_empty());
        assert_eq!(accepted, accepted_fixture());
    }

    #[test]
    fn strict_json_rejects_unknown_missing_and_unsupported_fields() {
        let valid = serde_json::to_value(valid_all_operations()).unwrap();
        let mut cases = Vec::new();
        let mut root_extra = valid.clone();
        root_extra["unexpected"] = json!(true);
        cases.push(root_extra);
        let mut operation_extra = valid.clone();
        operation_extra["operations"][0]["unexpected"] = json!(true);
        cases.push(operation_extra);
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("summary");
        cases.push(missing);
        let mut unknown_operation = valid.clone();
        unknown_operation["operations"][0]["op"] = json!("patch_dom");
        cases.push(unknown_operation);
        let mut wrong_schema = valid.clone();
        wrong_schema["schema"] = json!("other");
        cases.push(wrong_schema);
        let mut wrong_version = valid;
        wrong_version["version"] = json!(2);
        cases.push(wrong_version);

        for value in cases {
            assert_eq!(
                parse_and_apply_change_set(&value.to_string(), BASE, &accepted_fixture()),
                Err(ChangeSetError::InvalidProtocol)
            );
        }
        assert_eq!(
            parse_and_apply_change_set("{", BASE, &accepted_fixture()),
            Err(ChangeSetError::InvalidProtocol)
        );
    }

    #[test]
    fn stale_base_and_operation_limits_fail_closed() {
        let accepted = accepted_fixture();
        let change_set = valid_all_operations();
        assert_eq!(
            parse_and_apply_change_set(
                &change_set.to_json().unwrap(),
                "rev_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                &accepted
            ),
            Err(ChangeSetError::StaleBase)
        );
        let too_many = ChangeSetV1::new(
            BASE,
            "too many",
            (0..=MAX_OPERATIONS)
                .map(|index| {
                    ChangeOperationV1::create_text(
                        format!("generated/{index}.txt"),
                        "text/plain",
                        "x",
                    )
                })
                .collect(),
        );
        assert_eq!(apply(&too_many), Err(ChangeSetError::LimitExceeded));
        assert_eq!(accepted, accepted_fixture());
    }

    #[test]
    fn unsafe_reserved_and_credential_paths_are_rejected() {
        let decomposed = "cafe\u{301}.txt";
        let deep = format!("{}/x.txt", vec!["x"; MAX_PATH_DEPTH].join("/"));
        let long = format!("{}.txt", "x".repeat(MAX_PATH_BYTES));
        for path in [
            "/absolute.txt",
            "../escape.txt",
            "a/./b.txt",
            "a//b.txt",
            "a\\b.txt",
            "bad\0name.txt",
            decomposed,
            &deep,
            &long,
        ] {
            let change_set = ChangeSetV1::new(
                BASE,
                "unsafe path",
                vec![ChangeOperationV1::create_text(path, "text/plain", "x")],
            );
            assert_eq!(
                apply(&change_set),
                Err(ChangeSetError::UnsafePath),
                "{path}"
            );
        }
        for path in [
            ".studio/context.json",
            "nested/.git/config.txt",
            ".env",
            ".env.local",
            "keys/private.pem",
            "credentials.json",
        ] {
            let change_set = ChangeSetV1::new(
                BASE,
                "reserved path",
                vec![ChangeOperationV1::create_text(path, "text/plain", "x")],
            );
            assert_eq!(
                apply(&change_set),
                Err(ChangeSetError::ReservedPath),
                "{path}"
            );
        }
    }

    #[test]
    fn media_types_binary_generation_and_size_limits_are_strict() {
        for (path, media_type) in [
            ("page.html", "text/plain"),
            ("style.css", "text/html"),
            ("image.png", "image/png"),
            ("unknown.bin", "text/plain"),
        ] {
            let change_set = ChangeSetV1::new(
                BASE,
                "bad media",
                vec![ChangeOperationV1::create_text(path, media_type, "x")],
            );
            assert_eq!(
                apply(&change_set),
                Err(ChangeSetError::UnsupportedMediaType)
            );
        }
        let oversized = ChangeSetV1::new(
            BASE,
            "large",
            vec![ChangeOperationV1::create_text(
                "large.txt",
                "text/plain",
                "x".repeat(MAX_GENERATED_FILE_BYTES + 1),
            )],
        );
        assert_eq!(apply(&oversized), Err(ChangeSetError::LimitExceeded));
        let total = ChangeSetV1::new(
            BASE,
            "large total",
            (0..5)
                .map(|index| {
                    ChangeOperationV1::create_text(
                        format!("large/{index}.txt"),
                        "text/plain",
                        "x".repeat(2 * 1024 * 1024),
                    )
                })
                .collect(),
        );
        assert_eq!(apply(&total), Err(ChangeSetError::LimitExceeded));
    }

    #[test]
    fn context_redaction_placeholders_can_never_become_site_content() {
        let accepted = accepted_fixture();
        let cases = [
            ChangeSetV1::new(
                BASE,
                "placeholder create",
                vec![ChangeOperationV1::create_text(
                    "generated.txt",
                    "text/plain",
                    "[LP_STUDIO_REDACTED]",
                )],
            ),
            ChangeSetV1::new(
                BASE,
                "placeholder replace",
                vec![ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    "<html><body>LP_STUDIO_REDACTED</body></html>",
                )],
            ),
        ];

        for change_set in cases {
            assert_eq!(apply(&change_set), Err(ChangeSetError::ReservedPlaceholder));
            assert_eq!(accepted, accepted_fixture());
        }
    }

    #[test]
    fn hash_preconditions_are_lowercase_exact_and_atomic() {
        let accepted = accepted_fixture();
        for expected in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
            let change_set = ChangeSetV1::new(
                BASE,
                "bad hash",
                vec![ChangeOperationV1::delete("delete.txt", expected)],
            );
            assert_eq!(apply(&change_set), Err(ChangeSetError::InvalidHash));
            assert_eq!(accepted, accepted_fixture());
        }
        let mismatch = ChangeSetV1::new(
            BASE,
            "hash mismatch",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                "0".repeat(64),
                "text/html",
                "<html></html>",
            )],
        );
        assert_eq!(apply(&mismatch), Err(ChangeSetError::PreconditionFailed));
        assert_eq!(accepted, accepted_fixture());
    }

    #[test]
    fn graph_collisions_casefolds_and_rename_cycles_are_rejected() {
        let accepted = accepted_fixture();
        let duplicate = ChangeSetV1::new(
            BASE,
            "duplicate",
            vec![
                ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    "<html></html>",
                ),
                ChangeOperationV1::delete("index.html", sha256(&accepted["index.html"])),
            ],
        );
        assert_eq!(apply(&duplicate), Err(ChangeSetError::OperationConflict));

        let casefold = ChangeSetV1::new(
            BASE,
            "casefold",
            vec![
                ChangeOperationV1::create_text("New.txt", "text/plain", "a"),
                ChangeOperationV1::create_text("new.TXT", "text/plain", "b"),
            ],
        );
        assert_eq!(apply(&casefold), Err(ChangeSetError::OperationConflict));

        let cycle = ChangeSetV1::new(
            BASE,
            "cycle",
            vec![
                ChangeOperationV1::rename(
                    "rename.txt",
                    "delete.txt",
                    sha256(&accepted["rename.txt"]),
                ),
                ChangeOperationV1::rename(
                    "delete.txt",
                    "rename.txt",
                    sha256(&accepted["delete.txt"]),
                ),
            ],
        );
        assert_eq!(apply(&cycle), Err(ChangeSetError::RenameCycle));
    }

    #[test]
    fn missing_sources_existing_destinations_and_extension_changing_renames_fail() {
        let accepted = accepted_fixture();
        let cases = [
            ChangeSetV1::new(
                BASE,
                "existing create",
                vec![ChangeOperationV1::create_text(
                    "index.html",
                    "text/html",
                    "<html></html>",
                )],
            ),
            ChangeSetV1::new(
                BASE,
                "missing replace",
                vec![ChangeOperationV1::replace_text(
                    "missing.html",
                    "0".repeat(64),
                    "text/html",
                    "<html></html>",
                )],
            ),
            ChangeSetV1::new(
                BASE,
                "rename destination",
                vec![ChangeOperationV1::rename(
                    "rename.txt",
                    "delete.txt",
                    sha256(&accepted["rename.txt"]),
                )],
            ),
        ];
        for change_set in cases {
            assert_eq!(apply(&change_set), Err(ChangeSetError::PreconditionFailed));
        }
        let extension_change = ChangeSetV1::new(
            BASE,
            "extension change",
            vec![ChangeOperationV1::rename(
                "rename.txt",
                "rename.html",
                sha256(&accepted["rename.txt"]),
            )],
        );
        assert_eq!(
            apply(&extension_change),
            Err(ChangeSetError::OperationConflict)
        );
    }

    #[test]
    fn final_entry_point_syntax_and_local_references_are_validated() {
        let accepted = accepted_fixture();
        let delete_entry = ChangeSetV1::new(
            BASE,
            "delete entry",
            vec![ChangeOperationV1::delete(
                "index.html",
                sha256(&accepted["index.html"]),
            )],
        );
        assert_eq!(apply(&delete_entry), Err(ChangeSetError::MissingEntryPoint));

        let bad_css = ChangeSetV1::new(
            BASE,
            "bad css",
            vec![ChangeOperationV1::replace_text(
                "styles/base.css",
                sha256(&accepted["styles/base.css"]),
                "text/css",
                "body { color: red;",
            )],
        );
        assert_eq!(apply(&bad_css), Err(ChangeSetError::InvalidSyntax));

        let missing_reference = ChangeSetV1::new(
            BASE,
            "missing ref",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<html><body><img src="assets/missing.png"></body></html>"#,
            )],
        );
        assert_eq!(
            apply(&missing_reference),
            Err(ChangeSetError::InvalidLocalReference)
        );
        assert_eq!(accepted, accepted_fixture());
    }

    #[test]
    fn inline_css_and_quote_aware_local_references_fail_closed() {
        let accepted = accepted_fixture();
        for (html, expected) in [
            (
                r#"<html><body><div style="background:url('assets/missing.png')"></div></body></html>"#,
                ChangeSetError::InvalidLocalReference,
            ),
            (
                r#"<html><head><style>body{background:url('assets/missing.png')}</style></head></html>"#,
                ChangeSetError::InvalidLocalReference,
            ),
            (
                r#"<html><body><img alt="1 > 0" src="assets/missing.png"></body></html>"#,
                ChangeSetError::InvalidLocalReference,
            ),
            (
                r#"<html><head><style>body{color:red}</head></html>"#,
                ChangeSetError::InvalidSyntax,
            ),
        ] {
            let change_set = ChangeSetV1::new(
                BASE,
                "strict inline references",
                vec![ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    html,
                )],
            );
            assert_eq!(
                parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted),
                Err(expected),
                "validation accepted {html}"
            );
        }

        let ambiguous = ChangeSetV1::new(
            BASE,
            "ambiguous inline CSS",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<html><head><style>body{background:u\72l('assets/missing.png')}</style></head><body></body></html>"#,
            )],
        );
        let applied =
            parse_and_apply_change_set(&ambiguous.to_json().unwrap(), BASE, &accepted).unwrap();
        assert!(applied.blocking_warnings.iter().any(|warning| {
            warning.code == "static_reference_analysis_incomplete" && warning.path == "index.html"
        }));
        assert_eq!(
            applied
                .checks
                .iter()
                .find(|check| check.id == "local-references")
                .unwrap()
                .status,
            StaticCheckStatus::BlockingWarning
        );
    }

    #[test]
    fn removing_any_path_blocks_when_an_unchanged_reference_source_is_opaque() {
        let mut accepted = accepted_fixture();
        accepted.insert(
            "index.html".to_owned(),
            br#"<html><head><style>body{background:u\72l('hero.png')}</style></head></html>"#
                .to_vec(),
        );
        accepted.insert("hero.png".to_owned(), vec![0x89, b'P', b'N', b'G']);

        let operations = [
            ChangeOperationV1::delete("hero.png", sha256(&accepted["hero.png"])),
            ChangeOperationV1::rename(
                "hero.png",
                "renamed-hero.png",
                sha256(&accepted["hero.png"]),
            ),
        ];
        for operation in operations {
            let change_set = ChangeSetV1::new(BASE, "opaque reference", vec![operation]);
            let applied =
                parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted)
                    .unwrap();
            assert!(applied.blocking_warnings.iter().any(|warning| {
                warning.code == "static_reference_analysis_incomplete"
                    && warning.path == "index.html"
            }));
            assert_eq!(
                applied
                    .checks
                    .iter()
                    .find(|check| check.id == "local-references")
                    .unwrap()
                    .status,
                StaticCheckStatus::BlockingWarning
            );
        }
    }

    #[test]
    fn new_active_behavior_is_returned_as_deterministic_blocking_metadata() {
        let accepted = accepted_fixture();
        let active = ChangeSetV1::new(
            BASE,
            "active behavior",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<!doctype html><html><body><form action="https://forms.example/submit"></form><script src="https://cdn.example/app.js"></script><iframe src="https://frame.example/"></iframe><a href="https://files.example/a.zip" download>Download</a><script>document.cookie="x=1"; gtag("event")</script></body></html>"#,
            )],
        );
        let applied = apply(&active).unwrap();
        let codes = applied
            .blocking_warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            codes,
            BTreeSet::from([
                "new_analytics",
                "new_cookie_behavior",
                "new_download",
                "new_form_action",
                "new_iframe",
                "new_script"
            ])
        );
        assert!(
            applied
                .blocking_warnings
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
        assert_eq!(
            applied.checks.last().unwrap().status,
            StaticCheckStatus::BlockingWarning
        );
    }

    #[test]
    fn protocol_relative_origins_and_inline_event_handlers_are_blocking() {
        let accepted = accepted_fixture();
        let active = ChangeSetV1::new(
            BASE,
            "protocol-relative and inline handler",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<!doctype html><html><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>Before</h1><img src="//tracker.example/pixel.gif"><button onclick="fetch('/collect')">Send</button></body></html>"#,
            )],
        );

        let applied =
            parse_and_apply_change_set(&active.to_json().unwrap(), BASE, &accepted).unwrap();
        let warnings = applied
            .blocking_warnings
            .iter()
            .map(|warning| {
                (
                    warning.code.as_str(),
                    warning.path.as_str(),
                    warning.destination.as_str(),
                )
            })
            .collect::<BTreeSet<_>>();

        assert!(warnings.contains(&("new_inline_event_handler", "index.html", "button[onclick]")));
        assert_eq!(
            applied.checks.last().unwrap().status,
            StaticCheckStatus::BlockingWarning
        );
    }

    #[test]
    fn quoted_greater_than_cannot_hide_active_behavior_attributes() {
        let accepted = accepted_fixture();
        let active = ChangeSetV1::new(
            BASE,
            "quote-aware active behavior",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<!doctype html><html><body><button title="1 > 0" onclick="location.hash='changed'">Send</button><form title='2 > 1' action="https://forms.example.test/submit"></form><a title="3 > 2" href="https://files.example.test/a.zip" download>Download</a></body></html>"#,
            )],
        );

        let applied =
            parse_and_apply_change_set(&active.to_json().unwrap(), BASE, &accepted).unwrap();
        let codes = applied
            .blocking_warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .collect::<BTreeSet<_>>();
        for expected in [
            "new_download",
            "new_form_action",
            "new_inline_event_handler",
        ] {
            assert!(codes.contains(expected), "missing warning {expected}");
        }
    }

    #[test]
    fn raw_text_false_tags_cannot_hide_later_event_handlers() {
        let cases = [
            (
                r#"<!doctype html><script>const a="</scriptx>"; const b="<foo title='";</script>' >"#,
                r#"<!doctype html><script>const a="</scriptx>"; const b="<foo title='";</script><button onclick="location.hash='script'">X</button>' >"#,
            ),
            (
                r#"<!doctype html><style>.x{content:"<foo title='"}</style>' >"#,
                r#"<!doctype html><style>.x{content:"<foo title='"}</style><button onclick="location.hash='style'">X</button>' >"#,
            ),
        ];

        for (before, after) in cases {
            let mut accepted = accepted_fixture();
            accepted.insert("index.html".to_owned(), before.as_bytes().to_vec());
            let change_set = ChangeSetV1::new(
                BASE,
                "raw text boundary",
                vec![ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    after,
                )],
            );

            let applied =
                parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted)
                    .unwrap();
            assert!(applied.blocking_warnings.iter().any(|warning| {
                warning.code == "new_inline_event_handler"
                    && warning.destination == "button[onclick]"
            }));
        }
    }

    #[test]
    fn tokenizer_specific_html_boundaries_are_blocking_when_changed() {
        let accepted = accepted_fixture();
        for html in [
            r#"<!doctype html><script><!-- legacy escaped script marker --></script>"#,
            r#"<!doctype html><?bounded-processing "quoted > text"?><main>Ready</main>"#,
            r#"<!doctype html><![CDATA[html-context text]]><main>Ready</main>"#,
        ] {
            let change_set = ChangeSetV1::new(
                BASE,
                "tokenizer boundary",
                vec![ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    html,
                )],
            );
            let applied =
                parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted)
                    .unwrap();
            assert!(applied.blocking_warnings.iter().any(|warning| {
                warning.code == "static_reference_analysis_incomplete"
                    && warning.path == "index.html"
            }));
        }
    }

    #[test]
    fn duplicate_active_destinations_remain_distinct_review_evidence() {
        let mut accepted = accepted_fixture();
        accepted.insert(
            "frame.html".to_owned(),
            b"<!doctype html><p>frame</p>".to_vec(),
        );
        accepted.insert(
            "index.html".to_owned(),
            br#"<!doctype html><html><body><iframe src="frame.html"></iframe><form action="https://forms.example.test/submit"></form><a href="https://files.example.test/a.zip" download>Download</a><img alt="pixel" src="https://tracker.example.test/pixel.png"></body></html>"#
                .to_vec(),
        );
        let proposed = r#"<!doctype html><html><body><iframe src="frame.html"></iframe><iframe src="frame.html"></iframe><form action="https://forms.example.test/submit"></form><form action="https://forms.example.test/submit"></form><a href="https://files.example.test/a.zip" download>Download</a><a href="https://files.example.test/a.zip" download>Download again</a><img alt="pixel" src="https://tracker.example.test/pixel.png"><img alt="pixel 2" src="https://tracker.example.test/pixel.png"></body></html>"#;
        let change_set = ChangeSetV1::new(
            BASE,
            "duplicate active destinations",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                proposed,
            )],
        );

        let applied =
            parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted).unwrap();
        let codes = applied
            .blocking_warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .collect::<BTreeSet<_>>();
        for expected in ["new_download", "new_form_action", "new_iframe"] {
            assert!(codes.contains(expected), "missing warning {expected}");
        }
    }

    #[test]
    fn inert_url_text_and_passive_external_references_do_not_block_adoption() {
        let mut accepted = accepted_fixture();
        accepted.insert(
            "index.html".to_owned(),
            br#"<!doctype html><html><body><p>Documentation: https://tracker.example.test</p></body></html>"#
                .to_vec(),
        );
        let change_set = ChangeSetV1::new(
            BASE,
            "activate documented origin",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<!doctype html><html><body><p>Documentation: https://tracker.example.test</p><img alt="pixel" src="https://tracker.example.test/pixel.png"></body></html>"#,
            )],
        );

        let applied =
            parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted).unwrap();
        assert!(applied.blocking_warnings.is_empty());
    }

    #[test]
    fn html_character_reference_cannot_bypass_external_review_with_a_dummy_path() {
        let accepted = accepted_fixture();
        let change_set = ChangeSetV1::new(
            BASE,
            "encoded external reference",
            vec![
                ChangeOperationV1::create_text(
                    "https&colon;/evil.example.test/theme.css",
                    "text/css",
                    "body { color: red; }",
                ),
                ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    r#"<!doctype html><link rel="stylesheet" href="https&colon;//evil.example.test/theme.css">"#,
                ),
            ],
        );

        let applied =
            parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted).unwrap();
        assert!(applied.blocking_warnings.iter().any(|warning| {
            warning.code == "static_reference_analysis_incomplete" && warning.path == "index.html"
        }));
        assert_eq!(
            applied
                .checks
                .iter()
                .find(|check| check.id == "local-references")
                .unwrap()
                .status,
            StaticCheckStatus::BlockingWarning
        );
    }

    #[test]
    fn ping_image_srcset_and_image_set_are_not_silent_fetch_surfaces() {
        let accepted = accepted_fixture();
        let markup = ChangeSetV1::new(
            BASE,
            "additional fetch attributes",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r##"<!doctype html><link rel="preload" imagesrcset="https://images.example.test/a.png 1x"><a href="#ready" ping="https://audit.example.test/p">Ready</a>"##,
            )],
        );
        let applied =
            parse_and_apply_change_set(&markup.to_json().unwrap(), BASE, &accepted).unwrap();
        assert!(applied.blocking_warnings.is_empty());

        let image_set = ChangeSetV1::new(
            BASE,
            "quoted image set",
            vec![ChangeOperationV1::replace_text(
                "styles/base.css",
                sha256(&accepted["styles/base.css"]),
                "text/css",
                r#".hero { background: image-set("https://images.example.test/a.png" 1x); }"#,
            )],
        );
        let applied =
            parse_and_apply_change_set(&image_set.to_json().unwrap(), BASE, &accepted).unwrap();
        assert!(applied.blocking_warnings.iter().any(|warning| {
            warning.code == "static_reference_analysis_incomplete"
                && warning.path == "styles/base.css"
        }));
    }

    #[test]
    fn script_execution_and_iframe_permission_transitions_are_blocking() {
        let mut accepted = accepted_fixture();
        accepted.insert(
            "frame.html".to_owned(),
            b"<!doctype html><p>frame</p>".to_vec(),
        );
        accepted.insert(
            "index.html".to_owned(),
            br#"<!doctype html><html><body><script type="application/json">window.example = 1;</script><iframe src="frame.html" sandbox></iframe></body></html>"#
                .to_vec(),
        );
        let change_set = ChangeSetV1::new(
            BASE,
            "activate script and widen frame",
            vec![ChangeOperationV1::replace_text(
                "index.html",
                sha256(&accepted["index.html"]),
                "text/html",
                r#"<!doctype html><html><body><script>window.example = 1;</script><iframe src="frame.html"></iframe></body></html>"#,
            )],
        );

        let applied =
            parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted).unwrap();
        let codes = applied
            .blocking_warnings
            .iter()
            .map(|warning| warning.code.as_str())
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("new_script"));
        assert!(codes.contains("new_iframe"));
    }

    #[test]
    fn changing_javascript_behind_an_existing_script_src_is_blocking() {
        let mut accepted = accepted_fixture();
        accepted.insert(
            "index.html".to_owned(),
            br#"<!doctype html><html><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>Before</h1><script src="app.js"></script></body></html>"#.to_vec(),
        );
        accepted.insert("app.js".to_owned(), b"window.app = 'before';\n".to_vec());
        let active = ChangeSetV1::new(
            BASE,
            "change an existing executable dependency",
            vec![ChangeOperationV1::replace_text(
                "app.js",
                sha256(&accepted["app.js"]),
                "application/javascript",
                "window.app = 'after';\n",
            )],
        );

        let applied =
            parse_and_apply_change_set(&active.to_json().unwrap(), BASE, &accepted).unwrap();

        assert!(applied.blocking_warnings.iter().any(|warning| {
            warning.code == "changed_script_content"
                && warning.path == "app.js"
                && warning.destination == "app.js"
        }));
        assert!(
            !applied
                .blocking_warnings
                .iter()
                .any(|warning| warning.code == "new_script")
        );
    }

    #[test]
    fn changing_a_transitively_imported_module_is_conservatively_blocking() {
        let mut accepted = accepted_fixture();
        accepted.insert(
            "index.html".to_owned(),
            br#"<!doctype html><script type="module" src="app.js"></script>"#.to_vec(),
        );
        accepted.insert(
            "app.js".to_owned(),
            b"import './module.js';\nconsole.log('app');\n".to_vec(),
        );
        accepted.insert(
            "module.js".to_owned(),
            b"export const value = 'before';\n".to_vec(),
        );
        let change_set = ChangeSetV1::new(
            BASE,
            "change nested module",
            vec![ChangeOperationV1::replace_text(
                "module.js",
                sha256(&accepted["module.js"]),
                "application/javascript",
                "export const value = 'after';\n",
            )],
        );

        let applied =
            parse_and_apply_change_set(&change_set.to_json().unwrap(), BASE, &accepted).unwrap();
        assert!(applied.blocking_warnings.iter().any(|warning| {
            warning.code == "changed_script_content"
                && warning.path == "module.js"
                && warning.destination == "module.js"
        }));
    }

    #[test]
    fn changed_or_duplicated_inline_script_content_is_blocking() {
        let mut accepted = accepted_fixture();
        let before = r#"<!doctype html><html><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>Before</h1><script>window.app = 1;</script></body></html>"#;
        accepted.insert("index.html".to_owned(), before.as_bytes().to_vec());
        let cases = [
            r#"<!doctype html><html><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>Before</h1><script>window.app = 2;</script></body></html>"#,
            r#"<!doctype html><html><head><link rel="stylesheet" href="styles/base.css"></head><body><h1>Before</h1><script>window.app = 1;</script><script>window.app = 1;</script></body></html>"#,
        ];

        for content in cases {
            let active = ChangeSetV1::new(
                BASE,
                "change inline executable behavior",
                vec![ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    content,
                )],
            );
            let applied =
                parse_and_apply_change_set(&active.to_json().unwrap(), BASE, &accepted).unwrap();

            assert!(applied.blocking_warnings.iter().any(|warning| {
                warning.code == "new_script"
                    && warning.path == "index.html"
                    && warning.destination == "inline-script"
            }));
        }
    }

    #[test]
    fn matching_start_tag_counter_ignores_comments_script_bodies_and_wrong_tags() {
        let html = r#"<!doctype html>
<!-- <h1 data-lp-id="hero-heading">comment</h1> -->
<script>const inert = '<h1 data-lp-id="hero-heading">script text</h1>';</script>
<div data-lp-id="hero-heading">wrong tag</div>
<h1 id="hero-heading" data-lp-id="hero-heading">first real match</h1>
<H1 data-studio-block='hero-heading'>second real match</H1>"#;

        assert_eq!(
            count_matching_start_tags(html, "h1", "hero-heading"),
            (3, 2)
        );
        assert_eq!(count_matching_start_tags(html, "p", "hero-heading"), (3, 0));
        assert_eq!(count_matching_start_tags(html, "h1", "missing"), (0, 0));
    }

    #[test]
    fn percent_encoded_escape_and_javascript_references_are_rejected() {
        let accepted = accepted_fixture();
        for reference in [
            "../../secret.txt",
            "%2e%2e/%2e%2e/secret.txt",
            "javascript:alert(1)",
        ] {
            let change_set = ChangeSetV1::new(
                BASE,
                "unsafe reference",
                vec![ChangeOperationV1::replace_text(
                    "index.html",
                    sha256(&accepted["index.html"]),
                    "text/html",
                    format!("<html><body><img src=\"{reference}\"></body></html>"),
                )],
            );
            assert_eq!(
                apply(&change_set),
                Err(ChangeSetError::InvalidLocalReference),
                "{reference}"
            );
        }
    }

    #[test]
    fn error_codes_are_stable_and_do_not_include_model_content() {
        assert_eq!(
            ChangeSetError::InvalidProtocol.code(),
            "change_set_invalid_protocol"
        );
        assert_eq!(ChangeSetError::StaleBase.code(), "change_set_stale_base");
        assert_eq!(
            ChangeSetError::InvalidLocalReference.code(),
            "change_set_invalid_local_reference"
        );
        assert_eq!(
            ChangeSetError::ReservedPlaceholder.code(),
            "change_set_reserved_placeholder"
        );
        assert!(
            !ChangeSetError::InvalidProtocol
                .to_string()
                .contains("secret")
        );
    }
}
