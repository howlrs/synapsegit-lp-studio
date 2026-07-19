//! Pure, local-only generation of the LP publication projection.
//!
//! This module intentionally has no filesystem, process, Git, HTTP client, or
//! remote-publication API. Its input contains only the public-safe fields that
//! the LP Studio publication review is allowed to disclose. In particular,
//! there is no input slot for raw LP bytes, prompts, provider responses,
//! private rationale, credentials, internal Actor values, repository paths, or
//! full Target/DOM quotes.

use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const PUBLICATION_SCHEMA: &str = "org.synapsegit-lp-studio.publication";
const PUBLICATION_SCHEMA_VERSION: u16 = 1;
const PUBLICATION_GENERATOR: &str = "org.synapsegit-lp-studio.publication-generator";
const PUBLICATION_GENERATOR_VERSION: u16 = 1;
const PUBLICATION_RENDERER: &str = "org.synapsegit-lp-studio.publication-renderer";
const PUBLICATION_RENDERER_VERSION: u16 = 1;
const SYNAPSEGIT_CONTRACT: &str = "synapsegit.generic-artifact";
const SYNAPSEGIT_CONTRACT_VERSION: u16 = 1;
const MAX_PUBLIC_LABEL_BYTES: usize = 256;
const MAX_PUBLIC_TITLE_BYTES: usize = 256;
const MAX_PUBLIC_SUMMARY_BYTES: usize = 4 * 1024;
const MAX_PUBLIC_NOTE_BYTES: usize = 2 * 1024;
const MAX_PROVIDER_LABEL_BYTES: usize = 128;
const MAX_MODEL_LABEL_BYTES: usize = 256;
const MAX_PUBLIC_ID_BYTES: usize = 128;

