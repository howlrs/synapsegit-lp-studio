//! Exact-pinned durable SynapseGit orchestration for C7.
//!
//! This module is a trusted Rust control-plane boundary. Browser/API values
//! must be validated and authorized before they reach it. In particular, the
//! SQLite journal is a locator and deduplication aid, never authority: every
//! recovered publication is rebuilt from server-owned configuration and sent
//! through `synapse-application` and the complete Core Human Decision runtime.

use serde_json::{Map as JsonMap, Value as JsonValue, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use synapse_application::{
    AiAuthorityProfileConfig, AiExecutionContext, AiExecutor, Application, ApplicationError,
    AuthenticatedSession, AuthenticationFailure, Authenticator, DurableProposalBinding,
    ExecutedAiProposal, ExecutionFailure, HumanAuthorityProfileConfig, HumanDecisionCandidate,
    ProjectSelector, RegisteredProject,
};
use synapse_artifact::{
    ArtifactDisposition, ArtifactLimits, ArtifactManifestEntry, ArtifactSourceAttribution,
    RegularFileManifest, artifact_manifest_sha256, map_regular_files,
};
use synapse_artifact_journal::{
    DecisionIntent, DecisionIntentRequest, JournalError, ReviewBinding, ReviewId, ReviewState,
    SqliteReviewJournal,
};
use synapse_canonical::{ObjectKind, canonical_bytes, parse_oid, parse_strict};
use synapse_core::{
    AiCapability, AiSideEffectClass, AuthorizationClock, DecisionDisposition,
    HumanDecisionAuthority, HumanDecisionRuntime, HumanDecisionUpdate, Repository,
    SystemAuthorizationClock,
};
use synapse_sqlite::{RefUpdate, ReflogMetadata};

use crate::fault_injection::{self, Failpoint};

const SCHEMA_VERSION: &str = "0.1.0";
const CONTRACT_NAME: &str = "synapsegit.generic-artifact";
const CONTRACT_VERSION: u32 = 1;
const AGENT_CREDENTIAL: &str = "lp-studio-sidecar-agent";
const HUMAN_CREDENTIAL: &str = "lp-studio-sidecar-human";
const PERMIT_TTL_NANOS: i128 = 60_000_000_000;
const MAX_OUTPUT_BYTES: i64 = 1_073_741_824;
const MAX_CONTROL_GRAPH_OBJECTS: usize = 20_000;
const MAX_DECISION_ANCESTRY: usize = 4_096;
const MAX_OPERATION_KEY_BYTES: usize = 128;
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 4_096;
const MAX_PRIVATE_RATIONALE_BYTES: usize = 2_000;
const PRIVATE_MEMO_CLASSIFICATION: &str = "private_memo_validated_and_omitted";

/// Server-owned configuration. None of these values may come from a browser
/// Decision request.
#[derive(Clone)]
pub(super) struct SidecarConfig {
    repository: PathBuf,
    journal: PathBuf,
    project_key: String,
    creator_display_name: String,
    agent_display_name: String,
    recorded_at: String,
    grant_expires_at: String,
    artifact_limits: ArtifactLimits,
}

impl SidecarConfig {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        repository: impl Into<PathBuf>,
        journal: impl Into<PathBuf>,
        project_key: impl Into<String>,
        creator_display_name: impl Into<String>,
        agent_display_name: impl Into<String>,
        recorded_at: impl Into<String>,
        grant_expires_at: impl Into<String>,
        artifact_limits: ArtifactLimits,
    ) -> Self {
        Self {
            repository: repository.into(),
            journal: journal.into(),
            project_key: project_key.into(),
            creator_display_name: creator_display_name.into(),
            agent_display_name: agent_display_name.into(),
            recorded_at: recorded_at.into(),
            grant_expires_at: grant_expires_at.into(),
            artifact_limits,
        }
    }
}

impl fmt::Debug for SidecarConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SidecarConfig")
            .field("project_key", &self.project_key)
            .field("repository", &"<redacted>")
            .field("journal", &"<redacted>")
            .finish_non_exhaustive()
    }
}

pub(super) struct ProposalInput<'a> {
    pub operation_key: &'a str,
    pub accepted: &'a RegularFileManifest,
    pub proposed: &'a RegularFileManifest,
    pub review_context_json: &'a [u8],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SidecarProposalReceipt {
    pub review_id: String,
    pub base_artifact_manifest_sha256: String,
    pub artifact_manifest_sha256: String,
    pub review_context_sha256: String,
    pub source_attribution: ArtifactSourceAttribution,
    pub execution_verified: bool,
}

impl SidecarProposalReceipt {
    pub(super) const fn contract(&self) -> &'static str {
        CONTRACT_NAME
    }

    pub(super) const fn contract_version(&self) -> u32 {
        CONTRACT_VERSION
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SidecarDecisionReceipt {
    pub disposition: ArtifactDisposition,
    pub reviewed_artifact_manifest_sha256: String,
    pub selected_snapshot: &'static str,
}

impl SidecarDecisionReceipt {
    pub(super) const fn contract(&self) -> &'static str {
        CONTRACT_NAME
    }

    pub(super) const fn contract_version(&self) -> u32 {
        CONTRACT_VERSION
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ReviewOutcome {
    PendingReview,
    RetryableFailure,
    OutcomeUnknown,
    TerminalDenial,
    DecisionCommitted(SidecarDecisionReceipt),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidecarError {
    InvalidArgument,
    StaleBase,
    ActiveReview,
    IdempotencyConflict,
    DecisionIntentExists,
    ReviewNotFound,
    TerminalState,
    Integrity,
    Storage,
}

impl SidecarError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::InvalidArgument => "invalid_argument",
            Self::StaleBase => "stale_base",
            Self::ActiveReview => "active_review",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::DecisionIntentExists => "decision_intent_exists",
            Self::ReviewNotFound => "review_not_found",
            Self::TerminalState => "review_terminal",
            Self::Integrity => "artifact_integrity_error",
            Self::Storage => "storage_error",
        }
    }
}

impl fmt::Display for SidecarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for SidecarError {}

type Result<T> = std::result::Result<T, SidecarError>;

fn injected_sidecar_fault(failpoint: Failpoint) -> Result<()> {
    fault_injection::check(failpoint).map_err(|_| SidecarError::Storage)
}

/// Stateless facade over one canonical repository and its separate review
/// journal. Reopening this value deliberately reconstructs all process-local
/// Application handles.
#[derive(Clone, Debug)]
pub(super) struct SynapseSidecar {
    config: SidecarConfig,
}

impl SynapseSidecar {
    pub(super) fn open(config: SidecarConfig) -> Result<Self> {
        validate_config(&config)?;
        if let Some(parent) = config.journal.parent() {
            std::fs::create_dir_all(parent).map_err(|_| SidecarError::Storage)?;
        }
        // Opening both stores here makes unsupported/corrupt schemas fail
        // before the facade is exposed. No handle is retained as authority.
        Repository::open(&config.repository).map_err(|_| SidecarError::Storage)?;
        SqliteReviewJournal::open(&config.journal).map_err(map_journal_error)?;
        Ok(Self { config })
    }

    pub(super) fn begin_proposal(
        &self,
        input: ProposalInput<'_>,
    ) -> Result<SidecarProposalReceipt> {
        validate_operation_key(input.operation_key)?;
        let canonical_context = canonical_review_context(input.review_context_json)?;
        let context_sha256 = raw_sha256(&canonical_context);
        let accepted_sha256 = artifact_manifest_sha256(input.accepted);
        let proposed_sha256 = artifact_manifest_sha256(input.proposed);
        let proposal_ref = self.proposal_ref(input.operation_key);
        let decision_ref = self.decision_ref();

        let mut repository =
            Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
        let (base_head, controls) = self.ensure_bootstrap(&mut repository, input.accepted)?;
        if manifest_digest_at_commit(&repository, &base_head, self.config.artifact_limits)?
            != accepted_sha256
        {
            return Err(SidecarError::StaleBase);
        }

        let refs = repository
            .refs()
            .list()
            .map_err(|_| SidecarError::Storage)?;
        if let Some(existing) = refs.iter().find(|record| record.name == proposal_ref) {
            return self.replay_existing_proposal(
                &repository,
                &proposal_ref,
                &existing.head,
                &accepted_sha256,
                &proposed_sha256,
                &context_sha256,
            );
        }
        for proposal in refs.iter().filter(|record| {
            record
                .name
                .starts_with(&format!("{}/", self.proposal_prefix()))
        }) {
            if !decision_ancestry_contains_proposal(&repository, &base_head, &proposal.head)?
                && !self.proposal_has_terminal_denial(
                    &repository,
                    &proposal.name,
                    &proposal.head,
                )?
            {
                return Err(SidecarError::ActiveReview);
            }
        }
        let mapped_proposal =
            map_regular_files(&repository, input.proposed).map_err(|_| SidecarError::Integrity)?;
        let output_blob_oids = mapped_proposal
            .files
            .iter()
            .map(|file| file.blob_oid.clone())
            .collect::<BTreeSet<_>>();
        let context_blob = repository
            .put_blob(canonical_context.as_slice())
            .map_err(|_| SidecarError::Storage)?
            .oid;
        let proposal_scope = format!("{}:{}", self.config.project_key, input.operation_key);
        let context_entity = entity_id(&proposal_scope, "context");
        let activity_entity = entity_id(&proposal_scope, "activity");
        let context_oid = put_json(
            &repository,
            context_record(
                &context_entity,
                &controls.ids.human,
                &controls.ids.subject,
                &base_head,
                &decision_ref,
                &controls.policy_oid,
                &controls.grant_oid,
                &context_blob,
                &self.config.recorded_at,
            ),
        )?;
        let activity_oid = put_json(
            &repository,
            activity_record(
                &activity_entity,
                &controls.ids.agent,
                &controls.ids.human,
                &controls.ids.subject,
                &context_oid,
                &controls.grant_oid,
                &context_blob,
                &mapped_proposal.site_tree_oid,
                &output_blob_oids,
                &self.config.recorded_at,
            ),
        )?;
        let base_commit = load_value(&repository, &base_head, ObjectKind::Commit)?;
        let base_snapshot = required_string(&base_commit, "snapshot")?;
        let proposal_snapshot = artifact_snapshot(
            &repository,
            &mapped_proposal.site_tree_oid,
            Some((base_snapshot, Some((&context_oid, &activity_oid)))),
        )?;
        let proposal_head = put_json(
            &repository,
            commit(
                "checkpoint",
                std::slice::from_ref(&base_head),
                &proposal_snapshot,
                std::slice::from_ref(&activity_oid),
                &controls.ids.agent,
                &self.config.recorded_at,
                "Generic artifact Proposal; canonical Decision unchanged",
            ),
        )?;

        let selector = ProjectSelector::new(controls.ids.project.clone());
        let application = Application::new(
            SidecarAuthenticator::new(&controls.ids),
            PreparedExecutor {
                proposal_head: proposal_head.clone(),
                activity_oid: activity_oid.clone(),
            },
            SystemAuthorizationClock,
            PERMIT_TTL_NANOS,
            [RegisteredProject::new(selector.clone(), repository)],
        )
        .map_err(map_application_error)?;
        application
            .grant_project_access(&selector, controls.ids.agent.clone())
            .map_err(map_application_error)?;
        application
            .grant_project_access(&selector, controls.ids.human.clone())
            .map_err(map_application_error)?;
        let profile = application
            .register_authority_profile(AiAuthorityProfileConfig::new(
                selector.clone(),
                controls.ids.agent.clone(),
                controls.ids.human.clone(),
                decision_ref.clone(),
                controls.ai_actor_oid.clone(),
                controls.human_actor_oid.clone(),
                context_oid,
                proposal_ref.clone(),
                vec![AiCapability::ProposeBranch, AiCapability::ReadContext],
                vec![AiCapability::ProposeBranch, AiCapability::ReadContext],
                AiSideEffectClass::None,
            ))
            .map_err(map_application_error)?;
        let execution = application
            .register_execution(&profile)
            .map_err(map_application_error)?;
        let permit = application
            .prepare_ai(AGENT_CREDENTIAL, &selector, &execution)
            .map_err(map_application_error)?;
        let publication = match application.execute_and_publish_ai(AGENT_CREDENTIAL, &permit) {
            Ok(publication) => publication,
            Err(error) if error.code() == "ref_conflict" => {
                drop(application);
                let repository =
                    Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
                let existing = repository
                    .refs()
                    .get(&proposal_ref)
                    .map_err(|_| SidecarError::Storage)?
                    .ok_or(SidecarError::ActiveReview)?;
                return self.replay_existing_proposal(
                    &repository,
                    &proposal_ref,
                    &existing.head,
                    &accepted_sha256,
                    &proposed_sha256,
                    &context_sha256,
                );
            }
            Err(error) => return Err(map_application_error(error)),
        };
        if publication.reflog.ref_name != proposal_ref
            || publication.reflog.old_head.is_some()
            || publication.reflog.new_head != proposal_head
            || publication.activity_oid != activity_oid
        {
            return Err(SidecarError::Integrity);
        }
        drop(application);

        let binding = ReviewBinding::new(
            controls.ids.project,
            proposal_ref,
            proposal_head,
            decision_ref,
            base_head,
        )
        .map_err(map_journal_error)?;
        let mut journal = self.open_journal()?;
        let registered = journal
            .create_or_get_review(binding)
            .map_err(map_journal_error)?;
        Ok(SidecarProposalReceipt {
            review_id: registered.review().review_id().as_str().to_owned(),
            base_artifact_manifest_sha256: accepted_sha256,
            artifact_manifest_sha256: proposed_sha256,
            review_context_sha256: context_sha256,
            source_attribution: ArtifactSourceAttribution::CallerSuppliedAiAttributed,
            execution_verified: false,
        })
    }

