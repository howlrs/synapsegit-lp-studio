//! Pure analysis and deterministic archive generation for an immutable
//! Accepted static site.
//!
//! The module accepts an already managed `BTreeMap` of canonical relative file
//! paths and bytes. It has no filesystem or network API. ZIP construction is
//! delegated to the crate's single fixed-metadata implementation so Preview,
//! API, and receipt tests cannot silently grow different archive profiles.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use unicase::UniCase;
use unicode_normalization::UnicodeNormalization;
use url::Url;

const EXPORT_RECEIPT_SCHEMA: &str = "org.synapsegit-lp-studio.export-receipt";
const EXPORT_RECEIPT_VERSION: u16 = 1;
const EXPORT_MANIFEST_SCHEMA: &str = "org.synapsegit-lp-studio.export-file-manifest";
const EXPORT_MANIFEST_VERSION: u16 = 1;
const MAX_SOURCE_REVISION_ID_BYTES: usize = 128;
const MAX_TIMESTAMP_BYTES: usize = 40;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StaticExportInput<'a> {
    pub(super) source_revision_id: &'a str,
    pub(super) source_manifest_sha256: &'a str,
    pub(super) generated_at_utc: &'a str,
    pub(super) entry_point: &'a str,
    pub(super) files: &'a BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StaticExportArtifact {
    pub(super) zip_bytes: Vec<u8>,
    pub(super) receipt: ExportReceiptV1,
    /// Canonical persistable bytes for the receipt. Persistence metadata and
    /// storage paths remain the caller's responsibility.
    pub(super) receipt_json: Vec<u8>,
    pub(super) receipt_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportReceiptV1 {
    pub(super) schema: SchemaIdentity,
    pub(super) source_revision_id: String,
    pub(super) source_manifest_sha256: String,
    pub(super) options: ExportOptionsV1,
    pub(super) file_manifest: ExportFileManifestV1,
    pub(super) validation: ExportValidationReportV1,
    pub(super) archive: ExportArchiveIdentityV1,
    pub(super) generated_at_utc: String,
    pub(super) application_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SchemaIdentity {
    pub(super) name: String,
    pub(super) version: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportOptionsV1 {
    pub(super) entry_point: String,
    pub(super) base_path_profile: String,
    pub(super) external_asset_policy: String,
    pub(super) artifact_format: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportFileManifestV1 {
    pub(super) schema: SchemaIdentity,
    pub(super) sha256: String,
    pub(super) total_byte_length: usize,
    pub(super) files: Vec<ExportFileManifestEntryV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportFileManifestEntryV1 {
    pub(super) path: String,
    pub(super) media_type: String,
    pub(super) byte_length: usize,
    pub(super) sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StaticHostingProfile {
    OfflineSelfContained,
    StandaloneStatic,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportValidationReportV1 {
    pub(super) profile: StaticHostingProfile,
    pub(super) local_reference_count: usize,
    pub(super) external_references: Vec<ExternalReferenceV1>,
    pub(super) external_origins: Vec<String>,
    pub(super) dynamic_reference_sources: Vec<String>,
    pub(super) warnings: Vec<ExportWarningV1>,
    pub(super) limitations: Vec<ExportLimitationV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExternalReferenceV1 {
    pub(super) source_path: String,
    /// HTTP(S) URL with userinfo, query, and fragment values removed.
    pub(super) sanitized_url: String,
    pub(super) origin: String,
    pub(super) userinfo_redacted: bool,
    pub(super) query_redacted: bool,
    pub(super) fragment_redacted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportWarningV1 {
    pub(super) code: String,
    pub(super) message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportLimitationV1 {
    pub(super) code: String,
    pub(super) message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ExportArchiveIdentityV1 {
    pub(super) sha256: String,
    pub(super) byte_length: usize,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub(super) enum StaticExportError {
    #[error("Accepted export input is empty")]
    EmptyAcceptedSite,
    #[error("the source revision ID is invalid")]
    InvalidSourceRevisionId,
    #[error("the source manifest SHA-256 is invalid")]
    InvalidSourceManifest,
    #[error("the generated-at UTC value is invalid")]
    InvalidGeneratedAt,
    #[error("the export entry point is invalid")]
    InvalidEntryPoint,
    #[error("the Accepted file path is invalid: {0}")]
    InvalidFilePath(String),
    #[error("the Accepted file paths collide under case folding")]
    FilePathCollision,
    #[error("a text reference source is not valid UTF-8: {0}")]
    InvalidTextEncoding(String),
    #[error("a markup reference source is malformed: {0}")]
    MalformedMarkup(String),
    #[error("the static site uses an unsupported base element: {0}")]
    UnsupportedBaseElement(String),
    #[error("a static reference is unsafe: {0}")]
    UnsafeReference(String),
    #[error("a local static reference is missing: {source_path} -> {resolved_path}")]
    MissingLocalReference {
        source_path: String,
        resolved_path: String,
    },
    #[error("deterministic ZIP generation failed")]
    ArchiveGeneration,
    #[error("export receipt serialization failed")]
    Serialization,
}

#[derive(Clone, Debug)]
pub(super) struct ReferenceScan {
    pub(super) references: Vec<String>,
    pub(super) dynamic_capable: bool,
    pub(super) analysis_incomplete: bool,
}

#[derive(Default)]
struct CssReferenceScan {
    references: Vec<String>,
    analysis_incomplete: bool,
}

#[derive(Clone, Debug)]
struct MarkupTag {
    name: String,
    closing: bool,
    attributes: Vec<(String, String)>,
    inline_css: Option<String>,
    raw_text_analysis_incomplete: bool,
}

enum ReferenceResolution {
    Ignored,
    RuntimeOnly,
    Local(String),
    External(ExternalReferenceV1),
}

/// Analyze one immutable Accepted file map and build its deterministic ZIP and
/// persistable receipt. The function does not read the clock, filesystem, or
/// network; every receipt-varying value is explicit input.
pub(super) fn generate_static_export(
    input: &StaticExportInput<'_>,
) -> Result<StaticExportArtifact, StaticExportError> {
    validate_export_input(input)?;
    let file_entries = canonical_file_entries(input.files)?;
    let file_manifest_bytes = canonical_json(&FileManifestIdentityInput {
        schema: SchemaIdentityRef {
            name: EXPORT_MANIFEST_SCHEMA,
            version: EXPORT_MANIFEST_VERSION,
        },
        files: &file_entries,
    })?;
    let total_byte_length = file_entries
        .iter()
        .try_fold(0_usize, |total, file| total.checked_add(file.byte_length));
    let Some(total_byte_length) = total_byte_length else {
        return Err(StaticExportError::Serialization);
    };
    let validation = analyze_references(input.files)?;
    let zip_bytes =
        super::deterministic_zip(input.files).map_err(|_| StaticExportError::ArchiveGeneration)?;
    let receipt = ExportReceiptV1 {
        schema: SchemaIdentity {
            name: EXPORT_RECEIPT_SCHEMA.to_owned(),
            version: EXPORT_RECEIPT_VERSION,
        },
        source_revision_id: input.source_revision_id.to_owned(),
        source_manifest_sha256: input.source_manifest_sha256.to_owned(),
        options: ExportOptionsV1 {
            entry_point: input.entry_point.to_owned(),
            base_path_profile: "relative_static_http".to_owned(),
            external_asset_policy: "report".to_owned(),
            artifact_format: "zip_stored_v1".to_owned(),
        },
        file_manifest: ExportFileManifestV1 {
            schema: SchemaIdentity {
                name: EXPORT_MANIFEST_SCHEMA.to_owned(),
                version: EXPORT_MANIFEST_VERSION,
            },
            sha256: sha256(&file_manifest_bytes),
            total_byte_length,
            files: file_entries,
        },
        validation,
        archive: ExportArchiveIdentityV1 {
            sha256: sha256(&zip_bytes),
            byte_length: zip_bytes.len(),
        },
        generated_at_utc: input.generated_at_utc.to_owned(),
        application_version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    let receipt_json = canonical_json(&receipt)?;
    let receipt_sha256 = sha256(&receipt_json);
    Ok(StaticExportArtifact {
        zip_bytes,
        receipt,
        receipt_json,
        receipt_sha256,
    })
}

#[derive(Serialize)]
struct SchemaIdentityRef<'a> {
    name: &'a str,
    version: u16,
}

#[derive(Serialize)]
struct FileManifestIdentityInput<'a> {
    schema: SchemaIdentityRef<'a>,
    files: &'a [ExportFileManifestEntryV1],
}

fn validate_export_input(input: &StaticExportInput<'_>) -> Result<(), StaticExportError> {
    if input.files.is_empty() {
        return Err(StaticExportError::EmptyAcceptedSite);
    }
    if input.source_revision_id.is_empty()
        || input.source_revision_id.len() > MAX_SOURCE_REVISION_ID_BYTES
        || !input
            .source_revision_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(StaticExportError::InvalidSourceRevisionId);
    }
    if !valid_sha256(input.source_manifest_sha256) {
        return Err(StaticExportError::InvalidSourceManifest);
    }
    if !valid_utc_timestamp(input.generated_at_utc) {
        return Err(StaticExportError::InvalidGeneratedAt);
    }
    super::storage::validate_canonical_path(input.entry_point)
        .map_err(|_| StaticExportError::InvalidEntryPoint)?;
    if !is_html_path(input.entry_point) || !input.files.contains_key(input.entry_point) {
        return Err(StaticExportError::InvalidEntryPoint);
    }
    std::str::from_utf8(&input.files[input.entry_point])
        .map_err(|_| StaticExportError::InvalidTextEncoding(input.entry_point.to_owned()))?;
    Ok(())
}

fn canonical_file_entries(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<ExportFileManifestEntryV1>, StaticExportError> {
    let mut folded_paths = BTreeSet::new();
    let mut entries = Vec::with_capacity(files.len());
    for (path, bytes) in files {
        super::storage::validate_canonical_path(path)
            .map_err(|_| StaticExportError::InvalidFilePath(path.clone()))?;
        if reserved_export_path(path) {
            return Err(StaticExportError::InvalidFilePath(path.clone()));
        }
        if !folded_paths.insert(UniCase::new(path.clone()).to_folded_case()) {
            return Err(StaticExportError::FilePathCollision);
        }
        entries.push(ExportFileManifestEntryV1 {
            path: path.clone(),
            media_type: mime_guess::from_path(path)
                .first_or_octet_stream()
                .essence_str()
                .to_owned(),
            byte_length: bytes.len(),
            sha256: sha256(bytes),
        });
    }
    Ok(entries)
}

fn analyze_references(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<ExportValidationReportV1, StaticExportError> {
    let mut local_reference_count = 0_usize;
    let mut external_references = BTreeSet::new();
    let mut external_origins = BTreeSet::new();
    let mut dynamic_reference_sources = BTreeSet::new();
    let mut incomplete_reference_sources = BTreeSet::new();

    for (source_path, bytes) in files {
        let scan = scan_references(source_path, bytes)?;
        if scan.dynamic_capable {
            dynamic_reference_sources.insert(source_path.clone());
        }
        if scan.analysis_incomplete {
            incomplete_reference_sources.insert(source_path.clone());
            dynamic_reference_sources.insert(source_path.clone());
        }
        for raw_reference in scan.references {
            match resolve_reference(source_path, &raw_reference)? {
                ReferenceResolution::Ignored => {}
                ReferenceResolution::RuntimeOnly => {
                    dynamic_reference_sources.insert(source_path.clone());
                }
                ReferenceResolution::Local(resolved_path) => {
                    local_reference_count = local_reference_count
                        .checked_add(1)
                        .ok_or(StaticExportError::Serialization)?;
                    if !files.contains_key(&resolved_path)
                        && !files.contains_key(&format!("{resolved_path}/index.html"))
                    {
                        return Err(StaticExportError::MissingLocalReference {
                            source_path: source_path.clone(),
                            resolved_path,
                        });
                    }
                }
                ReferenceResolution::External(reference) => {
                    external_origins.insert(reference.origin.clone());
                    external_references.insert(reference);
                }
            }
        }
    }

    let external_references = external_references.into_iter().collect::<Vec<_>>();
    let dynamic_reference_sources = dynamic_reference_sources.into_iter().collect::<Vec<_>>();
    let profile = if external_references.is_empty() && incomplete_reference_sources.is_empty() {
        StaticHostingProfile::OfflineSelfContained
    } else {
        StaticHostingProfile::StandaloneStatic
    };
    let mut warnings = BTreeSet::new();
    if !dynamic_reference_sources.is_empty() {
        warnings.insert(ExportWarningV1 {
            code: "dynamic_references_unchecked".to_owned(),
            message: format!(
                "{} file(s) can construct runtime references that static analysis cannot exhaustively verify.",
                dynamic_reference_sources.len()
            ),
        });
    }
    if !incomplete_reference_sources.is_empty() {
        warnings.insert(ExportWarningV1 {
            code: "static_reference_analysis_incomplete".to_owned(),
            message: format!(
                "{} file(s) contain escaped, commented, or ambiguous CSS syntax; offline self-containment is not claimed.",
                incomplete_reference_sources.len()
            ),
        });
    }
    if !external_references.is_empty() {
        warnings.insert(ExportWarningV1 {
            code: "external_dependencies_present".to_owned(),
            message: format!(
                "{} redacted external HTTP(S) reference(s) remain; this export is not offline self-contained.",
                external_references.len()
            ),
        });
    }
    if external_references.iter().any(|reference| {
        reference.userinfo_redacted || reference.query_redacted || reference.fragment_redacted
    }) {
        warnings.insert(ExportWarningV1 {
            code: "external_url_components_redacted".to_owned(),
            message:
                "External URL userinfo, query, and fragment values are omitted from the receipt."
                    .to_owned(),
        });
    }

    Ok(ExportValidationReportV1 {
        profile,
        local_reference_count,
        external_references,
        external_origins: external_origins.into_iter().collect(),
        dynamic_reference_sources,
        warnings: warnings.into_iter().collect(),
        limitations: vec![
            ExportLimitationV1 {
                code: "dynamic_reference_analysis_best_effort".to_owned(),
                message: "JavaScript-built URLs, runtime fetches, navigation, and framework routing are not exhaustively inspected."
                    .to_owned(),
            },
            ExportLimitationV1 {
                code: "ordinary_static_http_profile".to_owned(),
                message: "The supported profile is ordinary unprivileged static HTTP hosting; direct file:// compatibility is not guaranteed."
                    .to_owned(),
            },
        ],
    })
}

pub(super) fn scan_references(
    source_path: &str,
    bytes: &[u8],
) -> Result<ReferenceScan, StaticExportError> {
    let media_type = text_media_type_for_path(source_path);
    let Some(media_type) = media_type else {
        return Ok(ReferenceScan {
            references: Vec::new(),
            dynamic_capable: false,
            analysis_incomplete: false,
        });
    };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| StaticExportError::InvalidTextEncoding(source_path.to_owned()))?;
    match media_type {
        "text/html" | "image/svg+xml" | "application/xml" => {
            let mut references = Vec::new();
            let mut dynamic_capable = false;
            let mut analysis_incomplete = if media_type == "text/html" {
                html_tokenizer_boundary_analysis_incomplete(text)
            } else {
                xml_external_boundary_analysis_incomplete(text)
            };
            for tag in scan_markup_tags(source_path, text)? {
                if tag.closing {
                    continue;
                }
                if tag.attributes.iter().any(|(name, value)| {
                    value.contains('&') && character_reference_sensitive_attribute(&tag.name, name)
                }) {
                    // Browsers decode character references before interpreting
                    // URL-bearing HTML attributes. This bounded scanner does
                    // not duplicate that standards tokenizer, so a raw value
                    // such as `https&colon;//…` must never be classified as a
                    // verified local path or support an offline claim.
                    analysis_incomplete = true;
                }
                if tag.name == "base"
                    && tag
                        .attributes
                        .iter()
                        .any(|(name, value)| name == "href" && !value.trim().is_empty())
                {
                    return Err(StaticExportError::UnsupportedBaseElement(
                        source_path.to_owned(),
                    ));
                }
                if tag.name == "script" {
                    dynamic_capable = true;
                }
                if tag.name == "noscript" {
                    // Its tokenization depends on whether scripting is enabled.
                    // One bounded pass cannot prove both rendering modes.
                    analysis_incomplete = true;
                }
                if let Some(inline_css) = tag.inline_css.as_deref() {
                    let css = css_references(inline_css);
                    references.extend(css.references);
                    analysis_incomplete |= css.analysis_incomplete;
                }
                analysis_incomplete |= tag.raw_text_analysis_incomplete;
                let is_meta_refresh = tag.name == "meta"
                    && tag.attributes.iter().any(|(name, value)| {
                        name == "http-equiv" && value.trim().eq_ignore_ascii_case("refresh")
                    });
                if is_meta_refresh {
                    dynamic_capable = true;
                    match tag
                        .attributes
                        .iter()
                        .find(|(name, _)| name == "content")
                        .map(|(_, value)| meta_refresh_reference(value))
                    {
                        Some(Ok(Some(reference))) => references.push(reference),
                        Some(Ok(None)) => {}
                        Some(Err(())) | None => analysis_incomplete = true,
                    }
                }
                for (name, value) in tag.attributes {
                    if name.starts_with("on") && name.len() > 2 {
                        dynamic_capable = true;
                    }
                    if tag.name == "iframe" && name == "srcdoc" {
                        analysis_incomplete = true;
                    }
                    if matches!(name.as_str(), "archive" | "codebase" | "profile")
                        && !value.trim().is_empty()
                    {
                        // Obsolete fetch/base surfaces have browser-dependent
                        // tokenization and are outside the bounded URL profile.
                        analysis_incomplete = true;
                    }
                    if matches!(
                        name.as_str(),
                        "src"
                            | "href"
                            | "xlink:href"
                            | "poster"
                            | "action"
                            | "formaction"
                            | "background"
                            | "manifest"
                    ) || (tag.name == "object" && name == "data")
                    {
                        references.push(value);
                    } else if matches!(name.as_str(), "srcset" | "imagesrcset") {
                        references.extend(srcset_references(&value));
                    } else if name == "ping" {
                        references.extend(
                            value
                                .split_ascii_whitespace()
                                .filter(|reference| !reference.is_empty())
                                .map(ToOwned::to_owned),
                        );
                    } else if name == "style"
                        || (media_type != "text/html"
                            && svg_presentation_reference_attribute(&name))
                    {
                        let css = css_references(&value);
                        references.extend(css.references);
                        analysis_incomplete |= css.analysis_incomplete;
                    }
                }
            }
            Ok(ReferenceScan {
                references,
                dynamic_capable,
                analysis_incomplete,
            })
        }
        "text/css" => {
            let css = css_references(text);
            Ok(ReferenceScan {
                references: css.references,
                dynamic_capable: false,
                analysis_incomplete: css.analysis_incomplete,
            })
        }
        "text/javascript" => Ok(ReferenceScan {
            references: Vec::new(),
            dynamic_capable: true,
            analysis_incomplete: false,
        }),
        _ => unreachable!("text media type routing is exhaustive"),
    }
}

fn html_tokenizer_boundary_analysis_incomplete(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(relative) = lower[cursor..].find("<") {
        let start = cursor + relative;
        let tail = &lower[start..];
        if tail.starts_with("<?") || tail.starts_with("<![cdata[") {
            return true;
        }
        if let Some(comment_body) = tail.strip_prefix("<!--") {
            let Some(end) = comment_body.find("-->") else {
                return true;
            };
            cursor = start + 4 + end + 3;
            continue;
        }
        if tail.starts_with("<!") {
            const CANONICAL_DOCTYPE: &str = "<!doctype html>";
            if tail.starts_with(CANONICAL_DOCTYPE) {
                cursor = start + CANONICAL_DOCTYPE.len();
                continue;
            }
            return true;
        }
        cursor = start + 1;
    }
    false
}

fn xml_external_boundary_analysis_incomplete(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if lower.contains("<!doctype") {
        return true;
    }
    let mut remainder = lower.trim_start();
    if remainder.starts_with("<?xml") {
        let Some(end) = remainder.find("?>") else {
            return true;
        };
        remainder = &remainder[end + 2..];
    }
    remainder.contains("<?")
}

fn svg_presentation_reference_attribute(name: &str) -> bool {
    matches!(
        name,
        "clip-path"
            | "cursor"
            | "fill"
            | "filter"
            | "marker"
            | "marker-start"
            | "marker-mid"
            | "marker-end"
            | "mask"
            | "stroke"
    )
}

fn character_reference_sensitive_attribute(tag_name: &str, name: &str) -> bool {
    matches!(
        name,
        "src"
            | "href"
            | "xlink:href"
            | "poster"
            | "action"
            | "formaction"
            | "srcset"
            | "imagesrcset"
            | "ping"
            | "background"
            | "manifest"
            | "style"
            | "srcdoc"
    ) || svg_presentation_reference_attribute(name)
        || matches!(name, "archive" | "codebase" | "profile")
        || (tag_name == "object" && name == "data")
        || (tag_name == "meta" && matches!(name, "http-equiv" | "content"))
}

fn resolve_reference(
    source_path: &str,
    raw_reference: &str,
) -> Result<ReferenceResolution, StaticExportError> {
    let reference = raw_reference.trim();
    if reference.is_empty() || reference.starts_with('#') {
        return Ok(ReferenceResolution::Ignored);
    }
    if reference.chars().any(char::is_control) || reference.contains('\\') {
        return Err(StaticExportError::UnsafeReference(source_path.to_owned()));
    }
    let lower = reference.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("file:") {
        return Err(StaticExportError::UnsafeReference(source_path.to_owned()));
    }
    if lower.starts_with("http:") || lower.starts_with("https:") {
        return external_reference(source_path, reference, false)
            .map(ReferenceResolution::External);
    }
    if reference.starts_with("//") {
        return external_reference(source_path, reference, true).map(ReferenceResolution::External);
    }
    if lower.starts_with("blob:") {
        return Ok(ReferenceResolution::RuntimeOnly);
    }
    if ["data:", "mailto:", "tel:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        return Ok(ReferenceResolution::Ignored);
    }
    if has_explicit_scheme(reference) {
        return Err(StaticExportError::UnsafeReference(source_path.to_owned()));
    }

    let without_suffix = reference.split(['?', '#']).next().unwrap_or_default();
    if without_suffix.is_empty() {
        return Ok(ReferenceResolution::Local(source_path.to_owned()));
    }
    let decoded = percent_decode(without_suffix)
        .map_err(|()| StaticExportError::UnsafeReference(source_path.to_owned()))?;
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
                    return Err(StaticExportError::UnsafeReference(source_path.to_owned()));
                }
            }
            value => segments.push(value.to_owned()),
        }
    }
    let resolved = segments.join("/").nfc().collect::<String>();
    let resolved = if resolved.is_empty() {
        "index.html".to_owned()
    } else {
        super::storage::validate_canonical_path(&resolved)
            .map_err(|_| StaticExportError::UnsafeReference(source_path.to_owned()))?;
        resolved
    };
    Ok(ReferenceResolution::Local(resolved))
}

fn external_reference(
    source_path: &str,
    reference: &str,
    protocol_relative: bool,
) -> Result<ExternalReferenceV1, StaticExportError> {
    let parse_value = if protocol_relative {
        format!("https:{reference}")
    } else {
        reference.to_owned()
    };
    let mut url = Url::parse(&parse_value)
        .map_err(|_| StaticExportError::UnsafeReference(source_path.to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(StaticExportError::UnsafeReference(source_path.to_owned()));
    }
    let userinfo_redacted = !url.username().is_empty() || url.password().is_some();
    let query_redacted = url.query().is_some();
    let fragment_redacted = url.fragment().is_some();
    url.set_username("")
        .map_err(|()| StaticExportError::UnsafeReference(source_path.to_owned()))?;
    url.set_password(None)
        .map_err(|()| StaticExportError::UnsafeReference(source_path.to_owned()))?;
    url.set_query(None);
    url.set_fragment(None);
    let mut sanitized_url = url.as_str().to_owned();
    let mut origin = url.origin().ascii_serialization();
    if protocol_relative {
        sanitized_url = sanitized_url
            .strip_prefix("https:")
            .unwrap_or(&sanitized_url)
            .to_owned();
        origin = origin.strip_prefix("https:").unwrap_or(&origin).to_owned();
    }
    Ok(ExternalReferenceV1 {
        source_path: source_path.to_owned(),
        sanitized_url,
        origin,
        userinfo_redacted,
        query_redacted,
        fragment_redacted,
    })
}

fn scan_markup_tags(source_path: &str, text: &str) -> Result<Vec<MarkupTag>, StaticExportError> {
    let bytes = text.as_bytes();
    let html_mode = text_media_type_for_path(source_path) == Some("text/html");
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
                .ok_or_else(|| StaticExportError::MalformedMarkup(source_path.to_owned()))?;
            continue;
        }
        if text[start..].starts_with("<![CDATA[") {
            cursor = text[start + "<![CDATA[".len()..]
                .find("]]>")
                .map(|end| start + "<![CDATA[".len() + end + 3)
                .ok_or_else(|| StaticExportError::MalformedMarkup(source_path.to_owned()))?;
            continue;
        }
        if !html_mode && text[start..].starts_with("<?") {
            cursor = text[start + 2..]
                .find("?>")
                .map(|end| start + 2 + end + 2)
                .ok_or_else(|| StaticExportError::MalformedMarkup(source_path.to_owned()))?;
            continue;
        }
        let Some(end) = markup_tag_end(text, start + 1) else {
            let looks_like_markup = text[start + 1..].chars().next().is_some_and(|character| {
                character.is_ascii_alphabetic() || matches!(character, '/' | '!' | '?')
            });
            if looks_like_markup {
                return Err(StaticExportError::MalformedMarkup(source_path.to_owned()));
            }
            cursor = start + 1;
            continue;
        };
        let body = text[start + 1..end].trim();
        if let Some(mut tag) = parse_tag_body(source_path, body)? {
            if !tag.closing && html_mode && tag.name == "plaintext" {
                tags.push(tag);
                cursor = text.len();
                continue;
            }
            if !tag.closing && raw_text_element(&tag.name, html_mode) {
                let body_start = end + 1;
                let (raw_body, next_cursor) =
                    scan_raw_text_element(source_path, text, body_start, &tag.name)?;
                if tag.name == "style" {
                    tag.inline_css = Some(raw_body.to_owned());
                } else if tag.name == "script" && raw_body.contains("<!--") {
                    // HTML script-data escaped/double-escaped substates need a
                    // standards tokenizer. A legacy escape opener makes the
                    // bounded end-tag scan non-authoritative, so preserve the
                    // export but never issue a complete static-analysis claim.
                    tag.raw_text_analysis_incomplete = true;
                }
                tags.push(tag);
                cursor = next_cursor;
                continue;
            }
            tags.push(tag);
        }
        cursor = end + 1;
    }
    Ok(tags)
}

fn raw_text_element(name: &str, html_mode: bool) -> bool {
    matches!(name, "style" | "script")
        || (html_mode
            && matches!(
                name,
                "title" | "textarea" | "xmp" | "iframe" | "noembed" | "noframes" | "noscript"
            ))
}

fn scan_raw_text_element<'a>(
    source_path: &str,
    text: &'a str,
    body_start: usize,
    tag_name: &str,
) -> Result<(&'a str, usize), StaticExportError> {
    let closing_prefix = format!("</{tag_name}");

    // ASCII case folding preserves byte offsets, including when raw text
    // contains non-ASCII text. Only an appropriate end tag closes the element;
    // markup-looking content before it stays in the raw/RCDATA state.
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

        let close_end = markup_tag_end(text, close_start + 1)
            .ok_or_else(|| StaticExportError::MalformedMarkup(source_path.to_owned()))?;
        let close_body = text[close_start + 1..close_end].trim();
        let closing_body_prefix = &closing_prefix[1..];
        if !close_body
            .get(..closing_body_prefix.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(closing_body_prefix))
        {
            return Err(StaticExportError::MalformedMarkup(source_path.to_owned()));
        }
        let trailing = close_body
            .get(closing_body_prefix.len()..)
            .unwrap_or_default()
            .trim();
        if !trailing.is_empty() && trailing != "/" {
            return Err(StaticExportError::MalformedMarkup(source_path.to_owned()));
        }
        return Ok((&text[body_start..close_start], close_end + 1));
    }

    Err(StaticExportError::MalformedMarkup(source_path.to_owned()))
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

fn parse_tag_body(source_path: &str, body: &str) -> Result<Option<MarkupTag>, StaticExportError> {
    if body.is_empty() || body.starts_with(['!', '?']) {
        return Ok(None);
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
        return Ok(None);
    }
    Ok(Some(MarkupTag {
        name,
        closing,
        attributes: if closing {
            Vec::new()
        } else {
            parse_attributes(source_path, &body[name_end..])?
        },
        inline_css: None,
        raw_text_analysis_incomplete: false,
    }))
}

fn parse_attributes(
    source_path: &str,
    mut input: &str,
) -> Result<Vec<(String, String)>, StaticExportError> {
    let mut attributes = Vec::new();
    while !input.is_empty() {
        input = input.trim_start();
        if input.is_empty() {
            break;
        }
        if input == "/" {
            break;
        }
        let name_end = input
            .find(|character: char| character.is_whitespace() || character == '=')
            .unwrap_or(input.len());
        if name_end == 0
            || input[..name_end].chars().any(|character| {
                character.is_control() || matches!(character, '/' | '<' | '>' | '\'' | '"' | '`')
            })
        {
            return Err(StaticExportError::MalformedMarkup(source_path.to_owned()));
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
                    return Err(StaticExportError::MalformedMarkup(source_path.to_owned()));
                }
            } else {
                let end = input.find(char::is_whitespace).unwrap_or(input.len());
                value = input[..end].to_owned();
                if value
                    .chars()
                    .any(|character| matches!(character, '<' | '>' | '\'' | '"' | '`' | '='))
                {
                    return Err(StaticExportError::MalformedMarkup(source_path.to_owned()));
                }
                input = &input[end..];
            }
        }
        attributes.push((name, value));
    }
    Ok(attributes)
}

fn srcset_references(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|candidate| candidate.split_whitespace().next())
        .filter(|candidate| !candidate.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn meta_refresh_reference(value: &str) -> Result<Option<String>, ()> {
    let value = value.trim();
    let Some((delay, directive)) = value.split_once(';') else {
        return valid_meta_refresh_delay(value).then_some(None).ok_or(());
    };
    if !valid_meta_refresh_delay(delay.trim()) {
        return Err(());
    }

    let directive = directive.trim();
    let keyword = directive.get(..3).ok_or(())?;
    if !keyword.eq_ignore_ascii_case("url") {
        return Err(());
    }
    let after_keyword = directive.get(3..).ok_or(())?;
    if after_keyword
        .chars()
        .next()
        .is_some_and(|character| !character.is_ascii_whitespace() && character != '=')
    {
        return Err(());
    }
    let reference = after_keyword
        .trim_start()
        .strip_prefix('=')
        .ok_or(())?
        .trim();
    if reference.is_empty() {
        return Err(());
    }

    if let Some(quote @ ('\'' | '"')) = reference.chars().next() {
        let quoted = &reference[quote.len_utf8()..];
        let end = quoted.find(quote).ok_or(())?;
        if !quoted[end + quote.len_utf8()..].trim().is_empty() || end == 0 {
            return Err(());
        }
        return Ok(Some(quoted[..end].to_owned()));
    }
    if reference.contains(['\'', '"']) {
        return Err(());
    }
    Ok(Some(reference.to_owned()))
}

fn valid_meta_refresh_delay(value: &str) -> bool {
    let mut decimal_point_seen = false;
    let mut digit_seen = false;
    !value.is_empty()
        && value.bytes().all(|byte| match byte {
            b'0'..=b'9' => {
                digit_seen = true;
                true
            }
            b'.' if !decimal_point_seen => {
                decimal_point_seen = true;
                true
            }
            _ => false,
        })
        && digit_seen
}

fn css_references(text: &str) -> CssReferenceScan {
    let bytes = text.as_bytes();
    let mut scan = CssReferenceScan::default();

    // Correctly decoding CSS escapes and comment-spliced token streams needs
    // a standards tokenizer. Until one is part of this small deterministic
    // exporter, their presence is an explicit analysis boundary: never issue
    // the stronger offline-self-contained claim.
    if bytes.contains(&b'\\')
        || bytes.windows(2).any(|window| window == b"/*")
        || bytes.windows(2).any(|window| window == b"*/")
    {
        scan.analysis_incomplete = true;
        return scan;
    }

    let lower = text.to_ascii_lowercase();
    if lower.contains("image-set(") || lower.contains("-webkit-image-set(") {
        // Quoted image-set candidates need a complete CSS value tokenizer.
        // Continue collecting ordinary url(...) candidates, but withhold the
        // stronger completeness claim for this grammar.
        scan.analysis_incomplete = true;
    }

    let mut cursor = 0;
    while cursor < bytes.len() {
        if matches!(bytes[cursor], b'\'' | b'"') {
            match css_quoted_end(bytes, cursor) {
                Some(end) => cursor = end + 1,
                None => {
                    scan.analysis_incomplete = true;
                    break;
                }
            }
            continue;
        }

        if bytes[cursor] == b'@'
            && css_keyword_at(bytes, cursor + 1, b"import")
            && css_token_boundary(bytes.get(cursor + 1 + b"import".len()).copied())
        {
            let mut value_start = cursor + 1 + b"import".len();
            skip_ascii_whitespace(bytes, &mut value_start);
            match bytes.get(value_start).copied() {
                Some(quote @ (b'\'' | b'"')) => {
                    let Some(end) = css_quoted_end(bytes, value_start) else {
                        scan.analysis_incomplete = true;
                        break;
                    };
                    let value = &text[value_start + 1..end];
                    if !value.is_empty() {
                        scan.references.push(value.to_owned());
                    }
                    cursor = end + 1;
                    debug_assert_eq!(bytes[value_start], quote);
                    continue;
                }
                Some(_) if css_keyword_at(bytes, value_start, b"url") => {
                    // Let the common url(...) branch below parse it once.
                    cursor = value_start;
                    continue;
                }
                _ => {
                    scan.analysis_incomplete = true;
                    break;
                }
            }
        }

        let previous_is_name = cursor
            .checked_sub(1)
            .and_then(|index| bytes.get(index).copied())
            .is_some_and(css_name_byte);
        if !previous_is_name && css_keyword_at(bytes, cursor, b"url") {
            let mut open = cursor + b"url".len();
            skip_ascii_whitespace(bytes, &mut open);
            if bytes.get(open) == Some(&b'(') {
                match css_url_argument(text, open) {
                    Some((value, next)) => {
                        if !value.is_empty() {
                            scan.references.push(value);
                        }
                        cursor = next;
                        continue;
                    }
                    None => {
                        scan.analysis_incomplete = true;
                        break;
                    }
                }
            }
        }
        cursor += 1;
    }
    scan
}

fn css_keyword_at(bytes: &[u8], start: usize, keyword: &[u8]) -> bool {
    bytes
        .get(start..start.saturating_add(keyword.len()))
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(keyword))
}

fn css_token_boundary(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| !css_name_byte(byte))
}

fn css_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') || !byte.is_ascii()
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}