const LIMITATIONS: [(&str, &str); 7] = [
    (
        "derived_read_only_snapshot",
        "This bundle is a derived read-only snapshot; SynapseGit remains the provenance and Decision authority.",
    ),
    (
        "checksum_not_signature",
        "Checksums detect byte changes inside this bundle but do not prove publisher identity, authorship, or publication permission.",
    ),
    (
        "identifier_correlation",
        "Revision identifiers and checksums can correlate this record with another copy and require review before external publication.",
    ),
    (
        "identity_scope",
        "Byte and graph identity do not prove truth, rights, semantic correctness, visual correctness, or physical change.",
    ),
    (
        "raw_assets_omitted",
        "Raw LP assets and thumbnails are omitted; publishing them requires a separate opt-in derivative pipeline and rights review.",
    ),
    (
        "private_fields_omitted",
        "Prompts, provider responses, private rationale, credentials, internal authority values, repository paths, and full Target quotes are structurally omitted.",
    ),
    (
        "no_remote_operation",
        "Generation performs no Git, GitHub, network, upload, or remote publication operation.",
    ),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicationInput {
    pub(super) project: PublicProjectInput,
    pub(super) accepted: PublicAcceptedBindingInput,
    pub(super) session: PublicSessionInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicProjectInput {
    pub(super) label: String,
    pub(super) title: String,
    pub(super) summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicAcceptedBindingInput {
    pub(super) revision_id: String,
    pub(super) site_manifest_sha256: String,
    /// Present only after the pinned SynapseGit adapter has reported the
    /// corresponding generic-artifact manifest digest.
    pub(super) synapse_artifact_manifest_sha256: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicProposalAttributionInput {
    /// A bounded public display label, not a credential or provider receipt.
    pub(super) provider_label: String,
    /// A bounded public display label, not proof that SynapseGit ran a model.
    pub(super) model_label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicDisposition {
    AdoptedUnchanged,
    Rejected,
    Deferred,
}

impl PublicDisposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::AdoptedUnchanged => "adopted_unchanged",
            Self::Rejected => "rejected",
            Self::Deferred => "deferred",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum IncompleteReason {
    ProposalPending,
    DecisionOutcomeUnknown,
}

impl IncompleteReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProposalPending => "proposal_pending",
            Self::DecisionOutcomeUnknown => "decision_outcome_unknown",
        }
    }

    fn limitation(self) -> Limitation {
        match self {
            Self::ProposalPending => Limitation {
                code: "incomplete_proposal_pending".to_owned(),
                message: "A Proposal is recorded, but no Human Decision is present.".to_owned(),
            },
            Self::DecisionOutcomeUnknown => Limitation {
                code: "incomplete_decision_outcome_unknown".to_owned(),
                message: "A Decision may have run, but no authoritative outcome receipt has been reconciled; no disposition is claimed.".to_owned(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PublicSessionInput {
    Complete {
        proposal_manifest_sha256: String,
        decision_receipt_sha256: String,
        attribution: PublicProposalAttributionInput,
        disposition: PublicDisposition,
        /// Explicitly authored for publication. This is never populated from
        /// the private Decision rationale.
        public_decision_note: Option<String>,
    },
    Incomplete {
        proposal_manifest_sha256: String,
        attribution: PublicProposalAttributionInput,
        reason: IncompleteReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicationBundle {
    pub(super) files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub(super) enum PublicationError {
    #[error("invalid public publication input: {0}")]
    InvalidInput(&'static str),
    #[error("publication serialization failed")]
    Serialization,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ValueOrigin {
    VerifiedFromSynapse,
    ObservedFromSynapse,
    DerivedSummary,
    AuthorSupplied,
}

impl ValueOrigin {
    fn as_str(self) -> &'static str {
        match self {
            Self::VerifiedFromSynapse => "verified_from_synapse",
            Self::ObservedFromSynapse => "observed_from_synapse",
            Self::DerivedSummary => "derived_summary",
            Self::AuthorSupplied => "author_supplied",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OriginValue<T: Serialize> {
    value: T,
    origin: ValueOrigin,
}

impl<T: Serialize> OriginValue<T> {
    fn new(value: T, origin: ValueOrigin) -> Self {
        Self { value, origin }
    }
}

#[derive(Serialize)]
struct SchemaIdentity {
    name: &'static str,
    version: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicProjection {
    schema: SchemaIdentity,
    generator: SchemaIdentity,
    application: ApplicationProjection,
    synapse_git: SynapseGitProjection,
    project: ProjectProjection,
    accepted: AcceptedProjection,
    session: SessionProjection,
    publication: PublicationPolicy,
    limitations: Vec<Limitation>,
}

#[derive(Serialize)]
struct ApplicationProjection {
    version: OriginValue<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynapseGitProjection {
    contract: SchemaIdentity,
    assurance: &'static str,
}

#[derive(Serialize)]
struct ProjectProjection {
    label: OriginValue<String>,
    title: OriginValue<String>,
    summary: OriginValue<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptedProjection {
    revision_id: OriginValue<String>,
    site_manifest_sha256: OriginValue<String>,
    synapse_artifact_manifest_sha256: Option<OriginValue<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionProjection {
    completeness: CompletenessProjection,
    proposal: ProposalProjection,
    human_decision: Option<HumanDecisionProjection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletenessProjection {
    state: &'static str,
    proposal_present: bool,
    decision_present: bool,
    incomplete_reason: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposalProjection {
    manifest_sha256: OriginValue<String>,
    attribution: AttributionProjection,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttributionProjection {
    source: OriginValue<&'static str>,
    execution_verified: OriginValue<bool>,
    provider_label: OriginValue<String>,
    model_label: OriginValue<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanDecisionProjection {
    disposition: OriginValue<&'static str>,
    receipt_sha256: OriginValue<String>,
    public_note: Option<OriginValue<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicationPolicy {
    visibility: &'static str,
    network_operations: &'static str,
    remote_publication: &'static str,
    raw_lp_assets_included: bool,
    thumbnails_included: bool,
    source_private_rationale_included: bool,
    omitted_by_structure: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Limitation {
    code: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleManifest {
    schema: SchemaIdentity,
    generator: SchemaIdentity,
    renderer: SchemaIdentity,
    target: &'static str,
    projection_sha256: String,
    projection_byte_length: usize,
    checksum_profile: &'static str,
    files: Vec<ManifestFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    path: &'static str,
    role: &'static str,
    media_type: &'static str,
}

#[derive(Serialize)]
struct BundleChecksums {
    schema: SchemaIdentity,
    algorithm: &'static str,
    files: BTreeMap<String, FileChecksum>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileChecksum {
    sha256: String,
    byte_length: usize,
}

/// Generate a byte-deterministic, provider-neutral, local-only publication
/// bundle. The function is pure: identical input produces identical files and
/// it cannot perform filesystem, Git, GitHub, process, or network operations.
pub(super) fn generate_publication(
    input: &PublicationInput,
) -> Result<PublicationBundle, PublicationError> {
    validate_input(input)?;
    let projection = build_projection(input);
    let projection_bytes = canonical_json(&projection)?;
    let story_bytes = render_story(&projection).into_bytes();
    let html_bytes = render_html(&projection).into_bytes();

    let manifest = BundleManifest {
        schema: SchemaIdentity {
            name: "org.synapsegit-lp-studio.publication-manifest",
            version: 1,
        },
        generator: SchemaIdentity {
            name: PUBLICATION_GENERATOR,
            version: PUBLICATION_GENERATOR_VERSION,
        },
        renderer: SchemaIdentity {
            name: PUBLICATION_RENDERER,
            version: PUBLICATION_RENDERER_VERSION,
        },
        target: "github_ready_local",
        projection_sha256: sha256(&projection_bytes),
        projection_byte_length: projection_bytes.len(),
        checksum_profile: "sha256-all-non-checksum-files-v1",
        files: manifest_inventory(),
    };
    let manifest_bytes = canonical_json(&manifest)?;

    let payload = BTreeMap::from([
        ("index.html".to_owned(), html_bytes),
        ("manifest.json".to_owned(), manifest_bytes),
        ("projection.json".to_owned(), projection_bytes),
        ("story.md".to_owned(), story_bytes),
    ]);
    let checksums = BundleChecksums {
        schema: SchemaIdentity {
            name: "org.synapsegit-lp-studio.publication-checksums",
            version: 1,
        },
        algorithm: "sha256",
        files: payload
            .iter()
            .map(|(path, bytes)| {
                (
                    path.clone(),
                    FileChecksum {
                        sha256: sha256(bytes),
                        byte_length: bytes.len(),
                    },
                )
            })
            .collect(),
    };

    let mut files = payload;
    files.insert("checksums.json".to_owned(), canonical_json(&checksums)?);
    Ok(PublicationBundle { files })
}

fn validate_input(input: &PublicationInput) -> Result<(), PublicationError> {
    validate_public_text(
        &input.project.label,
        MAX_PUBLIC_LABEL_BYTES,
        "project public label",
    )?;
    validate_public_text(&input.project.title, MAX_PUBLIC_TITLE_BYTES, "public title")?;
    validate_public_text(
        &input.project.summary,
        MAX_PUBLIC_SUMMARY_BYTES,
        "public summary",
    )?;
    validate_public_id(&input.accepted.revision_id)?;
    validate_sha256(&input.accepted.site_manifest_sha256)?;
    if let Some(digest) = &input.accepted.synapse_artifact_manifest_sha256 {
        validate_sha256(digest)?;
    }

    match &input.session {
        PublicSessionInput::Complete {
            proposal_manifest_sha256,
            decision_receipt_sha256,
            attribution,
            public_decision_note,
            ..
        } => {
            if input.accepted.synapse_artifact_manifest_sha256.is_none() {
                return Err(PublicationError::InvalidInput(
                    "complete history requires a Synapse artifact binding",
                ));
            }
            validate_sha256(proposal_manifest_sha256)?;
            validate_sha256(decision_receipt_sha256)?;
            validate_attribution(attribution)?;
            if let Some(note) = public_decision_note {
                validate_public_text(note, MAX_PUBLIC_NOTE_BYTES, "public Decision note")?;
            }
        }
        PublicSessionInput::Incomplete {
            proposal_manifest_sha256,
            attribution,
            ..
        } => {
            validate_sha256(proposal_manifest_sha256)?;
            validate_attribution(attribution)?;
        }
    }
    Ok(())
}

fn validate_attribution(
    attribution: &PublicProposalAttributionInput,
) -> Result<(), PublicationError> {
    validate_public_text(
        &attribution.provider_label,
        MAX_PROVIDER_LABEL_BYTES,
        "provider label",
    )?;
    validate_public_text(
        &attribution.model_label,
        MAX_MODEL_LABEL_BYTES,
        "model label",
    )
}

fn validate_public_text(
    value: &str,
    max_bytes: usize,
    label: &'static str,
) -> Result<(), PublicationError> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(PublicationError::InvalidInput(label));
    }
    if value.chars().any(|character| {
        character == '\0'
            || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            || matches!(
                character,
                '\u{200b}'
                    | '\u{200c}'
                    | '\u{200d}'
                    | '\u{2060}'
                    | '\u{202a}'
                    | '\u{202b}'
                    | '\u{202c}'
                    | '\u{202d}'
                    | '\u{202e}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
            )
    }) {
        return Err(PublicationError::InvalidInput(label));
    }
    Ok(())
}

fn validate_public_id(value: &str) -> Result<(), PublicationError> {
    if value.is_empty()
        || value.len() > MAX_PUBLIC_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(PublicationError::InvalidInput("Accepted revision ID"));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), PublicationError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PublicationError::InvalidInput("SHA-256 digest"));
    }
    Ok(())
}

fn build_projection(input: &PublicationInput) -> PublicProjection {
    let (completeness, proposal, human_decision, incomplete_limitation) = match &input.session {
        PublicSessionInput::Complete {
            proposal_manifest_sha256,
            decision_receipt_sha256,
            attribution,
            disposition,
            public_decision_note,
        } => (
            CompletenessProjection {
                state: "complete",
                proposal_present: true,
                decision_present: true,
                incomplete_reason: None,
            },
            proposal_projection(proposal_manifest_sha256, attribution),
            Some(HumanDecisionProjection {
                disposition: OriginValue::new(
                    disposition.as_str(),
                    ValueOrigin::ObservedFromSynapse,
                ),
                receipt_sha256: OriginValue::new(
                    decision_receipt_sha256.clone(),
                    ValueOrigin::ObservedFromSynapse,
                ),
                public_note: public_decision_note
                    .clone()
                    .map(|note| OriginValue::new(note, ValueOrigin::AuthorSupplied)),
            }),
            None,
        ),
        PublicSessionInput::Incomplete {
            proposal_manifest_sha256,
            attribution,
            reason,
        } => (
            CompletenessProjection {
                state: "incomplete",
                proposal_present: true,
                decision_present: false,
                incomplete_reason: Some(reason.as_str()),
            },
            proposal_projection(proposal_manifest_sha256, attribution),
            None,
            Some(reason.limitation()),
        ),
    };

    let mut limitations = LIMITATIONS
        .iter()
        .map(|(code, message)| Limitation {
            code: (*code).to_owned(),
            message: (*message).to_owned(),
        })
        .collect::<Vec<_>>();
    if let Some(limitation) = incomplete_limitation {
        limitations.push(limitation);
    }

    PublicProjection {
        schema: SchemaIdentity {
            name: PUBLICATION_SCHEMA,
            version: PUBLICATION_SCHEMA_VERSION,
        },
        generator: SchemaIdentity {
            name: PUBLICATION_GENERATOR,
            version: PUBLICATION_GENERATOR_VERSION,
        },
        application: ApplicationProjection {
            version: OriginValue::new(env!("CARGO_PKG_VERSION"), ValueOrigin::DerivedSummary),
        },
        synapse_git: SynapseGitProjection {
            contract: SchemaIdentity {
                name: SYNAPSEGIT_CONTRACT,
                version: SYNAPSEGIT_CONTRACT_VERSION,
            },
            assurance: "The pinned contract reports caller-supplied AI attribution and does not verify model execution.",
        },
        project: ProjectProjection {
            label: OriginValue::new(input.project.label.clone(), ValueOrigin::AuthorSupplied),
            title: OriginValue::new(input.project.title.clone(), ValueOrigin::AuthorSupplied),
            summary: OriginValue::new(input.project.summary.clone(), ValueOrigin::AuthorSupplied),
        },
        accepted: AcceptedProjection {
            revision_id: OriginValue::new(
                input.accepted.revision_id.clone(),
                ValueOrigin::DerivedSummary,
            ),
            site_manifest_sha256: OriginValue::new(
                input.accepted.site_manifest_sha256.clone(),
                ValueOrigin::DerivedSummary,
            ),
            synapse_artifact_manifest_sha256: input
                .accepted
                .synapse_artifact_manifest_sha256
                .clone()
                .map(|digest| OriginValue::new(digest, ValueOrigin::VerifiedFromSynapse)),
        },
        session: SessionProjection {
            completeness,
            proposal,
            human_decision,
        },
        publication: PublicationPolicy {
            visibility: "private_review",
            network_operations: "none",
            remote_publication: "not_performed",
            raw_lp_assets_included: false,
            thumbnails_included: false,
            source_private_rationale_included: false,
            omitted_by_structure: vec![
                "raw_lp_assets",
                "full_prompt",
                "raw_provider_response",
                "private_rationale",
                "credentials",
                "internal_actor_or_authority",
                "repository_path",
                "full_target_or_dom_quote",
            ],
        },
        limitations,
    }
}

fn proposal_projection(
    proposal_manifest_sha256: &str,
    attribution: &PublicProposalAttributionInput,
) -> ProposalProjection {
    ProposalProjection {
        manifest_sha256: OriginValue::new(
            proposal_manifest_sha256.to_owned(),
            ValueOrigin::ObservedFromSynapse,
        ),
        attribution: AttributionProjection {
            source: OriginValue::new(
                "caller_supplied_ai_attributed",
                ValueOrigin::ObservedFromSynapse,
            ),
            execution_verified: OriginValue::new(false, ValueOrigin::ObservedFromSynapse),
            provider_label: OriginValue::new(
                attribution.provider_label.clone(),
                ValueOrigin::DerivedSummary,
            ),
            model_label: OriginValue::new(
                attribution.model_label.clone(),
                ValueOrigin::DerivedSummary,
            ),
        },
    }
}

fn manifest_inventory() -> Vec<ManifestFile> {
    vec![
        ManifestFile {
            path: "checksums.json",
            role: "checksums",
            media_type: "application/json",
        },
        ManifestFile {
            path: "index.html",
            role: "human_view",
            media_type: "text/html",
        },
        ManifestFile {
            path: "manifest.json",
            role: "bundle_manifest",
            media_type: "application/json",
        },
        ManifestFile {
            path: "projection.json",
            role: "semantic_source",
            media_type: "application/json",
        },
        ManifestFile {
            path: "story.md",
            role: "human_story",
            media_type: "text/markdown",
        },
    ]
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, PublicationError> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| PublicationError::Serialization)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn render_story(projection: &PublicProjection) -> String {
    let mut output = String::new();
    output.push_str("# ");
    output.push_str(&markdown_inline(&projection.project.title.value));
    output.push_str("\n\n> Derived read-only LP publication preview. No Git, GitHub, upload, or network operation was performed.\n\n");
    output.push_str("## Public project\n\n");
    markdown_field(
        &mut output,
        "Label",
        &projection.project.label.value,
        projection.project.label.origin,
    );
    markdown_field(
        &mut output,
        "Summary",
        &projection.project.summary.value,
        projection.project.summary.origin,
    );
    output.push_str("\n## Accepted revision binding\n\n");
    markdown_field(
        &mut output,
        "Revision",
        &projection.accepted.revision_id.value,
        projection.accepted.revision_id.origin,
    );
    markdown_field(
        &mut output,
        "Site manifest SHA-256",
        &projection.accepted.site_manifest_sha256.value,
        projection.accepted.site_manifest_sha256.origin,
    );
    if let Some(binding) = &projection.accepted.synapse_artifact_manifest_sha256 {
        markdown_field(
            &mut output,
            "Synapse artifact manifest SHA-256",
            &binding.value,
            binding.origin,
        );
    } else {
        output.push_str("- **Synapse artifact manifest SHA-256:** unavailable\n");
    }
    output.push_str("\n## Proposal and Decision\n\n");
    output.push_str(&format!(
        "- **Completeness:** `{}`\n",
        projection.session.completeness.state
    ));
    if let Some(reason) = projection.session.completeness.incomplete_reason {
        output.push_str(&format!("- **Incomplete reason:** `{reason}`\n"));
    }
    markdown_field(
        &mut output,
        "Proposal manifest SHA-256",
        &projection.session.proposal.manifest_sha256.value,
        projection.session.proposal.manifest_sha256.origin,
    );
    markdown_field(
        &mut output,
        "Attribution",
        projection.session.proposal.attribution.source.value,
        projection.session.proposal.attribution.source.origin,
    );
    output.push_str("- **Execution verified:** `false` (origin: `observed_from_synapse`)\n");
    markdown_field(
        &mut output,
        "Provider label",
        &projection.session.proposal.attribution.provider_label.value,
        projection
            .session
            .proposal
            .attribution
            .provider_label
            .origin,
    );
    markdown_field(
        &mut output,
        "Model label",
        &projection.session.proposal.attribution.model_label.value,
        projection.session.proposal.attribution.model_label.origin,
    );
    if let Some(decision) = &projection.session.human_decision {
        markdown_field(
            &mut output,
            "Human disposition",
            decision.disposition.value,
            decision.disposition.origin,
        );
        markdown_field(
            &mut output,
            "Decision receipt SHA-256",
            &decision.receipt_sha256.value,
            decision.receipt_sha256.origin,
        );
        if let Some(note) = &decision.public_note {
            markdown_field(
                &mut output,
                "Public Decision note",
                &note.value,
                note.origin,
            );
        }
    } else {
        output.push_str("- **Human disposition:** unavailable; no disposition is claimed\n");
    }
    output.push_str("\n## Publication policy\n\n");
    output.push_str("- Network operations: `none`\n");
    output.push_str("- Remote publication: `not_performed`\n");
    output.push_str("- Raw LP assets: omitted by structural policy\n");
    output.push_str("- Source-private rationale: omitted by structural policy\n");
    output.push_str("\n## Limitations\n\n");
    for limitation in &projection.limitations {
        output.push_str("- **");
        output.push_str(&markdown_inline(&limitation.code));
        output.push_str(":** ");
        output.push_str(&markdown_inline(&limitation.message));
        output.push('\n');
    }
    output
}

fn markdown_field(output: &mut String, label: &str, value: &str, origin: ValueOrigin) {
    output.push_str("- **");
    output.push_str(label);
    output.push_str(":** ");
    output.push_str(&markdown_inline(value));
    output.push_str(" (origin: `");
    output.push_str(origin.as_str());
    output.push_str("`)\n");
}

fn markdown_inline(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\n' | '\r' | '\t' => escaped.push(' '),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

fn render_html(projection: &PublicProjection) -> String {
    let decision = if let Some(decision) = &projection.session.human_decision {
        let note = decision.public_note.as_ref().map_or_else(
            || "<p>Public Decision note: not supplied.</p>".to_owned(),
            |note| {
                format!(
                    "<p><strong>Public Decision note</strong> <span class=\"origin\">author_supplied</span><br>{}</p>",
                    html(&note.value)
                )
            },
        );
        format!(
            "<dl><dt>Human disposition</dt><dd><code>{}</code> <span class=\"origin\">observed_from_synapse</span></dd><dt>Decision receipt SHA-256</dt><dd><code>{}</code> <span class=\"origin\">observed_from_synapse</span></dd></dl>{note}",
            html(decision.disposition.value),
            html(&decision.receipt_sha256.value),
        )
    } else {
        "<p><strong>Human disposition:</strong> unavailable; no disposition is claimed.</p>"
            .to_owned()
    };
    let synapse_binding = projection
        .accepted
        .synapse_artifact_manifest_sha256
        .as_ref()
        .map_or_else(|| "unavailable".to_owned(), |binding| html(&binding.value));
    let incomplete = projection
        .session
        .completeness
        .incomplete_reason
        .map_or_else(String::new, |reason| {
            format!(
                "<p><strong>Incomplete reason:</strong> <code>{}</code></p>",
                html(reason)
            )
        });
    let limitations = projection
        .limitations
        .iter()
        .map(|limitation| {
            format!(
                "<li><strong>{}</strong> — {}</li>",
                html(&limitation.code),
                html(&limitation.message)
            )
        })
        .collect::<String>();

    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"robots\" content=\"noindex,nofollow\"><title>{title}</title><style>body{{margin:0;background:#f4f1e8;color:#172019;font:16px/1.6 system-ui,sans-serif}}main{{width:min(900px,calc(100% - 2rem));margin:auto;padding:3rem 0}}section,header{{background:#fffdf7;border:1px solid #d9d3c5;border-radius:16px;padding:1.5rem;margin:1rem 0}}h1{{line-height:1.1}}code{{overflow-wrap:anywhere}}.origin{{color:#526058;font-size:.8rem}}.notice{{border-left:5px solid #276749}}</style></head><body><main><header><p>LOCAL GITHUB-READY PREVIEW</p><h1>{title}</h1><p>{summary}</p><p class=\"notice\">Derived read-only snapshot. No Git, GitHub, upload, or network operation was performed.</p></header><section><h2>Public project</h2><dl><dt>Label</dt><dd>{label} <span class=\"origin\">author_supplied</span></dd><dt>Accepted revision</dt><dd><code>{revision}</code> <span class=\"origin\">derived_summary</span></dd><dt>Site manifest SHA-256</dt><dd><code>{site_digest}</code> <span class=\"origin\">derived_summary</span></dd><dt>Synapse artifact manifest SHA-256</dt><dd><code>{synapse_binding}</code></dd></dl></section><section><h2>Proposal and Decision</h2><p><strong>Completeness:</strong> <code>{completeness}</code></p>{incomplete}<dl><dt>Proposal manifest SHA-256</dt><dd><code>{proposal_digest}</code> <span class=\"origin\">observed_from_synapse</span></dd><dt>Attribution</dt><dd><code>caller_supplied_ai_attributed</code> <span class=\"origin\">observed_from_synapse</span></dd><dt>Execution verified</dt><dd><code>false</code> <span class=\"origin\">observed_from_synapse</span></dd><dt>Provider / model</dt><dd>{provider} / {model} <span class=\"origin\">derived_summary</span></dd></dl>{decision}</section><section><h2>Publication policy</h2><ul><li>Network operations: <code>none</code></li><li>Remote publication: <code>not_performed</code></li><li>Raw LP assets and source-private rationale: omitted by structural policy</li></ul></section><section><h2>Limitations</h2><ul>{limitations}</ul></section></main></body></html>",
        title = html(&projection.project.title.value),
        summary = html(&projection.project.summary.value),
        label = html(&projection.project.label.value),
        revision = html(&projection.accepted.revision_id.value),
        site_digest = html(&projection.accepted.site_manifest_sha256.value),
        proposal_digest = html(&projection.session.proposal.manifest_sha256.value),
        provider = html(&projection.session.proposal.attribution.provider_label.value),
        model = html(&projection.session.proposal.attribution.model_label.value),
        completeness = projection.session.completeness.state,
    )
}

fn html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn digest(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn attribution() -> PublicProposalAttributionInput {
        PublicProposalAttributionInput {
            provider_label: "Deterministic fake provider".to_owned(),
            model_label: "fake-lp-v1".to_owned(),
        }
    }

    fn complete_input() -> PublicationInput {
        PublicationInput {
            project: PublicProjectInput {
                label: "Public sample LP".to_owned(),
                title: "A reviewed landing page".to_owned(),
                summary: "A bounded, privacy-filtered adoption record.".to_owned(),
            },
            accepted: PublicAcceptedBindingInput {
                revision_id: "rev_0123456789abcdef".to_owned(),
                site_manifest_sha256: digest('a'),
                synapse_artifact_manifest_sha256: Some(digest('b')),
            },
            session: PublicSessionInput::Complete {
                proposal_manifest_sha256: digest('c'),
                decision_receipt_sha256: digest('d'),
                attribution: attribution(),
                disposition: PublicDisposition::AdoptedUnchanged,
                public_decision_note: Some("Approved for the public example.".to_owned()),
            },
        }
    }

    fn projection(bundle: &PublicationBundle) -> Value {
        serde_json::from_slice(bundle.files.get("projection.json").unwrap()).unwrap()
    }

    #[test]
    fn same_public_input_produces_exactly_the_same_bytes() {
        let input = complete_input();
        let first = generate_publication(&input).unwrap();
        let second = generate_publication(&input).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first.files.keys().map(String::as_str).collect::<Vec<_>>(),
            vec![
                "checksums.json",
                "index.html",
                "manifest.json",
                "projection.json",
                "story.md"
            ]
        );
    }

    #[test]
    fn projection_structurally_omits_sensitive_private_inputs_and_escapes_views() {
        let mut input = complete_input();
        input.project.title = "Review <script>alert(1)</script>".to_owned();
        input.project.summary = "Markdown [link](https://attacker.invalid)".to_owned();
        let bundle = generate_publication(&input).unwrap();
        let all_bytes = bundle
            .files
            .values()
            .flat_map(|bytes| bytes.iter().copied())
            .collect::<Vec<_>>();
        let all = String::from_utf8(all_bytes).unwrap();

        for private_canary in [
            "FULL_PROMPT_CANARY_DO_NOT_PUBLISH",
            "RAW_PROVIDER_RESPONSE_CANARY",
            "PRIVATE_RATIONALE_CANARY",
            "ghp_CREDENTIAL_CANARY",
            "INTERNAL_ACTOR_CANARY",
            "/private/repository/path/canary",
            "FULL_TARGET_DOM_QUOTE_CANARY",
            "RAW_LP_ASSET_CANARY",
        ] {
            assert!(!all.contains(private_canary), "leaked {private_canary}");
        }
        let html = String::from_utf8(bundle.files["index.html"].clone()).unwrap();
        let story = String::from_utf8(bundle.files["story.md"].clone()).unwrap();
        assert!(!html.to_ascii_lowercase().contains("<script"));
        assert!(!html.contains("href=\"https://attacker.invalid"));
        assert!(!html.contains("src=\"https://attacker.invalid"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!story.contains("[link](https://attacker.invalid)"));
        assert!(story.contains(r"\[link\]\(https://attacker\.invalid\)"));

        let projection = projection(&bundle);
        assert_eq!(projection["publication"]["networkOperations"], "none");
        assert_eq!(
            projection["publication"]["remotePublication"],
            "not_performed"
        );
        assert_eq!(projection["publication"]["rawLpAssetsIncluded"], false);
        assert_eq!(
            projection["publication"]["sourcePrivateRationaleIncluded"],
            false
        );
        assert_eq!(
            projection["session"]["proposal"]["attribution"]["executionVerified"]["value"],
            false
        );
    }

    #[test]
    fn manifest_and_checksums_are_self_consistent() {
        let bundle = generate_publication(&complete_input()).unwrap();
        let manifest: Value =
            serde_json::from_slice(bundle.files.get("manifest.json").unwrap()).unwrap();
        let checksums: Value =
            serde_json::from_slice(bundle.files.get("checksums.json").unwrap()).unwrap();

        let inventory = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            inventory,
            vec![
                "checksums.json",
                "index.html",
                "manifest.json",
                "projection.json",
                "story.md"
            ]
        );
        let covered = checksums["files"].as_object().unwrap();
        assert!(!covered.contains_key("checksums.json"));
        assert_eq!(covered.len(), bundle.files.len() - 1);
        for (path, bytes) in &bundle.files {
            if path == "checksums.json" {
                continue;
            }
            assert_eq!(covered[path]["sha256"], sha256(bytes));
            assert_eq!(covered[path]["byteLength"], bytes.len());
        }
        let projection_bytes = &bundle.files["projection.json"];
        assert_eq!(manifest["projectionSha256"], sha256(projection_bytes));
        assert_eq!(manifest["projectionByteLength"], projection_bytes.len());
    }

    #[test]
    fn complete_and_incomplete_histories_have_distinct_truthful_claims() {
        let complete = generate_publication(&complete_input()).unwrap();
        let complete_projection = projection(&complete);
        assert_eq!(
            complete_projection["session"]["completeness"]["state"],
            "complete"
        );
        assert_eq!(
            complete_projection["session"]["completeness"]["decisionPresent"],
            true
        );
        assert_eq!(
            complete_projection["session"]["humanDecision"]["disposition"]["value"],
            "adopted_unchanged"
        );

        let mut incomplete_input = complete_input();
        incomplete_input.accepted.synapse_artifact_manifest_sha256 = None;
        incomplete_input.session = PublicSessionInput::Incomplete {
            proposal_manifest_sha256: digest('c'),
            attribution: attribution(),
            reason: IncompleteReason::DecisionOutcomeUnknown,
        };
        let incomplete = generate_publication(&incomplete_input).unwrap();
        let incomplete_projection = projection(&incomplete);
        assert_eq!(
            incomplete_projection["session"]["completeness"]["state"],
            "incomplete"
        );
        assert_eq!(
            incomplete_projection["session"]["completeness"]["decisionPresent"],
            false
        );
        assert_eq!(
            incomplete_projection["session"]["completeness"]["incompleteReason"],
            "decision_outcome_unknown"
        );
        assert!(
            incomplete_projection["session"]["humanDecision"].is_null(),
            "an outcome-unknown session must not claim a disposition"
        );
        assert_ne!(
            complete.files["projection.json"],
            incomplete.files["projection.json"]
        );
    }

    #[test]
    fn invalid_or_overclaiming_inputs_fail_closed() {
        let mut no_synapse_binding = complete_input();
        no_synapse_binding.accepted.synapse_artifact_manifest_sha256 = None;
        assert_eq!(
            generate_publication(&no_synapse_binding),
            Err(PublicationError::InvalidInput(
                "complete history requires a Synapse artifact binding"
            ))
        );

        let mut uppercase_digest = complete_input();
        uppercase_digest.accepted.site_manifest_sha256 = "A".repeat(64);
        assert_eq!(
            generate_publication(&uppercase_digest),
            Err(PublicationError::InvalidInput("SHA-256 digest"))
        );

        let mut bidi = complete_input();
        bidi.project.title = "safe\u{202e}hidden".to_owned();
        assert_eq!(
            generate_publication(&bidi),
            Err(PublicationError::InvalidInput("public title"))
        );
    }

    #[test]
    fn all_supported_dispositions_and_incomplete_reasons_render() {
        for disposition in [
            PublicDisposition::AdoptedUnchanged,
            PublicDisposition::Rejected,
            PublicDisposition::Deferred,
        ] {
            let mut input = complete_input();
            let PublicSessionInput::Complete {
                disposition: current,
                ..
            } = &mut input.session
            else {
                unreachable!()
            };
            *current = disposition;
            let bundle = generate_publication(&input).unwrap();
            assert_eq!(
                projection(&bundle)["session"]["humanDecision"]["disposition"]["value"],
                disposition.as_str()
            );
        }

        for reason in [
            IncompleteReason::ProposalPending,
            IncompleteReason::DecisionOutcomeUnknown,
        ] {
            let mut input = complete_input();
            input.session = PublicSessionInput::Incomplete {
                proposal_manifest_sha256: digest('c'),
                attribution: attribution(),
                reason,
            };
            let bundle = generate_publication(&input).unwrap();
            assert_eq!(
                projection(&bundle)["session"]["completeness"]["incompleteReason"],
                reason.as_str()
            );
        }
    }
}