    pub(super) fn decide(
        &self,
        review_id: &str,
        disposition: ArtifactDisposition,
        private_rationale: Option<&str>,
        idempotency_key: &[u8],
    ) -> Result<ReviewOutcome> {
        validate_idempotency_key(idempotency_key)?;
        validate_private_rationale(private_rationale)?;
        let review_id = ReviewId::parse(review_id.to_owned()).map_err(map_journal_error)?;
        let mut journal = self.open_journal()?;
        let review = journal.get_review(&review_id).map_err(map_journal_error)?;
        self.validate_review_binding(review.binding())?;

        let intent = if let Some(existing) = journal
            .get_decision_intent(&review_id)
            .map_err(map_journal_error)?
        {
            let repository =
                Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
            let existing_disposition = disposition_from_feedback(
                &repository,
                existing.feedback_oid(),
                review.binding().proposal_head(),
            )?;
            if existing_disposition != disposition {
                return Err(SidecarError::IdempotencyConflict);
            }
            let request = canonical_decision_request(
                &review_id,
                disposition,
                existing.candidate_head(),
                existing.feedback_oid(),
                existing.expected_decision_head(),
            )?;
            injected_sidecar_fault(Failpoint::DecisionJournalIntentBefore)?;
            let registered = journal
                .register_decision_intent(
                    &review_id,
                    DecisionIntentRequest {
                        idempotency_key,
                        canonical_request: &request,
                        candidate_head: existing.candidate_head(),
                        feedback_oid: existing.feedback_oid(),
                        expected_decision_head: existing.expected_decision_head(),
                    },
                )
                .map_err(map_journal_error)?;
            injected_sidecar_fault(Failpoint::DecisionJournalIntentAfter)?;
            registered.into_intent()
        } else {
            self.create_decision_intent(
                &mut journal,
                &review_id,
                review.binding(),
                disposition,
                idempotency_key,
            )?
        };
        drop(journal);

        match self.reconcile_parsed(&review_id)? {
            committed @ ReviewOutcome::DecisionCommitted(_)
            | committed @ ReviewOutcome::TerminalDenial
            | committed @ ReviewOutcome::OutcomeUnknown => return Ok(committed),
            ReviewOutcome::PendingReview | ReviewOutcome::RetryableFailure => {}
        }
        match self.call_synapse(&review_id, review.binding(), &intent, disposition) {
            Ok(()) => self.reconcile_parsed(&review_id),
            Err(publication_error) => match self.reconcile_parsed(&review_id)? {
                ReviewOutcome::PendingReview
                    if matches!(publication_error, SidecarError::Storage) =>
                {
                    self.transition_retryable(&review_id)
                }
                ReviewOutcome::PendingReview => {
                    self.transition_terminal(&review_id, ReviewState::PendingReview)?;
                    Ok(ReviewOutcome::TerminalDenial)
                }
                outcome => Ok(outcome),
            },
        }
    }

    pub(super) fn reconcile(&self, review_id: &str) -> Result<ReviewOutcome> {
        let review_id = ReviewId::parse(review_id.to_owned()).map_err(map_journal_error)?;
        self.reconcile_parsed(&review_id)
    }

    /// Resume a server-owned Decision intent after a process interruption.
    ///
    /// Live Refs are always reconciled before the durable intent is considered.
    /// A review without an intent remains query-only, while an existing intent
    /// is revalidated and published through the normal trusted Core path without
    /// requiring the browser's idempotency key or private rationale again.
    pub(super) fn resume(&self, review_id: &str) -> Result<ReviewOutcome> {
        let observed = self.reconcile(review_id)?;
        let review_id = ReviewId::parse(review_id.to_owned()).map_err(map_journal_error)?;
        if matches!(
            observed,
            ReviewOutcome::DecisionCommitted(_) | ReviewOutcome::TerminalDenial
        ) {
            return Ok(observed);
        }

        let journal = self.open_journal()?;
        let review = journal.get_review(&review_id).map_err(map_journal_error)?;
        self.validate_review_binding(review.binding())?;
        if matches!(
            review.state(),
            ReviewState::DecisionCommitted | ReviewState::TerminalDenial
        ) {
            drop(journal);
            return self.reconcile_parsed(&review_id);
        }
        let binding = review.binding().clone();
        let intent = journal
            .get_decision_intent(&review_id)
            .map_err(map_journal_error)?;
        drop(journal);
        let Some(intent) = intent else {
            return Ok(observed);
        };
        if intent.expected_decision_head() != binding.expected_decision_head() {
            return Err(SidecarError::Integrity);
        }

        let publication = (|| {
            let repository =
                Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
            let disposition = disposition_from_feedback(
                &repository,
                intent.feedback_oid(),
                binding.proposal_head(),
            )?;
            drop(repository);
            self.call_synapse(&review_id, &binding, &intent, disposition)
        })();

        match publication {
            Ok(()) => self.reconcile_parsed(&review_id),
            Err(publication_error) => match self.reconcile_parsed(&review_id)? {
                committed @ ReviewOutcome::DecisionCommitted(_)
                | committed @ ReviewOutcome::TerminalDenial => Ok(committed),
                ReviewOutcome::PendingReview
                    if matches!(publication_error, SidecarError::Storage) =>
                {
                    self.transition_retryable(&review_id)
                }
                retryable @ ReviewOutcome::RetryableFailure
                | retryable @ ReviewOutcome::OutcomeUnknown
                    if matches!(publication_error, SidecarError::Storage) =>
                {
                    Ok(retryable)
                }
                ReviewOutcome::PendingReview => {
                    self.transition_terminal(&review_id, ReviewState::PendingReview)?;
                    Ok(ReviewOutcome::TerminalDenial)
                }
                ReviewOutcome::RetryableFailure => {
                    self.transition_terminal(&review_id, ReviewState::RetryableFailure)?;
                    Ok(ReviewOutcome::TerminalDenial)
                }
                ReviewOutcome::OutcomeUnknown => {
                    self.transition_terminal(&review_id, ReviewState::OutcomeUnknown)?;
                    Ok(ReviewOutcome::TerminalDenial)
                }
            },
        }
    }

    /// Inspect immutable Proposal facts after any review state, including a
    /// terminal Decision. The journal supplies only the locator/binding; live
    /// Refs and the complete CAS closure are re-read before a receipt is made.
    pub(super) fn inspect_proposal(&self, review_id: &str) -> Result<SidecarProposalReceipt> {
        let review_id = ReviewId::parse(review_id.to_owned()).map_err(map_journal_error)?;
        let journal = self.open_journal()?;
        let review = journal.get_review(&review_id).map_err(map_journal_error)?;
        self.validate_review_binding(review.binding())?;
        let binding = review.binding().clone();
        drop(journal);

        let repository =
            Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
        let live_proposal = repository
            .refs()
            .get(binding.proposal_ref_name())
            .map_err(|_| SidecarError::Storage)?
            .ok_or(SidecarError::Integrity)?;
        if live_proposal.head != binding.proposal_head() {
            return Err(SidecarError::Integrity);
        }
        repository
            .validate_head(binding.proposal_head())
            .map_err(|_| SidecarError::Integrity)?;
        repository
            .validate_head(binding.expected_decision_head())
            .map_err(|_| SidecarError::Integrity)?;
        let proposal = load_value(&repository, binding.proposal_head(), ObjectKind::Commit)?;
        require_single_parent_value(&proposal, binding.expected_decision_head())?;
        self.controls_at_commit(&repository, binding.expected_decision_head())?;

        let live_decision = repository
            .refs()
            .get(binding.decision_ref_name())
            .map_err(|_| SidecarError::Storage)?
            .ok_or(SidecarError::Integrity)?;
        repository
            .validate_head(&live_decision.head)
            .map_err(|_| SidecarError::Integrity)?;
        if !decision_chain_contains_commit(
            &repository,
            &live_decision.head,
            binding.expected_decision_head(),
        )? {
            return Err(SidecarError::Integrity);
        }

        Ok(SidecarProposalReceipt {
            review_id: review_id.as_str().to_owned(),
            base_artifact_manifest_sha256: manifest_digest_at_commit(
                &repository,
                binding.expected_decision_head(),
                self.config.artifact_limits,
            )?,
            artifact_manifest_sha256: manifest_digest_at_commit(
                &repository,
                binding.proposal_head(),
                self.config.artifact_limits,
            )?,
            review_context_sha256: review_context_digest_from_proposal(
                &repository,
                binding.proposal_head(),
            )?,
            source_attribution: ArtifactSourceAttribution::CallerSuppliedAiAttributed,
            execution_verified: false,
        })
    }

    fn decision_ref(&self) -> String {
        format!("decision/artifact/{}", self.config.project_key)
    }

    fn proposal_prefix(&self) -> String {
        format!("proposal/artifact/{}", self.config.project_key)
    }

    fn proposal_ref(&self, operation_key: &str) -> String {
        format!("{}/{}", self.proposal_prefix(), operation_key)
    }

    fn open_journal(&self) -> Result<SqliteReviewJournal> {
        SqliteReviewJournal::open(&self.config.journal).map_err(map_journal_error)
    }
}

impl SynapseSidecar {
    fn ensure_bootstrap(
        &self,
        repository: &mut Repository,
        accepted: &RegularFileManifest,
    ) -> Result<(String, Controls)> {
        if let Some(current) = repository
            .refs()
            .get(&self.decision_ref())
            .map_err(|_| SidecarError::Storage)?
        {
            let controls = self.controls_at_commit(repository, &current.head)?;
            return Ok((current.head, controls));
        }
        if !repository
            .refs()
            .list()
            .map_err(|_| SidecarError::Storage)?
            .is_empty()
        {
            return Err(SidecarError::Integrity);
        }

        let controls = self.expected_controls(repository)?;
        let mapped =
            map_regular_files(repository, accepted).map_err(|_| SidecarError::Integrity)?;
        let mut control_entries = JsonMap::new();
        insert_entry(
            &mut control_entries,
            "creator.actor.json",
            "record",
            &controls.human_actor_oid,
        );
        insert_entry(
            &mut control_entries,
            "agent.actor.json",
            "record",
            &controls.ai_actor_oid,
        );
        insert_entry(
            &mut control_entries,
            "policy.json",
            "record",
            &controls.policy_oid,
        );
        insert_entry(
            &mut control_entries,
            "grant.json",
            "record",
            &controls.grant_oid,
        );
        insert_entry(
            &mut control_entries,
            "subject.json",
            "record",
            &controls.subject_oid,
        );
        let control_tree = put_json(repository, manifest_tree(control_entries))?;
        let snapshot = artifact_snapshot(
            repository,
            &mapped.site_tree_oid,
            Some((&control_tree, None)),
        )?;
        let base_head = put_json(
            repository,
            commit(
                "checkpoint",
                &[],
                &snapshot,
                &[],
                &controls.ids.human,
                &self.config.recorded_at,
                "LP Studio generic artifact project initialized",
            ),
        )?;
        let observed = SystemAuthorizationClock
            .now_unix_nanos()
            .map_err(|_| SidecarError::Storage)?;
        let occurred_at = i64::try_from(observed).map_err(|_| SidecarError::Storage)?;
        match repository.update_ref(RefUpdate {
            ref_name: &self.decision_ref(),
            expected_head: None,
            new_head: &base_head,
            metadata: ReflogMetadata {
                occurred_at_unix_nanos: occurred_at,
                actor: Some(&controls.ids.human),
                message: Some("initialize LP Studio generic artifact project"),
            },
        }) {
            Ok(_) => Ok((base_head, controls)),
            Err(error) if error.code() == "ref_conflict" => {
                let current = repository
                    .refs()
                    .get(&self.decision_ref())
                    .map_err(|_| SidecarError::Storage)?
                    .ok_or(SidecarError::Integrity)?;
                let controls = self.controls_at_commit(repository, &current.head)?;
                Ok((current.head, controls))
            }
            Err(_) => Err(SidecarError::Storage),
        }
    }