fn css_quoted_end(bytes: &[u8], quote_start: usize) -> Option<usize> {
    let quote = *bytes.get(quote_start)?;
    debug_assert!(matches!(quote, b'\'' | b'"'));
    bytes[quote_start + 1..]
        .iter()
        .position(|byte| *byte == quote)
        .map(|relative| quote_start + 1 + relative)
}

fn css_url_argument(text: &str, open: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let mut cursor = open + 1;
    skip_ascii_whitespace(bytes, &mut cursor);
    if let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() {
        let end = css_quoted_end(bytes, cursor)?;
        let value = text[cursor + 1..end].to_owned();
        cursor = end + 1;
        skip_ascii_whitespace(bytes, &mut cursor);
        if bytes.get(cursor) != Some(&b')') {
            return None;
        }
        debug_assert_eq!(bytes[end], quote);
        return Some((value, cursor + 1));
    }

    let close = bytes[cursor..]
        .iter()
        .position(|byte| *byte == b')')
        .map(|relative| cursor + relative)?;
    let raw = text[cursor..close].trim();
    if raw
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b'(' | b'\'' | b'"' | b'`'))
    {
        return None;
    }
    Some((raw.to_owned(), close + 1))
}

fn percent_decode(value: &str) -> Result<String, ()> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes
                .get(index + 1)
                .and_then(|byte| hex_value(*byte))
                .ok_or(())?;
            let low = bytes
                .get(index + 2)
                .and_then(|byte| hex_value(*byte))
                .ok_or(())?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn text_media_type_for_path(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        Some("text/html")
    } else if lower.ends_with(".css") {
        Some("text/css")
    } else if lower.ends_with(".svg") {
        Some("image/svg+xml")
    } else if lower.ends_with(".xml") {
        Some("application/xml")
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
        Some("text/javascript")
    } else {
        None
    }
}