    fn expected_controls(&self, repository: &Repository) -> Result<Controls> {
        let ids = WorkflowIds::from_key(&self.config.project_key);
        let human_actor_oid = put_json(
            repository,
            actor_record(
                &ids.human,
                &ids.human,
                &self.config.recorded_at,
                "human",
                &self.config.creator_display_name,
                None,
            ),
        )?;
        let ai_actor_oid = put_json(
            repository,
            actor_record(
                &ids.agent,
                &ids.human,
                &self.config.recorded_at,
                "ai_agent",
                &self.config.agent_display_name,
                Some(json!({
                    "provider": "application-owned",
                    "model_id": "caller-supplied-output",
                    "model_version": "lp-studio-generic-artifact-v1",
                    "capabilities": canonical_set(vec![json!("propose_branch"), json!("read_context")])
                })),
            ),
        )?;
        let policy_oid = put_json(
            repository,
            policy_record(
                &ids.policy,
                &ids.human,
                &ids.project,
                &self.decision_ref(),
                &self.proposal_prefix(),
                &self.config.recorded_at,
            ),
        )?;
        let grant_oid = put_json(
            repository,
            grant_record(
                &ids.grant,
                &ids.human,
                &ids.agent,
                &ids.project,
                &self.proposal_prefix(),
                &self.config.recorded_at,
                &self.config.grant_expires_at,
            ),
        )?;
        let subject_oid = put_json(
            repository,
            subject_record(
                &ids.subject,
                &ids.human,
                &self.config.recorded_at,
                &self.config.project_key,
            ),
        )?;
        Ok(Controls {
            ids,
            human_actor_oid,
            ai_actor_oid,
            policy_oid,
            grant_oid,
            subject_oid,
        })
    }

    fn controls_at_commit(&self, repository: &Repository, commit_oid: &str) -> Result<Controls> {
        repository
            .validate_head(commit_oid)
            .map_err(|_| SidecarError::Integrity)?;
        let ids = WorkflowIds::from_key(&self.config.project_key);
        let objects = snapshot_object_set(repository, commit_oid)?;
        let mut found = BTreeMap::new();
        for oid in objects {
            if parse_oid(&oid).ok() != Some(ObjectKind::Record) {
                continue;
            }
            let record = load_value(repository, &oid, ObjectKind::Record)?;
            let entity = required_string(&record, "entity_id")?;
            let expected_type = if entity == ids.human || entity == ids.agent {
                Some("actor")
            } else if entity == ids.policy {
                Some("policy")
            } else if entity == ids.grant {
                Some("delegation_grant")
            } else if entity == ids.subject {
                Some("subject")
            } else {
                None
            };
            let Some(expected_type) = expected_type else {
                continue;
            };
            if required_string(&record, "record_type")? != expected_type
                || found.insert(entity.to_owned(), oid).is_some()
            {
                return Err(SidecarError::Integrity);
            }
        }
        Ok(Controls {
            human_actor_oid: found.remove(&ids.human).ok_or(SidecarError::Integrity)?,
            ai_actor_oid: found.remove(&ids.agent).ok_or(SidecarError::Integrity)?,
            policy_oid: found.remove(&ids.policy).ok_or(SidecarError::Integrity)?,
            grant_oid: found.remove(&ids.grant).ok_or(SidecarError::Integrity)?,
            subject_oid: found.remove(&ids.subject).ok_or(SidecarError::Integrity)?,
            ids,
        })
    }

    fn replay_existing_proposal(
        &self,
        repository: &Repository,
        proposal_ref: &str,
        proposal_head: &str,
        accepted_sha256: &str,
        proposed_sha256: &str,
        context_sha256: &str,
    ) -> Result<SidecarProposalReceipt> {
        repository
            .validate_head(proposal_head)
            .map_err(|_| SidecarError::Integrity)?;
        let proposal = load_value(repository, proposal_head, ObjectKind::Commit)?;
        let parents = required_array(&proposal, "parents")?;
        if parents.len() != 1 {
            return Err(SidecarError::Integrity);
        }
        let base_head = parents[0].as_str().ok_or(SidecarError::Integrity)?;
        let current = repository
            .refs()
            .get(&self.decision_ref())
            .map_err(|_| SidecarError::Storage)?
            .ok_or(SidecarError::Integrity)?;
        if current.head != base_head {
            return Err(SidecarError::StaleBase);
        }
        if manifest_digest_at_commit(repository, base_head, self.config.artifact_limits)?
            != accepted_sha256
            || manifest_digest_at_commit(repository, proposal_head, self.config.artifact_limits)?
                != proposed_sha256
            || review_context_digest_from_proposal(repository, proposal_head)? != context_sha256
        {
            return Err(SidecarError::IdempotencyConflict);
        }
        self.controls_at_commit(repository, base_head)?;
        let binding = ReviewBinding::new(
            WorkflowIds::from_key(&self.config.project_key).project,
            proposal_ref,
            proposal_head,
            self.decision_ref(),
            base_head,
        )
        .map_err(map_journal_error)?;
        let mut journal = self.open_journal()?;
        let registered = journal
            .create_or_get_review(binding)
            .map_err(map_journal_error)?;
        Ok(SidecarProposalReceipt {
            review_id: registered.review().review_id().as_str().to_owned(),
            base_artifact_manifest_sha256: accepted_sha256.to_owned(),
            artifact_manifest_sha256: proposed_sha256.to_owned(),
            review_context_sha256: context_sha256.to_owned(),
            source_attribution: ArtifactSourceAttribution::CallerSuppliedAiAttributed,
            execution_verified: false,
        })
    }

    /// A Proposal Ref outside Decision ancestry is non-active only when the
    /// durable journal proves that the exact live Ref/head/base binding reached
    /// terminal denial. The immutable Ref and journal row are retained as
    /// evidence; missing, mismatched, or non-terminal rows remain fail-closed.
    fn proposal_has_terminal_denial(
        &self,
        repository: &Repository,
        proposal_ref: &str,
        proposal_head: &str,
    ) -> Result<bool> {
        repository
            .validate_head(proposal_head)
            .map_err(|_| SidecarError::Integrity)?;
        let proposal = load_value(repository, proposal_head, ObjectKind::Commit)?;
        let parents = required_array(&proposal, "parents")?;
        if parents.len() != 1 {
            return Err(SidecarError::Integrity);
        }
        let base_head = parents[0].as_str().ok_or(SidecarError::Integrity)?;
        repository
            .validate_head(base_head)
            .map_err(|_| SidecarError::Integrity)?;
        self.controls_at_commit(repository, base_head)?;
        manifest_digest_at_commit(repository, proposal_head, self.config.artifact_limits)?;
        review_context_digest_from_proposal(repository, proposal_head)?;

        let decision = repository
            .refs()
            .get(&self.decision_ref())
            .map_err(|_| SidecarError::Storage)?
            .ok_or(SidecarError::Integrity)?;
        repository
            .validate_head(&decision.head)
            .map_err(|_| SidecarError::Integrity)?;
        if !decision_chain_contains_commit(repository, &decision.head, base_head)? {
            return Err(SidecarError::Integrity);
        }

        let binding = ReviewBinding::new(
            WorkflowIds::from_key(&self.config.project_key).project,
            proposal_ref,
            proposal_head,
            self.decision_ref(),
            base_head,
        )
        .map_err(map_journal_error)?;
        self.validate_review_binding(&binding)?;
        let review = self
            .open_journal()?
            .get_review_by_binding(&binding)
            .map_err(map_journal_error)?;
        Ok(review.is_some_and(|review| review.state() == ReviewState::TerminalDenial))
    }

    fn validate_review_binding(&self, binding: &ReviewBinding) -> Result<()> {
        let ids = WorkflowIds::from_key(&self.config.project_key);
        if binding.project_scope() != ids.project
            || binding.decision_ref_name() != self.decision_ref()
            || !binding
                .proposal_ref_name()
                .starts_with(&format!("{}/", self.proposal_prefix()))
            || parse_oid(binding.proposal_head()).ok() != Some(ObjectKind::Commit)
            || parse_oid(binding.expected_decision_head()).ok() != Some(ObjectKind::Commit)
        {
            return Err(SidecarError::Integrity);
        }
        let operation = binding
            .proposal_ref_name()
            .strip_prefix(&format!("{}/", self.proposal_prefix()))
            .ok_or(SidecarError::Integrity)?;
        validate_operation_key(operation).map_err(|_| SidecarError::Integrity)
    }

    #[allow(clippy::too_many_arguments)]
    fn create_decision_intent(
        &self,
        journal: &mut SqliteReviewJournal,
        review_id: &ReviewId,
        binding: &ReviewBinding,
        disposition: ArtifactDisposition,
        idempotency_key: &[u8],
    ) -> Result<DecisionIntent> {
        let repository =
            Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
        self.validate_live_binding(&repository, binding)?;
        let controls = self.controls_at_commit(&repository, binding.expected_decision_head())?;
        let base = load_value(
            &repository,
            binding.expected_decision_head(),
            ObjectKind::Commit,
        )?;
        let proposal = load_value(&repository, binding.proposal_head(), ObjectKind::Commit)?;
        require_single_parent_value(&proposal, binding.expected_decision_head())?;
        let base_snapshot = required_string(&base, "snapshot")?;
        let proposal_snapshot = required_string(&proposal, "snapshot")?;
        let selected_snapshot = match disposition {
            ArtifactDisposition::AdoptedUnchanged => proposal_snapshot,
            ArtifactDisposition::Rejected | ArtifactDisposition::Deferred => base_snapshot,
        };
        injected_sidecar_fault(Failpoint::DecisionObjectWriteBefore)?;
        let feedback_oid = put_json(
            &repository,
            feedback_record(
                &entity_id(review_id.as_str(), "feedback"),
                &controls.ids.human,
                &controls.ids.subject,
                binding.proposal_head(),
                disposition,
                &self.config.recorded_at,
            ),
        )?;
        injected_sidecar_fault(Failpoint::DecisionObjectWriteAfter)?;
        injected_sidecar_fault(Failpoint::DecisionObjectWriteBefore)?;
        let candidate_head = put_json(
            &repository,
            commit(
                "decision",
                &[binding.expected_decision_head().to_owned()],
                selected_snapshot,
                std::slice::from_ref(&feedback_oid),
                &controls.ids.human,
                &self.config.recorded_at,
                "Human reviewed LP Studio generic artifact Proposal",
            ),
        )?;
        injected_sidecar_fault(Failpoint::DecisionObjectWriteAfter)?;
        let request = canonical_decision_request(
            review_id,
            disposition,
            &candidate_head,
            &feedback_oid,
            binding.expected_decision_head(),
        )?;
        injected_sidecar_fault(Failpoint::DecisionJournalIntentBefore)?;
        let registered = journal
            .register_decision_intent(
                review_id,
                DecisionIntentRequest {
                    idempotency_key,
                    canonical_request: &request,
                    candidate_head: &candidate_head,
                    feedback_oid: &feedback_oid,
                    expected_decision_head: binding.expected_decision_head(),
                },
            )
            .map_err(map_journal_error)?;
        injected_sidecar_fault(Failpoint::DecisionJournalIntentAfter)?;
        Ok(registered.into_intent())
    }

    fn validate_live_binding(
        &self,
        repository: &Repository,
        binding: &ReviewBinding,
    ) -> Result<()> {
        self.validate_review_binding(binding)?;
        let proposal = repository
            .refs()
            .get(binding.proposal_ref_name())
            .map_err(|_| SidecarError::Storage)?;
        let decision = repository
            .refs()
            .get(binding.decision_ref_name())
            .map_err(|_| SidecarError::Storage)?;
        if proposal.as_ref().map(|record| record.head.as_str()) != Some(binding.proposal_head()) {
            return Err(SidecarError::Integrity);
        }
        if decision.as_ref().map(|record| record.head.as_str())
            != Some(binding.expected_decision_head())
        {
            return Err(SidecarError::StaleBase);
        }
        repository
            .validate_head(binding.proposal_head())
            .map_err(|_| SidecarError::Integrity)?;
        require_single_parent_value(
            &load_value(repository, binding.proposal_head(), ObjectKind::Commit)?,
            binding.expected_decision_head(),
        )
    }

    fn publish_recovered_decision(
        &self,
        _review_id: &ReviewId,
        binding: &ReviewBinding,
        intent: &DecisionIntent,
        disposition: ArtifactDisposition,
    ) -> Result<()> {
        if intent.expected_decision_head() != binding.expected_decision_head() {
            return Err(SidecarError::Integrity);
        }
        let repository =
            Repository::open(&self.config.repository).map_err(|_| SidecarError::Storage)?;
        self.validate_live_binding(&repository, binding)?;
        let controls = self.controls_at_commit(&repository, binding.expected_decision_head())?;
        if disposition_from_feedback(&repository, intent.feedback_oid(), binding.proposal_head())?
            != disposition
        {
            return Err(SidecarError::Integrity);
        }
        let proposal = load_value(&repository, binding.proposal_head(), ObjectKind::Commit)?;
        let activity_oid = required_array(&proposal, "transition_refs")?
            .first()
            .and_then(JsonValue::as_str)
            .ok_or(SidecarError::Integrity)?
            .to_owned();
        let selector = ProjectSelector::new(controls.ids.project.clone());
        let application = Application::new(
            SidecarAuthenticator::new(&controls.ids),
            PreparedExecutor {
                proposal_head: binding.proposal_head().to_owned(),
                activity_oid,
            },
            SystemAuthorizationClock,
            PERMIT_TTL_NANOS,
            [RegisteredProject::new(selector.clone(), repository)],
        )
        .map_err(map_application_error)?;
        application
            .grant_project_access(&selector, controls.ids.human.clone())
            .map_err(map_application_error)?;
        let profile = application
            .register_human_profile(HumanAuthorityProfileConfig::new(
                selector.clone(),
                controls.ids.human.clone(),
                binding.decision_ref_name(),
                controls.human_actor_oid.clone(),
                controls.policy_oid.clone(),
            ))
            .map_err(map_application_error)?;
        let durable = DurableProposalBinding::new(
            selector.clone(),
            binding.proposal_ref_name(),
            binding.proposal_head(),
            binding.decision_ref_name(),
            binding.expected_decision_head(),
        );
        let registration = application
            .register_recovered_human_decision(
                &profile,
                &durable,
                HumanDecisionCandidate::new(
                    intent.candidate_head(),
                    intent.feedback_oid(),
                    Some("LP Studio generic artifact Human Decision"),
                ),
            )
            .map_err(map_application_error)?;
        let permit = application
            .prepare_human_decision(HUMAN_CREDENTIAL, &selector, &registration)
            .map_err(map_application_error)?;
        injected_sidecar_fault(Failpoint::DecisionCasPublishBefore)?;
        let receipt = application
            .publish_human_decision(HUMAN_CREDENTIAL, &permit)
            .map_err(map_application_error)?;
        injected_sidecar_fault(Failpoint::DecisionCasPublishAfter)?;
        if receipt.reflog.ref_name != binding.decision_ref_name()
            || receipt.reflog.old_head.as_deref() != Some(binding.expected_decision_head())
            || receipt.reflog.new_head != intent.candidate_head()
            || receipt.proposal_commit_oid != binding.proposal_head()
            || receipt.decision_feedback_oid != intent.feedback_oid()
            || receipt.disposition != core_disposition(disposition)
        {
            return Err(SidecarError::Integrity);
        }
        Ok(())
    }

    fn call_synapse(
        &self,
        review_id: &ReviewId,
        binding: &ReviewBinding,
        intent: &DecisionIntent,
        disposition: ArtifactDisposition,
    ) -> Result<()> {
        injected_sidecar_fault(Failpoint::DecisionSynapseCallBefore)?;
        self.publish_recovered_decision(review_id, binding, intent, disposition)?;
        injected_sidecar_fault(Failpoint::DecisionSynapseCallAfter)
    }

    fn reconcile_parsed(&self, review_id: &ReviewId) -> Result<ReviewOutcome> {
        injected_sidecar_fault(Failpoint::DecisionReceiptQueryBefore)?;
        let outcome = self.reconcile_parsed_inner(review_id)?;
        injected_sidecar_fault(Failpoint::DecisionReceiptQueryAfter)?;
        Ok(outcome)
    }

    fn reconcile_parsed_inner(&self, review_id: &ReviewId) -> Result<ReviewOutcome> {
        let journal = self.open_journal()?;
        let review = journal.get_review(review_id).map_err(map_journal_error)?;
        self.validate_review_binding(review.binding())?;
        let binding = review.binding().clone();
        let state = review.state();
        let intent = journal
            .get_decision_intent(review_id)
            .map_err(map_journal_error)?;
        drop(journal);

        let mut repository = match Repository::open(&self.config.repository) {
            Ok(repository) => repository,
            Err(_) => return self.mark_outcome_unknown(review_id, state, intent.is_some()),
        };
        let proposal = match repository.refs().get(binding.proposal_ref_name()) {
            Ok(proposal) => proposal,
            Err(_) => return self.mark_outcome_unknown(review_id, state, intent.is_some()),
        };
        let decision = match repository.refs().get(binding.decision_ref_name()) {
            Ok(decision) => decision,
            Err(_) => return self.mark_outcome_unknown(review_id, state, intent.is_some()),
        };
        if proposal.as_ref().map(|record| record.head.as_str()) != Some(binding.proposal_head()) {
            self.transition_terminal(review_id, state)?;
            return Ok(ReviewOutcome::TerminalDenial);
        }
        let current = decision.ok_or(SidecarError::Integrity)?.head;
        if current == binding.expected_decision_head() {
            return match state {
                ReviewState::PendingReview => Ok(ReviewOutcome::PendingReview),
                ReviewState::RetryableFailure => Ok(ReviewOutcome::RetryableFailure),
                ReviewState::OutcomeUnknown => Ok(ReviewOutcome::OutcomeUnknown),
                ReviewState::TerminalDenial => Ok(ReviewOutcome::TerminalDenial),
                ReviewState::DecisionCommitted => Err(SidecarError::Integrity),
            };
        }

        let Some(intent) = intent else {
            self.transition_terminal(review_id, state)?;
            return Ok(ReviewOutcome::TerminalDenial);
        };
        if intent.expected_decision_head() != binding.expected_decision_head() {
            return Err(SidecarError::Integrity);
        }
        if !decision_chain_contains_commit(&repository, &current, intent.candidate_head())? {
            self.transition_terminal(review_id, state)?;
            return Ok(ReviewOutcome::TerminalDenial);
        }
        self.validate_committed_candidate(&mut repository, &binding, &intent)?;
        self.transition_committed(review_id, state)?;
        Ok(ReviewOutcome::DecisionCommitted(decision_receipt(
            &repository,
            &binding,
            &intent,
            self.config.artifact_limits,
        )?))
    }

    fn validate_committed_candidate(
        &self,
        repository: &mut Repository,
        binding: &ReviewBinding,
        intent: &DecisionIntent,
    ) -> Result<()> {
        let controls = self.controls_at_commit(repository, binding.expected_decision_head())?;
        let authority = HumanDecisionAuthority::new(
            &controls.ids.human,
            &controls.ids.project,
            binding.decision_ref_name(),
            binding.expected_decision_head(),
            binding.proposal_ref_name(),
            binding.proposal_head(),
            &controls.human_actor_oid,
            &controls.policy_oid,
        );
        match HumanDecisionRuntime::new(repository, authority).publish_decision(
            HumanDecisionUpdate {
                new_head: intent.candidate_head(),
                decision_feedback_oid: intent.feedback_oid(),
                message: Some("LP Studio reconciliation validation"),
            },
        ) {
            Ok(receipt)
                if receipt.reflog.new_head == intent.candidate_head()
                    && receipt.proposal_commit_oid == binding.proposal_head()
                    && receipt.decision_feedback_oid == intent.feedback_oid() => {}
            Ok(_) => return Err(SidecarError::Integrity),
            Err(error) if error.code() == "ref_conflict" => {}
            Err(_) => return Err(SidecarError::Integrity),
        }
        let current = repository
            .refs()
            .get(binding.decision_ref_name())
            .map_err(|_| SidecarError::Storage)?
            .ok_or(SidecarError::Integrity)?;
        if !decision_chain_contains_commit(repository, &current.head, intent.candidate_head())? {
            return Err(SidecarError::Integrity);
        }
        Ok(())
    }

    fn mark_outcome_unknown(
        &self,
        review_id: &ReviewId,
        state: ReviewState,
        has_intent: bool,
    ) -> Result<ReviewOutcome> {
        if !has_intent {
            return Err(SidecarError::Storage);
        }
        match self.transition_state(review_id, state, ReviewState::OutcomeUnknown)? {
            ReviewState::OutcomeUnknown => Ok(ReviewOutcome::OutcomeUnknown),
            ReviewState::DecisionCommitted => Err(SidecarError::Storage),
            ReviewState::TerminalDenial => Ok(ReviewOutcome::TerminalDenial),
            ReviewState::PendingReview | ReviewState::RetryableFailure => {
                Err(SidecarError::Integrity)
            }
        }
    }

    fn transition_committed(&self, review_id: &ReviewId, state: ReviewState) -> Result<()> {
        match self.transition_state(review_id, state, ReviewState::DecisionCommitted)? {
            ReviewState::DecisionCommitted => Ok(()),
            _ => Err(SidecarError::Integrity),
        }
    }

    fn transition_terminal(&self, review_id: &ReviewId, state: ReviewState) -> Result<()> {
        match self.transition_state(review_id, state, ReviewState::TerminalDenial)? {
            ReviewState::TerminalDenial => Ok(()),
            _ => Err(SidecarError::Integrity),
        }
    }

    fn transition_retryable(&self, review_id: &ReviewId) -> Result<ReviewOutcome> {
        let state = self
            .open_journal()?
            .get_review(review_id)
            .map_err(map_journal_error)?
            .state();
        match self.transition_state(review_id, state, ReviewState::RetryableFailure)? {
            ReviewState::RetryableFailure => Ok(ReviewOutcome::RetryableFailure),
            ReviewState::OutcomeUnknown => Ok(ReviewOutcome::OutcomeUnknown),
            ReviewState::TerminalDenial => Ok(ReviewOutcome::TerminalDenial),
            ReviewState::DecisionCommitted => self.reconcile_parsed(review_id),
            ReviewState::PendingReview => Err(SidecarError::Integrity),
        }
    }

    fn transition_state(
        &self,
        review_id: &ReviewId,
        initial: ReviewState,
        target: ReviewState,
    ) -> Result<ReviewState> {
        let mut observed = initial;
        for _ in 0..3 {
            if observed == target
                || matches!(
                    observed,
                    ReviewState::DecisionCommitted | ReviewState::TerminalDenial
                )
                || (observed == ReviewState::OutcomeUnknown
                    && !matches!(
                        target,
                        ReviewState::DecisionCommitted | ReviewState::TerminalDenial
                    ))
                || (observed == ReviewState::RetryableFailure
                    && target == ReviewState::RetryableFailure)
            {
                return Ok(observed);
            }
            let mut journal = self.open_journal()?;
            match journal.transition_review_state(review_id, observed, target) {
                Ok(record) => return Ok(record.state()),
                Err(JournalError::StateConflict { actual, .. }) => observed = actual,
                Err(error) => return Err(map_journal_error(error)),
            }
        }
        Err(SidecarError::Storage)
    }
}

#[derive(Clone)]
struct SidecarAuthenticator {
    agent_id: String,
    human_id: String,
}

impl SidecarAuthenticator {
    fn new(ids: &WorkflowIds) -> Self {
        Self {
            agent_id: ids.agent.clone(),
            human_id: ids.human.clone(),
        }
    }
}

impl Authenticator for SidecarAuthenticator {
    type Credential = str;

    fn authenticate(
        &self,
        credential: &Self::Credential,
    ) -> std::result::Result<AuthenticatedSession, AuthenticationFailure> {
        match credential {
            AGENT_CREDENTIAL => {
                AuthenticatedSession::new(&self.agent_id, "lp-studio-agent-session")
            }
            HUMAN_CREDENTIAL => {
                AuthenticatedSession::new(&self.human_id, "lp-studio-human-session")
            }
            _ => Err(AuthenticationFailure),
        }
    }
}

#[derive(Clone)]
struct PreparedExecutor {
    proposal_head: String,
    activity_oid: String,
}

impl AiExecutor for PreparedExecutor {
    fn execute(
        &self,
        _context: &AiExecutionContext,
    ) -> std::result::Result<ExecutedAiProposal, ExecutionFailure> {
        Ok(ExecutedAiProposal::new(
            self.proposal_head.clone(),
            self.activity_oid.clone(),
            Some("LP Studio generic artifact Proposal"),
        ))
    }
}