fn is_html_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

fn has_explicit_scheme(reference: &str) -> bool {
    let Some(colon) = reference.find(':') else {
        return false;
    };
    let boundary = reference.find(['/', '?', '#']).unwrap_or(reference.len());
    colon < boundary
        && reference[..colon].bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_alphabetic()
            } else {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')
            }
        })
}

fn reserved_export_path(path: &str) -> bool {
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

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_utc_timestamp(value: &str) -> bool {
    (20..=MAX_TIMESTAMP_BYTES).contains(&value.len())
        && value.ends_with('Z')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b':' | b'.' | b'T' | b'Z'))
        && value.get(4..5) == Some("-")
        && value.get(7..8) == Some("-")
        && value.get(10..11) == Some("T")
        && value.get(13..14) == Some(":")
        && value.get(16..17) == Some(":")
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, StaticExportError> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| StaticExportError::Serialization)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    const DIGEST_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn input<'a>(files: &'a BTreeMap<String, Vec<u8>>) -> StaticExportInput<'a> {
        StaticExportInput {
            source_revision_id: "rev_0123456789abcdef",
            source_manifest_sha256: DIGEST_A,
            generated_at_utc: "2026-07-19T12:00:00Z",
            entry_point: "index.html",
            files,
        }
    }

    fn ordinary_static_host_fixture() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (
                "assets/app.js".to_owned(),
                b"const route = location.pathname; console.log(route);".to_vec(),
            ),
            ("assets/bg.png".to_owned(), vec![0x89, b'P', b'N', b'G']),
            (
                "assets/hero.svg".to_owned(),
                br#"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"#
                    .to_vec(),
            ),
            (
                "assets/site.css".to_owned(),
                b"body{background:url('../assets/bg.png')}".to_vec(),
            ),
            (
                "docs/index.html".to_owned(),
                br#"<!doctype html><a href="../">Home</a>"#.to_vec(),
            ),
            (
                "index.html".to_owned(),
                br#"<!doctype html><link rel="stylesheet" href="assets/site.css"><img src="/assets/hero.svg?cache=1#hero"><script src="assets/app.js"></script><a href="docs/">Docs</a>"#
                    .to_vec(),
            ),
        ])
    }

    #[test]
    fn same_input_produces_exact_receipt_and_zip_bytes() {
        let files = ordinary_static_host_fixture();
        let first = generate_static_export(&input(&files)).unwrap();
        let second = generate_static_export(&input(&files)).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.receipt.archive.sha256, sha256(&first.zip_bytes));
        assert_eq!(first.receipt.archive.byte_length, first.zip_bytes.len());
        assert_eq!(first.receipt_sha256, sha256(&first.receipt_json));
        let persisted: ExportReceiptV1 = serde_json::from_slice(&first.receipt_json).unwrap();
        assert_eq!(persisted, first.receipt);
    }

    #[test]
    fn ordinary_static_http_fixture_has_canonical_manifest_and_resolved_assets() {
        let files = ordinary_static_host_fixture();
        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact
                .receipt
                .file_manifest
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "assets/app.js",
                "assets/bg.png",
                "assets/hero.svg",
                "assets/site.css",
                "docs/index.html",
                "index.html"
            ]
        );
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::OfflineSelfContained
        );
        assert_eq!(artifact.receipt.validation.local_reference_count, 6);
        assert_eq!(
            artifact.receipt.validation.dynamic_reference_sources,
            vec!["assets/app.js", "index.html"]
        );
        assert!(
            artifact
                .receipt
                .validation
                .warnings
                .iter()
                .any(|warning| { warning.code == "dynamic_references_unchecked" })
        );
        assert!(
            artifact
                .receipt
                .validation
                .limitations
                .iter()
                .any(|limitation| { limitation.code == "ordinary_static_http_profile" })
        );

        let mut archive = zip::ZipArchive::new(Cursor::new(&artifact.zip_bytes)).unwrap();
        assert_eq!(archive.len(), files.len());
        for (expected_path, expected_bytes) in &files {
            let mut actual = Vec::new();
            archive
                .by_name(expected_path)
                .unwrap()
                .read_to_end(&mut actual)
                .unwrap();
            assert_eq!(&actual, expected_bytes);
        }
    }

    #[test]
    fn broken_escape_javascript_and_file_references_fail_closed() {
        for (reference, expected) in [
            (
                "missing.css",
                StaticExportError::MissingLocalReference {
                    source_path: "index.html".to_owned(),
                    resolved_path: "missing.css".to_owned(),
                },
            ),
            (
                "../../outside.css",
                StaticExportError::UnsafeReference("index.html".to_owned()),
            ),
            (
                "javascript:alert(1)",
                StaticExportError::UnsafeReference("index.html".to_owned()),
            ),
            (
                "file:///etc/passwd",
                StaticExportError::UnsafeReference("index.html".to_owned()),
            ),
            (
                "%2e%2e/%2e%2e/secret.css",
                StaticExportError::UnsafeReference("index.html".to_owned()),
            ),
        ] {
            let files = BTreeMap::from([(
                "index.html".to_owned(),
                format!(r#"<!doctype html><link rel="stylesheet" href="{reference}">"#)
                    .into_bytes(),
            )]);
            assert_eq!(generate_static_export(&input(&files)), Err(expected));
        }
    }

    #[test]
    fn inline_style_and_object_data_missing_local_references_fail_closed() {
        for markup in [
            "<!doctype html><style>body{background:url('missing.png')}</style>",
            "<!doctype html><object data='missing.pdf'></object>",
        ] {
            let files = BTreeMap::from([("index.html".to_owned(), markup.as_bytes().to_vec())]);
            let expected_path = if markup.contains("missing.png") {
                "missing.png"
            } else {
                "missing.pdf"
            };
            assert_eq!(
                generate_static_export(&input(&files)),
                Err(StaticExportError::MissingLocalReference {
                    source_path: "index.html".to_owned(),
                    resolved_path: expected_path.to_owned(),
                })
            );
        }
    }

    #[test]
    fn inline_style_external_import_and_url_are_reported_and_redacted() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><STYLE>@IMPORT 'https://styles.example.test/site.css?token=secret'; body{background:url(//images.example.test/hero.png#private)}</STYLE>"#
                .to_vec(),
        )]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        let report = &artifact.receipt.validation;
        assert_eq!(report.profile, StaticHostingProfile::StandaloneStatic);
        assert_eq!(report.external_references.len(), 2);
        assert_eq!(
            report.external_origins,
            vec!["//images.example.test", "https://styles.example.test"]
        );
        assert!(
            report
                .external_references
                .iter()
                .all(|reference| !reference.sanitized_url.contains("secret")
                    && !reference.sanitized_url.contains("private"))
        );
    }

    #[test]
    fn unclosed_or_malformed_style_elements_fail_closed() {
        for markup in [
            "<!doctype html><style>body{color:red}",
            "<!doctype html><style>body{color:red}</style media=screen>",
            "<!doctype html><style media='unterminated>body{color:red}</style>",
        ] {
            let files = BTreeMap::from([("index.html".to_owned(), markup.as_bytes().to_vec())]);
            assert_eq!(
                generate_static_export(&input(&files)),
                Err(StaticExportError::MalformedMarkup("index.html".to_owned()))
            );
        }
    }

    #[test]
    fn inline_style_ambiguity_never_claims_offline_containment() {
        for css in [
            r#"body{background:u\72l(https://escape.example.test/a.png)}"#,
            r#"body{background:u/**/rl(https://comment.example.test/a.png)}"#,
            r#"body{background:url('https://unterminated.example.test/a.png)}"#,
        ] {
            let files = BTreeMap::from([(
                "index.html".to_owned(),
                format!("<!doctype html><style>{css}</style>").into_bytes(),
            )]);
            let artifact = generate_static_export(&input(&files)).unwrap();
            assert_eq!(
                artifact.receipt.validation.profile,
                StaticHostingProfile::StandaloneStatic,
                "inline CSS analysis ambiguity was over-claimed: {css}"
            );
            assert!(
                artifact
                    .receipt
                    .validation
                    .warnings
                    .iter()
                    .any(|warning| warning.code == "static_reference_analysis_incomplete")
            );
            assert_eq!(
                artifact.receipt.validation.dynamic_reference_sources,
                vec!["index.html"]
            );
        }
    }

    #[test]
    fn style_and_script_raw_text_do_not_create_markup_attribute_references() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><style>p::before{content:'<object data=https://css-text.example.test/a>'}</style><script>const template = '<img src=https://script-text.example.test/a>';</script>"#
                .to_vec(),
        )]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::OfflineSelfContained
        );
        assert!(artifact.receipt.validation.external_references.is_empty());
        assert_eq!(
            artifact.receipt.validation.dynamic_reference_sources,
            vec!["index.html"]
        );
    }

    #[test]
    fn appropriate_raw_text_end_tags_prevent_hidden_external_markup() {
        for markup in [
            r#"<!doctype html><script>const a="</scriptx>"; const b="<foo title='";</script><img src="https://evil.example.test/script.png">' >"#,
            r#"<!doctype html><textarea><foo title='</textarea><img src="https://evil.example.test/textarea.png">' >"#,
        ] {
            let files = BTreeMap::from([("index.html".to_owned(), markup.as_bytes().to_vec())]);
            let artifact = generate_static_export(&input(&files)).unwrap();
            assert_eq!(
                artifact.receipt.validation.profile,
                StaticHostingProfile::StandaloneStatic
            );
            assert_eq!(artifact.receipt.validation.external_references.len(), 1);
            assert_eq!(
                artifact.receipt.validation.external_references[0].origin,
                "https://evil.example.test"
            );
        }
    }

    #[test]
    fn html_tokenizer_specific_boundaries_never_support_an_offline_claim() {
        for markup in [
            r#"<!doctype html><script><!-- legacy escaped script marker --></script>"#,
            r#"<!doctype html><?bounded-processing "quoted > text"?><main>Ready</main>"#,
            r#"<!doctype html><![CDATA[html-context text]]><main>Ready</main>"#,
        ] {
            let files = BTreeMap::from([("index.html".to_owned(), markup.as_bytes().to_vec())]);
            let artifact = generate_static_export(&input(&files)).unwrap();
            assert_eq!(
                artifact.receipt.validation.profile,
                StaticHostingProfile::StandaloneStatic
            );
            assert!(
                artifact
                    .receipt
                    .validation
                    .warnings
                    .iter()
                    .any(|warning| { warning.code == "static_reference_analysis_incomplete" })
            );
        }
    }

    #[test]
    fn meta_refresh_extracts_quoted_case_insensitive_url_and_rejects_unsafe_scheme() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><meta HTTP-EQUIV=' ReFrEsH ' content=' 0 ; UrL = "https://redirect.example.test/next?token=secret#private" '>"#
                .to_vec(),
        )]);
        let artifact = generate_static_export(&input(&files)).unwrap();
        let external = &artifact.receipt.validation.external_references[0];
        assert_eq!(external.sanitized_url, "https://redirect.example.test/next");
        assert!(external.query_redacted);
        assert!(external.fragment_redacted);

        let unsafe_files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><meta http-equiv=refresh content="0; URL=javascript:alert(1)">"#
                .to_vec(),
        )]);
        assert_eq!(
            generate_static_export(&input(&unsafe_files)),
            Err(StaticExportError::UnsafeReference("index.html".to_owned()))
        );
    }

    #[test]
    fn ambiguous_meta_refresh_and_iframe_srcdoc_are_analysis_incomplete() {
        for markup in [
            "<!doctype html><meta http-equiv=refresh content='0; target=next.html'>",
            "<!doctype html><iframe src='frame.html' srcdoc=\"<img src='missing.png'>\"></iframe>",
        ] {
            let mut files = BTreeMap::from([("index.html".to_owned(), markup.as_bytes().to_vec())]);
            if markup.contains("frame.html") {
                files.insert("frame.html".to_owned(), b"<!doctype html>".to_vec());
            }
            let artifact = generate_static_export(&input(&files)).unwrap();
            assert_eq!(
                artifact.receipt.validation.profile,
                StaticHostingProfile::StandaloneStatic
            );
            assert!(
                artifact
                    .receipt
                    .validation
                    .warnings
                    .iter()
                    .any(|warning| warning.code == "static_reference_analysis_incomplete")
            );
            assert!(
                artifact
                    .receipt
                    .validation
                    .dynamic_reference_sources
                    .contains(&"index.html".to_owned())
            );
        }
    }

    #[test]
    fn html_character_references_cannot_disguise_an_external_url_as_local() {
        let files = BTreeMap::from([
            (
                "index.html".to_owned(),
                br#"<!doctype html><link rel="stylesheet" href="https&colon;//evil.example.test/theme.css">"#
                    .to_vec(),
            ),
            (
                "https&colon;/evil.example.test/theme.css".to_owned(),
                b"body { color: red; }".to_vec(),
            ),
        ]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::StandaloneStatic
        );
        assert!(
            artifact
                .receipt
                .validation
                .warnings
                .iter()
                .any(|warning| { warning.code == "static_reference_analysis_incomplete" })
        );
        assert!(
            artifact
                .receipt
                .validation
                .dynamic_reference_sources
                .contains(&"index.html".to_owned())
        );
    }

    #[test]
    fn ping_and_image_srcset_destinations_are_reported() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br##"<!doctype html><link rel="preload" imagesrcset="https://images.example.test/hero.png 1x, //images.example.test/hero@2x.png 2x"><a href="#ready" ping="https://audit.example.test/a //audit.example.test/b">Ready</a>"##
                .to_vec(),
        )]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::StandaloneStatic
        );
        assert_eq!(
            artifact.receipt.validation.external_origins,
            vec![
                "//audit.example.test",
                "//images.example.test",
                "https://audit.example.test",
                "https://images.example.test",
            ]
        );
    }

    #[test]
    fn quoted_css_image_set_and_encoded_ping_are_analysis_incomplete() {
        let files = BTreeMap::from([
            (
                "index.html".to_owned(),
                br#"<!doctype html><link rel="stylesheet" href="site.css"><a ping="https&colon;//audit.example.test/p">Ready</a>"#
                    .to_vec(),
            ),
            (
                "site.css".to_owned(),
                br#".hero { background: image-set("https://images.example.test/a.png" 1x); }"#
                    .to_vec(),
            ),
            (
                "https&colon;/audit.example.test/p".to_owned(),
                b"placeholder".to_vec(),
            ),
        ]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::StandaloneStatic
        );
        for source in ["index.html", "site.css"] {
            assert!(
                artifact
                    .receipt
                    .validation
                    .dynamic_reference_sources
                    .contains(&source.to_owned())
            );
        }
    }

    #[test]
    fn xml_processing_instructions_and_svg_presentation_urls_keep_reference_boundaries() {
        let files = BTreeMap::from([
            (
                "index.html".to_owned(),
                br#"<!doctype html><img src="asset.svg">"#.to_vec(),
            ),
            (
                "asset.svg".to_owned(),
                br#"<?probe data='?><svg xmlns="http://www.w3.org/2000/svg"><image href="https://images.example.test/a.png"/><path fill="url(https://paint.example.test/fill.svg)"/></svg>"#
                    .to_vec(),
            ),
        ]);
        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::StandaloneStatic
        );
        assert_eq!(
            artifact.receipt.validation.external_origins,
            vec!["https://images.example.test", "https://paint.example.test"]
        );

        let missing = BTreeMap::from([
            (
                "index.html".to_owned(),
                br#"<!doctype html><img src="asset.svg">"#.to_vec(),
            ),
            (
                "asset.svg".to_owned(),
                br#"<?probe data='?><svg xmlns="http://www.w3.org/2000/svg"><image href="missing.png"/></svg>"#
                    .to_vec(),
            ),
        ]);
        assert_eq!(
            generate_static_export(&input(&missing)),
            Err(StaticExportError::MissingLocalReference {
                source_path: "asset.svg".to_owned(),
                resolved_path: "missing.png".to_owned(),
            })
        );
    }

    #[test]
    fn external_urls_are_redacted_without_retaining_secret_components() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><script src="https://alice:password@example.test/assets/app.js?token=TOP_SECRET#session"></script><link href="//cdn.example.test/site.css?key=SECOND_SECRET#theme"><img src="https://example.test/logo.svg">"#
                .to_vec(),
        )]);
        let artifact = generate_static_export(&input(&files)).unwrap();
        let report = &artifact.receipt.validation;
        assert_eq!(report.profile, StaticHostingProfile::StandaloneStatic);
        assert_eq!(
            report.external_origins,
            vec!["//cdn.example.test", "https://example.test"]
        );
        assert_eq!(report.external_references.len(), 3);
        let credentialed = report
            .external_references
            .iter()
            .find(|reference| reference.sanitized_url.contains("assets/app.js"))
            .unwrap();
        assert_eq!(
            credentialed.sanitized_url,
            "https://example.test/assets/app.js"
        );
        assert!(credentialed.userinfo_redacted);
        assert!(credentialed.query_redacted);
        assert!(credentialed.fragment_redacted);
        let receipt = String::from_utf8(artifact.receipt_json).unwrap();
        for secret in [
            "alice",
            "password",
            "TOP_SECRET",
            "SECOND_SECRET",
            "session",
        ] {
            assert!(!receipt.contains(secret), "receipt leaked {secret}");
        }
    }

    #[test]
    fn quoted_greater_than_does_not_hide_a_later_external_attribute() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><img alt="1 > 0" data-note='still > quoted' src="https://cdn.example.test/hero.svg?secret=redacted">"#
                .to_vec(),
        )]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        let report = &artifact.receipt.validation;
        assert_eq!(report.profile, StaticHostingProfile::StandaloneStatic);
        assert_eq!(report.external_references.len(), 1);
        assert_eq!(
            report.external_references[0].sanitized_url,
            "https://cdn.example.test/hero.svg"
        );
        assert!(report.external_references[0].query_redacted);
    }

    #[test]
    fn unterminated_quoted_markup_and_comments_fail_closed() {
        for markup in [
            r#"<!doctype html><img alt="unterminated > <script src=https://evil.example.test/x.js>"#,
            "<!doctype html><!-- unterminated",
        ] {
            let files = BTreeMap::from([("index.html".to_owned(), markup.as_bytes().to_vec())]);
            assert_eq!(
                generate_static_export(&input(&files)),
                Err(StaticExportError::MalformedMarkup("index.html".to_owned()))
            );
        }
    }

    #[test]
    fn css_escape_comment_and_parse_ambiguity_never_claim_offline_containment() {
        for css in [
            r#"body{background:u\72l(https://escape.example.test/a.png)}"#,
            r#"@im\port "https://escape.example.test/site.css";"#,
            r#"body{background:u/**/rl(https://comment.example.test/a.png)}"#,
            r#"body{background:url("https://unterminated.example.test/a.png)}"#,
        ] {
            let files = BTreeMap::from([
                (
                    "index.html".to_owned(),
                    br#"<!doctype html><link rel="stylesheet" href="site.css">"#.to_vec(),
                ),
                ("site.css".to_owned(), css.as_bytes().to_vec()),
            ]);
            let artifact = generate_static_export(&input(&files)).unwrap();
            assert_eq!(
                artifact.receipt.validation.profile,
                StaticHostingProfile::StandaloneStatic,
                "CSS analysis ambiguity was over-claimed: {css}"
            );
            assert!(
                artifact
                    .receipt
                    .validation
                    .warnings
                    .iter()
                    .any(|warning| warning.code == "static_reference_analysis_incomplete"),
                "missing fail-closed warning: {css}"
            );
            assert!(
                artifact
                    .receipt
                    .validation
                    .dynamic_reference_sources
                    .contains(&"site.css".to_owned())
            );
        }
    }

    #[test]
    fn quoted_css_url_parenthesis_does_not_truncate_external_reference() {
        let files = BTreeMap::from([
            (
                "index.html".to_owned(),
                br#"<!doctype html><link rel="stylesheet" href="site.css">"#.to_vec(),
            ),
            (
                "site.css".to_owned(),
                br#"body{background-image:url("https://cdn.example.test/a)b.png?secret=hidden")}"#
                    .to_vec(),
            ),
        ]);

        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::StandaloneStatic
        );
        assert_eq!(
            artifact.receipt.validation.external_references[0].sanitized_url,
            "https://cdn.example.test/a)b.png"
        );
        assert!(artifact.receipt.validation.external_references[0].query_redacted);
        assert!(
            !artifact
                .receipt
                .validation
                .warnings
                .iter()
                .any(|warning| warning.code == "static_reference_analysis_incomplete")
        );
    }

    #[test]
    fn dynamic_references_are_reported_as_unchecked_not_as_verified() {
        let files = BTreeMap::from([
            (
                "app.js".to_owned(),
                b"fetch('/runtime/' + window.location.hash)".to_vec(),
            ),
            (
                "index.html".to_owned(),
                br#"<!doctype html><button onclick="location.href='/next'">Next</button><script src="app.js"></script>"#
                    .to_vec(),
            ),
        ]);
        let artifact = generate_static_export(&input(&files)).unwrap();
        assert_eq!(
            artifact.receipt.validation.profile,
            StaticHostingProfile::OfflineSelfContained
        );
        assert_eq!(
            artifact.receipt.validation.dynamic_reference_sources,
            vec!["app.js", "index.html"]
        );
        assert!(
            artifact
                .receipt
                .validation
                .warnings
                .iter()
                .any(|warning| { warning.code == "dynamic_references_unchecked" })
        );
        assert!(
            artifact
                .receipt
                .validation
                .limitations
                .iter()
                .any(|limitation| { limitation.code == "dynamic_reference_analysis_best_effort" })
        );
    }

    #[test]
    fn entry_point_and_manifest_inputs_are_bound_and_fail_closed() {
        let files = ordinary_static_host_fixture();
        let mut request = input(&files);
        request.entry_point = "missing.html";
        assert_eq!(
            generate_static_export(&request),
            Err(StaticExportError::InvalidEntryPoint)
        );

        let mut request = input(&files);
        request.source_manifest_sha256 = "A";
        assert_eq!(
            generate_static_export(&request),
            Err(StaticExportError::InvalidSourceManifest)
        );
    }
}