#[derive(Clone, Debug)]
struct WorkflowIds {
    human: String,
    agent: String,
    project: String,
    subject: String,
    policy: String,
    grant: String,
}

impl WorkflowIds {
    fn from_key(project_key: &str) -> Self {
        Self {
            human: entity_id(project_key, "human"),
            agent: entity_id(project_key, "agent"),
            project: entity_id(project_key, "project"),
            subject: entity_id(project_key, "subject"),
            policy: entity_id(project_key, "policy"),
            grant: entity_id(project_key, "grant"),
        }
    }
}

#[derive(Clone, Debug)]
struct Controls {
    ids: WorkflowIds,
    human_actor_oid: String,
    ai_actor_oid: String,
    policy_oid: String,
    grant_oid: String,
    subject_oid: String,
}

fn validate_config(config: &SidecarConfig) -> Result<()> {
    validate_key(&config.project_key, 128)?;
    if config.repository.as_os_str().is_empty()
        || config.journal.as_os_str().is_empty()
        || config.repository == config.journal
    {
        return Err(SidecarError::InvalidArgument);
    }
    for (value, max) in [
        (config.creator_display_name.as_str(), 200_usize),
        (config.agent_display_name.as_str(), 200),
        (config.recorded_at.as_str(), 64),
        (config.grant_expires_at.as_str(), 64),
    ] {
        if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
            return Err(SidecarError::InvalidArgument);
        }
    }
    Ok(())
}

fn validate_key(value: &str, max: usize) -> Result<()> {
    let mut bytes = value.bytes();
    if value.is_empty()
        || value.len() > max
        || !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return Err(SidecarError::InvalidArgument);
    }
    Ok(())
}

fn validate_operation_key(value: &str) -> Result<()> {
    validate_key(value, MAX_OPERATION_KEY_BYTES)
}

fn validate_idempotency_key(value: &[u8]) -> Result<()> {
    if value.is_empty() || value.len() > MAX_IDEMPOTENCY_KEY_BYTES {
        return Err(SidecarError::InvalidArgument);
    }
    Ok(())
}

fn validate_private_rationale(rationale: Option<&str>) -> Result<()> {
    if rationale.is_some_and(|rationale| {
        rationale.is_empty()
            || rationale.len() > MAX_PRIVATE_RATIONALE_BYTES
            || rationale.chars().any(char::is_control)
    }) {
        return Err(SidecarError::InvalidArgument);
    }
    Ok(())
}

fn canonical_review_context(bytes: &[u8]) -> Result<Vec<u8>> {
    let parsed = parse_strict(bytes).map_err(|_| SidecarError::InvalidArgument)?;
    canonical_bytes(&parsed).map_err(|_| SidecarError::InvalidArgument)
}

fn raw_sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn map_journal_error(error: JournalError) -> SidecarError {
    match error {
        JournalError::InvalidArgument(_) => SidecarError::InvalidArgument,
        JournalError::ReviewNotFound => SidecarError::ReviewNotFound,
        JournalError::DecisionIntentExists => SidecarError::DecisionIntentExists,
        JournalError::IdempotencyConflict => SidecarError::IdempotencyConflict,
        JournalError::StateConflict { .. } | JournalError::InvalidStateTransition { .. } => {
            SidecarError::TerminalState
        }
        JournalError::ReviewBindingExists
        | JournalError::ReviewBindingConflict
        | JournalError::ProposalIntentNotFound
        | JournalError::ProposalIntentExists
        | JournalError::ProposalIntentConflict
        | JournalError::LegacyDecisionIntent
        | JournalError::DecisionIntentMismatch
        | JournalError::DecisionOutcomeConflict
        | JournalError::CorruptData(_) => SidecarError::Integrity,
        JournalError::Random(_) | JournalError::Storage(_) => SidecarError::Storage,
    }
}

fn map_application_error(error: ApplicationError) -> SidecarError {
    match error {
        ApplicationError::StaleBase => SidecarError::StaleBase,
        ApplicationError::RefConflict => SidecarError::ActiveReview,
        ApplicationError::AuthenticationRequired
        | ApplicationError::ProjectAccessDenied
        | ApplicationError::ExecutionPermitInvalid
        | ApplicationError::ExecutionFailed
        | ApplicationError::ConfigInvalid => SidecarError::Integrity,
        ApplicationError::ServiceUnavailable => SidecarError::Storage,
        ApplicationError::Core(ref core) if core.code() == "stale_base" => SidecarError::StaleBase,
        ApplicationError::Core(ref core) if core.code() == "ref_conflict" => {
            SidecarError::ActiveReview
        }
        ApplicationError::Core(_) => SidecarError::Integrity,
    }
}

fn entity_id(project_key: &str, role: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"synapsegit-generic-artifact-entity-v1\0");
    hash.update(project_key.as_bytes());
    hash.update(b"\0");
    hash.update(role.as_bytes());
    let mut bytes: [u8; 32] = hash.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "urn:uuid:{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn put_json(repository: &Repository, value: JsonValue) -> Result<String> {
    let encoded = serde_json::to_vec(&value).map_err(|_| SidecarError::Integrity)?;
    repository
        .put_object(&encoded)
        .map(|stored| stored.oid)
        .map_err(|_| SidecarError::Integrity)
}

fn artifact_snapshot(
    repository: &Repository,
    site_tree: &str,
    control: Option<(&str, Option<(&str, &str)>)>,
) -> Result<String> {
    let mut entries = JsonMap::new();
    insert_entry(&mut entries, "site", "tree", site_tree);
    if let Some((base_or_control, proposal_records)) = control {
        insert_entry(
            &mut entries,
            if proposal_records.is_some() {
                "base"
            } else {
                "control"
            },
            "tree",
            base_or_control,
        );
        if let Some((context, activity)) = proposal_records {
            insert_entry(&mut entries, "context.json", "record", context);
            insert_entry(&mut entries, "activity.json", "record", activity);
        }
    }
    put_json(repository, manifest_tree(entries))
}

fn envelope(
    record_type: &str,
    entity_id: &str,
    recorded_at: &str,
    asserted_by: &str,
    origin: &str,
    payload: JsonValue,
) -> JsonValue {
    json!({
        "object_type": "record",
        "schema_version": SCHEMA_VERSION,
        "record_type": record_type,
        "entity_id": entity_id,
        "recorded_at": recorded_at,
        "asserted_by": asserted_by,
        "origin": origin,
        "source_refs": [],
        "payload": payload,
        "extensions": {}
    })
}

fn actor_record(
    entity_id: &str,
    asserted_by: &str,
    recorded_at: &str,
    actor_kind: &str,
    display_name: &str,
    ai_profile: Option<JsonValue>,
) -> JsonValue {
    let mut payload = json!({"actor_kind": actor_kind, "display_name": display_name});
    if let Some(profile) = ai_profile {
        payload
            .as_object_mut()
            .expect("internal actor payload is an object")
            .insert("ai_profile".into(), profile);
    }
    envelope(
        "actor",
        entity_id,
        recorded_at,
        asserted_by,
        if actor_kind == "human" {
            "self_declared"
        } else {
            "tool_recorded"
        },
        payload,
    )
}

fn policy_record(
    entity_id: &str,
    human: &str,
    project: &str,
    decision_ref: &str,
    proposal_prefix: &str,
    recorded_at: &str,
) -> JsonValue {
    let proposal_selector = format!("{proposal_prefix}/**");
    envelope(
        "policy",
        entity_id,
        recorded_at,
        human,
        "self_declared",
        json!({
            "scope_refs": canonical_set(vec![json!(project)]),
            "rules": [
                {"rule_id":"allow-context-read","effect":"allow","action":"read","resource_selector":"project/**"},
                {"rule_id":"allow-artifact-proposal","effect":"allow","action":"propose","resource_selector":proposal_selector},
                {"rule_id":"gate-artifact-decision","effect":"require_human_gate","action":"publish","resource_selector":decision_ref,"human_gate":"before_decision_ref"}
            ],
            "default_effect": "deny"
        }),
    )
}

fn grant_record(
    entity_id: &str,
    human: &str,
    agent: &str,
    project: &str,
    proposal_prefix: &str,
    recorded_at: &str,
    expires_at: &str,
) -> JsonValue {
    envelope(
        "delegation_grant",
        entity_id,
        recorded_at,
        human,
        "self_declared",
        json!({
            "principal_ref": human,
            "delegate_ref": agent,
            "project_ref": project,
            "purpose": "Record bounded sequential generic artifact Proposals.",
            "capabilities": canonical_set(vec![json!("propose_branch"), json!("read_context")]),
            "resource_selectors": canonical_set(vec![json!("project/**")]),
            "writable_ref_prefixes": canonical_set(vec![json!(proposal_prefix)]),
            "data_classes": canonical_set(vec![json!("internal")]),
            "allowed_egress": [],
            "may_delegate": false,
            "max_child_depth": 0,
            "max_output_bytes": MAX_OUTPUT_BYTES,
            "required_human_gates": canonical_set(vec![json!("before_decision_ref"), json!("before_release_ref")]),
            "expires_at": expires_at
        }),
    )
}

fn subject_record(entity_id: &str, human: &str, recorded_at: &str, label: &str) -> JsonValue {
    envelope(
        "subject",
        entity_id,
        recorded_at,
        human,
        "self_declared",
        json!({
            "subject_kind": "digital",
            "label": label,
            "relation_refs": [],
            "spatial_frame_refs": []
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn context_record(
    entity_id: &str,
    human: &str,
    subject: &str,
    base_head: &str,
    decision_ref: &str,
    policy_oid: &str,
    grant_oid: &str,
    application_context_blob: &str,
    recorded_at: &str,
) -> JsonValue {
    envelope(
        "context_pack",
        entity_id,
        recorded_at,
        human,
        "tool_recorded",
        json!({
            "base_commit": base_head,
            "base_ref_name": decision_ref,
            "expected_ref_head": base_head,
            "subject_refs": canonical_set(vec![json!(subject)]),
            "selected_context_refs": canonical_set(vec![json!(base_head), json!(application_context_blob)]),
            "must_preserve_constraints": ["Preserve the accepted artifact base and protected controls."],
            "allowed_transformations": canonical_set(vec![json!("file_tree_proposal")]),
            "unresolved_questions": [],
            "policy_snapshot_ref": policy_oid,
            "delegation_grant_ref": grant_oid,
            "data_classification": "internal",
            "retrieval_method": "application-owned redacted context"
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn activity_record(
    entity_id: &str,
    agent: &str,
    human: &str,
    subject: &str,
    context_oid: &str,
    grant_oid: &str,
    context_blob: &str,
    output_tree: &str,
    output_blob_oids: &BTreeSet<String>,
    recorded_at: &str,
) -> JsonValue {
    let mut output_refs = vec![json!({"role":"proposal","oid":output_tree})];
    output_refs.extend(
        output_blob_oids
            .iter()
            .map(|oid| json!({"role":"proposal_file","oid":oid})),
    );
    let mut value = envelope(
        "activity",
        entity_id,
        recorded_at,
        agent,
        "tool_recorded",
        json!({
            "activity_kind": "ai_run",
            "actor_refs": canonical_set(vec![
                json!({"role":"agent","actor_ref":agent}),
                json!({"role":"responsible_principal","actor_ref":human})
            ]),
            "subject_refs": canonical_set(vec![json!(subject)]),
            "input_refs": canonical_set(vec![
                json!({"role":"context","oid":context_oid}),
                json!({"role":"application_context","oid":context_blob})
            ]),
            "output_refs": canonical_set(output_refs),
            "before_observation_refs": [],
            "after_observation_refs": [],
            "reversibility": "reversible",
            "summary": "Recorded caller-supplied bytes as an AI-attributed generic artifact Proposal.",
            "side_effect_class": "none",
            "ai_run": {
                "agent_ref": agent,
                "responsible_principal_ref": human,
                "context_pack_ref": context_oid,
                "delegation_grant_ref": grant_oid,
                "requested_capabilities": canonical_set(vec![json!("propose_branch"), json!("read_context")]),
                "required_human_gates": canonical_set(vec![json!("before_decision_ref"), json!("before_release_ref")]),
                "status": "proposal_ready",
                "reproducibility_class": "not_reproducible"
            }
        }),
    );
    value
        .as_object_mut()
        .expect("internal activity envelope is an object")
        .insert(
            "valid_time".into(),
            json!({"kind":"instant","at":recorded_at}),
        );
    value
}

fn feedback_record(
    entity_id: &str,
    human: &str,
    subject: &str,
    proposal_head: &str,
    disposition: ArtifactDisposition,
    recorded_at: &str,
) -> JsonValue {
    envelope(
        "decision_feedback",
        entity_id,
        recorded_at,
        human,
        "self_declared",
        json!({
            "proposal_ref": proposal_head,
            "disposition": disposition_protocol(disposition),
            "reason_codes": ["unspecified"],
            // The UI memo is intentionally ephemeral. Core receives only a
            // disposition-derived, privacy-safe summary required for durable
            // Human Decision semantics.
            "human_rationale": privacy_safe_rationale(disposition),
            "applies_to_subjects": canonical_set(vec![json!(subject)]),
            "visibility": "private",
            "training_use_policy": "prohibited"
        }),
    )
}

fn manifest_tree(entries: JsonMap<String, JsonValue>) -> JsonValue {
    json!({
        "object_type": "tree",
        "schema_version": SCHEMA_VERSION,
        "entries": entries,
        "extensions": {}
    })
}

fn commit(
    kind: &str,
    parents: &[String],
    snapshot: &str,
    transitions: &[String],
    author: &str,
    authored_at: &str,
    message: &str,
) -> JsonValue {
    json!({
        "object_type": "commit",
        "schema_version": SCHEMA_VERSION,
        "commit_kind": kind,
        "parents": parents,
        "snapshot": snapshot,
        "transition_refs": canonical_set(transitions.iter().map(|value| json!(value)).collect()),
        "bound_declaration_refs": [],
        "author_ref": author,
        "authored_at": authored_at,
        "message": message,
        "extensions": {}
    })
}

fn insert_entry(entries: &mut JsonMap<String, JsonValue>, name: &str, kind: &str, oid: &str) {
    entries.insert(name.into(), json!({"entry_kind":kind,"oid":oid}));
}

fn canonical_set(mut values: Vec<JsonValue>) -> Vec<JsonValue> {
    values.sort_by_cached_key(|value| {
        let bytes = serde_json::to_vec(value).expect("internal JSON serialization succeeds");
        let parsed = parse_strict(&bytes).expect("internal set member is strict JSON");
        canonical_bytes(&parsed).expect("internal set member fits canonical limits")
    });
    values
}

fn disposition_protocol(disposition: ArtifactDisposition) -> &'static str {
    match disposition {
        ArtifactDisposition::AdoptedUnchanged => "adopted_unchanged",
        ArtifactDisposition::Rejected => "rejected",
        ArtifactDisposition::Deferred => "deferred",
    }
}

fn privacy_safe_rationale(disposition: ArtifactDisposition) -> &'static str {
    match disposition {
        ArtifactDisposition::AdoptedUnchanged => "The creator adopted the artifact unchanged.",
        ArtifactDisposition::Rejected => "The creator rejected the artifact.",
        ArtifactDisposition::Deferred => "The creator deferred the artifact.",
    }
}

fn load_value(repository: &Repository, oid: &str, expected: ObjectKind) -> Result<JsonValue> {
    if parse_oid(oid).ok() != Some(expected) || !expected.is_structured() {
        return Err(SidecarError::Integrity);
    }
    let bytes = repository
        .objects()
        .read_raw(oid)
        .map_err(|_| SidecarError::Storage)?
        .ok_or(SidecarError::Integrity)?;
    let parsed = parse_strict(&bytes).map_err(|_| SidecarError::Integrity)?;
    if canonical_bytes(&parsed).map_err(|_| SidecarError::Integrity)? != bytes {
        return Err(SidecarError::Integrity);
    }
    serde_json::from_slice(&bytes).map_err(|_| SidecarError::Integrity)
}

fn required_string<'value>(value: &'value JsonValue, key: &str) -> Result<&'value str> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .ok_or(SidecarError::Integrity)
}

fn required_array<'value>(value: &'value JsonValue, key: &str) -> Result<&'value [JsonValue]> {
    value
        .get(key)
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .ok_or(SidecarError::Integrity)
}

fn required_object<'value>(
    value: &'value JsonValue,
    key: &str,
) -> Result<&'value JsonMap<String, JsonValue>> {
    value
        .get(key)
        .and_then(JsonValue::as_object)
        .ok_or(SidecarError::Integrity)
}

fn require_single_parent_value(commit: &JsonValue, expected: &str) -> Result<()> {
    let parents = required_array(commit, "parents")?;
    if parents.len() == 1 && parents[0].as_str() == Some(expected) {
        Ok(())
    } else {
        Err(SidecarError::Integrity)
    }
}

fn snapshot_object_set(repository: &Repository, commit_oid: &str) -> Result<BTreeSet<String>> {
    let commit = load_value(repository, commit_oid, ObjectKind::Commit)?;
    let snapshot = required_string(&commit, "snapshot")?.to_owned();
    let mut objects = BTreeSet::new();
    let mut pending = vec![snapshot];
    while let Some(tree_oid) = pending.pop() {
        if !objects.insert(tree_oid.clone()) {
            continue;
        }
        if objects.len() > MAX_CONTROL_GRAPH_OBJECTS {
            return Err(SidecarError::Integrity);
        }
        let tree = load_value(repository, &tree_oid, ObjectKind::Tree)?;
        for entry in required_object(&tree, "entries")?.values() {
            let entry_kind = required_string(entry, "entry_kind")?;
            let oid = required_string(entry, "oid")?;
            let expected = match entry_kind {
                "blob" => ObjectKind::Blob,
                "record" => ObjectKind::Record,
                "tree" => ObjectKind::Tree,
                _ => return Err(SidecarError::Integrity),
            };
            if parse_oid(oid).ok() != Some(expected) {
                return Err(SidecarError::Integrity);
            }
            if expected == ObjectKind::Tree {
                pending.push(oid.to_owned());
            } else {
                objects.insert(oid.to_owned());
            }
            if objects.len() + pending.len() > MAX_CONTROL_GRAPH_OBJECTS {
                return Err(SidecarError::Integrity);
            }
        }
    }
    Ok(objects)
}

fn site_tree_at_commit(repository: &Repository, commit_oid: &str) -> Result<String> {
    let commit = load_value(repository, commit_oid, ObjectKind::Commit)?;
    let snapshot = load_value(
        repository,
        required_string(&commit, "snapshot")?,
        ObjectKind::Tree,
    )?;
    let site = required_object(&snapshot, "entries")?
        .get("site")
        .ok_or(SidecarError::Integrity)?;
    if required_string(site, "entry_kind")? != "tree" {
        return Err(SidecarError::Integrity);
    }
    let oid = required_string(site, "oid")?;
    if parse_oid(oid).ok() != Some(ObjectKind::Tree) {
        return Err(SidecarError::Integrity);
    }
    Ok(oid.to_owned())
}

fn manifest_digest_at_commit(
    repository: &Repository,
    commit_oid: &str,
    limits: ArtifactLimits,
) -> Result<String> {
    let root = site_tree_at_commit(repository, commit_oid)?;
    let mut pending = vec![(root, String::new(), 0_usize)];
    let mut entries = Vec::new();
    let mut expanded_trees = 0_usize;
    let mut total_bytes = 0_u64;
    while let Some((tree_oid, prefix, depth)) = pending.pop() {
        expanded_trees = expanded_trees
            .checked_add(1)
            .ok_or(SidecarError::Integrity)?;
        if depth > limits.max_depth || expanded_trees > MAX_CONTROL_GRAPH_OBJECTS {
            return Err(SidecarError::Integrity);
        }
        let tree = load_value(repository, &tree_oid, ObjectKind::Tree)?;
        for (name, entry) in required_object(&tree, "entries")? {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let oid = required_string(entry, "oid")?;
            match required_string(entry, "entry_kind")? {
                "tree" if parse_oid(oid).ok() == Some(ObjectKind::Tree) => {
                    pending.push((oid.to_owned(), path, depth + 1));
                }
                "blob" if parse_oid(oid).ok() == Some(ObjectKind::Blob) => {
                    if entries.len() >= limits.max_files {
                        return Err(SidecarError::Integrity);
                    }
                    let bytes = repository
                        .objects()
                        .read_verified_blob_limited(oid, limits.max_file_bytes)
                        .map_err(|_| SidecarError::Integrity)?
                        .ok_or(SidecarError::Integrity)?;
                    let byte_len =
                        u64::try_from(bytes.len()).map_err(|_| SidecarError::Integrity)?;
                    total_bytes = total_bytes
                        .checked_add(byte_len)
                        .ok_or(SidecarError::Integrity)?;
                    if total_bytes > limits.max_total_bytes {
                        return Err(SidecarError::Integrity);
                    }
                    entries.push(ArtifactManifestEntry::regular_file(path, bytes));
                }
                _ => return Err(SidecarError::Integrity),
            }
        }
        if expanded_trees + pending.len() > MAX_CONTROL_GRAPH_OBJECTS {
            return Err(SidecarError::Integrity);
        }
    }
    let manifest =
        RegularFileManifest::from_entries(entries, limits).map_err(|_| SidecarError::Integrity)?;
    Ok(artifact_manifest_sha256(&manifest))
}

fn review_context_digest_from_proposal(
    repository: &Repository,
    proposal_head: &str,
) -> Result<String> {
    let proposal = load_value(repository, proposal_head, ObjectKind::Commit)?;
    let transitions = required_array(&proposal, "transition_refs")?;
    if transitions.len() != 1 {
        return Err(SidecarError::Integrity);
    }
    let activity = load_value(
        repository,
        transitions[0].as_str().ok_or(SidecarError::Integrity)?,
        ObjectKind::Record,
    )?;
    if required_string(&activity, "record_type")? != "activity" {
        return Err(SidecarError::Integrity);
    }
    let payload = required_object(&activity, "payload")?;
    let refs = payload
        .get("input_refs")
        .and_then(JsonValue::as_array)
        .ok_or(SidecarError::Integrity)?;
    let contexts = refs
        .iter()
        .filter(|entry| {
            entry.get("role").and_then(JsonValue::as_str) == Some("application_context")
        })
        .collect::<Vec<_>>();
    if contexts.len() != 1 {
        return Err(SidecarError::Integrity);
    }
    let blob_oid = contexts[0]
        .get("oid")
        .and_then(JsonValue::as_str)
        .ok_or(SidecarError::Integrity)?;
    if parse_oid(blob_oid).ok() != Some(ObjectKind::Blob) {
        return Err(SidecarError::Integrity);
    }
    let bytes = repository
        .objects()
        .read_raw(blob_oid)
        .map_err(|_| SidecarError::Storage)?
        .ok_or(SidecarError::Integrity)?;
    let canonical = canonical_review_context(&bytes).map_err(|_| SidecarError::Integrity)?;
    if canonical != bytes {
        return Err(SidecarError::Integrity);
    }
    Ok(raw_sha256(&bytes))
}

fn decision_ancestry_contains_proposal(
    repository: &Repository,
    decision_head: &str,
    proposal_head: &str,
) -> Result<bool> {
    let mut pending = vec![decision_head.to_owned()];
    let mut visited = BTreeSet::new();
    while let Some(commit_oid) = pending.pop() {
        if !visited.insert(commit_oid.clone()) {
            continue;
        }
        if visited.len() > MAX_DECISION_ANCESTRY {
            return Err(SidecarError::Integrity);
        }
        let commit = load_value(repository, &commit_oid, ObjectKind::Commit)?;
        for transition in required_array(&commit, "transition_refs")? {
            let oid = transition.as_str().ok_or(SidecarError::Integrity)?;
            let record = load_value(repository, oid, ObjectKind::Record)?;
            if record.get("record_type").and_then(JsonValue::as_str) == Some("decision_feedback")
                && record
                    .get("payload")
                    .and_then(|payload| payload.get("proposal_ref"))
                    .and_then(JsonValue::as_str)
                    == Some(proposal_head)
            {
                return Ok(true);
            }
        }
        for parent in required_array(&commit, "parents")? {
            pending.push(parent.as_str().ok_or(SidecarError::Integrity)?.to_owned());
        }
    }
    Ok(false)
}

fn decision_chain_contains_commit(
    repository: &Repository,
    head: &str,
    candidate: &str,
) -> Result<bool> {
    let mut pending = vec![head.to_owned()];
    let mut visited = BTreeSet::new();
    while let Some(commit_oid) = pending.pop() {
        if commit_oid == candidate {
            return Ok(true);
        }
        if !visited.insert(commit_oid.clone()) {
            continue;
        }
        if visited.len() > MAX_DECISION_ANCESTRY {
            return Err(SidecarError::Integrity);
        }
        let commit = load_value(repository, &commit_oid, ObjectKind::Commit)?;
        for parent in required_array(&commit, "parents")? {
            pending.push(parent.as_str().ok_or(SidecarError::Integrity)?.to_owned());
        }
    }
    Ok(false)
}

fn canonical_decision_request(
    review_id: &ReviewId,
    disposition: ArtifactDisposition,
    candidate_head: &str,
    feedback_oid: &str,
    expected_decision_head: &str,
) -> Result<Vec<u8>> {
    let encoded = serde_json::to_vec(&json!({
        "contract": CONTRACT_NAME,
        "contract_version": CONTRACT_VERSION,
        "review_id": review_id.as_str(),
        "disposition": disposition_protocol(disposition),
        "private_memo_policy": PRIVATE_MEMO_CLASSIFICATION,
        "candidate_head": candidate_head,
        "feedback_oid": feedback_oid,
        "expected_decision_head": expected_decision_head
    }))
    .map_err(|_| SidecarError::Integrity)?;
    let parsed = parse_strict(&encoded).map_err(|_| SidecarError::Integrity)?;
    canonical_bytes(&parsed).map_err(|_| SidecarError::Integrity)
}

fn disposition_from_feedback(
    repository: &Repository,
    feedback_oid: &str,
    proposal_head: &str,
) -> Result<ArtifactDisposition> {
    let feedback = load_value(repository, feedback_oid, ObjectKind::Record)?;
    if required_string(&feedback, "record_type")? != "decision_feedback" {
        return Err(SidecarError::Integrity);
    }
    let payload = required_object(&feedback, "payload")?;
    if payload.get("proposal_ref").and_then(JsonValue::as_str) != Some(proposal_head)
        || payload.get("visibility").and_then(JsonValue::as_str) != Some("private")
        || payload
            .get("training_use_policy")
            .and_then(JsonValue::as_str)
            != Some("prohibited")
    {
        return Err(SidecarError::Integrity);
    }
    let disposition = match payload.get("disposition").and_then(JsonValue::as_str) {
        Some("adopted_unchanged") => ArtifactDisposition::AdoptedUnchanged,
        Some("rejected") => ArtifactDisposition::Rejected,
        Some("deferred") => ArtifactDisposition::Deferred,
        _ => return Err(SidecarError::Integrity),
    };
    let rationale = payload
        .get("human_rationale")
        .and_then(JsonValue::as_str)
        .ok_or(SidecarError::Integrity)?;
    if rationale != privacy_safe_rationale(disposition)
        || payload.get("reason_codes") != Some(&json!(["unspecified"]))
    {
        return Err(SidecarError::Integrity);
    }
    Ok(disposition)
}

fn core_disposition(disposition: ArtifactDisposition) -> DecisionDisposition {
    match disposition {
        ArtifactDisposition::AdoptedUnchanged => DecisionDisposition::AdoptedUnchanged,
        ArtifactDisposition::Rejected => DecisionDisposition::Rejected,
        ArtifactDisposition::Deferred => DecisionDisposition::Deferred,
    }
}

fn decision_receipt(
    repository: &Repository,
    binding: &ReviewBinding,
    intent: &DecisionIntent,
    limits: ArtifactLimits,
) -> Result<SidecarDecisionReceipt> {
    let disposition =
        disposition_from_feedback(repository, intent.feedback_oid(), binding.proposal_head())?;
    let (selected_commit, selected_snapshot) = match disposition {
        ArtifactDisposition::AdoptedUnchanged => (binding.proposal_head(), "proposal"),
        ArtifactDisposition::Rejected | ArtifactDisposition::Deferred => {
            (binding.expected_decision_head(), "base")
        }
    };
    Ok(SidecarDecisionReceipt {
        disposition,
        reviewed_artifact_manifest_sha256: manifest_digest_at_commit(
            repository,
            selected_commit,
            limits,
        )?,
        selected_snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        config: SidecarConfig,
        sidecar: SynapseSidecar,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("temporary project directory");
            let config = SidecarConfig::new(
                temp.path().join("canonical-repository"),
                temp.path().join("private-review-journal.sqlite3"),
                "landing-page",
                "Creator",
                "LP Studio Agent",
                "2026-07-19T00:00:00.000000000Z",
                "2100-01-01T00:00:00.000000000Z",
                ArtifactLimits::default(),
            );
            let sidecar = SynapseSidecar::open(config.clone()).expect("sidecar opens");
            Self {
                _temp: temp,
                config,
                sidecar,
            }
        }
    }

    fn manifest(contents: &str) -> RegularFileManifest {
        RegularFileManifest::from_entries(
            [ArtifactManifestEntry::regular_file(
                "index.html",
                contents.as_bytes().to_vec(),
            )],
            ArtifactLimits::default(),
        )
        .expect("test manifest is valid")
    }

    fn begin(
        sidecar: &SynapseSidecar,
        operation_key: &str,
        accepted: &RegularFileManifest,
        proposed: &RegularFileManifest,
    ) -> SidecarProposalReceipt {
        let context = format!(r#"{{"operation":"{operation_key}"}}"#);
        sidecar
            .begin_proposal(ProposalInput {
                operation_key,
                accepted,
                proposed,
                review_context_json: context.as_bytes(),
            })
            .expect("proposal is admitted")
    }

    fn committed(outcome: ReviewOutcome) -> SidecarDecisionReceipt {
        match outcome {
            ReviewOutcome::DecisionCommitted(receipt) => receipt,
            other => panic!("expected committed decision, got {other:?}"),
        }
    }

    fn transition_to_terminal_denial(sidecar: &SynapseSidecar, review_id: &str) {
        let review_id = ReviewId::parse(review_id.to_owned()).unwrap();
        let mut journal = sidecar.open_journal().unwrap();
        let review = journal.get_review(&review_id).unwrap();
        assert_eq!(review.state(), ReviewState::PendingReview);
        assert_eq!(
            journal
                .transition_review_state(
                    &review_id,
                    ReviewState::PendingReview,
                    ReviewState::TerminalDenial,
                )
                .unwrap()
                .state(),
            ReviewState::TerminalDenial
        );
    }

    fn assert_file_tree_omits(root: &std::path::Path, needles: &[&[u8]]) {
        fn visit(directory: &std::path::Path, needles: &[&[u8]]) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let file_type = entry.file_type().unwrap();
                if file_type.is_dir() {
                    visit(&entry.path(), needles);
                } else if file_type.is_file() {
                    let bytes = std::fs::read(entry.path()).unwrap();
                    for needle in needles {
                        assert!(
                            !bytes.windows(needle.len()).any(|window| window == *needle),
                            "persistent private canary found in {}",
                            entry.path().display()
                        );
                    }
                }
            }
        }
        visit(root, needles);
    }

    #[test]
    fn adopt_reject_and_defer_select_the_contract_snapshot() {
        for (disposition, expected_snapshot, expected_contents) in [
            (
                ArtifactDisposition::AdoptedUnchanged,
                "proposal",
                "proposed",
            ),
            (ArtifactDisposition::Rejected, "base", "accepted"),
            (ArtifactDisposition::Deferred, "base", "accepted"),
        ] {
            let fixture = Fixture::new();
            let accepted = manifest("accepted");
            let proposed = manifest("proposed");
            let proposal = begin(&fixture.sidecar, "first", &accepted, &proposed);
            assert_eq!(proposal.contract(), CONTRACT_NAME);
            assert_eq!(proposal.contract_version(), CONTRACT_VERSION);
            let receipt = committed(
                fixture
                    .sidecar
                    .decide(
                        &proposal.review_id,
                        disposition,
                        Some("bounded private review note"),
                        b"decision-key",
                    )
                    .expect("decision succeeds"),
            );
            assert_eq!(receipt.disposition, disposition);
            assert_eq!(receipt.contract(), CONTRACT_NAME);
            assert_eq!(receipt.contract_version(), CONTRACT_VERSION);
            assert_eq!(receipt.selected_snapshot, expected_snapshot);
            assert_eq!(
                receipt.reviewed_artifact_manifest_sha256,
                artifact_manifest_sha256(&manifest(expected_contents))
            );
            assert_eq!(
                fixture.sidecar.reconcile(&proposal.review_id).unwrap(),
                ReviewOutcome::DecisionCommitted(receipt)
            );
            assert_eq!(
                fixture
                    .sidecar
                    .inspect_proposal(&proposal.review_id)
                    .unwrap(),
                proposal
            );
        }
    }

    #[test]
    fn exact_replay_is_stable_and_private_memo_never_reaches_durable_state() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let first = begin(&fixture.sidecar, "first", &accepted, &proposed);
        let replay = begin(&fixture.sidecar, "first", &accepted, &proposed);
        assert_eq!(replay, first);
        assert_eq!(
            fixture
                .sidecar
                .begin_proposal(ProposalInput {
                    operation_key: "first",
                    accepted: &accepted,
                    proposed: &manifest("changed proposal"),
                    review_context_json: br#"{"operation":"first"}"#,
                })
                .unwrap_err(),
            SidecarError::IdempotencyConflict
        );

        let raw_key = b"raw-idempotency-key-must-not-be-stored";
        let rationale = "RAW_PRIVATE_MEMO_CANARY_94f0_actual_creator_note";
        let changed_rationale = "RAW_PRIVATE_MEMO_CANARY_1a27_changed_creator_note";
        let committed_once = fixture
            .sidecar
            .decide(
                &first.review_id,
                ArtifactDisposition::AdoptedUnchanged,
                Some(rationale),
                raw_key,
            )
            .expect("first decision succeeds");
        assert_eq!(
            fixture
                .sidecar
                .decide(
                    &first.review_id,
                    ArtifactDisposition::AdoptedUnchanged,
                    Some(rationale),
                    raw_key,
                )
                .expect("exact decision replay succeeds"),
            committed_once
        );
        assert_eq!(
            fixture
                .sidecar
                .decide(
                    &first.review_id,
                    ArtifactDisposition::Rejected,
                    Some(rationale),
                    raw_key,
                )
                .unwrap_err(),
            SidecarError::IdempotencyConflict
        );
        assert_eq!(
            fixture
                .sidecar
                .decide(
                    &first.review_id,
                    ArtifactDisposition::AdoptedUnchanged,
                    Some(changed_rationale),
                    raw_key,
                )
                .expect("private memo does not alter the durable Decision binding"),
            committed_once
        );
        assert_eq!(
            fixture
                .sidecar
                .decide(
                    &first.review_id,
                    ArtifactDisposition::AdoptedUnchanged,
                    Some(rationale),
                    b"different-key",
                )
                .unwrap_err(),
            SidecarError::DecisionIntentExists
        );

        let review_id = ReviewId::parse(first.review_id).unwrap();
        let journal = fixture.sidecar.open_journal().unwrap();
        let intent = journal
            .get_decision_intent(&review_id)
            .unwrap()
            .expect("intent exists");
        let repository = Repository::open(&fixture.config.repository).unwrap();
        let feedback = load_value(&repository, intent.feedback_oid(), ObjectKind::Record).unwrap();
        let feedback_payload = feedback.get("payload").unwrap();
        assert_eq!(
            feedback_payload
                .get("human_rationale")
                .and_then(JsonValue::as_str),
            Some(privacy_safe_rationale(
                ArtifactDisposition::AdoptedUnchanged
            ))
        );
        assert_eq!(
            feedback_payload.get("reason_codes"),
            Some(&json!(["unspecified"]))
        );
        drop(journal);
        drop(repository);
        assert_file_tree_omits(
            fixture._temp.path(),
            &[raw_key, rationale.as_bytes(), changed_rationale.as_bytes()],
        );
        assert_eq!(
            fixture.sidecar.decide(
                &replay.review_id,
                ArtifactDisposition::AdoptedUnchanged,
                Some(""),
                b"invalid-rationale-key"
            ),
            Err(SidecarError::InvalidArgument)
        );
    }

    #[test]
    fn restart_recovers_publication_that_preceded_the_journal_transition() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let proposal = begin(&fixture.sidecar, "first", &accepted, &proposed);
        let review_id = ReviewId::parse(proposal.review_id.clone()).unwrap();
        let mut journal = fixture.sidecar.open_journal().unwrap();
        let review = journal.get_review(&review_id).unwrap();
        let binding = review.binding().clone();
        let intent = fixture
            .sidecar
            .create_decision_intent(
                &mut journal,
                &review_id,
                &binding,
                ArtifactDisposition::AdoptedUnchanged,
                b"recovery-key",
            )
            .unwrap();
        drop(journal);
        fixture
            .sidecar
            .publish_recovered_decision(
                &review_id,
                &binding,
                &intent,
                ArtifactDisposition::AdoptedUnchanged,
            )
            .expect("Core publication succeeds before simulated crash");
        assert_eq!(
            fixture
                .sidecar
                .open_journal()
                .unwrap()
                .get_review(&review_id)
                .unwrap()
                .state(),
            ReviewState::PendingReview
        );

        let restarted = SynapseSidecar::open(fixture.config.clone()).unwrap();
        let receipt = committed(restarted.reconcile(&proposal.review_id).unwrap());
        assert_eq!(receipt.disposition, ArtifactDisposition::AdoptedUnchanged);
        assert_eq!(receipt.selected_snapshot, "proposal");
        assert_eq!(
            restarted
                .open_journal()
                .unwrap()
                .get_review(&review_id)
                .unwrap()
                .state(),
            ReviewState::DecisionCommitted
        );
    }

    #[test]
    fn restart_rebuilds_application_authority_from_a_durable_intent() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let proposal = begin(&fixture.sidecar, "first", &accepted, &proposed);
        assert_eq!(
            fixture.sidecar.resume(&proposal.review_id).unwrap(),
            ReviewOutcome::PendingReview,
            "a pending review without an intent stays query-only"
        );
        let review_id = ReviewId::parse(proposal.review_id.clone()).unwrap();
        let mut journal = fixture.sidecar.open_journal().unwrap();
        let binding = journal.get_review(&review_id).unwrap().binding().clone();
        fixture
            .sidecar
            .create_decision_intent(
                &mut journal,
                &review_id,
                &binding,
                ArtifactDisposition::AdoptedUnchanged,
                b"pre-publication-restart-key",
            )
            .unwrap();
        drop(journal);

        let restarted = SynapseSidecar::open(fixture.config.clone()).unwrap();
        assert_eq!(
            restarted.reconcile(&proposal.review_id).unwrap(),
            ReviewOutcome::PendingReview,
            "read-only reconciliation must not publish the durable intent"
        );
        let receipt = committed(restarted.resume(&proposal.review_id).unwrap());
        assert_eq!(receipt.selected_snapshot, "proposal");
        let first_published_head = Repository::open(&fixture.config.repository)
            .unwrap()
            .refs()
            .get(binding.decision_ref_name())
            .unwrap()
            .unwrap()
            .head;
        assert_eq!(
            restarted.resume(&proposal.review_id).unwrap(),
            ReviewOutcome::DecisionCommitted(receipt.clone()),
            "a repeated resume observes the first publication instead of publishing blindly"
        );
        assert_eq!(
            Repository::open(&fixture.config.repository)
                .unwrap()
                .refs()
                .get(binding.decision_ref_name())
                .unwrap()
                .unwrap()
                .head,
            first_published_head
        );
        assert_eq!(
            restarted.reconcile(&proposal.review_id).unwrap(),
            ReviewOutcome::DecisionCommitted(receipt)
        );
    }

    #[test]
    fn concurrent_exact_decision_retries_converge_on_one_committed_candidate() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let proposal = begin(&fixture.sidecar, "first", &accepted, &proposed);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let sidecar = fixture.sidecar.clone();
            let review_id = proposal.review_id.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                sidecar.decide(
                    &review_id,
                    ArtifactDisposition::AdoptedUnchanged,
                    Some("same concurrent rationale"),
                    b"same-concurrent-key",
                )
            }));
        }
        let first = committed(workers.remove(0).join().unwrap().unwrap());
        let second = committed(workers.remove(0).join().unwrap().unwrap());
        assert_eq!(first, second);
        assert_eq!(
            fixture.sidecar.reconcile(&proposal.review_id).unwrap(),
            ReviewOutcome::DecisionCommitted(first)
        );
    }

    #[test]
    fn concurrent_exact_proposal_retries_converge_on_one_review_binding() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let sidecar = fixture.sidecar.clone();
            let accepted = accepted.clone();
            let proposed = proposed.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                begin(&sidecar, "same-operation", &accepted, &proposed)
            }));
        }
        let first = workers.remove(0).join().unwrap();
        let second = workers.remove(0).join().unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fixture.sidecar.reconcile(&first.review_id).unwrap(),
            ReviewOutcome::PendingReview
        );
    }

    #[test]
    fn journal_intent_cannot_override_a_live_proposal_ref_drift() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let proposal = begin(&fixture.sidecar, "first", &accepted, &proposed);
        let review_id = ReviewId::parse(proposal.review_id.clone()).unwrap();
        let mut journal = fixture.sidecar.open_journal().unwrap();
        let review = journal.get_review(&review_id).unwrap();
        let binding = review.binding().clone();
        fixture
            .sidecar
            .create_decision_intent(
                &mut journal,
                &review_id,
                &binding,
                ArtifactDisposition::AdoptedUnchanged,
                b"drift-key",
            )
            .unwrap();
        drop(journal);

        let mut repository = Repository::open(&fixture.config.repository).unwrap();
        let occurred_at_unix_nanos =
            i64::try_from(SystemAuthorizationClock.now_unix_nanos().unwrap()).unwrap();
        repository
            .update_ref(RefUpdate {
                ref_name: binding.proposal_ref_name(),
                expected_head: Some(binding.proposal_head()),
                new_head: binding.expected_decision_head(),
                metadata: ReflogMetadata {
                    occurred_at_unix_nanos,
                    actor: None,
                    message: Some("test-only proposal Ref drift"),
                },
            })
            .unwrap();
        assert_eq!(
            fixture.sidecar.reconcile(&proposal.review_id).unwrap(),
            ReviewOutcome::TerminalDenial
        );
        assert_eq!(
            fixture
                .sidecar
                .open_journal()
                .unwrap()
                .get_review(&review_id)
                .unwrap()
                .state(),
            ReviewState::TerminalDenial
        );
        assert_eq!(
            fixture
                .sidecar
                .inspect_proposal(&proposal.review_id)
                .unwrap_err(),
            SidecarError::Integrity
        );
    }

    #[test]
    fn exact_terminal_denial_refs_allow_next_proposal_before_and_after_restart() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let first_proposed = manifest("first proposed");
        let second_proposed = manifest("second proposed");
        let third_proposed = manifest("third proposed");

        let first = begin(&fixture.sidecar, "first-denial", &accepted, &first_proposed);
        transition_to_terminal_denial(&fixture.sidecar, &first.review_id);
        let second = begin(
            &fixture.sidecar,
            "second-denial",
            &accepted,
            &second_proposed,
        );
        transition_to_terminal_denial(&fixture.sidecar, &second.review_id);

        let reopened = SynapseSidecar::open(fixture.config.clone()).unwrap();
        let third = begin(&reopened, "after-restart", &accepted, &third_proposed);
        assert_eq!(
            reopened.reconcile(&first.review_id).unwrap(),
            ReviewOutcome::TerminalDenial
        );
        assert_eq!(
            reopened.reconcile(&second.review_id).unwrap(),
            ReviewOutcome::TerminalDenial
        );
        assert_eq!(
            reopened.reconcile(&third.review_id).unwrap(),
            ReviewOutcome::PendingReview
        );

        let repository = Repository::open(&fixture.config.repository).unwrap();
        for operation in ["first-denial", "second-denial", "after-restart"] {
            assert!(
                repository
                    .refs()
                    .get(&reopened.proposal_ref(operation))
                    .unwrap()
                    .is_some(),
                "retained Proposal Ref {operation} remains authoritative evidence"
            );
        }
    }

    #[test]
    fn terminal_state_with_a_mismatched_reconstructed_binding_stays_active() {
        let fixture = Fixture::new();
        let accepted = manifest("accepted");
        let proposed = manifest("proposed");
        let first = begin(&fixture.sidecar, "first", &accepted, &proposed);
        transition_to_terminal_denial(&fixture.sidecar, &first.review_id);

        let first_review_id = ReviewId::parse(first.review_id).unwrap();
        let mut journal = fixture.sidecar.open_journal().unwrap();
        let first_binding = journal
            .get_review(&first_review_id)
            .unwrap()
            .binding()
            .clone();
        let tampered_ref = fixture.sidecar.proposal_ref("tampered-terminal");
        let mut repository = Repository::open(&fixture.config.repository).unwrap();
        let occurred_at_unix_nanos =
            i64::try_from(SystemAuthorizationClock.now_unix_nanos().unwrap()).unwrap();
        repository
            .update_ref(RefUpdate {
                ref_name: &tampered_ref,
                expected_head: None,
                new_head: first_binding.proposal_head(),
                metadata: ReflogMetadata {
                    occurred_at_unix_nanos,
                    actor: None,
                    message: Some("test-only mismatched terminal binding"),
                },
            })
            .unwrap();
        drop(repository);

        let mismatched = ReviewBinding::new(
            first_binding.project_scope(),
            &tampered_ref,
            first_binding.proposal_head(),
            first_binding.decision_ref_name(),
            first_binding.proposal_head(),
        )
        .unwrap();
        let tampered_review = journal
            .create_or_get_review(mismatched)
            .unwrap()
            .into_review();
        journal
            .transition_review_state(
                tampered_review.review_id(),
                ReviewState::PendingReview,
                ReviewState::TerminalDenial,
            )
            .unwrap();
        drop(journal);

        assert_eq!(
            fixture
                .sidecar
                .begin_proposal(ProposalInput {
                    operation_key: "must-remain-blocked",
                    accepted: &accepted,
                    proposed: &manifest("next"),
                    review_context_json: br#"{"operation":"must-remain-blocked"}"#,
                })
                .unwrap_err(),
            SidecarError::ActiveReview
        );
    }

    #[test]
    fn sequential_proposals_use_unique_refs_and_the_live_accepted_base() {
        let fixture = Fixture::new();
        let version_zero = manifest("version zero");
        let version_one = manifest("version one");
        let version_two = manifest("version two");
        let version_three = manifest("version three");

        let first = begin(&fixture.sidecar, "first", &version_zero, &version_one);
        assert_eq!(
            fixture
                .sidecar
                .begin_proposal(ProposalInput {
                    operation_key: "blocked-while-first-is-pending",
                    accepted: &version_zero,
                    proposed: &version_two,
                    review_context_json: br#"{"operation":"blocked"}"#,
                })
                .unwrap_err(),
            SidecarError::ActiveReview
        );
        assert_eq!(
            fixture.sidecar.reconcile(&first.review_id).unwrap(),
            ReviewOutcome::PendingReview
        );
        let first_decision = committed(
            fixture
                .sidecar
                .decide(
                    &first.review_id,
                    ArtifactDisposition::AdoptedUnchanged,
                    None,
                    b"first-key",
                )
                .unwrap(),
        );
        let mut reopened_config = fixture.config.clone();
        reopened_config.creator_display_name = "Renamed creator display".into();
        reopened_config.agent_display_name = "Renamed agent display".into();
        reopened_config.recorded_at = "2026-07-20T00:00:00.000000000Z".into();
        reopened_config.grant_expires_at = "2099-01-01T00:00:00.000000000Z".into();
        let reopened = SynapseSidecar::open(reopened_config).unwrap();
        let second = begin(&reopened, "second", &version_one, &version_two);
        let second_decision = committed(
            reopened
                .decide(
                    &second.review_id,
                    ArtifactDisposition::Rejected,
                    None,
                    b"second-key",
                )
                .unwrap(),
        );
        let third = begin(&reopened, "third", &version_one, &version_three);
        let third_decision = committed(
            reopened
                .decide(
                    &third.review_id,
                    ArtifactDisposition::Deferred,
                    None,
                    b"third-key",
                )
                .unwrap(),
        );

        for (review_id, expected) in [
            (&first.review_id, first_decision),
            (&second.review_id, second_decision),
            (&third.review_id, third_decision),
        ] {
            assert_eq!(
                reopened.reconcile(review_id).unwrap(),
                ReviewOutcome::DecisionCommitted(expected)
            );
        }

        let review_ids = BTreeSet::from([
            first.review_id.clone(),
            second.review_id.clone(),
            third.review_id.clone(),
        ]);
        assert_eq!(review_ids.len(), 3);
        let repository = Repository::open(&fixture.config.repository).unwrap();
        for operation in ["first", "second", "third"] {
            assert!(
                repository
                    .refs()
                    .get(&reopened.proposal_ref(operation))
                    .unwrap()
                    .is_some()
            );
        }
        let current = repository
            .refs()
            .get(&fixture.sidecar.decision_ref())
            .unwrap()
            .unwrap();
        assert_eq!(
            manifest_digest_at_commit(&repository, &current.head, ArtifactLimits::default())
                .unwrap(),
            artifact_manifest_sha256(&version_one)
        );
        assert_eq!(
            fixture
                .sidecar
                .begin_proposal(ProposalInput {
                    operation_key: "fourth",
                    accepted: &version_zero,
                    proposed: &version_two,
                    review_context_json: br#"{"operation":"fourth"}"#,
                })
                .unwrap_err(),
            SidecarError::StaleBase
        );
    }
}
