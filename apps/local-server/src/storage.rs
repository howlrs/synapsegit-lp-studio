use mime_guess::MimeGuess;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use synapse_canonical::parse_strict;
use unicase::UniCase;
use unicode_normalization::UnicodeNormalization as _;
use uuid::Uuid;

use crate::fault_injection::{self, Failpoint};

pub(super) const MAX_FILES: usize = 1_000;
pub(super) const MAX_TOTAL_BYTES: usize = 200 * 1024 * 1024;
pub(super) const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
pub(super) const MAX_PATH_BYTES: usize = 512;
pub(super) const MAX_DEPTH: usize = 16;

const STORAGE_SCHEMA: &str = "1";
const STORAGE_DIRECTORY: &str = "managed-v1";
const PROJECT_LAYOUT_VERSION: &str = "2";
const PROJECT_LAYOUT_MARKER: &str = "layout.json";
const RECOVERY_BACKUPS_DIRECTORY: &str = "recovery-backups";
const MIGRATIONS_DIRECTORY: &str = "migrations";
const BACKUP_FORMAT_VERSION: &str = "1";
const MAX_PERSISTED_PROJECTS: usize = 8;
const MAX_RECOVERY_BACKUPS: usize = 64;
const MAX_RECOVERY_POINTS: usize = MAX_RECOVERY_BACKUPS + MAX_PERSISTED_PROJECTS;
const MAX_RETAINED_REVISIONS: usize = 256;
const MAX_BACKUP_TREE_ENTRIES: usize = 16_384;
const MAX_BACKUP_TREE_DEPTH: usize = 32;
const MAX_BACKUP_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BACKUP_TOTAL_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const MAX_PERSISTED_TARGETS: usize = 32;
pub(super) const MAX_TARGET_METADATA_BYTES: usize = 64 * 1024;
pub(super) const MAX_PROJECT_DISPLAY_NAME_CODE_POINTS: usize = 256;
pub(super) const MAX_PROJECT_DISPLAY_NAME_BYTES: usize = 1_024;
const MAX_PERSISTED_PROPOSALS: usize = 128;
const MAX_PROPOSAL_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROPOSAL_STAGING_ENTRIES: usize = 16;
const MAX_PERSISTED_EXPORTS: usize = 16;
const MAX_PERSISTED_PUBLICATIONS: usize = 16;
const MAX_GENERATED_STAGING_ENTRIES: usize = 16;
const MAX_GENERATED_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_GENERATED_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_GENERATED_COLLECTION_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_GENERATED_MANIFEST_BYTES: usize = 16 * 1024;

#[derive(Debug)]
pub(super) enum StorageError {
    Io(io::Error),
    Corrupt,
    Drift,
    UnsafeImport,
    ImportLimit,
}

impl From<io::Error> for StorageError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
pub(super) struct ManagedStorage {
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct PersistedProject {
    pub id: String,
    pub display_name: String,
    pub revision_id: String,
    pub artifact_manifest_sha256: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub targets: Vec<(String, Vec<u8>)>,
    pub proposals: Vec<PersistedProposal>,
    pub exports: Vec<PersistedExport>,
    pub publications: Vec<PersistedPublication>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectMetadataUpdate {
    Updated,
    Idempotent,
    Stale,
}

#[derive(Clone, Debug)]
pub(super) struct PersistedProposal {
    pub id: String,
    pub metadata: Option<Vec<u8>>,
    pub binding: Option<Vec<u8>>,
    pub decision: Option<Vec<u8>>,
    pub completion: Option<Vec<u8>>,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PersistedExport {
    pub id: String,
    pub project_id: String,
    pub revision_id: String,
    pub archive_sha256: String,
    pub archive_zip: Vec<u8>,
    pub receipt_json: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PersistedPublication {
    pub id: String,
    pub project_id: String,
    pub revision_id: String,
    pub archive_sha256: String,
    pub archive_zip: Vec<u8>,
    pub bundle_metadata_json: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryPointKind {
    VersionedBackup,
    LastAccepted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RecoveryDiagnostic {
    pub verified: bool,
    pub code: &'static str,
    pub manifest_sha256: Option<String>,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RecoveryPointSummary {
    pub id: String,
    pub kind: RecoveryPointKind,
    pub project_id: String,
    pub revision_id: Option<String>,
    pub artifact_manifest_sha256: Option<String>,
    pub diagnostic: RecoveryDiagnostic,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RecoverySnapshot {
    pub point: RecoveryPointSummary,
    files: BTreeMap<String, Vec<u8>>,
}

impl RecoverySnapshot {
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}

/// A capability-limited reader. It intentionally exposes no mutation or
/// restore method, so recovery inspection remains read-only even when normal
/// managed storage refuses to open.
#[derive(Clone, Debug)]
pub(super) struct RecoveryReader {
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub(super) struct ImportScan {
    pub display_name: String,
    pub manifest_sha256: String,
    pub total_bytes: usize,
    pub entry_point: Option<String>,
    pub included: Vec<ImportIncluded>,
    pub excluded: Vec<ImportExcluded>,
    pub warnings: Vec<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportIncluded {
    pub path: String,
    pub byte_length: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportExcluded {
    pub path: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectMetadata {
    schema_version: String,
    id: String,
    display_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CurrentPointer {
    schema_version: String,
    revision_id: String,
    canonical_manifest_sha256: String,
    artifact_manifest_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevisionManifest {
    schema_version: String,
    revision_id: String,
    artifact_manifest_sha256: String,
    files: Vec<StoredFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredFile {
    path: String,
    media_type: String,
    byte_length: usize,
    sha256: String,
}

type ObjectBindings = BTreeMap<String, usize>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransactionMarker {
    schema_version: String,
    target_revision_id: String,
    canonical_manifest_sha256: String,
    artifact_manifest_sha256: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewedImportProjection<'a> {
    schema_version: &'static str,
    display_name: &'a str,
    entry_point: &'a Option<String>,
    included: &'a [ImportIncluded],
    excluded: &'a [ImportExcluded],
    warnings: &'a [String],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreationMarker {
    schema_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectLayoutMarker {
    schema_version: String,
    layout_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MigrationJournal {
    schema_version: String,
    target_layout_version: String,
    migration_id: String,
    backup_id: String,
    project_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupManifest {
    schema_version: String,
    format_version: String,
    backup_id: String,
    project_id: String,
    revision_id: String,
    artifact_manifest_sha256: String,
    directories: Vec<String>,
    files: Vec<BackupFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupFile {
    path: String,
    byte_length: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TargetReplaceJournal {
    schema_version: String,
    project_id: String,
    old_target_id: String,
    old_sha256: String,
    old_byte_length: usize,
    new_target_id: String,
    new_sha256: String,
    new_byte_length: usize,
    nonce: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum GeneratedArtifactKind {
    StaticExport,
    PublicationDraft,
}

impl GeneratedArtifactKind {
    const fn directory_name(self) -> &'static str {
        match self {
            Self::StaticExport => "exports",
            Self::PublicationDraft => "publications",
        }
    }

    const fn id_prefix(self) -> &'static str {
        match self {
            Self::StaticExport => "exp_",
            Self::PublicationDraft => "pub_",
        }
    }

    const fn staging_prefix(self) -> &'static str {
        match self {
            Self::StaticExport => ".creating-export-",
            Self::PublicationDraft => ".creating-publication-",
        }
    }

    const fn deleting_prefix(self) -> &'static str {
        match self {
            Self::StaticExport => ".deleting-export-",
            Self::PublicationDraft => ".deleting-publication-",
        }
    }

    const fn max_records(self) -> usize {
        match self {
            Self::StaticExport => MAX_PERSISTED_EXPORTS,
            Self::PublicationDraft => MAX_PERSISTED_PUBLICATIONS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedArtifactManifest {
    schema_version: String,
    kind: GeneratedArtifactKind,
    project_id: String,
    artifact_id: String,
    revision_id: String,
    archive_sha256: String,
    archive_byte_length: usize,
    metadata_sha256: String,
    metadata_byte_length: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LoadedGeneratedArtifact {
    manifest: GeneratedArtifactManifest,
    archive_zip: Vec<u8>,
    metadata_json: Vec<u8>,
}

#[derive(Default)]
struct BackupBudget {
    entries: usize,
    total_bytes: u64,
}

#[derive(Default)]
struct ScanBudget {
    visited_entries: usize,
    total_bytes: usize,
}

#[derive(Default)]
struct ImportWalk {
    files: BTreeMap<String, Vec<u8>>,
    excluded: Vec<ImportExcluded>,
    identities: BTreeSet<FileIdentity>,
    folded_paths: BTreeSet<String>,
    budget: ScanBudget,
}

impl ScanBudget {
    fn visit_import_entry(&mut self) -> Result<(), StorageError> {
        self.visited_entries = self
            .visited_entries
            .checked_add(1)
            .ok_or(StorageError::ImportLimit)?;
        if self.visited_entries > MAX_FILES {
            return Err(StorageError::ImportLimit);
        }
        Ok(())
    }

    fn visit_materialized_entry(&mut self) -> Result<(), StorageError> {
        self.visited_entries = self
            .visited_entries
            .checked_add(1)
            .ok_or(StorageError::Drift)?;
        if self.visited_entries > MAX_FILES {
            return Err(StorageError::Drift);
        }
        Ok(())
    }

    fn add_import_file(&mut self, byte_length: usize) -> Result<(), StorageError> {
        self.total_bytes = self
            .total_bytes
            .checked_add(byte_length)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or(StorageError::ImportLimit)?;
        Ok(())
    }

    fn add_materialized_file(&mut self, byte_length: usize) -> Result<(), StorageError> {
        self.total_bytes = self
            .total_bytes
            .checked_add(byte_length)
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or(StorageError::Drift)?;
        Ok(())
    }
}

impl ManagedStorage {
    pub fn open(state_root: &Path) -> Result<(Self, Vec<PersistedProject>), StorageError> {
        validate_real_directory(state_root)?;
        let root = state_root.join(STORAGE_DIRECTORY);
        match fs::symlink_metadata(&root) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(StorageError::Corrupt);
            }
            Ok(_) => preflight_existing_managed_root(&root)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Self::open_after_preflight(state_root)
    }

    fn open_after_preflight(
        state_root: &Path,
    ) -> Result<(Self, Vec<PersistedProject>), StorageError> {
        let root = state_root.join(STORAGE_DIRECTORY);
        ensure_real_directory(&root)?;
        ensure_real_directory(&root.join("objects"))?;
        ensure_real_directory(&root.join("projects"))?;
        ensure_real_directory(&root.join(RECOVERY_BACKUPS_DIRECTORY))?;
        ensure_real_directory(&root.join(MIGRATIONS_DIRECTORY))?;
        let storage = Self { root };
        storage.recover_migrations()?;
        storage.recover_owned_project_deletions()?;
        let projects = storage.load_projects()?;
        Ok((storage, projects))
    }

    pub fn open_recovery(state_root: &Path) -> Result<RecoveryReader, StorageError> {
        validate_real_directory(state_root)?;
        let root = state_root.join(STORAGE_DIRECTORY);
        validate_real_directory(&root)?;
        validate_real_directory(&root.join("objects"))?;
        validate_real_directory(&root.join("projects"))?;
        validate_real_directory(&root.join(RECOVERY_BACKUPS_DIRECTORY))?;
        Ok(RecoveryReader { root })
    }

    pub fn create_project(
        &self,
        id: &str,
        display_name: &str,
        revision_id: &str,
        artifact_manifest_sha256: &str,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), StorageError> {
        validate_identifier(id, "prj_")?;
        validate_identifier(revision_id, "rev_")?;
        validate_project_display_name(display_name)?;
        let projects_root = self.root.join("projects");
        validate_real_directory(&projects_root)?;
        let project_root = self.project_root(id);
        match fs::symlink_metadata(&project_root) {
            Ok(_) => return Err(StorageError::Corrupt),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let staging = projects_root.join(format!(".creating-{id}.{}", Uuid::new_v4().simple()));
        fs::create_dir(&staging)?;
        sync_directory(&projects_root)?;
        let result = (|| -> Result<(), StorageError> {
            write_immutable_json(
                &staging.join("creating.json"),
                &CreationMarker {
                    schema_version: STORAGE_SCHEMA.into(),
                },
            )?;
            fs::create_dir(staging.join("revisions"))?;
            fs::create_dir(staging.join("proposals"))?;
            fs::create_dir(staging.join("targets"))?;
            fs::create_dir(staging.join("synapse"))?;
            fs::create_dir(staging.join("exports"))?;
            fs::create_dir(staging.join("publications"))?;
            write_immutable_json(
                &staging.join(PROJECT_LAYOUT_MARKER),
                &ProjectLayoutMarker {
                    schema_version: STORAGE_SCHEMA.to_owned(),
                    layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
                },
            )?;
            let metadata = ProjectMetadata {
                schema_version: STORAGE_SCHEMA.into(),
                id: id.into(),
                display_name: display_name.into(),
            };
            write_immutable_json(&staging.join("project.json"), &metadata)?;
            self.commit_revision_at(&staging, revision_id, artifact_manifest_sha256, files)?;
            remove_internal_file_if_present(&staging.join("creating.json"))?;
            sync_directory(&staging)?;
            match fs::symlink_metadata(&project_root) {
                Ok(_) => return Err(StorageError::Corrupt),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            rename_internal(&staging, &project_root)?;
            sync_directory(&projects_root)
        })();
        if result.is_err() {
            let _ = remove_internal_tree_if_present(&staging);
        }
        result
    }

    pub fn update_project_display_name(
        &self,
        project_id: &str,
        expected_display_name: &str,
        display_name: &str,
    ) -> Result<ProjectMetadataUpdate, StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_project_display_name(expected_display_name)?;
        validate_project_display_name(display_name)?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        let metadata_path = project_root.join("project.json");
        let current: ProjectMetadata = read_canonical_json_bounded(&metadata_path, 64 * 1024)?;
        if current.schema_version != STORAGE_SCHEMA || current.id != project_id {
            return Err(StorageError::Corrupt);
        }
        validate_project_display_name(&current.display_name)?;
        if current.display_name == display_name {
            // A previous request may have reached atomic replace but lost its
            // response. Re-syncing the parent makes the retry a durable,
            // idempotent acknowledgement rather than an in-memory inference.
            sync_directory(&project_root)?;
            return Ok(ProjectMetadataUpdate::Idempotent);
        }
        if current.display_name != expected_display_name {
            return Ok(ProjectMetadataUpdate::Stale);
        }
        let updated = ProjectMetadata {
            schema_version: STORAGE_SCHEMA.to_owned(),
            id: project_id.to_owned(),
            display_name: display_name.to_owned(),
        };
        match write_atomic_json(&metadata_path, &updated) {
            Ok(()) => Ok(ProjectMetadataUpdate::Updated),
            Err(write_error) => {
                // StorageRenameAfter and the following directory fsync are
                // outcome-ambiguous: the replacement may already be visible.
                // Accept it only after exact canonical bytes and a fresh parent
                // fsync succeed. Otherwise preserve the original I/O failure.
                match read_canonical_json_bounded::<ProjectMetadata>(&metadata_path, 64 * 1024) {
                    Ok(actual) if canonical_json(&actual)? == canonical_json(&updated)? => {
                        sync_directory(&project_root)?;
                        Ok(ProjectMetadataUpdate::Updated)
                    }
                    Ok(actual) if canonical_json(&actual)? == canonical_json(&current)? => {
                        Err(write_error)
                    }
                    Ok(_) => Err(StorageError::Corrupt),
                    Err(_) => Err(write_error),
                }
            }
        }
    }

    pub fn commit_revision(
        &self,
        project_id: &str,
        revision_id: &str,
        artifact_manifest_sha256: &str,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(revision_id, "rev_")?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        self.commit_revision_at(&project_root, revision_id, artifact_manifest_sha256, files)
    }

    fn commit_revision_at(
        &self,
        project_root: &Path,
        revision_id: &str,
        artifact_manifest_sha256: &str,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), StorageError> {
        validate_identifier(revision_id, "rev_")?;
        validate_sha256(artifact_manifest_sha256)?;
        validate_real_directory(project_root)?;
        validate_real_directory(&project_root.join("revisions"))?;
        let stored_files = stored_files(files)?;
        for entry in &stored_files {
            let bytes = files.get(&entry.path).ok_or(StorageError::Corrupt)?;
            self.write_object(&entry.sha256, bytes)?;
        }
        let manifest = RevisionManifest {
            schema_version: STORAGE_SCHEMA.into(),
            revision_id: revision_id.into(),
            artifact_manifest_sha256: artifact_manifest_sha256.into(),
            files: stored_files,
        };
        let manifest_bytes = canonical_json(&manifest)?;
        let canonical_manifest_sha256 = sha256(&manifest_bytes);
        write_immutable(
            &project_root
                .join("revisions")
                .join(format!("{revision_id}.json")),
            &manifest_bytes,
        )?;
        let pointer = CurrentPointer {
            schema_version: STORAGE_SCHEMA.into(),
            revision_id: revision_id.into(),
            canonical_manifest_sha256,
            artifact_manifest_sha256: artifact_manifest_sha256.into(),
        };
        let marker = TransactionMarker {
            schema_version: STORAGE_SCHEMA.into(),
            target_revision_id: pointer.revision_id.clone(),
            canonical_manifest_sha256: pointer.canonical_manifest_sha256.clone(),
            artifact_manifest_sha256: pointer.artifact_manifest_sha256.clone(),
        };
        write_atomic_json(&project_root.join("transaction.json"), &marker)?;
        injected_storage_fault(Failpoint::DecisionMaterializeBefore)?;
        self.materialize_to(&project_root.join("site.staging"), &manifest)?;
        injected_storage_fault(Failpoint::DecisionMaterializeAfter)?;
        let site = project_root.join("site");
        let backup = project_root.join("site.backup");
        remove_internal_tree_if_present(&backup)?;
        if site.exists() {
            rename_internal(&site, &backup)?;
        }
        if let Err(error) = rename_internal(&project_root.join("site.staging"), &site) {
            if backup.exists() && !site.exists() {
                let _ = fs::rename(&backup, &site);
            }
            return Err(error);
        }
        injected_storage_fault(Failpoint::DecisionAcceptedPointerBefore)?;
        write_atomic_json(&project_root.join("current.json"), &pointer)?;
        injected_storage_fault(Failpoint::DecisionAcceptedPointerAfter)?;
        sync_directory(project_root)?;
        remove_internal_tree_if_present(&backup)?;
        remove_internal_file_if_present(&project_root.join("transaction.json"))?;
        Ok(())
    }

    pub fn verify_project(
        &self,
        project_id: &str,
        revision_id: &str,
        expected_artifact_manifest_sha256: &str,
    ) -> Result<(), StorageError> {
        let project_root = self.project_root(project_id);
        let pointer: CurrentPointer = read_json(&project_root.join("current.json"))?;
        validate_pointer(&pointer)?;
        if pointer.revision_id != revision_id
            || pointer.artifact_manifest_sha256 != expected_artifact_manifest_sha256
        {
            return Err(StorageError::Corrupt);
        }
        let manifest = self.read_manifest(project_id, &pointer)?;
        let actual = scan_materialized(&project_root.join("site"))?;
        if actual != manifest.files {
            return Err(StorageError::Drift);
        }
        Ok(())
    }

    pub fn verify_retained_revision(
        &self,
        project_id: &str,
        revision_id: &str,
        expected_artifact_manifest_sha256: &str,
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(revision_id, "rev_")?;
        validate_sha256(expected_artifact_manifest_sha256)?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        validate_real_directory(&project_root.join("revisions"))?;
        let manifest = read_retained_revision_manifest(&project_root, revision_id)?;
        if manifest.artifact_manifest_sha256 != expected_artifact_manifest_sha256 {
            return Err(StorageError::Corrupt);
        }
        self.load_manifest_files(&manifest)?;
        Ok(())
    }

    pub fn delete_project(
        &self,
        project_id: &str,
        expected_current_revision: &str,
        expected_manifest: &str,
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(expected_current_revision, "rev_")?;
        validate_sha256(expected_manifest)?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        if path_exists_strict(
            &self
                .root
                .join(MIGRATIONS_DIRECTORY)
                .join(format!("{project_id}.json")),
        )? {
            return Err(StorageError::Corrupt);
        }
        self.validate_new_project(project_id)?;
        self.verify_project(project_id, expected_current_revision, expected_manifest)?;
        collect_backup_tree(&project_root)?;
        self.validate_recovery_backups_for_project_delete()?;
        let projects_root = self.root.join("projects");
        let nonce = Uuid::new_v4().simple().to_string();
        let deleting = projects_root.join(format!(".deleting-project-{project_id}.{nonce}"));
        rename_internal(&project_root, &deleting)?;
        sync_directory(&projects_root)?;
        self.complete_project_delete(&deleting, project_id, &nonce)
    }

    pub fn prepare_proposal(
        &self,
        project_id: &str,
        proposal_id: &str,
        files: &BTreeMap<String, Vec<u8>>,
        canonical_metadata: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(proposal_id, "pro_")?;
        if canonical_metadata.is_empty() || canonical_metadata.len() > MAX_PROPOSAL_METADATA_BYTES {
            return Err(StorageError::Corrupt);
        }
        let proposals_root = self.project_root(project_id).join("proposals");
        validate_real_directory(&proposals_root)?;
        self.recover_owned_proposal_staging(project_id)?;
        let retained = enumerate_proposal_directories(&proposals_root)?;
        if retained.len() >= MAX_PERSISTED_PROPOSALS {
            return Err(StorageError::Corrupt);
        }
        let destination = proposals_root.join(proposal_id);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(StorageError::Corrupt),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let staging = proposals_root.join(format!(
            ".creating-proposal-{proposal_id}.{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir(&staging)?;
        let result = (|| -> Result<(), StorageError> {
            let stored = stored_files(files)?;
            for entry in &stored {
                let bytes = files.get(&entry.path).ok_or(StorageError::Corrupt)?;
                self.write_object(&entry.sha256, bytes)?;
            }
            let manifest = RevisionManifest {
                schema_version: STORAGE_SCHEMA.into(),
                revision_id: proposal_id.into(),
                artifact_manifest_sha256: sha256(b"proposal-workspace"),
                files: stored,
            };
            self.materialize_to(&staging.join("site"), &manifest)?;
            write_immutable(&staging.join("proposal.json"), canonical_metadata)?;
            sync_directory(&staging)?;
            rename_internal(&staging, &destination)?;
            sync_directory(&proposals_root)
        })();
        if result.is_err() {
            // Objects written before a failed proposal publication are not
            // subject to general GC. Only the exact bytes materialized in our
            // owned staging tree are eligible for reachability-safe cleanup.
            let _ = self.complete_proposal_staging_cleanup(&staging, false);
        }
        result
    }

    pub fn persist_decision_metadata(
        &self,
        project_id: &str,
        proposal_id: &str,
        canonical_bytes: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(proposal_id, "pro_")?;
        if canonical_bytes.is_empty() || canonical_bytes.len() > MAX_PROPOSAL_METADATA_BYTES {
            return Err(StorageError::Corrupt);
        }
        let proposal_root = self
            .project_root(project_id)
            .join("proposals")
            .join(proposal_id);
        validate_real_directory(&proposal_root)?;
        injected_storage_fault(Failpoint::DecisionReceiptPersistBefore)?;
        write_immutable(&proposal_root.join("decision.json"), canonical_bytes)?;
        injected_storage_fault(Failpoint::DecisionReceiptPersistAfter)
    }

    pub fn persist_decision_completion(
        &self,
        project_id: &str,
        proposal_id: &str,
        canonical_bytes: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(proposal_id, "pro_")?;
        if canonical_bytes.is_empty() || canonical_bytes.len() > MAX_PROPOSAL_METADATA_BYTES {
            return Err(StorageError::Corrupt);
        }
        let proposal_root = self
            .project_root(project_id)
            .join("proposals")
            .join(proposal_id);
        validate_real_directory(&proposal_root)?;
        injected_storage_fault(Failpoint::DecisionJournalCompleteBefore)?;
        write_immutable(&proposal_root.join("completion.json"), canonical_bytes)?;
        injected_storage_fault(Failpoint::DecisionJournalCompleteAfter)
    }

    pub fn persist_proposal_binding(
        &self,
        project_id: &str,
        proposal_id: &str,
        canonical_bytes: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(proposal_id, "pro_")?;
        if canonical_bytes.is_empty() || canonical_bytes.len() > MAX_PROPOSAL_METADATA_BYTES {
            return Err(StorageError::Corrupt);
        }
        let proposal_root = self
            .project_root(project_id)
            .join("proposals")
            .join(proposal_id);
        validate_real_directory(&proposal_root)?;
        write_immutable(&proposal_root.join("review.json"), canonical_bytes)
    }

    pub fn delete_proposal(
        &self,
        project_id: &str,
        proposal_id: &str,
        expected_metadata_sha256: &str,
        expected_metadata: &[u8],
        expected_binding: Option<(&str, &[u8])>,
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(proposal_id, "pro_")?;
        validate_sha256(expected_metadata_sha256)?;
        if expected_metadata.is_empty()
            || expected_metadata.len() > MAX_PROPOSAL_METADATA_BYTES
            || sha256(expected_metadata) != expected_metadata_sha256
        {
            return Err(StorageError::Corrupt);
        }
        if let Some((digest, bytes)) = expected_binding {
            validate_sha256(digest)?;
            if bytes.is_empty()
                || bytes.len() > MAX_PROPOSAL_METADATA_BYTES
                || sha256(bytes) != digest
            {
                return Err(StorageError::Corrupt);
            }
        }
        let proposals = self.load_proposals(project_id)?;
        let proposal = proposals
            .iter()
            .find(|proposal| proposal.id == proposal_id)
            .ok_or(StorageError::Corrupt)?;
        if proposal.metadata.as_deref() != Some(expected_metadata)
            || proposal
                .binding
                .as_deref()
                .zip(expected_binding.map(|(_, bytes)| bytes))
                .is_some_and(|(actual, expected)| actual != expected)
            || proposal.binding.is_some() != expected_binding.is_some()
        {
            return Err(StorageError::Corrupt);
        }
        let proposals_root = self.project_root(project_id).join("proposals");
        let destination = proposals_root.join(proposal_id);
        validate_stable_proposal_inventory(&destination)?;
        let deleting = proposals_root.join(format!(
            ".deleting-proposal-{proposal_id}.{}",
            Uuid::new_v4().simple()
        ));
        rename_internal(&destination, &deleting)?;
        sync_directory(&proposals_root)?;
        self.complete_proposal_staging_cleanup(&deleting, true)
    }

    fn recover_owned_proposal_staging(&self, project_id: &str) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        let proposals_root = self.project_root(project_id).join("proposals");
        let entries = bounded_directory_entries(
            &proposals_root,
            MAX_PERSISTED_PROPOSALS + MAX_PROPOSAL_STAGING_ENTRIES,
        )?;
        let mut retained = 0_usize;
        let mut staging = Vec::with_capacity(MAX_PROPOSAL_STAGING_ENTRIES);
        for entry in entries {
            let name = utf8_name(&entry.file_name())?;
            let metadata = entry.file_type()?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Corrupt);
            }
            if proposal_creating_staging_parts(&name).is_some() {
                if staging.len() == MAX_PROPOSAL_STAGING_ENTRIES {
                    return Err(StorageError::Corrupt);
                }
                staging.push((entry.path(), false));
            } else if proposal_deleting_staging_parts(&name).is_some() {
                if staging.len() == MAX_PROPOSAL_STAGING_ENTRIES {
                    return Err(StorageError::Corrupt);
                }
                staging.push((entry.path(), true));
            } else if name.starts_with(".creating-proposal-")
                || name.starts_with(".deleting-proposal-")
            {
                return Err(StorageError::Corrupt);
            } else {
                validate_identifier(&name, "pro_")?;
                retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
                if retained > MAX_PERSISTED_PROPOSALS {
                    return Err(StorageError::Corrupt);
                }
            }
        }
        for (path, require_complete) in staging {
            self.complete_proposal_staging_cleanup(&path, require_complete)?;
        }
        Ok(())
    }

    fn complete_proposal_staging_cleanup(
        &self,
        staging: &Path,
        require_complete: bool,
    ) -> Result<(), StorageError> {
        match fs::symlink_metadata(staging) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(StorageError::Corrupt);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let candidates = proposal_site_object_bindings(staging, require_complete)?;
        self.cleanup_candidate_objects(&candidates)?;
        remove_internal_tree_if_present(staging)?;
        sync_directory(staging.parent().ok_or(StorageError::Corrupt)?)
    }

    fn cleanup_candidate_objects(&self, candidates: &ObjectBindings) -> Result<(), StorageError> {
        if candidates.is_empty() {
            return Ok(());
        }
        let reachable = self.collect_active_object_bindings()?;
        for (digest, byte_length) in candidates {
            if let Some(reachable_length) = reachable.get(digest) {
                if reachable_length != byte_length {
                    return Err(StorageError::Corrupt);
                }
                continue;
            }
            self.remove_exact_candidate_object(digest, *byte_length)?;
        }
        Ok(())
    }

    fn collect_active_object_bindings(&self) -> Result<ObjectBindings, StorageError> {
        let projects_root = self.root.join("projects");
        let entries = bounded_directory_entries(&projects_root, MAX_PERSISTED_PROJECTS + 32)?;
        let mut bindings = ObjectBindings::new();
        let mut retained = 0_usize;
        for entry in entries {
            let name = utf8_name(&entry.file_name())?;
            let metadata = entry.file_type()?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Corrupt);
            }
            if name.starts_with('.') {
                if !(is_owned_creation_staging(&name)
                    || is_owned_project_migration_staging(&name)
                    || is_owned_project_deleting_staging(&name)
                    || is_owned_migration_old(&name))
                {
                    return Err(StorageError::Corrupt);
                }
                continue;
            }
            validate_identifier(&name, "prj_")?;
            retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
            if retained > MAX_PERSISTED_PROJECTS {
                return Err(StorageError::Corrupt);
            }
            let project_root = entry.path();
            validate_real_directory(&project_root)?;
            if !path_is_regular_file_if_present(&project_root.join("current.json"))? {
                let marker: CreationMarker =
                    read_canonical_json_bounded(&project_root.join("creating.json"), 16 * 1024)?;
                if marker.schema_version != STORAGE_SCHEMA {
                    return Err(StorageError::Corrupt);
                }
                continue;
            }
            let metadata: ProjectMetadata =
                read_canonical_json_bounded(&project_root.join("project.json"), 64 * 1024)?;
            if metadata.schema_version != STORAGE_SCHEMA
                || metadata.id != name
                || validate_project_display_name(&metadata.display_name).is_err()
            {
                return Err(StorageError::Corrupt);
            }
            let pointer: CurrentPointer =
                read_canonical_json_bounded(&project_root.join("current.json"), 64 * 1024)?;
            validate_pointer(&pointer)?;
            self.read_manifest_from_root(&project_root, &pointer)?;
            collect_revision_object_bindings(&project_root, &mut bindings)?;
            collect_active_proposal_object_bindings(&project_root, &mut bindings)?;
        }
        Ok(bindings)
    }

    fn collect_project_candidate_objects(
        &self,
        project_root: &Path,
    ) -> Result<ObjectBindings, StorageError> {
        let mut candidates = ObjectBindings::new();
        collect_revision_object_bindings(project_root, &mut candidates)?;
        collect_all_proposal_object_bindings(project_root, &mut candidates)?;
        Ok(candidates)
    }

    fn remove_exact_candidate_object(
        &self,
        digest: &str,
        expected_length: usize,
    ) -> Result<(), StorageError> {
        validate_sha256(digest)?;
        if expected_length > MAX_FILE_BYTES {
            return Err(StorageError::Corrupt);
        }
        validate_real_directory(&self.root.join("objects"))?;
        let object = self.object_path(digest);
        let parent = object.parent().ok_or(StorageError::Corrupt)?;
        match fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(StorageError::Corrupt);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
        let before = match fs::symlink_metadata(&object) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if before.file_type().is_symlink()
            || !before.is_file()
            || hard_link_count(&before) != 1
            || usize::try_from(before.len()).ok() != Some(expected_length)
        {
            return Err(StorageError::Corrupt);
        }
        let identity = file_identity(&before, &object)?;
        let bytes = read_regular_file_exact(&object, expected_length, StorageError::Corrupt)?;
        if sha256(&bytes) != digest {
            return Err(StorageError::Corrupt);
        }
        let after = fs::symlink_metadata(&object)?;
        if after.file_type().is_symlink()
            || !after.is_file()
            || hard_link_count(&after) != 1
            || file_identity(&after, &object)? != identity
            || usize::try_from(after.len()).ok() != Some(expected_length)
        {
            return Err(StorageError::Corrupt);
        }
        fs::remove_file(&object)?;
        sync_directory(parent)
    }

    pub fn persist_export(
        &self,
        project_id: &str,
        export_id: &str,
        revision_id: &str,
        archive_zip: &[u8],
        receipt_json: &[u8],
    ) -> Result<(), StorageError> {
        self.persist_generated_artifact(
            project_id,
            export_id,
            revision_id,
            GeneratedArtifactKind::StaticExport,
            archive_zip,
            receipt_json,
        )
    }

    pub fn persist_publication(
        &self,
        project_id: &str,
        publication_id: &str,
        revision_id: &str,
        archive_zip: &[u8],
        bundle_metadata_json: &[u8],
    ) -> Result<(), StorageError> {
        self.persist_generated_artifact(
            project_id,
            publication_id,
            revision_id,
            GeneratedArtifactKind::PublicationDraft,
            archive_zip,
            bundle_metadata_json,
        )
    }

    pub fn delete_export(
        &self,
        project_id: &str,
        export_id: &str,
        expected_sha256: &str,
        expected_bytes: &[u8],
    ) -> Result<(), StorageError> {
        self.delete_generated_artifact(
            project_id,
            export_id,
            expected_sha256,
            expected_bytes,
            GeneratedArtifactKind::StaticExport,
        )
    }

    pub fn delete_publication(
        &self,
        project_id: &str,
        publication_id: &str,
        expected_sha256: &str,
        expected_bytes: &[u8],
    ) -> Result<(), StorageError> {
        self.delete_generated_artifact(
            project_id,
            publication_id,
            expected_sha256,
            expected_bytes,
            GeneratedArtifactKind::PublicationDraft,
        )
    }

    fn delete_generated_artifact(
        &self,
        project_id: &str,
        artifact_id: &str,
        expected_sha256: &str,
        expected_bytes: &[u8],
        kind: GeneratedArtifactKind,
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(artifact_id, kind.id_prefix())?;
        validate_sha256(expected_sha256)?;
        if expected_bytes.is_empty()
            || expected_bytes.len() > MAX_GENERATED_ARCHIVE_BYTES
            || sha256(expected_bytes) != expected_sha256
        {
            return Err(StorageError::Corrupt);
        }
        let records = self.load_generated_collection(project_id, kind)?;
        let record = records
            .iter()
            .find(|record| record.manifest.artifact_id == artifact_id)
            .ok_or(StorageError::Corrupt)?;
        if record.manifest.archive_sha256 != expected_sha256 || record.archive_zip != expected_bytes
        {
            return Err(StorageError::Corrupt);
        }
        let collection = self.project_root(project_id).join(kind.directory_name());
        let destination = collection.join(artifact_id);
        validate_real_directory(&destination)?;
        let deleting = collection.join(format!(
            "{}{artifact_id}.{}",
            kind.deleting_prefix(),
            Uuid::new_v4().simple()
        ));
        rename_internal(&destination, &deleting)?;
        sync_directory(&collection)?;
        remove_internal_tree_if_present(&deleting)?;
        sync_directory(&collection)
    }

    fn persist_generated_artifact(
        &self,
        project_id: &str,
        artifact_id: &str,
        revision_id: &str,
        kind: GeneratedArtifactKind,
        archive_zip: &[u8],
        metadata_json: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(artifact_id, kind.id_prefix())?;
        validate_identifier(revision_id, "rev_")?;
        validate_generated_payload(archive_zip, metadata_json)?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        read_retained_revision_manifest(&project_root, revision_id)?;
        let collection = project_root.join(kind.directory_name());
        validate_real_directory(&collection)?;

        let manifest = GeneratedArtifactManifest {
            schema_version: STORAGE_SCHEMA.to_owned(),
            kind,
            project_id: project_id.to_owned(),
            artifact_id: artifact_id.to_owned(),
            revision_id: revision_id.to_owned(),
            archive_sha256: sha256(archive_zip),
            archive_byte_length: archive_zip.len(),
            metadata_sha256: sha256(metadata_json),
            metadata_byte_length: metadata_json.len(),
        };
        let existing = self.load_generated_collection(project_id, kind)?;
        if let Some(existing) = existing
            .iter()
            .find(|record| record.manifest.artifact_id == artifact_id)
        {
            return if existing.manifest == manifest
                && existing.archive_zip == archive_zip
                && existing.metadata_json == metadata_json
            {
                Ok(())
            } else {
                Err(StorageError::Corrupt)
            };
        }
        if existing.len() >= kind.max_records() {
            return Err(StorageError::Corrupt);
        }
        let retained_archive_bytes = existing.iter().try_fold(0_usize, |total, record| {
            total.checked_add(record.archive_zip.len())
        });
        let retained_metadata_bytes = existing.iter().try_fold(0_usize, |total, record| {
            total.checked_add(record.metadata_json.len())
        });
        if retained_archive_bytes
            .and_then(|total| total.checked_add(archive_zip.len()))
            .is_none_or(|total| total > MAX_GENERATED_ARCHIVE_BYTES)
            || retained_metadata_bytes
                .and_then(|total| total.checked_add(metadata_json.len()))
                .is_none_or(|total| total > MAX_GENERATED_COLLECTION_METADATA_BYTES)
        {
            return Err(StorageError::Corrupt);
        }

        let destination = collection.join(artifact_id);
        match fs::symlink_metadata(&destination) {
            Ok(_) => return Err(StorageError::Corrupt),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let staging = collection.join(format!(
            "{}{artifact_id}.{}",
            kind.staging_prefix(),
            Uuid::new_v4().simple()
        ));
        fs::create_dir(&staging)?;
        sync_directory(&collection)?;
        let result = (|| -> Result<(), StorageError> {
            write_immutable(&staging.join("archive.zip"), archive_zip)?;
            write_immutable(&staging.join("metadata.json"), metadata_json)?;
            write_immutable_json(&staging.join("manifest.json"), &manifest)?;
            sync_directory(&staging)?;
            match fs::symlink_metadata(&destination) {
                Ok(_) => return Err(StorageError::Corrupt),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            rename_internal(&staging, &destination)?;
            sync_directory(&collection)
        })();
        if result.is_err() {
            let _ = remove_internal_tree_if_present(&staging);
            let _ = sync_directory(&collection);
        }
        result
    }

    pub fn synapse_paths(&self, project_id: &str) -> Result<(PathBuf, PathBuf), StorageError> {
        validate_identifier(project_id, "prj_")?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        let synapse_root = project_root.join("synapse");
        ensure_real_directory(&synapse_root)?;
        Ok((
            synapse_root.join("repository"),
            synapse_root.join("review-journal.sqlite3"),
        ))
    }

    pub fn persist_target(
        &self,
        project_id: &str,
        target_id: &str,
        canonical_bytes: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(target_id, "tgt_")?;
        if canonical_bytes.is_empty() || canonical_bytes.len() > MAX_TARGET_METADATA_BYTES {
            return Err(StorageError::Corrupt);
        }
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        self.recover_target_transaction(&project_root, project_id)?;
        let targets_root = project_root.join("targets");
        validate_real_directory(&targets_root)?;
        let existing = enumerate_target_files(&targets_root)?;
        let destination = targets_root.join(format!("{target_id}.json"));
        if existing.iter().any(|(id, _)| id == target_id) {
            return verify_immutable_bytes(&destination, canonical_bytes);
        }
        if existing.len() >= MAX_PERSISTED_TARGETS {
            return Err(StorageError::Corrupt);
        }
        write_immutable(&destination, canonical_bytes)
    }

    pub fn replace_target(
        &self,
        project_id: &str,
        old_target_id: &str,
        expected_old_bytes: &[u8],
        new_target_id: &str,
        new_bytes: &[u8],
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(old_target_id, "tgt_")?;
        validate_identifier(new_target_id, "tgt_")?;
        if old_target_id == new_target_id
            || expected_old_bytes.is_empty()
            || expected_old_bytes.len() > MAX_TARGET_METADATA_BYTES
            || new_bytes.is_empty()
            || new_bytes.len() > MAX_TARGET_METADATA_BYTES
        {
            return Err(StorageError::Corrupt);
        }
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        self.recover_target_transaction(&project_root, project_id)?;
        let targets_root = project_root.join("targets");
        let retained = enumerate_target_files(&targets_root)?;
        if retained.len() != MAX_PERSISTED_TARGETS
            || !retained.iter().any(|(id, _)| id == old_target_id)
            || retained.iter().any(|(id, _)| id == new_target_id)
        {
            return Err(StorageError::Corrupt);
        }
        let old_path = targets_root.join(format!("{old_target_id}.json"));
        verify_immutable_bytes(&old_path, expected_old_bytes)?;

        let nonce = Uuid::new_v4().simple().to_string();
        let staging = targets_root.join(format!(".replacing-target-{new_target_id}.{nonce}.tmp"));
        write_new_file(&staging, new_bytes)?;
        sync_directory(&targets_root)?;
        let journal = TargetReplaceJournal {
            schema_version: STORAGE_SCHEMA.to_owned(),
            project_id: project_id.to_owned(),
            old_target_id: old_target_id.to_owned(),
            old_sha256: sha256(expected_old_bytes),
            old_byte_length: expected_old_bytes.len(),
            new_target_id: new_target_id.to_owned(),
            new_sha256: sha256(new_bytes),
            new_byte_length: new_bytes.len(),
            nonce,
        };
        if let Err(error) =
            write_immutable_json(&project_root.join("target-transaction.json"), &journal)
        {
            let _ = fs::remove_file(&staging);
            let _ = sync_directory(&targets_root);
            return Err(error);
        }
        self.complete_target_replace(&project_root, &journal)
    }

    fn recover_target_transaction(
        &self,
        project_root: &Path,
        project_id: &str,
    ) -> Result<(), StorageError> {
        let journal_path = project_root.join("target-transaction.json");
        match fs::symlink_metadata(&journal_path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err(StorageError::Corrupt)
            }
            Ok(_) => {
                let journal: TargetReplaceJournal =
                    read_canonical_json_bounded(&journal_path, MAX_TARGET_METADATA_BYTES)?;
                if journal.project_id != project_id {
                    return Err(StorageError::Corrupt);
                }
                self.complete_target_replace(project_root, &journal)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                cleanup_orphan_target_temporaries(&project_root.join("targets"))?;
                cleanup_exact_atomic_temporary(project_root, "target-transaction.json")
            }
            Err(error) => Err(error.into()),
        }
    }

    fn complete_target_replace(
        &self,
        project_root: &Path,
        journal: &TargetReplaceJournal,
    ) -> Result<(), StorageError> {
        validate_identifier(&journal.project_id, "prj_")?;
        validate_identifier(&journal.old_target_id, "tgt_")?;
        validate_identifier(&journal.new_target_id, "tgt_")?;
        validate_nonce(&journal.nonce)?;
        validate_sha256(&journal.old_sha256)?;
        validate_sha256(&journal.new_sha256)?;
        if journal.schema_version != STORAGE_SCHEMA
            || journal.old_target_id == journal.new_target_id
            || journal.old_byte_length == 0
            || journal.old_byte_length > MAX_TARGET_METADATA_BYTES
            || journal.new_byte_length == 0
            || journal.new_byte_length > MAX_TARGET_METADATA_BYTES
        {
            return Err(StorageError::Corrupt);
        }
        let targets_root = project_root.join("targets");
        validate_target_transaction_inventory(&targets_root, journal)?;
        let old_path = targets_root.join(format!("{}.json", journal.old_target_id));
        let new_path = targets_root.join(format!("{}.json", journal.new_target_id));
        let staging = targets_root.join(format!(
            ".replacing-target-{}.{}.tmp",
            journal.new_target_id, journal.nonce
        ));
        let old_exists = path_is_regular_file_if_present(&old_path)?;
        let new_exists = path_is_regular_file_if_present(&new_path)?;
        let staging_exists = path_is_regular_file_if_present(&staging)?;
        if old_exists {
            verify_file_digest_and_length(
                &old_path,
                journal.old_byte_length,
                &journal.old_sha256,
                MAX_TARGET_METADATA_BYTES,
            )?;
        }
        if new_exists {
            verify_file_digest_and_length(
                &new_path,
                journal.new_byte_length,
                &journal.new_sha256,
                MAX_TARGET_METADATA_BYTES,
            )?;
        }
        if staging_exists {
            verify_file_digest_and_length(
                &staging,
                journal.new_byte_length,
                &journal.new_sha256,
                MAX_TARGET_METADATA_BYTES,
            )?;
        }
        if !new_exists {
            if !old_exists || !staging_exists {
                return Err(StorageError::Corrupt);
            }
            rename_internal(&staging, &new_path)?;
            sync_directory(&targets_root)?;
        } else if staging_exists {
            return Err(StorageError::Corrupt);
        }
        if old_exists {
            fs::remove_file(&old_path)?;
            sync_directory(&targets_root)?;
        }
        remove_internal_file_if_present(&project_root.join("target-transaction.json"))?;
        sync_directory(project_root)?;
        if enumerate_target_files(&targets_root)?.len() != MAX_PERSISTED_TARGETS {
            return Err(StorageError::Corrupt);
        }
        Ok(())
    }

    fn recover_migrations(&self) -> Result<(), StorageError> {
        let migrations_root = self.root.join(MIGRATIONS_DIRECTORY);
        let projects_root = self.root.join("projects");
        let backups_root = self.root.join(RECOVERY_BACKUPS_DIRECTORY);
        validate_real_directory(&migrations_root)?;
        validate_real_directory(&projects_root)?;
        validate_real_directory(&backups_root)?;

        let mut journals = Vec::with_capacity(MAX_PERSISTED_PROJECTS + 16);
        for entry in fs::read_dir(&migrations_root)? {
            if journals.len() == MAX_PERSISTED_PROJECTS + 16 {
                return Err(StorageError::Corrupt);
            }
            journals.push(entry?);
        }
        journals.sort_by_key(|entry| entry.file_name());
        let mut journal_count = 0_usize;
        for entry in journals {
            let name = utf8_name(&entry.file_name())?;
            let metadata = entry.file_type()?;
            if is_owned_migration_journal_temporary(&name) {
                if metadata.is_symlink() || !metadata.is_file() {
                    return Err(StorageError::Corrupt);
                }
                fs::remove_file(entry.path())?;
                sync_directory(&migrations_root)?;
                continue;
            }
            if metadata.is_symlink() || !metadata.is_file() {
                return Err(StorageError::Corrupt);
            }
            let project_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
            validate_identifier(project_id, "prj_")?;
            journal_count = journal_count.checked_add(1).ok_or(StorageError::Corrupt)?;
            if journal_count > MAX_PERSISTED_PROJECTS {
                return Err(StorageError::Corrupt);
            }
            let journal: MigrationJournal = read_canonical_json_bounded(&entry.path(), 16 * 1024)?;
            if journal.project_id != project_id {
                return Err(StorageError::Corrupt);
            }
            self.resume_migration(&journal)?;
        }
        self.cleanup_orphan_migration_staging()
    }

    fn cleanup_orphan_migration_staging(&self) -> Result<(), StorageError> {
        let projects_root = self.root.join("projects");
        let mut changed = false;
        for entry in bounded_directory_entries(&projects_root, MAX_PERSISTED_PROJECTS + 32)? {
            let name = utf8_name(&entry.file_name())?;
            if name.starts_with(".migrating-") {
                if !is_owned_project_migration_staging(&name) {
                    return Err(StorageError::Corrupt);
                }
                let metadata = entry.file_type()?;
                if metadata.is_symlink() || !metadata.is_dir() {
                    return Err(StorageError::Corrupt);
                }
                remove_internal_tree_if_present(&entry.path())?;
                changed = true;
            } else if name.starts_with(".migration-old-") {
                // An old active tree is never disposable without its canonical
                // migration journal.
                return Err(StorageError::Corrupt);
            }
        }
        if changed {
            sync_directory(&projects_root)?;
        }

        let backups_root = self.root.join(RECOVERY_BACKUPS_DIRECTORY);
        let mut backup_changed = false;
        let mut retained_backups = 0_usize;
        for entry in
            bounded_directory_entries(&backups_root, MAX_RECOVERY_BACKUPS + MAX_PERSISTED_PROJECTS)?
        {
            let name = utf8_name(&entry.file_name())?;
            if name.starts_with(".creating-backup-") {
                if !is_owned_backup_staging(&name) {
                    return Err(StorageError::Corrupt);
                }
                let metadata = entry.file_type()?;
                if metadata.is_symlink() || !metadata.is_dir() {
                    return Err(StorageError::Corrupt);
                }
                remove_internal_tree_if_present(&entry.path())?;
                backup_changed = true;
            } else if name.starts_with(".deleting-backup-") {
                if backup_deleting_staging_parts(&name).is_none() {
                    return Err(StorageError::Corrupt);
                }
                let metadata = entry.file_type()?;
                if metadata.is_symlink() || !metadata.is_dir() {
                    return Err(StorageError::Corrupt);
                }
            } else {
                validate_identifier(&name, "bkp_")?;
                retained_backups = retained_backups
                    .checked_add(1)
                    .ok_or(StorageError::Corrupt)?;
                if retained_backups > MAX_RECOVERY_BACKUPS {
                    return Err(StorageError::Corrupt);
                }
                let metadata = entry.file_type()?;
                if metadata.is_symlink() || !metadata.is_dir() {
                    return Err(StorageError::Corrupt);
                }
                verify_backup_root(&entry.path(), None)?;
            }
        }
        if backup_changed {
            sync_directory(&backups_root)?;
        }
        Ok(())
    }

    fn recover_owned_project_deletions(&self) -> Result<(), StorageError> {
        let projects_root = self.root.join("projects");
        let mut recovered = BTreeSet::new();
        for entry in bounded_directory_entries(&projects_root, MAX_PERSISTED_PROJECTS + 32)? {
            let name = utf8_name(&entry.file_name())?;
            if !name.starts_with(".deleting-project-") {
                continue;
            }
            let Some((project_id, nonce)) = project_deleting_staging_parts(&name) else {
                return Err(StorageError::Corrupt);
            };
            let metadata = entry.file_type()?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Corrupt);
            }
            self.complete_project_delete(&entry.path(), project_id, nonce)?;
            recovered.insert(nonce.to_owned());
        }

        let backups_root = self.root.join(RECOVERY_BACKUPS_DIRECTORY);
        for entry in
            bounded_directory_entries(&backups_root, MAX_RECOVERY_BACKUPS + MAX_PERSISTED_PROJECTS)?
        {
            let name = utf8_name(&entry.file_name())?;
            if name.starts_with(".deleting-backup-") {
                let Some((_backup_id, nonce)) = backup_deleting_staging_parts(&name) else {
                    return Err(StorageError::Corrupt);
                };
                if !recovered.contains(nonce) {
                    return Err(StorageError::Corrupt);
                }
            }
        }
        Ok(())
    }

    fn validate_recovery_backups_for_project_delete(&self) -> Result<(), StorageError> {
        let backups_root = self.root.join(RECOVERY_BACKUPS_DIRECTORY);
        for entry in
            bounded_directory_entries(&backups_root, MAX_RECOVERY_BACKUPS + MAX_PERSISTED_PROJECTS)?
        {
            let name = utf8_name(&entry.file_name())?;
            if name.starts_with('.') {
                return Err(StorageError::Corrupt);
            }
            validate_identifier(&name, "bkp_")?;
            verify_backup_root(&entry.path(), None)?;
        }
        Ok(())
    }

    fn complete_project_delete(
        &self,
        deleting_project: &Path,
        project_id: &str,
        nonce: &str,
    ) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_nonce(nonce)?;
        validate_real_directory(deleting_project)?;
        validate_project_layout_marker(deleting_project)?;
        validate_stable_project_inventory(deleting_project, true)?;
        read_last_accepted_from_project_tree(deleting_project)?;
        collect_backup_tree(deleting_project)?;

        // The renamed project tree is the durable deletion journal. Its
        // manifests and proposal sites identify the only CAS candidates this
        // manual operation may collect. A restart can recompute that same set;
        // already-removed exact objects are intentionally idempotent.
        let candidates = self.collect_project_candidate_objects(deleting_project)?;
        self.cleanup_candidate_objects(&candidates)?;

        let backups_root = self.root.join(RECOVERY_BACKUPS_DIRECTORY);
        for entry in
            bounded_directory_entries(&backups_root, MAX_RECOVERY_BACKUPS + MAX_PERSISTED_PROJECTS)?
        {
            let name = utf8_name(&entry.file_name())?;
            if name.starts_with(".deleting-backup-") {
                let Some((_backup_id, backup_nonce)) = backup_deleting_staging_parts(&name) else {
                    return Err(StorageError::Corrupt);
                };
                if backup_nonce != nonce {
                    return Err(StorageError::Corrupt);
                }
                let manifest = verify_backup_root(&entry.path(), None)?;
                if manifest.project_id != project_id {
                    return Err(StorageError::Corrupt);
                }
                remove_internal_tree_if_present(&entry.path())?;
                sync_directory(&backups_root)?;
                continue;
            }
            if name.starts_with('.') {
                return Err(StorageError::Corrupt);
            }
            validate_identifier(&name, "bkp_")?;
            let manifest = verify_backup_root(&entry.path(), None)?;
            if manifest.project_id != project_id {
                continue;
            }
            let backup_deleting =
                backups_root.join(format!(".deleting-backup-{}.{nonce}", manifest.backup_id));
            rename_internal(&entry.path(), &backup_deleting)?;
            sync_directory(&backups_root)?;
            remove_internal_tree_if_present(&backup_deleting)?;
            sync_directory(&backups_root)?;
        }
        remove_internal_tree_if_present(deleting_project)?;
        sync_directory(&self.root.join("projects"))
    }

    fn migrate_legacy_project(&self, project_id: &str) -> Result<(), StorageError> {
        validate_identifier(project_id, "prj_")?;
        self.validate_legacy_project(project_id)?;
        let migrations_root = self.root.join(MIGRATIONS_DIRECTORY);
        let journal_path = migrations_root.join(format!("{project_id}.json"));
        if path_exists_strict(&journal_path)? {
            return Err(StorageError::Corrupt);
        }
        let nonce = Uuid::new_v4().simple().to_string();
        let journal = MigrationJournal {
            schema_version: STORAGE_SCHEMA.to_owned(),
            target_layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
            migration_id: format!("mig_{nonce}"),
            backup_id: format!("bkp_{nonce}"),
            project_id: project_id.to_owned(),
        };
        write_immutable_json(&journal_path, &journal)?;
        sync_directory(&migrations_root)?;
        self.resume_migration(&journal)
    }

    fn resume_migration(&self, journal: &MigrationJournal) -> Result<(), StorageError> {
        validate_migration_journal(journal)?;
        let nonce = journal
            .migration_id
            .strip_prefix("mig_")
            .ok_or(StorageError::Corrupt)?;
        if journal.backup_id.strip_prefix("bkp_") != Some(nonce) {
            return Err(StorageError::Corrupt);
        }
        let projects_root = self.root.join("projects");
        let active = self.project_root(&journal.project_id);
        let staging = projects_root.join(format!(".migrating-{}.{}", journal.project_id, nonce));
        let old = projects_root.join(format!(".migration-old-{}.{}", journal.project_id, nonce));
        let backup = self
            .root
            .join(RECOVERY_BACKUPS_DIRECTORY)
            .join(&journal.backup_id);
        let backup_staging = self
            .root
            .join(RECOVERY_BACKUPS_DIRECTORY)
            .join(format!(".creating-backup-{}.{}", journal.backup_id, nonce));

        let active_exists = path_is_real_directory_if_present(&active)?;
        let old_exists = path_is_real_directory_if_present(&old)?;
        let staging_exists = path_is_real_directory_if_present(&staging)?;
        if active_exists && old_exists && !project_has_valid_layout(&active)? {
            return Err(StorageError::Corrupt);
        }
        if !active_exists && !old_exists {
            return Err(StorageError::Corrupt);
        }

        let backup_manifest = if path_is_real_directory_if_present(&backup)? {
            if path_is_real_directory_if_present(&backup_staging)? {
                remove_internal_tree_if_present(&backup_staging)?;
                sync_directory(&self.root.join(RECOVERY_BACKUPS_DIRECTORY))?;
            }
            verify_backup_root(&backup, Some(journal))?
        } else {
            let source = if active_exists && !project_has_valid_layout(&active)? {
                self.validate_legacy_project(&journal.project_id)?;
                &active
            } else {
                return Err(StorageError::Corrupt);
            };
            if path_is_real_directory_if_present(&backup_staging)? {
                remove_internal_tree_if_present(&backup_staging)?;
            }
            self.create_backup(source, &backup_staging, &backup, journal)?
        };

        if active_exists && project_has_valid_layout(&active)? {
            self.validate_new_project(&journal.project_id)?;
            if staging_exists {
                return Err(StorageError::Corrupt);
            }
            if old_exists {
                remove_internal_tree_if_present(&old)?;
                sync_directory(&projects_root)?;
            }
            self.finish_migration_journal(&journal.project_id)?;
            return Ok(());
        }

        if staging_exists {
            if validate_migrated_staging(&staging, &backup, &backup_manifest).is_err() {
                remove_internal_tree_if_present(&staging)?;
                sync_directory(&projects_root)?;
                self.create_migrated_staging(&staging, &backup, &backup_manifest)?;
            }
        } else {
            self.create_migrated_staging(&staging, &backup, &backup_manifest)?;
        }

        if active_exists {
            if old_exists {
                return Err(StorageError::Corrupt);
            }
            rename_internal(&active, &old)?;
            sync_directory(&projects_root)?;
        }
        if !path_is_real_directory_if_present(&old)? {
            return Err(StorageError::Corrupt);
        }
        rename_internal(&staging, &active)?;
        sync_directory(&projects_root)?;
        self.validate_new_project(&journal.project_id)?;
        remove_internal_tree_if_present(&old)?;
        sync_directory(&projects_root)?;
        self.finish_migration_journal(&journal.project_id)
    }

    fn create_backup(
        &self,
        source: &Path,
        staging: &Path,
        destination: &Path,
        journal: &MigrationJournal,
    ) -> Result<BackupManifest, StorageError> {
        match fs::symlink_metadata(destination) {
            Ok(_) => return Err(StorageError::Corrupt),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let pointer: CurrentPointer =
            read_canonical_json_bounded(&source.join("current.json"), 64 * 1024)?;
        validate_pointer(&pointer)?;
        let (directories, files) = collect_backup_tree(source)?;
        let manifest = BackupManifest {
            schema_version: STORAGE_SCHEMA.to_owned(),
            format_version: BACKUP_FORMAT_VERSION.to_owned(),
            backup_id: journal.backup_id.clone(),
            project_id: journal.project_id.clone(),
            revision_id: pointer.revision_id,
            artifact_manifest_sha256: pointer.artifact_manifest_sha256,
            directories,
            files,
        };
        fs::create_dir(staging)?;
        fs::create_dir(staging.join("project"))?;
        copy_backup_tree(source, &staging.join("project"), &manifest)?;
        write_immutable_json(&staging.join("manifest.json"), &manifest)?;
        sync_directory(staging)?;
        verify_backup_root(staging, Some(journal))?;
        rename_internal(staging, destination)?;
        sync_directory(&self.root.join(RECOVERY_BACKUPS_DIRECTORY))?;
        verify_backup_root(destination, Some(journal))
    }

    fn create_migrated_staging(
        &self,
        staging: &Path,
        backup: &Path,
        manifest: &BackupManifest,
    ) -> Result<(), StorageError> {
        match fs::symlink_metadata(staging) {
            Ok(_) => return Err(StorageError::Corrupt),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::create_dir(staging)?;
        copy_backup_tree(&backup.join("project"), staging, manifest)?;
        for directory in ["targets", "synapse", "exports", "publications"] {
            ensure_real_directory(&staging.join(directory))?;
        }
        write_immutable_json(
            &staging.join(PROJECT_LAYOUT_MARKER),
            &ProjectLayoutMarker {
                schema_version: STORAGE_SCHEMA.to_owned(),
                layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
            },
        )?;
        sync_directory(staging)?;
        validate_migrated_staging(staging, backup, manifest)
    }

    fn finish_migration_journal(&self, project_id: &str) -> Result<(), StorageError> {
        let migrations_root = self.root.join(MIGRATIONS_DIRECTORY);
        remove_internal_file_if_present(&migrations_root.join(format!("{project_id}.json")))?;
        sync_directory(&migrations_root)
    }

    fn remove_valid_creation_marker_if_present(
        &self,
        project_root: &Path,
    ) -> Result<(), StorageError> {
        let path = project_root.join("creating.json");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err(StorageError::Corrupt)
            }
            Ok(_) => {
                let marker: CreationMarker = read_canonical_json_bounded(&path, 16 * 1024)?;
                if marker.schema_version != STORAGE_SCHEMA {
                    return Err(StorageError::Corrupt);
                }
                fs::remove_file(path)?;
                sync_directory(project_root)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn validate_legacy_project(&self, project_id: &str) -> Result<(), StorageError> {
        let project_root = self.project_root(project_id);
        validate_identifier(project_id, "prj_")?;
        validate_real_directory(&project_root)?;
        validate_stable_project_inventory(&project_root, false)?;
        self.validate_project_core(project_id, false)
    }

    fn validate_new_project(&self, project_id: &str) -> Result<(), StorageError> {
        let project_root = self.project_root(project_id);
        validate_identifier(project_id, "prj_")?;
        validate_real_directory(&project_root)?;
        validate_project_layout_marker(&project_root)?;
        self.recover_target_transaction(&project_root, project_id)?;
        validate_stable_project_inventory(&project_root, true)?;
        self.validate_project_core(project_id, true)
    }

    fn validate_project_core(
        &self,
        project_id: &str,
        require_new_layout: bool,
    ) -> Result<(), StorageError> {
        let project_root = self.project_root(project_id);
        let metadata: ProjectMetadata =
            read_canonical_json_bounded(&project_root.join("project.json"), 64 * 1024)?;
        if metadata.schema_version != STORAGE_SCHEMA
            || metadata.id != project_id
            || validate_project_display_name(&metadata.display_name).is_err()
        {
            return Err(StorageError::Corrupt);
        }
        let pointer: CurrentPointer =
            read_canonical_json_bounded(&project_root.join("current.json"), 64 * 1024)?;
        validate_pointer(&pointer)?;
        let manifest = self.read_manifest_from_root(&project_root, &pointer)?;
        self.load_manifest_files(&manifest)?;
        if scan_materialized(&project_root.join("site"))? != manifest.files {
            return Err(StorageError::Drift);
        }
        validate_all_retained_revisions(self, &project_root)?;
        self.load_proposals(project_id)?;
        if require_new_layout || project_root.join("targets").exists() {
            self.load_target_bytes(project_id)?;
        }
        if require_new_layout || project_root.join("exports").exists() {
            self.load_generated_collection(project_id, GeneratedArtifactKind::StaticExport)?;
        }
        if require_new_layout || project_root.join("publications").exists() {
            self.load_generated_collection(project_id, GeneratedArtifactKind::PublicationDraft)?;
        }
        if require_new_layout || project_root.join("synapse").exists() {
            validate_real_directory(&project_root.join("synapse"))?;
        }
        Ok(())
    }

    fn load_projects(&self) -> Result<Vec<PersistedProject>, StorageError> {
        let projects_root = self.root.join("projects");
        validate_real_directory(&projects_root)?;
        let maximum_entries = MAX_PERSISTED_PROJECTS
            .checked_add(16)
            .ok_or(StorageError::Corrupt)?;
        let mut project_dirs = Vec::with_capacity(maximum_entries);
        for entry in fs::read_dir(&projects_root)? {
            if project_dirs.len() == maximum_entries {
                return Err(StorageError::Corrupt);
            }
            project_dirs.push(entry?);
        }
        project_dirs.sort_by_key(|entry| entry.file_name());
        let mut result = Vec::new();
        for entry in project_dirs {
            let metadata = entry.file_type()?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Corrupt);
            }
            let project_root = entry.path();
            let directory_name = utf8_name(&entry.file_name())?;
            if is_owned_creation_staging(&directory_name) {
                validate_real_directory(&project_root)?;
                remove_internal_tree_if_present(&project_root)?;
                sync_directory(&self.root.join("projects"))?;
                continue;
            }
            validate_identifier(&directory_name, "prj_")?;
            if result.len() == MAX_PERSISTED_PROJECTS {
                return Err(StorageError::Corrupt);
            }
            validate_real_directory(&project_root)?;
            if !project_root.join("transaction.json").exists()
                && !project_root.join("current.json").exists()
            {
                let creating = project_root.join("creating.json");
                if creating.exists() {
                    let marker: CreationMarker = read_json(&creating)?;
                    if marker.schema_version != STORAGE_SCHEMA {
                        return Err(StorageError::Corrupt);
                    }
                    remove_internal_tree_if_present(&project_root)?;
                    sync_directory(&self.root.join("projects"))?;
                    continue;
                }
                return Err(StorageError::Corrupt);
            }
            validate_real_directory(&project_root.join("revisions"))?;
            validate_real_directory(&project_root.join("proposals"))?;
            self.recover_project(&project_root)?;
            self.remove_valid_creation_marker_if_present(&project_root)?;
            match fs::symlink_metadata(project_root.join(PROJECT_LAYOUT_MARKER)) {
                Ok(marker) if marker.file_type().is_symlink() || !marker.is_file() => {
                    return Err(StorageError::Corrupt);
                }
                Ok(_) => validate_project_layout_marker(&project_root)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    self.migrate_legacy_project(&directory_name)?;
                    validate_project_layout_marker(&project_root)?;
                }
                Err(error) => return Err(error.into()),
            }
            validate_real_directory(&project_root.join("targets"))?;
            validate_real_directory(&project_root.join("synapse"))?;
            validate_real_directory(&project_root.join("exports"))?;
            validate_real_directory(&project_root.join("publications"))?;
            self.recover_target_transaction(&project_root, &directory_name)?;
            self.validate_new_project(&directory_name)?;
            validate_real_directory(&project_root.join("site"))?;
            let metadata: ProjectMetadata = read_json(&project_root.join("project.json"))?;
            if metadata.schema_version != STORAGE_SCHEMA
                || metadata.id != directory_name
                || validate_project_display_name(&metadata.display_name).is_err()
            {
                return Err(StorageError::Corrupt);
            }
            let pointer: CurrentPointer = read_json(&project_root.join("current.json"))?;
            validate_pointer(&pointer)?;
            let manifest = self.read_manifest(&metadata.id, &pointer)?;
            let files = self.load_manifest_files(&manifest)?;
            let targets = self.load_target_bytes(&metadata.id)?;
            let proposals = self.load_proposals(&metadata.id)?;
            let exports = self
                .load_generated_collection(&metadata.id, GeneratedArtifactKind::StaticExport)?
                .into_iter()
                .map(|record| PersistedExport {
                    id: record.manifest.artifact_id,
                    project_id: record.manifest.project_id,
                    revision_id: record.manifest.revision_id,
                    archive_sha256: record.manifest.archive_sha256,
                    archive_zip: record.archive_zip,
                    receipt_json: record.metadata_json,
                })
                .collect();
            let publications = self
                .load_generated_collection(&metadata.id, GeneratedArtifactKind::PublicationDraft)?
                .into_iter()
                .map(|record| PersistedPublication {
                    id: record.manifest.artifact_id,
                    project_id: record.manifest.project_id,
                    revision_id: record.manifest.revision_id,
                    archive_sha256: record.manifest.archive_sha256,
                    archive_zip: record.archive_zip,
                    bundle_metadata_json: record.metadata_json,
                })
                .collect();
            result.push(PersistedProject {
                id: metadata.id,
                display_name: metadata.display_name,
                revision_id: pointer.revision_id,
                artifact_manifest_sha256: pointer.artifact_manifest_sha256,
                files,
                targets,
                proposals,
                exports,
                publications,
            });
        }
        Ok(result)
    }

    fn load_proposals(&self, project_id: &str) -> Result<Vec<PersistedProposal>, StorageError> {
        let proposals_root = self.project_root(project_id).join("proposals");
        self.recover_owned_proposal_staging(project_id)?;
        enumerate_proposal_directories(&proposals_root)?
            .into_iter()
            .map(|(proposal_id, root)| {
                (|| {
                    recover_proposal_root(&root)?;
                    let metadata = read_optional_bounded_file(
                        &root.join("proposal.json"),
                        MAX_PROPOSAL_METADATA_BYTES,
                    )?;
                    let decision = read_optional_bounded_file(
                        &root.join("decision.json"),
                        MAX_PROPOSAL_METADATA_BYTES,
                    )?;
                    let completion = read_optional_bounded_file(
                        &root.join("completion.json"),
                        MAX_PROPOSAL_METADATA_BYTES,
                    )?;
                    let binding = read_optional_bounded_file(
                        &root.join("review.json"),
                        MAX_PROPOSAL_METADATA_BYTES,
                    )?;
                    if (binding.is_some() || decision.is_some() || completion.is_some())
                        && metadata.is_none()
                        || decision.is_some() && binding.is_none()
                        || completion.is_some() && decision.is_none()
                    {
                        return Err(StorageError::Corrupt);
                    }
                    let site = root.join("site");
                    let files = match fs::symlink_metadata(&site) {
                        Ok(site_metadata)
                            if site_metadata.is_dir()
                                && !site_metadata.file_type().is_symlink() =>
                        {
                            scan_materialized_files(&site)?
                        }
                        Ok(_) => return Err(StorageError::Corrupt),
                        Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
                        Err(error) => return Err(error.into()),
                    };
                    Ok(PersistedProposal {
                        id: proposal_id,
                        metadata,
                        binding,
                        decision,
                        completion,
                        files,
                    })
                })()
            })
            .collect()
    }

    fn load_generated_collection(
        &self,
        project_id: &str,
        kind: GeneratedArtifactKind,
    ) -> Result<Vec<LoadedGeneratedArtifact>, StorageError> {
        validate_identifier(project_id, "prj_")?;
        let project_root = self.project_root(project_id);
        validate_real_directory(&project_root)?;
        let collection = project_root.join(kind.directory_name());
        validate_real_directory(&collection)?;
        let maximum_entries = kind
            .max_records()
            .checked_add(MAX_GENERATED_STAGING_ENTRIES)
            .ok_or(StorageError::Corrupt)?;
        let mut entries = Vec::with_capacity(maximum_entries);
        for entry in fs::read_dir(&collection)? {
            if entries.len() == maximum_entries {
                return Err(StorageError::Corrupt);
            }
            entries.push(entry?);
        }
        entries.sort_by_key(|entry| entry.file_name());
        let mut records = Vec::with_capacity(kind.max_records());
        let mut archive_total = 0_usize;
        let mut metadata_total = 0_usize;
        for entry in entries {
            let name = utf8_name(&entry.file_name())?;
            if name.starts_with(kind.staging_prefix()) {
                if !is_owned_generated_staging(&name, kind) {
                    return Err(StorageError::Corrupt);
                }
                let file_type = entry.file_type()?;
                if file_type.is_symlink() || !file_type.is_dir() {
                    return Err(StorageError::Corrupt);
                }
                validate_real_directory(&entry.path())?;
                remove_internal_tree_if_present(&entry.path())?;
                sync_directory(&collection)?;
                continue;
            }
            if name.starts_with(kind.deleting_prefix()) {
                if !is_owned_generated_deleting(&name, kind) {
                    return Err(StorageError::Corrupt);
                }
                let file_type = entry.file_type()?;
                if file_type.is_symlink() || !file_type.is_dir() {
                    return Err(StorageError::Corrupt);
                }
                validate_real_directory(&entry.path())?;
                remove_internal_tree_if_present(&entry.path())?;
                sync_directory(&collection)?;
                continue;
            }
            if records.len() == kind.max_records() {
                return Err(StorageError::Corrupt);
            }
            let file_type = entry.file_type()?;
            if file_type.is_symlink() || !file_type.is_dir() {
                return Err(StorageError::Corrupt);
            }
            validate_identifier(&name, kind.id_prefix())?;
            let record = read_generated_artifact(
                &project_root,
                project_id,
                &entry.path(),
                &name,
                kind,
                MAX_GENERATED_ARCHIVE_BYTES - archive_total,
                MAX_GENERATED_COLLECTION_METADATA_BYTES - metadata_total,
            )?;
            archive_total = archive_total
                .checked_add(record.archive_zip.len())
                .ok_or(StorageError::Corrupt)?;
            metadata_total = metadata_total
                .checked_add(record.metadata_json.len())
                .ok_or(StorageError::Corrupt)?;
            records.push(record);
        }
        Ok(records)
    }

    fn load_target_bytes(&self, project_id: &str) -> Result<Vec<(String, Vec<u8>)>, StorageError> {
        let targets_root = self.project_root(project_id).join("targets");
        enumerate_target_files(&targets_root)?
            .into_iter()
            .map(|(target_id, path)| {
                let bytes = read_regular_file_bounded(
                    &path,
                    MAX_TARGET_METADATA_BYTES,
                    StorageError::Corrupt,
                )?;
                Ok((target_id, bytes))
            })
            .collect()
    }

    fn recover_project(&self, project_root: &Path) -> Result<(), StorageError> {
        // A process may stop after an atomic temporary was made durable but
        // before it was renamed. Recover only the two exact, process-owned
        // pointer names and exact retained-revision immutable temporaries.
        // Lookalikes remain untouched and make the later inventory validation
        // fail closed.
        cleanup_exact_replace_temporary(project_root, "transaction.json")?;
        cleanup_exact_replace_temporary(project_root, "current.json")?;
        cleanup_exact_replace_temporary(project_root, "project.json")?;
        cleanup_orphan_revision_temporaries(&project_root.join("revisions"))?;
        let transaction_path = project_root.join("transaction.json");
        if !transaction_path.exists() {
            return Ok(());
        }
        let marker: TransactionMarker = read_json(&transaction_path)?;
        if marker.schema_version != STORAGE_SCHEMA {
            return Err(StorageError::Corrupt);
        }
        let target_pointer = CurrentPointer {
            schema_version: STORAGE_SCHEMA.into(),
            revision_id: marker.target_revision_id.clone(),
            canonical_manifest_sha256: marker.canonical_manifest_sha256.clone(),
            artifact_manifest_sha256: marker.artifact_manifest_sha256.clone(),
        };
        validate_pointer(&target_pointer)?;
        let current_path = project_root.join("current.json");
        let current: Option<CurrentPointer> = match fs::symlink_metadata(&current_path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(StorageError::Corrupt);
            }
            Ok(_) => Some(read_json(&current_path)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let backup = project_root.join("site.backup");
        let site = project_root.join("site");
        if current.as_ref() == Some(&target_pointer) {
            let pointer = current.ok_or(StorageError::Corrupt)?;
            validate_pointer(&pointer)?;
            let manifest = self.read_manifest_from_root(project_root, &pointer)?;
            if scan_materialized(&site).ok().as_ref() != Some(&manifest.files) {
                self.materialize_to(&project_root.join("site.staging"), &manifest)?;
                remove_internal_tree_if_present(&site)?;
                rename_internal(&project_root.join("site.staging"), &site)?;
            }
        } else if current
            .as_ref()
            .is_some_and(|pointer| pointer.revision_id == marker.target_revision_id)
        {
            return Err(StorageError::Corrupt);
        } else if current.is_none() {
            let manifest = self.read_manifest_from_root(project_root, &target_pointer)?;
            if scan_materialized(&site).ok().as_ref() != Some(&manifest.files) {
                self.materialize_to(&project_root.join("site.staging"), &manifest)?;
                remove_internal_tree_if_present(&site)?;
                rename_internal(&project_root.join("site.staging"), &site)?;
            }
            write_atomic_json(&project_root.join("current.json"), &target_pointer)?;
        } else if backup.exists() {
            remove_internal_tree_if_present(&site)?;
            rename_internal(&backup, &site)?;
        } else if let Some(pointer) = current {
            validate_pointer(&pointer)?;
            let manifest = self.read_manifest_from_root(project_root, &pointer)?;
            self.materialize_to(&project_root.join("site.staging"), &manifest)?;
            remove_internal_tree_if_present(&site)?;
            rename_internal(&project_root.join("site.staging"), &site)?;
        }
        remove_internal_tree_if_present(&backup)?;
        remove_internal_tree_if_present(&project_root.join("site.staging"))?;
        remove_internal_file_if_present(&transaction_path)?;
        sync_directory(project_root)?;
        Ok(())
    }

    fn read_manifest(
        &self,
        project_id: &str,
        pointer: &CurrentPointer,
    ) -> Result<RevisionManifest, StorageError> {
        self.read_manifest_from_root(&self.project_root(project_id), pointer)
    }

    fn read_manifest_from_root(
        &self,
        project_root: &Path,
        pointer: &CurrentPointer,
    ) -> Result<RevisionManifest, StorageError> {
        let path = project_root
            .join("revisions")
            .join(format!("{}.json", pointer.revision_id));
        let bytes = read_regular_file_bounded(&path, 4 * 1024 * 1024, StorageError::Corrupt)?;
        if sha256(&bytes) != pointer.canonical_manifest_sha256 {
            return Err(StorageError::Corrupt);
        }
        let manifest: RevisionManifest =
            serde_json::from_slice(&bytes).map_err(|_| StorageError::Corrupt)?;
        if manifest.schema_version != STORAGE_SCHEMA
            || manifest.revision_id != pointer.revision_id
            || manifest.artifact_manifest_sha256 != pointer.artifact_manifest_sha256
            || validate_sha256(&manifest.artifact_manifest_sha256).is_err()
            || canonical_json(&manifest)? != bytes
            || validate_stored_files(&manifest.files).is_err()
        {
            return Err(StorageError::Corrupt);
        }
        Ok(manifest)
    }

    fn load_manifest_files(
        &self,
        manifest: &RevisionManifest,
    ) -> Result<BTreeMap<String, Vec<u8>>, StorageError> {
        let mut files = BTreeMap::new();
        for entry in &manifest.files {
            let bytes = read_regular_file_exact(
                &self.object_path(&entry.sha256),
                entry.byte_length,
                StorageError::Corrupt,
            )?;
            if bytes.len() != entry.byte_length || sha256(&bytes) != entry.sha256 {
                return Err(StorageError::Corrupt);
            }
            files.insert(entry.path.clone(), bytes);
        }
        Ok(files)
    }

    fn materialize_to(
        &self,
        target: &Path,
        manifest: &RevisionManifest,
    ) -> Result<(), StorageError> {
        remove_internal_tree_if_present(target)?;
        fs::create_dir(target)?;
        for entry in &manifest.files {
            let destination = target.join(path_from_canonical(&entry.path)?);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let bytes = read_regular_file_exact(
                &self.object_path(&entry.sha256),
                entry.byte_length,
                StorageError::Corrupt,
            )?;
            if bytes.len() != entry.byte_length || sha256(&bytes) != entry.sha256 {
                return Err(StorageError::Corrupt);
            }
            write_new_file(&destination, &bytes)?;
        }
        sync_directory(target)?;
        Ok(())
    }

    fn write_object(&self, digest: &str, bytes: &[u8]) -> Result<(), StorageError> {
        validate_sha256(digest)?;
        if sha256(bytes) != digest {
            return Err(StorageError::Corrupt);
        }
        let path = self.object_path(digest);
        if let Some(parent) = path.parent() {
            ensure_real_directory(parent)?;
        }
        write_immutable(&path, bytes)
    }

    fn object_path(&self, digest: &str) -> PathBuf {
        self.root.join("objects").join(&digest[..2]).join(digest)
    }

    fn project_root(&self, project_id: &str) -> PathBuf {
        self.root.join("projects").join(project_id)
    }
}

impl RecoveryReader {
    pub fn list_points(&self) -> Result<Vec<RecoveryPointSummary>, StorageError> {
        let mut points = Vec::new();
        let mut point_ids = BTreeSet::new();
        let backups_root = self.root.join(RECOVERY_BACKUPS_DIRECTORY);
        let mut retained_backups = 0_usize;
        for entry in bounded_directory_entries(&backups_root, MAX_RECOVERY_POINTS)? {
            let raw_name = entry.file_name();
            let name = raw_name.to_str();
            let metadata = entry.file_type()?;
            let summary = if let Some(name) = name
                && validate_identifier(name, "bkp_").is_ok()
            {
                retained_backups = retained_backups
                    .checked_add(1)
                    .ok_or(StorageError::Corrupt)?;
                if retained_backups > MAX_RECOVERY_BACKUPS {
                    return Err(StorageError::Corrupt);
                }
                if metadata.is_symlink() || !metadata.is_dir() {
                    unverified_recovery_point(
                        RecoveryPointKind::VersionedBackup,
                        name.to_owned(),
                        String::new(),
                        "backup_unsafe_entry",
                    )
                } else {
                    match verified_backup_summary(&entry.path()) {
                        Ok(summary) => summary,
                        Err(_) => unverified_recovery_point(
                            RecoveryPointKind::VersionedBackup,
                            name.to_owned(),
                            String::new(),
                            "backup_corrupt",
                        ),
                    }
                }
            } else {
                let interrupted = name.is_some_and(|name| {
                    is_owned_backup_staging(name) || backup_deleting_staging_parts(name).is_some()
                });
                unverified_recovery_point(
                    RecoveryPointKind::VersionedBackup,
                    recovery_unverified_id("bkp_", b"backup-entry", &raw_name),
                    String::new(),
                    if interrupted {
                        "backup_interrupted"
                    } else if metadata.is_symlink() {
                        "backup_unsafe_entry"
                    } else {
                        "backup_unknown_entry"
                    },
                )
            };
            push_recovery_point(&mut points, &mut point_ids, summary)?;
        }

        let projects_root = self.root.join("projects");
        let mut retained_projects = 0_usize;
        for entry in bounded_directory_entries(&projects_root, MAX_RECOVERY_POINTS)? {
            let raw_name = entry.file_name();
            let name = raw_name.to_str();
            let metadata = entry.file_type()?;
            let summary = if let Some(name) = name
                && validate_identifier(name, "prj_").is_ok()
            {
                retained_projects = retained_projects
                    .checked_add(1)
                    .ok_or(StorageError::Corrupt)?;
                if retained_projects > MAX_PERSISTED_PROJECTS {
                    return Err(StorageError::Corrupt);
                }
                if metadata.is_symlink() || !metadata.is_dir() {
                    unverified_recovery_point(
                        RecoveryPointKind::LastAccepted,
                        accepted_recovery_id(name, None),
                        name.to_owned(),
                        "project_unsafe_entry",
                    )
                } else {
                    match active_recovery_summary(&self.root, &entry.path(), name) {
                        Ok(summary) => summary,
                        Err(_) => unverified_recovery_point(
                            RecoveryPointKind::LastAccepted,
                            accepted_recovery_id(name, None),
                            name.to_owned(),
                            "last_accepted_unverified",
                        ),
                    }
                }
            } else {
                let interrupted = name.is_some_and(|name| {
                    is_owned_creation_staging(name)
                        || is_owned_project_migration_staging(name)
                        || is_owned_project_deleting_staging(name)
                        || is_owned_migration_old(name)
                });
                unverified_recovery_point(
                    RecoveryPointKind::LastAccepted,
                    recovery_unverified_id("acc_", b"project-entry", &raw_name),
                    String::new(),
                    if interrupted {
                        "project_interrupted"
                    } else if metadata.is_symlink() {
                        "project_unsafe_entry"
                    } else {
                        "project_unknown_entry"
                    },
                )
            };
            push_recovery_point(&mut points, &mut point_ids, summary)?;
        }
        points.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(points)
    }

    pub fn open_snapshot(&self, id: &str) -> Result<RecoverySnapshot, StorageError> {
        let point = self
            .list_points()?
            .into_iter()
            .find(|point| point.id == id)
            .ok_or(StorageError::Corrupt)?;
        if !point.diagnostic.verified {
            return Err(StorageError::Corrupt);
        }
        let files = match point.kind {
            RecoveryPointKind::VersionedBackup => {
                validate_identifier(id, "bkp_")?;
                let backup = self.root.join(RECOVERY_BACKUPS_DIRECTORY).join(id);
                verify_backup_root(&backup, None)?;
                read_last_accepted_from_project_tree(&backup.join("project"))?
            }
            RecoveryPointKind::LastAccepted => {
                let project_root = self.root.join("projects").join(&point.project_id);
                let pointer: CurrentPointer =
                    read_canonical_json_bounded(&project_root.join("current.json"), 64 * 1024)?;
                validate_pointer(&pointer)?;
                let storage = ManagedStorage {
                    root: self.root.clone(),
                };
                let manifest = storage.read_manifest_from_root(&project_root, &pointer)?;
                storage.load_manifest_files(&manifest)?
            }
        };
        Ok(RecoverySnapshot { point, files })
    }
}

fn push_recovery_point(
    points: &mut Vec<RecoveryPointSummary>,
    point_ids: &mut BTreeSet<String>,
    point: RecoveryPointSummary,
) -> Result<(), StorageError> {
    if points.len() == MAX_RECOVERY_POINTS || !point_ids.insert(point.id.clone()) {
        return Err(StorageError::Corrupt);
    }
    points.push(point);
    Ok(())
}

fn unverified_recovery_point(
    kind: RecoveryPointKind,
    id: String,
    project_id: String,
    code: &'static str,
) -> RecoveryPointSummary {
    RecoveryPointSummary {
        id,
        kind,
        project_id,
        revision_id: None,
        artifact_manifest_sha256: None,
        diagnostic: RecoveryDiagnostic {
            verified: false,
            code,
            manifest_sha256: None,
            file_count: 0,
            total_bytes: 0,
        },
    }
}

fn recovery_unverified_id(
    prefix: &str,
    source_kind: &[u8],
    source_name: &std::ffi::OsStr,
) -> String {
    let mut fingerprint = Vec::with_capacity(source_kind.len() + 1 + source_name.len());
    fingerprint.extend_from_slice(source_kind);
    fingerprint.push(0);
    fingerprint.extend_from_slice(&recovery_os_name_bytes(source_name));
    format!("{prefix}{}", &sha256(&fingerprint)[..32])
}

#[cfg(unix)]
fn recovery_os_name_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;
    name.as_bytes().to_vec()
}

#[cfg(not(unix))]
fn recovery_os_name_bytes(name: &std::ffi::OsStr) -> Vec<u8> {
    name.to_string_lossy().as_bytes().to_vec()
}

fn verified_backup_summary(backup: &Path) -> Result<RecoveryPointSummary, StorageError> {
    let manifest = verify_backup_root(backup, None)?;
    let recoverable_files = read_last_accepted_from_project_tree(&backup.join("project"))?;
    let manifest_bytes = read_regular_file_bounded(
        &backup.join("manifest.json"),
        8 * 1024 * 1024,
        StorageError::Corrupt,
    )?;
    let total_bytes = recoverable_files.values().try_fold(0_u64, |total, bytes| {
        let byte_length = u64::try_from(bytes.len()).map_err(|_| StorageError::Corrupt)?;
        total.checked_add(byte_length).ok_or(StorageError::Corrupt)
    })?;
    Ok(RecoveryPointSummary {
        id: manifest.backup_id,
        kind: RecoveryPointKind::VersionedBackup,
        project_id: manifest.project_id,
        revision_id: Some(manifest.revision_id),
        artifact_manifest_sha256: Some(manifest.artifact_manifest_sha256),
        diagnostic: RecoveryDiagnostic {
            verified: true,
            code: "backup_verified",
            manifest_sha256: Some(sha256(&manifest_bytes)),
            file_count: recoverable_files.len(),
            total_bytes,
        },
    })
}

fn active_recovery_summary(
    storage_root: &Path,
    project_root: &Path,
    project_id: &str,
) -> Result<RecoveryPointSummary, StorageError> {
    validate_real_directory(project_root)?;
    let pointer_bytes = read_regular_file_bounded(
        &project_root.join("current.json"),
        64 * 1024,
        StorageError::Corrupt,
    )?;
    let pointer: CurrentPointer =
        serde_json::from_slice(&pointer_bytes).map_err(|_| StorageError::Corrupt)?;
    if canonical_json(&pointer)? != pointer_bytes {
        return Err(StorageError::Corrupt);
    }
    validate_pointer(&pointer)?;
    let storage = ManagedStorage {
        root: storage_root.to_path_buf(),
    };
    let manifest = storage.read_manifest_from_root(project_root, &pointer)?;
    storage.load_manifest_files(&manifest)?;
    let total_bytes = manifest.files.iter().try_fold(0_u64, |total, file| {
        total.checked_add(file.byte_length as u64)
    });
    Ok(RecoveryPointSummary {
        id: accepted_recovery_id(project_id, Some(&pointer)),
        kind: RecoveryPointKind::LastAccepted,
        project_id: project_id.to_owned(),
        revision_id: Some(pointer.revision_id),
        artifact_manifest_sha256: Some(pointer.artifact_manifest_sha256),
        diagnostic: RecoveryDiagnostic {
            verified: true,
            code: "last_accepted_verified",
            manifest_sha256: Some(pointer.canonical_manifest_sha256),
            file_count: manifest.files.len(),
            total_bytes: total_bytes.ok_or(StorageError::Corrupt)?,
        },
    })
}

fn accepted_recovery_id(project_id: &str, pointer: Option<&CurrentPointer>) -> String {
    let mut material = project_id.as_bytes().to_vec();
    if let Some(pointer) = pointer {
        material.extend_from_slice(pointer.revision_id.as_bytes());
        material.extend_from_slice(pointer.canonical_manifest_sha256.as_bytes());
    }
    format!("acc_{}", &sha256(&material)[..32])
}

fn read_last_accepted_from_project_tree(
    project_root: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, StorageError> {
    let pointer: CurrentPointer =
        read_canonical_json_bounded(&project_root.join("current.json"), 64 * 1024)?;
    validate_pointer(&pointer)?;
    let manifest_path = project_root
        .join("revisions")
        .join(format!("{}.json", pointer.revision_id));
    let manifest_bytes =
        read_regular_file_bounded(&manifest_path, 4 * 1024 * 1024, StorageError::Corrupt)?;
    if sha256(&manifest_bytes) != pointer.canonical_manifest_sha256 {
        return Err(StorageError::Corrupt);
    }
    let manifest: RevisionManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| StorageError::Corrupt)?;
    if canonical_json(&manifest)? != manifest_bytes
        || manifest.schema_version != STORAGE_SCHEMA
        || manifest.revision_id != pointer.revision_id
        || manifest.artifact_manifest_sha256 != pointer.artifact_manifest_sha256
        || validate_stored_files(&manifest.files).is_err()
    {
        return Err(StorageError::Corrupt);
    }
    let files = scan_materialized_files(&project_root.join("site"))?;
    if stored_files(&files).map_err(|_| StorageError::Corrupt)? != manifest.files {
        return Err(StorageError::Corrupt);
    }
    Ok(files)
}

fn validate_generated_payload(
    archive_zip: &[u8],
    metadata_json: &[u8],
) -> Result<(), StorageError> {
    if archive_zip.is_empty()
        || archive_zip.len() > MAX_GENERATED_ARCHIVE_BYTES
        || metadata_json.is_empty()
        || metadata_json.len() > MAX_GENERATED_METADATA_BYTES
    {
        return Err(StorageError::Corrupt);
    }
    let metadata = parse_strict(metadata_json).map_err(|_| StorageError::Corrupt)?;
    if metadata.as_object().is_none() {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn read_retained_revision_manifest(
    project_root: &Path,
    revision_id: &str,
) -> Result<RevisionManifest, StorageError> {
    validate_identifier(revision_id, "rev_")?;
    validate_real_directory(project_root)?;
    let revisions_root = project_root.join("revisions");
    validate_real_directory(&revisions_root)?;
    let path = revisions_root.join(format!("{revision_id}.json"));
    let bytes = read_regular_file_bounded(&path, 4 * 1024 * 1024, StorageError::Corrupt)?;
    let manifest: RevisionManifest =
        serde_json::from_slice(&bytes).map_err(|_| StorageError::Corrupt)?;
    if manifest.schema_version != STORAGE_SCHEMA
        || manifest.revision_id != revision_id
        || validate_sha256(&manifest.artifact_manifest_sha256).is_err()
        || validate_stored_files(&manifest.files).is_err()
        || canonical_json(&manifest)? != bytes
    {
        return Err(StorageError::Corrupt);
    }
    Ok(manifest)
}

fn read_generated_artifact(
    project_root: &Path,
    project_id: &str,
    artifact_root: &Path,
    artifact_id: &str,
    kind: GeneratedArtifactKind,
    remaining_archive_bytes: usize,
    remaining_metadata_bytes: usize,
) -> Result<LoadedGeneratedArtifact, StorageError> {
    validate_real_directory(artifact_root)?;
    let mut inventory = BTreeSet::new();
    for entry in fs::read_dir(artifact_root)? {
        if inventory.len() == 3 {
            return Err(StorageError::Corrupt);
        }
        let entry = entry?;
        inventory.insert(utf8_name(&entry.file_name())?);
    }
    if inventory
        != BTreeSet::from([
            "archive.zip".to_owned(),
            "manifest.json".to_owned(),
            "metadata.json".to_owned(),
        ])
    {
        return Err(StorageError::Corrupt);
    }

    let manifest_bytes = read_regular_file_bounded(
        &artifact_root.join("manifest.json"),
        MAX_GENERATED_MANIFEST_BYTES,
        StorageError::Corrupt,
    )?;
    let manifest: GeneratedArtifactManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| StorageError::Corrupt)?;
    if canonical_json(&manifest)? != manifest_bytes
        || manifest.schema_version != STORAGE_SCHEMA
        || manifest.kind != kind
        || manifest.project_id != project_id
        || manifest.artifact_id != artifact_id
        || validate_identifier(&manifest.project_id, "prj_").is_err()
        || validate_identifier(&manifest.artifact_id, kind.id_prefix()).is_err()
        || validate_identifier(&manifest.revision_id, "rev_").is_err()
        || validate_sha256(&manifest.archive_sha256).is_err()
        || validate_sha256(&manifest.metadata_sha256).is_err()
        || manifest.archive_byte_length == 0
        || manifest.archive_byte_length > MAX_GENERATED_ARCHIVE_BYTES
        || manifest.archive_byte_length > remaining_archive_bytes
        || manifest.metadata_byte_length == 0
        || manifest.metadata_byte_length > MAX_GENERATED_METADATA_BYTES
        || manifest.metadata_byte_length > remaining_metadata_bytes
    {
        return Err(StorageError::Corrupt);
    }
    read_retained_revision_manifest(project_root, &manifest.revision_id)?;
    let archive_zip = read_regular_file_exact_with_limit(
        &artifact_root.join("archive.zip"),
        manifest.archive_byte_length,
        MAX_GENERATED_ARCHIVE_BYTES,
        StorageError::Corrupt,
    )?;
    let metadata_json = read_regular_file_exact_with_limit(
        &artifact_root.join("metadata.json"),
        manifest.metadata_byte_length,
        MAX_GENERATED_METADATA_BYTES,
        StorageError::Corrupt,
    )?;
    validate_generated_payload(&archive_zip, &metadata_json)?;
    if sha256(&archive_zip) != manifest.archive_sha256
        || sha256(&metadata_json) != manifest.metadata_sha256
    {
        return Err(StorageError::Corrupt);
    }
    Ok(LoadedGeneratedArtifact {
        manifest,
        archive_zip,
        metadata_json,
    })
}

fn is_owned_generated_staging(value: &str, kind: GeneratedArtifactKind) -> bool {
    let Some(rest) = value.strip_prefix(kind.staging_prefix()) else {
        return false;
    };
    let Some((artifact_id, nonce)) = rest.split_once('.') else {
        return false;
    };
    validate_identifier(artifact_id, kind.id_prefix()).is_ok()
        && nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_owned_generated_deleting(value: &str, kind: GeneratedArtifactKind) -> bool {
    let Some(rest) = value.strip_prefix(kind.deleting_prefix()) else {
        return false;
    };
    let Some((artifact_id, nonce)) = rest.split_once('.') else {
        return false;
    };
    validate_identifier(artifact_id, kind.id_prefix()).is_ok() && validate_nonce(nonce).is_ok()
}

pub(super) fn scan_import(source_root: &Path) -> Result<ImportScan, StorageError> {
    validate_real_directory(source_root).map_err(|_| StorageError::UnsafeImport)?;
    let display_name = source_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Imported landing page")
        .nfc()
        .collect::<String>();
    let mut walk = ImportWalk::default();
    scan_import_directory(source_root, source_root, 0, &mut walk)?;
    let ImportWalk {
        files,
        mut excluded,
        identities: _,
        folded_paths: _,
        budget: _,
    } = walk;
    let stored = stored_files(&files).map_err(|error| match error {
        StorageError::ImportLimit => StorageError::ImportLimit,
        _ => StorageError::UnsafeImport,
    })?;
    let included = stored
        .iter()
        .map(|entry| ImportIncluded {
            path: entry.path.clone(),
            byte_length: entry.byte_length,
            sha256: entry.sha256.clone(),
        })
        .collect::<Vec<_>>();
    let total_bytes = stored.iter().map(|file| file.byte_length).sum();
    let entry_point = if files.contains_key("index.html") {
        Some("index.html".into())
    } else {
        files
            .keys()
            .find(|path| path.ends_with(".html") || path.ends_with(".htm"))
            .cloned()
    };
    excluded.sort_by(|left, right| left.path.cmp(&right.path));
    let mut warnings = Vec::new();
    if entry_point.as_deref() != Some("index.html") {
        warnings.push("root_index_html_missing".into());
    }
    if files.is_empty() {
        warnings.push("no_importable_files".into());
    }
    let manifest_sha256 = sha256(&canonical_json(&ReviewedImportProjection {
        schema_version: STORAGE_SCHEMA,
        display_name: &display_name,
        entry_point: &entry_point,
        included: &included,
        excluded: &excluded,
        warnings: &warnings,
    })?);
    Ok(ImportScan {
        display_name,
        manifest_sha256,
        total_bytes,
        entry_point,
        included,
        excluded,
        warnings,
        files,
    })
}

fn scan_import_directory(
    source_root: &Path,
    directory: &Path,
    depth: usize,
    walk: &mut ImportWalk,
) -> Result<(), StorageError> {
    if depth > MAX_DEPTH {
        return Err(StorageError::ImportLimit);
    }
    let directory_before =
        fs::symlink_metadata(directory).map_err(|_| StorageError::UnsafeImport)?;
    if directory_before.file_type().is_symlink() || !directory_before.is_dir() {
        return Err(StorageError::UnsafeImport);
    }
    let directory_identity = file_identity(&directory_before, directory)?;
    let entries = fs::read_dir(directory)?;
    for entry in entries {
        walk.budget.visit_import_entry()?;
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(StorageError::UnsafeImport);
        }
        let relative = entry
            .path()
            .strip_prefix(source_root)
            .map_err(|_| StorageError::UnsafeImport)?
            .to_path_buf();
        let canonical_path = canonical_relative_path(&relative)?;
        if let Some(reason) = excluded_reason(&canonical_path) {
            walk.excluded.push(ImportExcluded {
                path: canonical_path,
                reason,
            });
            continue;
        }
        let folded = UniCase::new(canonical_path.clone()).to_folded_case();
        if !walk.folded_paths.insert(folded) {
            return Err(StorageError::UnsafeImport);
        }
        if metadata.is_dir() {
            scan_import_directory(
                source_root,
                &entry.path(),
                depth.checked_add(1).ok_or(StorageError::ImportLimit)?,
                walk,
            )?;
            continue;
        }
        if !metadata.is_file() || hard_link_count(&metadata) > 1 {
            return Err(StorageError::UnsafeImport);
        }
        if !walk
            .identities
            .insert(file_identity(&metadata, &entry.path())?)
        {
            return Err(StorageError::UnsafeImport);
        }
        let byte_length = usize::try_from(metadata.len()).map_err(|_| StorageError::ImportLimit)?;
        if byte_length > MAX_FILE_BYTES || walk.files.len() >= MAX_FILES {
            return Err(StorageError::ImportLimit);
        }
        walk.budget.add_import_file(byte_length)?;
        let before_identity = file_identity(&metadata, &entry.path())?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options
            .open(entry.path())
            .map_err(|_| StorageError::UnsafeImport)?;
        let opened_metadata = file.metadata().map_err(|_| StorageError::UnsafeImport)?;
        if !opened_metadata.is_file()
            || hard_link_count(&opened_metadata) > 1
            || file_identity(&opened_metadata, &entry.path())? != before_identity
            || opened_metadata.len() != metadata.len()
        {
            return Err(StorageError::UnsafeImport);
        }
        let mut bytes = Vec::with_capacity(byte_length);
        file.take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        let after_metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| StorageError::UnsafeImport)?;
        if after_metadata.file_type().is_symlink()
            || !after_metadata.is_file()
            || hard_link_count(&after_metadata) > 1
            || file_identity(&after_metadata, &entry.path())? != before_identity
            || after_metadata.len() != metadata.len()
            || bytes.len() != byte_length
            || bytes.len() > MAX_FILE_BYTES
        {
            return Err(StorageError::UnsafeImport);
        }
        walk.files.insert(canonical_path, bytes);
    }
    let directory_after =
        fs::symlink_metadata(directory).map_err(|_| StorageError::UnsafeImport)?;
    if directory_after.file_type().is_symlink()
        || !directory_after.is_dir()
        || file_identity(&directory_after, directory)? != directory_identity
    {
        return Err(StorageError::UnsafeImport);
    }
    Ok(())
}

fn scan_materialized(root: &Path) -> Result<Vec<StoredFile>, StorageError> {
    let files = scan_materialized_files(root)?;
    stored_files(&files).map_err(|_| StorageError::Drift)
}

fn scan_materialized_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, StorageError> {
    validate_real_directory(root).map_err(|_| StorageError::Drift)?;
    let mut files = BTreeMap::new();
    let mut budget = ScanBudget::default();
    scan_materialized_directory(root, root, 0, &mut files, &mut budget)?;
    stored_files(&files).map_err(|_| StorageError::Drift)?;
    Ok(files)
}

fn scan_materialized_directory(
    root: &Path,
    directory: &Path,
    depth: usize,
    files: &mut BTreeMap<String, Vec<u8>>,
    budget: &mut ScanBudget,
) -> Result<(), StorageError> {
    if depth > MAX_DEPTH {
        return Err(StorageError::Drift);
    }
    let directory_before = fs::symlink_metadata(directory).map_err(|_| StorageError::Drift)?;
    if directory_before.file_type().is_symlink() || !directory_before.is_dir() {
        return Err(StorageError::Drift);
    }
    let directory_identity =
        file_identity(&directory_before, directory).map_err(|_| StorageError::Drift)?;
    let entries = fs::read_dir(directory).map_err(|_| StorageError::Drift)?;
    for entry in entries {
        budget.visit_materialized_entry()?;
        let entry = entry.map_err(|_| StorageError::Drift)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| StorageError::Drift)?;
        if metadata.file_type().is_symlink() {
            return Err(StorageError::Drift);
        }
        if metadata.is_dir() {
            scan_materialized_directory(
                root,
                &entry.path(),
                depth.checked_add(1).ok_or(StorageError::Drift)?,
                files,
                budget,
            )?;
        } else if metadata.is_file() && hard_link_count(&metadata) == 1 {
            let byte_length = usize::try_from(metadata.len()).map_err(|_| StorageError::Drift)?;
            if files.len() >= MAX_FILES || byte_length > MAX_FILE_BYTES {
                return Err(StorageError::Drift);
            }
            budget.add_materialized_file(byte_length)?;
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| StorageError::Drift)?
                .to_path_buf();
            let path = canonical_relative_path(&relative).map_err(|_| StorageError::Drift)?;
            let bytes = read_regular_file_exact(&entry.path(), byte_length, StorageError::Drift)?;
            if files.insert(path, bytes).is_some() {
                return Err(StorageError::Drift);
            }
        } else {
            return Err(StorageError::Drift);
        }
    }
    let directory_after = fs::symlink_metadata(directory).map_err(|_| StorageError::Drift)?;
    if directory_after.file_type().is_symlink()
        || !directory_after.is_dir()
        || file_identity(&directory_after, directory).map_err(|_| StorageError::Drift)?
            != directory_identity
    {
        return Err(StorageError::Drift);
    }
    Ok(())
}

fn stored_files(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<StoredFile>, StorageError> {
    if files.len() > MAX_FILES {
        return Err(StorageError::ImportLimit);
    }
    let mut total = 0usize;
    let mut stored = Vec::with_capacity(files.len());
    let mut folded_paths = BTreeSet::new();
    for (path, bytes) in files {
        validate_canonical_path(path)?;
        if !folded_paths.insert(UniCase::new(path.clone()).to_folded_case()) {
            return Err(StorageError::UnsafeImport);
        }
        if bytes.len() > MAX_FILE_BYTES {
            return Err(StorageError::ImportLimit);
        }
        total = total
            .checked_add(bytes.len())
            .ok_or(StorageError::ImportLimit)?;
        if total > MAX_TOTAL_BYTES {
            return Err(StorageError::ImportLimit);
        }
        stored.push(StoredFile {
            path: path.clone(),
            media_type: media_type(path),
            byte_length: bytes.len(),
            sha256: sha256(bytes),
        });
    }
    Ok(stored)
}

fn validate_stored_files(files: &[StoredFile]) -> Result<(), StorageError> {
    if files.len() > MAX_FILES {
        return Err(StorageError::Corrupt);
    }
    let mut previous = None;
    let mut total = 0usize;
    let mut folded = BTreeSet::new();
    for file in files {
        validate_canonical_path(&file.path).map_err(|_| StorageError::Corrupt)?;
        validate_sha256(&file.sha256)?;
        if file.media_type != media_type(&file.path)
            || file.byte_length > MAX_FILE_BYTES
            || previous.is_some_and(|path: &str| path >= file.path.as_str())
            || !folded.insert(UniCase::new(file.path.clone()).to_folded_case())
        {
            return Err(StorageError::Corrupt);
        }
        total = total
            .checked_add(file.byte_length)
            .ok_or(StorageError::Corrupt)?;
        if total > MAX_TOTAL_BYTES {
            return Err(StorageError::Corrupt);
        }
        previous = Some(file.path.as_str());
    }
    Ok(())
}

fn canonical_relative_path(path: &Path) -> Result<String, StorageError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(StorageError::UnsafeImport);
        };
        let part = part.to_str().ok_or(StorageError::UnsafeImport)?;
        parts.push(part.nfc().collect::<String>());
    }
    let canonical = parts.join("/");
    validate_canonical_path(&canonical)?;
    Ok(canonical)
}

pub(super) fn validate_project_display_name(value: &str) -> Result<(), StorageError> {
    let characters = value.chars().collect::<Vec<_>>();
    if value.len() > MAX_PROJECT_DISPLAY_NAME_BYTES
        || characters.is_empty()
        || characters.len() > MAX_PROJECT_DISPLAY_NAME_CODE_POINTS
        || value.trim().is_empty()
        || value.nfc().collect::<String>() != value
        || characters.iter().any(|character| character.is_control())
        || contains_absolute_path(&characters)
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn contains_absolute_path(characters: &[char]) -> bool {
    let boundary =
        |character: char| character.is_whitespace() || matches!(character, '(' | '"' | '\'' | '=');
    for (index, character) in characters.iter().copied().enumerate() {
        if character == '/'
            && (index == 0 || boundary(characters[index - 1]))
            && characters.get(index + 1).copied() != Some('/')
        {
            return true;
        }
        if character.is_ascii_alphabetic()
            && characters.get(index + 1).copied() == Some(':')
            && characters
                .get(index + 2)
                .is_some_and(|next| matches!(next, '/' | '\\'))
        {
            return true;
        }
        if character == '\\'
            && characters.get(index + 1).copied() == Some('\\')
            && (index == 0 || boundary(characters[index - 1]))
        {
            return true;
        }
    }
    false
}

pub(super) fn validate_canonical_path(path: &str) -> Result<(), StorageError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
        || path.nfc().collect::<String>() != path
    {
        return Err(StorageError::UnsafeImport);
    }
    let segments = path.split('/').collect::<Vec<_>>();
    if segments.len() > MAX_DEPTH {
        return Err(StorageError::ImportLimit);
    }
    for segment in segments {
        if segment.is_empty()
            || matches!(segment, "." | "..")
            || segment.ends_with([' ', '.'])
            || segment
                .chars()
                .any(|character| character.is_control() || "<>:\"|?*".contains(character))
            || windows_reserved(segment)
        {
            return Err(StorageError::UnsafeImport);
        }
    }
    Ok(())
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

fn excluded_reason(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    let segments = lower.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| matches!(*segment, ".git" | ".studio" | ".ssh" | ".aws" | "__macosx"))
    {
        return Some("reserved_metadata");
    }
    let name = segments.last().copied().unwrap_or_default();
    if name == ".ds_store" || name == "thumbs.db" || name == "desktop.ini" {
        return Some("os_metadata");
    }
    if name == ".env"
        || name.starts_with(".env.")
        || matches!(name, ".envrc" | ".netrc" | ".npmrc" | ".pypirc")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || matches!(
            name,
            "id_rsa" | "id_ed25519" | "credentials" | "credentials.json"
        )
    {
        return Some("credential_material");
    }
    None
}

fn path_from_canonical(path: &str) -> Result<PathBuf, StorageError> {
    validate_canonical_path(path)?;
    Ok(path.split('/').collect())
}

fn validate_identifier(value: &str, prefix: &str) -> Result<(), StorageError> {
    let suffix = value.strip_prefix(prefix).ok_or(StorageError::Corrupt)?;
    if suffix.len() != 32
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn validate_nonce(value: &str) -> Result<(), StorageError> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(StorageError::Corrupt)
    }
}

fn bounded_directory_entries(
    directory: &Path,
    maximum_entries: usize,
) -> Result<Vec<fs::DirEntry>, StorageError> {
    validate_real_directory(directory)?;
    let mut entries = Vec::with_capacity(maximum_entries);
    for entry in fs::read_dir(directory)? {
        if entries.len() == maximum_entries {
            return Err(StorageError::Corrupt);
        }
        entries.push(entry?);
    }
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

fn path_exists_strict(path: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StorageError::Corrupt),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn path_is_real_directory_if_present(path: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(StorageError::Corrupt)
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn path_is_regular_file_if_present(path: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || hard_link_count(&metadata) != 1 =>
        {
            Err(StorageError::Corrupt)
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn is_owned_creation_staging(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(".creating-") else {
        return false;
    };
    let Some((project_id, nonce)) = rest.split_once('.') else {
        return false;
    };
    validate_identifier(project_id, "prj_").is_ok()
        && nonce.len() == 32
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_owned_project_migration_staging(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(".migrating-") else {
        return false;
    };
    let Some((project_id, nonce)) = rest.split_once('.') else {
        return false;
    };
    validate_identifier(project_id, "prj_").is_ok() && validate_nonce(nonce).is_ok()
}

fn is_owned_migration_old(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(".migration-old-") else {
        return false;
    };
    let Some((project_id, nonce)) = rest.split_once('.') else {
        return false;
    };
    validate_identifier(project_id, "prj_").is_ok() && validate_nonce(nonce).is_ok()
}

fn is_owned_project_deleting_staging(value: &str) -> bool {
    project_deleting_staging_parts(value).is_some()
}

fn project_deleting_staging_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix(".deleting-project-")?;
    let (project_id, nonce) = rest.split_once('.')?;
    if validate_identifier(project_id, "prj_").is_ok() && validate_nonce(nonce).is_ok() {
        Some((project_id, nonce))
    } else {
        None
    }
}

fn backup_deleting_staging_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix(".deleting-backup-")?;
    let (backup_id, nonce) = rest.split_once('.')?;
    if validate_identifier(backup_id, "bkp_").is_ok() && validate_nonce(nonce).is_ok() {
        Some((backup_id, nonce))
    } else {
        None
    }
}

fn is_owned_backup_staging(value: &str) -> bool {
    let Some(rest) = value.strip_prefix(".creating-backup-") else {
        return false;
    };
    let Some((backup_id, nonce)) = rest.split_once('.') else {
        return false;
    };
    validate_identifier(backup_id, "bkp_").is_ok()
        && validate_nonce(nonce).is_ok()
        && backup_id.strip_prefix("bkp_") == Some(nonce)
}

fn is_owned_migration_journal_temporary(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('.') else {
        return false;
    };
    let Some(body) = rest.strip_suffix(".immutable.tmp") else {
        return false;
    };
    let Some((project_name, nonce)) = body.rsplit_once('.') else {
        return false;
    };
    let Some(project_id) = project_name.strip_suffix(".json") else {
        return false;
    };
    validate_identifier(project_id, "prj_").is_ok() && validate_nonce(nonce).is_ok()
}

fn proposal_creating_staging_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix(".creating-proposal-")?;
    let (proposal_id, nonce) = rest.split_once('.')?;
    if validate_identifier(proposal_id, "pro_").is_ok() && validate_nonce(nonce).is_ok() {
        Some((proposal_id, nonce))
    } else {
        None
    }
}

fn proposal_deleting_staging_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix(".deleting-proposal-")?;
    let (proposal_id, nonce) = rest.split_once('.')?;
    if validate_identifier(proposal_id, "pro_").is_ok() && validate_nonce(nonce).is_ok() {
        Some((proposal_id, nonce))
    } else {
        None
    }
}

fn recover_proposal_root(root: &Path) -> Result<(), StorageError> {
    const FILES: [&str; 4] = [
        "proposal.json",
        "review.json",
        "decision.json",
        "completion.json",
    ];
    validate_real_directory(root)?;
    let maximum_entries = FILES
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_add(MAX_PROPOSAL_STAGING_ENTRIES))
        .ok_or(StorageError::Corrupt)?;
    let mut entries = Vec::with_capacity(maximum_entries);
    for entry in fs::read_dir(root)? {
        if entries.len() == maximum_entries {
            return Err(StorageError::Corrupt);
        }
        entries.push(entry?);
    }
    let mut removed = false;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let file_type = entry.file_type()?;
        if name == "site" {
            if file_type.is_symlink() || !file_type.is_dir() {
                return Err(StorageError::Corrupt);
            }
            validate_real_directory(&entry.path())?;
        } else if FILES.contains(&name.as_str()) {
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(StorageError::Corrupt);
            }
        } else if is_owned_immutable_temp(&name, &FILES) {
            validate_owned_temporary_file(&entry.path())?;
            fs::remove_file(entry.path())?;
            removed = true;
        } else {
            return Err(StorageError::Corrupt);
        }
    }
    if removed {
        sync_directory(root)?;
    }
    Ok(())
}

fn validate_stable_proposal_inventory(root: &Path) -> Result<(), StorageError> {
    recover_proposal_root(root)?;
    validate_real_directory(&root.join("site"))?;
    if !path_is_regular_file_if_present(&root.join("proposal.json"))? {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn insert_object_binding(
    bindings: &mut ObjectBindings,
    digest: &str,
    byte_length: usize,
) -> Result<(), StorageError> {
    validate_sha256(digest)?;
    if byte_length > MAX_FILE_BYTES {
        return Err(StorageError::Corrupt);
    }
    if bindings
        .insert(digest.to_owned(), byte_length)
        .is_some_and(|existing| existing != byte_length)
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn extend_object_bindings(
    bindings: &mut ObjectBindings,
    files: &[StoredFile],
) -> Result<(), StorageError> {
    validate_stored_files(files)?;
    for file in files {
        insert_object_binding(bindings, &file.sha256, file.byte_length)?;
    }
    Ok(())
}

fn proposal_site_object_bindings(
    proposal_root: &Path,
    require_complete: bool,
) -> Result<ObjectBindings, StorageError> {
    recover_proposal_root(proposal_root)?;
    if require_complete {
        validate_stable_proposal_inventory(proposal_root)?;
    } else {
        for file in [
            "proposal.json",
            "review.json",
            "decision.json",
            "completion.json",
        ] {
            read_optional_bounded_file(&proposal_root.join(file), MAX_PROPOSAL_METADATA_BYTES)?;
        }
    }
    let site = proposal_root.join("site");
    let files = match fs::symlink_metadata(&site) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(StorageError::Corrupt);
        }
        Ok(_) => scan_materialized(&site)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound && !require_complete => Vec::new(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(StorageError::Corrupt);
        }
        Err(error) => return Err(error.into()),
    };
    let mut bindings = ObjectBindings::new();
    extend_object_bindings(&mut bindings, &files)?;
    Ok(bindings)
}

fn collect_revision_object_bindings(
    project_root: &Path,
    bindings: &mut ObjectBindings,
) -> Result<(), StorageError> {
    validate_real_directory(project_root)?;
    let revisions_root = project_root.join("revisions");
    let entries = bounded_directory_entries(&revisions_root, MAX_RETAINED_REVISIONS + 1)?;
    let mut retained = 0_usize;
    for entry in entries {
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        let name = utf8_name(&entry.file_name())?;
        let revision_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(revision_id, "rev_")?;
        retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
        if retained > MAX_RETAINED_REVISIONS {
            return Err(StorageError::Corrupt);
        }
        let manifest = read_retained_revision_manifest(project_root, revision_id)?;
        extend_object_bindings(bindings, &manifest.files)?;
    }
    if retained == 0 {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn collect_active_proposal_object_bindings(
    project_root: &Path,
    bindings: &mut ObjectBindings,
) -> Result<(), StorageError> {
    let proposals_root = project_root.join("proposals");
    let entries = bounded_directory_entries(
        &proposals_root,
        MAX_PERSISTED_PROPOSALS + MAX_PROPOSAL_STAGING_ENTRIES,
    )?;
    let mut retained = 0_usize;
    let mut staging = 0_usize;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(StorageError::Corrupt);
        }
        if proposal_creating_staging_parts(&name).is_some()
            || proposal_deleting_staging_parts(&name).is_some()
        {
            staging = staging.checked_add(1).ok_or(StorageError::Corrupt)?;
            if staging > MAX_PROPOSAL_STAGING_ENTRIES {
                return Err(StorageError::Corrupt);
            }
            continue;
        }
        if name.starts_with(".creating-proposal-") || name.starts_with(".deleting-proposal-") {
            return Err(StorageError::Corrupt);
        }
        validate_identifier(&name, "pro_")?;
        retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
        if retained > MAX_PERSISTED_PROPOSALS {
            return Err(StorageError::Corrupt);
        }
        let proposal = proposal_site_object_bindings(&entry.path(), true)?;
        for (digest, byte_length) in proposal {
            insert_object_binding(bindings, &digest, byte_length)?;
        }
    }
    Ok(())
}

fn collect_all_proposal_object_bindings(
    project_root: &Path,
    bindings: &mut ObjectBindings,
) -> Result<(), StorageError> {
    let proposals_root = project_root.join("proposals");
    let entries = bounded_directory_entries(
        &proposals_root,
        MAX_PERSISTED_PROPOSALS + MAX_PROPOSAL_STAGING_ENTRIES,
    )?;
    let mut retained = 0_usize;
    let mut staging = 0_usize;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(StorageError::Corrupt);
        }
        let require_complete = if proposal_creating_staging_parts(&name).is_some() {
            false
        } else if proposal_deleting_staging_parts(&name).is_some() {
            true
        } else if name.starts_with(".creating-proposal-") || name.starts_with(".deleting-proposal-")
        {
            return Err(StorageError::Corrupt);
        } else {
            validate_identifier(&name, "pro_")?;
            retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
            if retained > MAX_PERSISTED_PROPOSALS {
                return Err(StorageError::Corrupt);
            }
            true
        };
        if name.starts_with('.') {
            staging = staging.checked_add(1).ok_or(StorageError::Corrupt)?;
            if staging > MAX_PROPOSAL_STAGING_ENTRIES {
                return Err(StorageError::Corrupt);
            }
        }
        let proposal = proposal_site_object_bindings(&entry.path(), require_complete)?;
        for (digest, byte_length) in proposal {
            insert_object_binding(bindings, &digest, byte_length)?;
        }
    }
    Ok(())
}

fn is_owned_immutable_temp(value: &str, allowed_files: &[&str]) -> bool {
    allowed_files.iter().any(|file| {
        let prefix = format!(".{file}.");
        value
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".immutable.tmp"))
            .is_some_and(|nonce| {
                nonce.len() == 32
                    && nonce
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
    })
}

fn enumerate_proposal_directories(
    proposals_root: &Path,
) -> Result<Vec<(String, PathBuf)>, StorageError> {
    validate_real_directory(proposals_root)?;
    let maximum_entries = MAX_PERSISTED_PROPOSALS
        .checked_add(MAX_PROPOSAL_STAGING_ENTRIES)
        .ok_or(StorageError::Corrupt)?;
    let mut entries = Vec::with_capacity(maximum_entries);
    for entry in fs::read_dir(proposals_root)? {
        if entries.len() == maximum_entries {
            return Err(StorageError::Corrupt);
        }
        entries.push(entry?);
    }
    entries.sort_by_key(|entry| entry.file_name());

    let mut retained = Vec::with_capacity(MAX_PERSISTED_PROPOSALS);
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() || !file_type.is_dir() {
            return Err(StorageError::Corrupt);
        }
        let path = entry.path();
        validate_real_directory(&path)?;
        if name.starts_with(".creating-proposal-") || name.starts_with(".deleting-proposal-") {
            return Err(StorageError::Corrupt);
        }
        validate_identifier(&name, "pro_")?;
        if retained.len() == MAX_PERSISTED_PROPOSALS {
            return Err(StorageError::Corrupt);
        }
        retained.push((name, path));
    }
    Ok(retained)
}

fn enumerate_target_files(targets_root: &Path) -> Result<Vec<(String, PathBuf)>, StorageError> {
    let entries = bounded_directory_entries(targets_root, MAX_PERSISTED_TARGETS + 1)?;
    let mut targets = Vec::with_capacity(MAX_PERSISTED_TARGETS);
    for entry in entries {
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        let name = utf8_name(&entry.file_name())?;
        let target_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(target_id, "tgt_")?;
        if targets.len() == MAX_PERSISTED_TARGETS {
            return Err(StorageError::Corrupt);
        }
        targets.push((target_id.to_owned(), entry.path()));
    }
    Ok(targets)
}

fn target_replace_staging_parts(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix(".replacing-target-")?;
    let body = rest.strip_suffix(".tmp")?;
    let (target_id, nonce) = body.split_once('.')?;
    if validate_identifier(target_id, "tgt_").is_ok() && validate_nonce(nonce).is_ok() {
        Some((target_id, nonce))
    } else {
        None
    }
}

fn is_owned_target_immutable_temporary(value: &str) -> bool {
    let Some(rest) = value.strip_prefix('.') else {
        return false;
    };
    let Some(body) = rest.strip_suffix(".immutable.tmp") else {
        return false;
    };
    let Some((target_name, nonce)) = body.rsplit_once('.') else {
        return false;
    };
    let Some(target_id) = target_name.strip_suffix(".json") else {
        return false;
    };
    validate_identifier(target_id, "tgt_").is_ok() && validate_nonce(nonce).is_ok()
}

fn cleanup_orphan_target_temporaries(targets_root: &Path) -> Result<(), StorageError> {
    let entries = bounded_directory_entries(
        targets_root,
        MAX_PERSISTED_TARGETS + MAX_PROPOSAL_STAGING_ENTRIES,
    )?;
    let mut changed = false;
    let mut target_count = 0_usize;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        if name.starts_with(".replacing-target-") {
            if target_replace_staging_parts(&name).is_none() {
                return Err(StorageError::Corrupt);
            }
            let metadata = entry.file_type()?;
            if metadata.is_symlink() || !metadata.is_file() {
                return Err(StorageError::Corrupt);
            }
            fs::remove_file(entry.path())?;
            changed = true;
            continue;
        }
        if name.starts_with(".tgt_") {
            if !is_owned_target_immutable_temporary(&name) {
                return Err(StorageError::Corrupt);
            }
            let metadata = entry.file_type()?;
            if metadata.is_symlink() || !metadata.is_file() {
                return Err(StorageError::Corrupt);
            }
            fs::remove_file(entry.path())?;
            changed = true;
            continue;
        }
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        let target_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(target_id, "tgt_")?;
        target_count = target_count.checked_add(1).ok_or(StorageError::Corrupt)?;
        if target_count > MAX_PERSISTED_TARGETS {
            return Err(StorageError::Corrupt);
        }
    }
    if changed {
        sync_directory(targets_root)?;
    }
    Ok(())
}

fn cleanup_exact_atomic_temporary(parent: &Path, file_name: &str) -> Result<(), StorageError> {
    cleanup_exact_named_temporary(parent, file_name, ".immutable.tmp")
}

fn cleanup_exact_replace_temporary(parent: &Path, file_name: &str) -> Result<(), StorageError> {
    cleanup_exact_named_temporary(parent, file_name, ".tmp")
}

fn cleanup_exact_named_temporary(
    parent: &Path,
    file_name: &str,
    suffix: &str,
) -> Result<(), StorageError> {
    let prefix = format!(".{file_name}.");
    let mut changed = false;
    for entry in bounded_directory_entries(parent, 32)? {
        let name = utf8_name(&entry.file_name())?;
        if !name.starts_with(&prefix) {
            continue;
        }
        let Some(nonce) = name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
        else {
            return Err(StorageError::Corrupt);
        };
        if validate_nonce(nonce).is_err() {
            return Err(StorageError::Corrupt);
        }
        validate_owned_temporary_file(&entry.path())?;
        fs::remove_file(entry.path())?;
        changed = true;
    }
    if changed {
        sync_directory(parent)?;
    }
    Ok(())
}

fn cleanup_orphan_revision_temporaries(revisions_root: &Path) -> Result<(), StorageError> {
    validate_real_directory(revisions_root)?;
    let maximum_entries = MAX_RETAINED_REVISIONS
        .checked_add(MAX_PROPOSAL_STAGING_ENTRIES)
        .ok_or(StorageError::Corrupt)?;
    let entries = bounded_directory_entries(revisions_root, maximum_entries)?;
    let mut changed = false;
    let mut stable_count = 0_usize;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        if let Some(body) = name
            .strip_prefix('.')
            .and_then(|rest| rest.strip_suffix(".immutable.tmp"))
        {
            let Some((revision_name, nonce)) = body.rsplit_once('.') else {
                continue;
            };
            let Some(revision_id) = revision_name.strip_suffix(".json") else {
                continue;
            };
            if validate_identifier(revision_id, "rev_").is_err() || validate_nonce(nonce).is_err() {
                continue;
            }
            validate_owned_temporary_file(&entry.path())?;
            fs::remove_file(entry.path())?;
            changed = true;
            continue;
        }
        stable_count = stable_count.checked_add(1).ok_or(StorageError::Corrupt)?;
        if stable_count > MAX_RETAINED_REVISIONS {
            return Err(StorageError::Corrupt);
        }
    }
    if changed {
        sync_directory(revisions_root)?;
    }
    Ok(())
}

fn validate_owned_temporary_file(path: &Path) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || hard_link_count(&metadata) != 1
        || !temporary_permissions_are_private(&metadata)
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

#[cfg(unix)]
fn temporary_permissions_are_private(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn temporary_permissions_are_private(_metadata: &fs::Metadata) -> bool {
    true
}

fn validate_target_transaction_inventory(
    targets_root: &Path,
    journal: &TargetReplaceJournal,
) -> Result<(), StorageError> {
    let entries = bounded_directory_entries(targets_root, MAX_PERSISTED_TARGETS + 2)?;
    let expected_stage = format!(
        ".replacing-target-{}.{}.tmp",
        journal.new_target_id, journal.nonce
    );
    let mut final_count = 0_usize;
    let mut staging_count = 0_usize;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        if name == expected_stage {
            staging_count += 1;
            continue;
        }
        if name.starts_with(".replacing-target-") || name.starts_with(".tgt_") {
            return Err(StorageError::Corrupt);
        }
        let target_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(target_id, "tgt_")?;
        final_count += 1;
        if final_count > MAX_PERSISTED_TARGETS + 1 {
            return Err(StorageError::Corrupt);
        }
    }
    if staging_count > 1 {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn verify_file_digest_and_length(
    path: &Path,
    expected_length: usize,
    expected_sha256: &str,
    maximum_length: usize,
) -> Result<(), StorageError> {
    validate_sha256(expected_sha256)?;
    let bytes = read_regular_file_exact_with_limit(
        path,
        expected_length,
        maximum_length,
        StorageError::Corrupt,
    )?;
    if sha256(&bytes) != expected_sha256 {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn validate_pointer(pointer: &CurrentPointer) -> Result<(), StorageError> {
    if pointer.schema_version != STORAGE_SCHEMA {
        return Err(StorageError::Corrupt);
    }
    validate_identifier(&pointer.revision_id, "rev_")?;
    validate_sha256(&pointer.canonical_manifest_sha256)?;
    validate_sha256(&pointer.artifact_manifest_sha256)
}

fn validate_project_layout_marker(project_root: &Path) -> Result<(), StorageError> {
    let marker: ProjectLayoutMarker =
        read_canonical_json_bounded(&project_root.join(PROJECT_LAYOUT_MARKER), 16 * 1024)?;
    if marker.schema_version != STORAGE_SCHEMA || marker.layout_version != PROJECT_LAYOUT_VERSION {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn project_has_valid_layout(project_root: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(project_root.join(PROJECT_LAYOUT_MARKER)) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StorageError::Corrupt)
        }
        Ok(_) => {
            validate_project_layout_marker(project_root)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn validate_stable_project_inventory(
    project_root: &Path,
    require_new_layout: bool,
) -> Result<(), StorageError> {
    let entries = bounded_directory_entries(project_root, 16)?;
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() {
            return Err(StorageError::Corrupt);
        }
        if metadata.is_file() {
            if !matches!(
                name.as_str(),
                "project.json" | "current.json" | PROJECT_LAYOUT_MARKER
            ) {
                return Err(StorageError::Corrupt);
            }
            files.insert(name);
        } else if metadata.is_dir() {
            if !matches!(
                name.as_str(),
                "site"
                    | "revisions"
                    | "proposals"
                    | "targets"
                    | "synapse"
                    | "exports"
                    | "publications"
            ) {
                return Err(StorageError::Corrupt);
            }
            directories.insert(name);
        } else {
            return Err(StorageError::Corrupt);
        }
    }
    let required_files = if require_new_layout {
        BTreeSet::from([
            "current.json".to_owned(),
            PROJECT_LAYOUT_MARKER.to_owned(),
            "project.json".to_owned(),
        ])
    } else {
        BTreeSet::from(["current.json".to_owned(), "project.json".to_owned()])
    };
    if files != required_files {
        return Err(StorageError::Corrupt);
    }
    for required in ["site", "revisions", "proposals"] {
        if !directories.contains(required) {
            return Err(StorageError::Corrupt);
        }
    }
    if require_new_layout
        && ["targets", "synapse", "exports", "publications"]
            .into_iter()
            .any(|required| !directories.contains(required))
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn validate_migration_journal(journal: &MigrationJournal) -> Result<(), StorageError> {
    validate_identifier(&journal.project_id, "prj_")?;
    validate_identifier(&journal.migration_id, "mig_")?;
    validate_identifier(&journal.backup_id, "bkp_")?;
    if journal.schema_version != STORAGE_SCHEMA
        || journal.target_layout_version != PROJECT_LAYOUT_VERSION
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn validate_all_retained_revisions(
    storage: &ManagedStorage,
    project_root: &Path,
) -> Result<(), StorageError> {
    let revisions_root = project_root.join("revisions");
    let entries = bounded_directory_entries(&revisions_root, MAX_RETAINED_REVISIONS + 1)?;
    let mut count = 0_usize;
    for entry in entries {
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        let name = utf8_name(&entry.file_name())?;
        let revision_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(revision_id, "rev_")?;
        count += 1;
        if count > MAX_RETAINED_REVISIONS {
            return Err(StorageError::Corrupt);
        }
        let manifest = read_retained_revision_manifest(project_root, revision_id)?;
        storage.load_manifest_files(&manifest)?;
    }
    if count == 0 {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn collect_backup_tree(root: &Path) -> Result<(Vec<String>, Vec<BackupFile>), StorageError> {
    validate_real_directory(root)?;
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut budget = BackupBudget::default();
    collect_backup_tree_directory(root, root, 0, &mut directories, &mut files, &mut budget)?;
    Ok((directories, files))
}

fn preflight_existing_managed_root(root: &Path) -> Result<(), StorageError> {
    // Phase one is strictly read-only. It validates every schema and recovery
    // inventory which can be encountered by phase two before phase two is
    // allowed to remove an owned temporary or complete a transaction. Raw
    // project or Synapse bytes never leave the authoritative state root.
    let (directories_before, files_before) = collect_backup_tree(root)?;
    preflight_managed_inventory(root)?;
    let (directories_after, files_after) = collect_backup_tree(root)?;
    if directories_after != directories_before || files_after != files_before {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn preflight_managed_inventory(root: &Path) -> Result<(), StorageError> {
    validate_real_directory(root)?;
    let mut names = BTreeSet::new();
    for entry in bounded_directory_entries(root, 5)? {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink()
            || !metadata.is_dir()
            || !matches!(
                name.as_str(),
                "objects" | "projects" | RECOVERY_BACKUPS_DIRECTORY | MIGRATIONS_DIRECTORY
            )
            || !names.insert(name)
        {
            return Err(StorageError::Corrupt);
        }
    }
    if !names.contains("objects") || !names.contains("projects") {
        return Err(StorageError::Corrupt);
    }
    validate_real_directory(&root.join("objects"))?;
    preflight_migration_inventory(root, names.contains(MIGRATIONS_DIRECTORY))?;
    preflight_backup_inventory(root, names.contains(RECOVERY_BACKUPS_DIRECTORY))?;
    preflight_projects_inventory(&ManagedStorage {
        root: root.to_path_buf(),
    })
}

fn preflight_migration_inventory(root: &Path, directory_exists: bool) -> Result<(), StorageError> {
    if !directory_exists {
        return Ok(());
    }
    let migrations = root.join(MIGRATIONS_DIRECTORY);
    let entries = bounded_directory_entries(&migrations, MAX_PERSISTED_PROJECTS + 16)?;
    let mut journals = 0_usize;
    for entry in entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        if is_owned_migration_journal_temporary(&name) {
            validate_owned_temporary_file(&entry.path())?;
            continue;
        }
        let project_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(project_id, "prj_")?;
        journals = journals.checked_add(1).ok_or(StorageError::Corrupt)?;
        if journals > MAX_PERSISTED_PROJECTS {
            return Err(StorageError::Corrupt);
        }
        let journal: MigrationJournal = read_canonical_json_bounded(&entry.path(), 16 * 1024)?;
        validate_migration_journal(&journal)?;
        if journal.project_id != project_id {
            return Err(StorageError::Corrupt);
        }
    }
    Ok(())
}

fn preflight_backup_inventory(root: &Path, directory_exists: bool) -> Result<(), StorageError> {
    if !directory_exists {
        return Ok(());
    }
    let backups = root.join(RECOVERY_BACKUPS_DIRECTORY);
    let mut retained = 0_usize;
    for entry in bounded_directory_entries(&backups, MAX_RECOVERY_BACKUPS + MAX_PERSISTED_PROJECTS)?
    {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(StorageError::Corrupt);
        }
        if is_owned_backup_staging(&name) {
            // Creation staging is phase-two disposable even if its payload was
            // only partly written. The full tree scan above already proved it
            // contains no aliases or special entries.
            continue;
        }
        if backup_deleting_staging_parts(&name).is_some() {
            verify_backup_root(&entry.path(), None)?;
            continue;
        }
        validate_identifier(&name, "bkp_")?;
        retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
        if retained > MAX_RECOVERY_BACKUPS {
            return Err(StorageError::Corrupt);
        }
        verify_backup_root(&entry.path(), None)?;
    }
    Ok(())
}

fn preflight_projects_inventory(storage: &ManagedStorage) -> Result<(), StorageError> {
    let projects = storage.root.join("projects");
    let mut retained = 0_usize;
    for entry in bounded_directory_entries(&projects, MAX_PERSISTED_PROJECTS + 32)? {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(StorageError::Corrupt);
        }
        if is_owned_creation_staging(&name) || is_owned_project_migration_staging(&name) {
            continue;
        }
        if is_owned_migration_old(&name) {
            // Its matching canonical journal was validated before projects.
            continue;
        }
        if let Some((project_id, _)) = project_deleting_staging_parts(&name) {
            preflight_project_inventory(storage, &entry.path(), project_id, true)?;
            continue;
        }
        validate_identifier(&name, "prj_")?;
        retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
        if retained > MAX_PERSISTED_PROJECTS {
            return Err(StorageError::Corrupt);
        }
        preflight_project_inventory(storage, &entry.path(), &name, false)?;
    }
    Ok(())
}

fn preflight_project_inventory(
    storage: &ManagedStorage,
    project_root: &Path,
    project_id: &str,
    deleting: bool,
) -> Result<(), StorageError> {
    validate_real_directory(project_root)?;
    validate_identifier(project_id, "prj_")?;
    let entries = bounded_directory_entries(project_root, 32)?;
    for entry in &entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() {
            return Err(StorageError::Corrupt);
        }
        if metadata.is_file() {
            if matches!(
                name.as_str(),
                "project.json"
                    | "current.json"
                    | "transaction.json"
                    | "creating.json"
                    | "target-transaction.json"
                    | PROJECT_LAYOUT_MARKER
            ) {
                continue;
            }
            if is_exact_project_temporary(&name) {
                validate_owned_temporary_file(&entry.path())?;
                continue;
            }
            return Err(StorageError::Corrupt);
        }
        if metadata.is_dir()
            && matches!(
                name.as_str(),
                "site"
                    | "site.backup"
                    | "site.staging"
                    | "revisions"
                    | "proposals"
                    | "targets"
                    | "synapse"
                    | "exports"
                    | "publications"
            )
        {
            continue;
        }
        return Err(StorageError::Corrupt);
    }

    if path_is_regular_file_if_present(&project_root.join("creating.json"))? {
        let marker: CreationMarker =
            read_canonical_json_bounded(&project_root.join("creating.json"), 16 * 1024)?;
        if marker.schema_version != STORAGE_SCHEMA {
            return Err(StorageError::Corrupt);
        }
    }
    if path_is_regular_file_if_present(&project_root.join(PROJECT_LAYOUT_MARKER))? {
        validate_project_layout_marker(project_root)?;
    }
    if path_is_regular_file_if_present(&project_root.join("project.json"))? {
        let metadata: ProjectMetadata =
            read_canonical_json_bounded(&project_root.join("project.json"), 64 * 1024)?;
        if metadata.schema_version != STORAGE_SCHEMA
            || metadata.id != project_id
            || validate_project_display_name(&metadata.display_name).is_err()
        {
            return Err(StorageError::Corrupt);
        }
    }

    let has_current = path_is_regular_file_if_present(&project_root.join("current.json"))?;
    let has_transaction = path_is_regular_file_if_present(&project_root.join("transaction.json"))?;
    if !has_current && !has_transaction {
        if path_is_regular_file_if_present(&project_root.join("creating.json"))? && !deleting {
            return Ok(());
        }
        return Err(StorageError::Corrupt);
    }
    validate_real_directory(&project_root.join("revisions"))?;
    validate_real_directory(&project_root.join("proposals"))?;
    preflight_revision_inventory(storage, project_root)?;
    preflight_project_transaction(storage, project_root, has_current, has_transaction)?;
    preflight_proposal_inventory(&project_root.join("proposals"))?;

    let new_layout = path_is_regular_file_if_present(&project_root.join(PROJECT_LAYOUT_MARKER))?;
    if new_layout {
        for directory in ["targets", "synapse", "exports", "publications"] {
            validate_real_directory(&project_root.join(directory))?;
        }
        preflight_target_inventory(project_root, project_id)?;
        preflight_generated_inventory(
            project_root,
            project_id,
            GeneratedArtifactKind::StaticExport,
        )?;
        preflight_generated_inventory(
            project_root,
            project_id,
            GeneratedArtifactKind::PublicationDraft,
        )?;
    }
    Ok(())
}

fn is_exact_project_temporary(name: &str) -> bool {
    [
        ("transaction.json", ".tmp"),
        ("current.json", ".tmp"),
        ("project.json", ".tmp"),
        ("target-transaction.json", ".immutable.tmp"),
    ]
    .into_iter()
    .any(|(file, suffix)| {
        name.strip_prefix(&format!(".{file}."))
            .and_then(|rest| rest.strip_suffix(suffix))
            .is_some_and(|nonce| validate_nonce(nonce).is_ok())
    })
}

fn preflight_revision_inventory(
    storage: &ManagedStorage,
    project_root: &Path,
) -> Result<(), StorageError> {
    let revisions = project_root.join("revisions");
    let mut retained = 0_usize;
    for entry in bounded_directory_entries(
        &revisions,
        MAX_RETAINED_REVISIONS + MAX_PROPOSAL_STAGING_ENTRIES,
    )? {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        if is_owned_revision_temporary(&name) {
            validate_owned_temporary_file(&entry.path())?;
            continue;
        }
        let revision_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(revision_id, "rev_")?;
        retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
        if retained > MAX_RETAINED_REVISIONS {
            return Err(StorageError::Corrupt);
        }
        let manifest = read_retained_revision_manifest(project_root, revision_id)?;
        storage.load_manifest_files(&manifest)?;
    }
    if retained == 0 {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn is_owned_revision_temporary(name: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(".immutable.tmp"))
        .and_then(|body| body.rsplit_once('.'))
        .is_some_and(|(revision_name, nonce)| {
            revision_name
                .strip_suffix(".json")
                .is_some_and(|revision_id| validate_identifier(revision_id, "rev_").is_ok())
                && validate_nonce(nonce).is_ok()
        })
}

fn preflight_project_transaction(
    storage: &ManagedStorage,
    project_root: &Path,
    has_current: bool,
    has_transaction: bool,
) -> Result<(), StorageError> {
    let current = if has_current {
        let pointer: CurrentPointer =
            read_canonical_json_bounded(&project_root.join("current.json"), 64 * 1024)?;
        validate_pointer(&pointer)?;
        let manifest = storage.read_manifest_from_root(project_root, &pointer)?;
        storage.load_manifest_files(&manifest)?;
        Some((pointer, manifest))
    } else {
        None
    };
    if !has_transaction {
        let (_, manifest) = current.as_ref().ok_or(StorageError::Corrupt)?;
        if scan_materialized(&project_root.join("site"))? != manifest.files {
            return Err(StorageError::Drift);
        }
        return Ok(());
    }

    let marker: TransactionMarker =
        read_canonical_json_bounded(&project_root.join("transaction.json"), 64 * 1024)?;
    if marker.schema_version != STORAGE_SCHEMA {
        return Err(StorageError::Corrupt);
    }
    let target = CurrentPointer {
        schema_version: STORAGE_SCHEMA.to_owned(),
        revision_id: marker.target_revision_id,
        canonical_manifest_sha256: marker.canonical_manifest_sha256,
        artifact_manifest_sha256: marker.artifact_manifest_sha256,
    };
    validate_pointer(&target)?;
    let target_manifest = storage.read_manifest_from_root(project_root, &target)?;
    storage.load_manifest_files(&target_manifest)?;
    if current
        .as_ref()
        .is_some_and(|(pointer, _)| pointer.revision_id == target.revision_id && pointer != &target)
    {
        return Err(StorageError::Corrupt);
    }
    if let Some((pointer, manifest)) = current.as_ref()
        && pointer != &target
        && path_is_real_directory_if_present(&project_root.join("site.backup"))?
        && scan_materialized(&project_root.join("site.backup"))? != manifest.files
    {
        return Err(StorageError::Drift);
    }
    if path_is_real_directory_if_present(&project_root.join("site.staging"))? {
        // The owned staging tree is disposable. Its aliases and size were
        // already checked by the complete tree fingerprint scan.
    }
    Ok(())
}

fn preflight_target_inventory(project_root: &Path, project_id: &str) -> Result<(), StorageError> {
    let targets = project_root.join("targets");
    let journal_path = project_root.join("target-transaction.json");
    let journal = if path_is_regular_file_if_present(&journal_path)? {
        let journal: TargetReplaceJournal =
            read_canonical_json_bounded(&journal_path, MAX_TARGET_METADATA_BYTES)?;
        if journal.project_id != project_id {
            return Err(StorageError::Corrupt);
        }
        Some(journal)
    } else {
        None
    };
    let entries = bounded_directory_entries(
        &targets,
        MAX_PERSISTED_TARGETS + MAX_PROPOSAL_STAGING_ENTRIES,
    )?;
    let mut retained = 0_usize;
    for entry in &entries {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_file() {
            return Err(StorageError::Corrupt);
        }
        if target_replace_staging_parts(&name).is_some()
            || is_owned_target_immutable_temporary(&name)
        {
            continue;
        }
        let target_id = name.strip_suffix(".json").ok_or(StorageError::Corrupt)?;
        validate_identifier(target_id, "tgt_")?;
        retained = retained.checked_add(1).ok_or(StorageError::Corrupt)?;
        let retained_limit = if journal.is_some() {
            MAX_PERSISTED_TARGETS + 1
        } else {
            MAX_PERSISTED_TARGETS
        };
        if retained > retained_limit {
            return Err(StorageError::Corrupt);
        }
        read_regular_file_bounded(
            &entry.path(),
            MAX_TARGET_METADATA_BYTES,
            StorageError::Corrupt,
        )?;
    }
    if let Some(journal) = journal {
        preflight_target_transaction(project_root, &journal)?;
    }
    Ok(())
}

fn preflight_target_transaction(
    project_root: &Path,
    journal: &TargetReplaceJournal,
) -> Result<(), StorageError> {
    validate_identifier(&journal.project_id, "prj_")?;
    validate_identifier(&journal.old_target_id, "tgt_")?;
    validate_identifier(&journal.new_target_id, "tgt_")?;
    validate_nonce(&journal.nonce)?;
    validate_sha256(&journal.old_sha256)?;
    validate_sha256(&journal.new_sha256)?;
    if journal.schema_version != STORAGE_SCHEMA
        || journal.old_target_id == journal.new_target_id
        || journal.old_byte_length == 0
        || journal.old_byte_length > MAX_TARGET_METADATA_BYTES
        || journal.new_byte_length == 0
        || journal.new_byte_length > MAX_TARGET_METADATA_BYTES
    {
        return Err(StorageError::Corrupt);
    }
    let targets = project_root.join("targets");
    validate_target_transaction_inventory(&targets, journal)?;
    let old = targets.join(format!("{}.json", journal.old_target_id));
    let new = targets.join(format!("{}.json", journal.new_target_id));
    let staging = targets.join(format!(
        ".replacing-target-{}.{}.tmp",
        journal.new_target_id, journal.nonce
    ));
    let old_exists = path_is_regular_file_if_present(&old)?;
    let new_exists = path_is_regular_file_if_present(&new)?;
    let staging_exists = path_is_regular_file_if_present(&staging)?;
    if old_exists {
        verify_file_digest_and_length(
            &old,
            journal.old_byte_length,
            &journal.old_sha256,
            MAX_TARGET_METADATA_BYTES,
        )?;
    }
    if new_exists {
        verify_file_digest_and_length(
            &new,
            journal.new_byte_length,
            &journal.new_sha256,
            MAX_TARGET_METADATA_BYTES,
        )?;
    }
    if staging_exists {
        verify_file_digest_and_length(
            &staging,
            journal.new_byte_length,
            &journal.new_sha256,
            MAX_TARGET_METADATA_BYTES,
        )?;
    }
    if (!new_exists && (!old_exists || !staging_exists)) || (new_exists && staging_exists) {
        return Err(StorageError::Corrupt);
    }
    let projected = enumerate_target_files_ignoring_owned_temporaries(&targets)?
        .checked_add(usize::from(!new_exists))
        .and_then(|count| count.checked_sub(usize::from(old_exists)))
        .ok_or(StorageError::Corrupt)?;
    if projected != MAX_PERSISTED_TARGETS {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn enumerate_target_files_ignoring_owned_temporaries(
    targets: &Path,
) -> Result<usize, StorageError> {
    let mut count = 0_usize;
    for entry in bounded_directory_entries(
        targets,
        MAX_PERSISTED_TARGETS + MAX_PROPOSAL_STAGING_ENTRIES,
    )? {
        let name = utf8_name(&entry.file_name())?;
        if target_replace_staging_parts(&name).is_some()
            || is_owned_target_immutable_temporary(&name)
        {
            continue;
        }
        count = count.checked_add(1).ok_or(StorageError::Corrupt)?;
    }
    Ok(count)
}

fn preflight_proposal_inventory(proposals: &Path) -> Result<(), StorageError> {
    let mut retained = 0_usize;
    let mut staging = 0_usize;
    for entry in bounded_directory_entries(
        proposals,
        MAX_PERSISTED_PROPOSALS + MAX_PROPOSAL_STAGING_ENTRIES,
    )? {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(StorageError::Corrupt);
        }
        let require_complete = if proposal_creating_staging_parts(&name).is_some() {
            staging += 1;
            false
        } else if proposal_deleting_staging_parts(&name).is_some() {
            staging += 1;
            true
        } else if name.starts_with(".creating-proposal-") || name.starts_with(".deleting-proposal-")
        {
            return Err(StorageError::Corrupt);
        } else {
            validate_identifier(&name, "pro_")?;
            retained += 1;
            true
        };
        if staging > MAX_PROPOSAL_STAGING_ENTRIES || retained > MAX_PERSISTED_PROPOSALS {
            return Err(StorageError::Corrupt);
        }
        preflight_proposal_root(&entry.path(), require_complete)?;
    }
    Ok(())
}

fn preflight_proposal_root(root: &Path, require_complete: bool) -> Result<(), StorageError> {
    const FILES: [&str; 4] = [
        "proposal.json",
        "review.json",
        "decision.json",
        "completion.json",
    ];
    let mut stable = BTreeSet::new();
    for entry in bounded_directory_entries(root, FILES.len() + 1 + MAX_PROPOSAL_STAGING_ENTRIES)? {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if name == "site" {
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(StorageError::Corrupt);
            }
            scan_materialized(&entry.path())?;
            stable.insert(name);
        } else if FILES.contains(&name.as_str()) {
            if metadata.is_symlink() || !metadata.is_file() {
                return Err(StorageError::Corrupt);
            }
            read_regular_file_bounded(
                &entry.path(),
                MAX_PROPOSAL_METADATA_BYTES,
                StorageError::Corrupt,
            )?;
            stable.insert(name);
        } else if is_owned_immutable_temp(&name, &FILES) {
            validate_owned_temporary_file(&entry.path())?;
        } else {
            return Err(StorageError::Corrupt);
        }
    }
    if require_complete && (!stable.contains("site") || !stable.contains("proposal.json")) {
        return Err(StorageError::Corrupt);
    }
    if (stable.contains("review.json")
        || stable.contains("decision.json")
        || stable.contains("completion.json"))
        && !stable.contains("proposal.json")
        || stable.contains("decision.json") && !stable.contains("review.json")
        || stable.contains("completion.json") && !stable.contains("decision.json")
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn preflight_generated_inventory(
    project_root: &Path,
    project_id: &str,
    kind: GeneratedArtifactKind,
) -> Result<(), StorageError> {
    let collection = project_root.join(kind.directory_name());
    let mut retained = 0_usize;
    let mut staging = 0_usize;
    let mut archive_total = 0_usize;
    let mut metadata_total = 0_usize;
    for entry in bounded_directory_entries(
        &collection,
        kind.max_records() + MAX_GENERATED_STAGING_ENTRIES,
    )? {
        let name = utf8_name(&entry.file_name())?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(StorageError::Corrupt);
        }
        if is_owned_generated_staging(&name, kind) || is_owned_generated_deleting(&name, kind) {
            staging += 1;
            if staging > MAX_GENERATED_STAGING_ENTRIES {
                return Err(StorageError::Corrupt);
            }
            continue;
        }
        validate_identifier(&name, kind.id_prefix())?;
        retained += 1;
        if retained > kind.max_records() {
            return Err(StorageError::Corrupt);
        }
        let record = read_generated_artifact(
            project_root,
            project_id,
            &entry.path(),
            &name,
            kind,
            MAX_GENERATED_ARCHIVE_BYTES - archive_total,
            MAX_GENERATED_COLLECTION_METADATA_BYTES - metadata_total,
        )?;
        archive_total = archive_total
            .checked_add(record.archive_zip.len())
            .ok_or(StorageError::Corrupt)?;
        metadata_total = metadata_total
            .checked_add(record.metadata_json.len())
            .ok_or(StorageError::Corrupt)?;
    }
    Ok(())
}

fn collect_backup_tree_directory(
    root: &Path,
    directory: &Path,
    depth: usize,
    directories: &mut Vec<String>,
    files: &mut Vec<BackupFile>,
    budget: &mut BackupBudget,
) -> Result<(), StorageError> {
    if depth > MAX_BACKUP_TREE_DEPTH {
        return Err(StorageError::Corrupt);
    }
    validate_real_directory(directory)?;
    let directory_before = fs::symlink_metadata(directory)?;
    let directory_identity = file_identity(&directory_before, directory)?;
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        budget.entries = budget.entries.checked_add(1).ok_or(StorageError::Corrupt)?;
        if budget.entries > MAX_BACKUP_TREE_ENTRIES {
            return Err(StorageError::Corrupt);
        }
        let path = entry.path();
        let relative = backup_relative_path(root, &path)?;
        let metadata = entry.file_type()?;
        if metadata.is_symlink() {
            return Err(StorageError::Corrupt);
        }
        if metadata.is_dir() {
            directories.push(relative);
            collect_backup_tree_directory(root, &path, depth + 1, directories, files, budget)?;
        } else if metadata.is_file() {
            let (byte_length, digest) = hash_backup_file(&path, budget)?;
            files.push(BackupFile {
                path: relative,
                byte_length,
                sha256: digest,
            });
        } else {
            return Err(StorageError::Corrupt);
        }
    }
    let directory_after = fs::symlink_metadata(directory)?;
    if directory_after.file_type().is_symlink()
        || file_identity(&directory_after, directory)? != directory_identity
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn backup_relative_path(root: &Path, path: &Path) -> Result<String, StorageError> {
    let relative = path.strip_prefix(root).map_err(|_| StorageError::Corrupt)?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(StorageError::Corrupt);
        };
        let part = part.to_str().ok_or(StorageError::Corrupt)?;
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.contains(['\0', '/', '\\'])
            || part.nfc().collect::<String>() != part
            || part.chars().any(char::is_control)
        {
            return Err(StorageError::Corrupt);
        }
        parts.push(part);
    }
    if parts.is_empty() || parts.len() > MAX_BACKUP_TREE_DEPTH {
        return Err(StorageError::Corrupt);
    }
    let value = parts.join("/");
    if value.len() > 4 * MAX_PATH_BYTES {
        return Err(StorageError::Corrupt);
    }
    Ok(value)
}

fn backup_path(root: &Path, relative: &str) -> Result<PathBuf, StorageError> {
    if relative.is_empty() || relative.len() > 4 * MAX_PATH_BYTES {
        return Err(StorageError::Corrupt);
    }
    let parts = relative.split('/').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > MAX_BACKUP_TREE_DEPTH {
        return Err(StorageError::Corrupt);
    }
    let mut result = root.to_path_buf();
    for part in parts {
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.contains(['\0', '/', '\\'])
            || part.nfc().collect::<String>() != part
            || part.chars().any(char::is_control)
        {
            return Err(StorageError::Corrupt);
        }
        result.push(part);
    }
    Ok(result)
}

fn hash_backup_file(path: &Path, budget: &mut BackupBudget) -> Result<(u64, String), StorageError> {
    let before = fs::symlink_metadata(path)?;
    if before.file_type().is_symlink()
        || !before.is_file()
        || hard_link_count(&before) != 1
        || before.len() > MAX_BACKUP_FILE_BYTES
    {
        return Err(StorageError::Corrupt);
    }
    budget.total_bytes = budget
        .total_bytes
        .checked_add(before.len())
        .filter(|total| *total <= MAX_BACKUP_TOTAL_BYTES)
        .ok_or(StorageError::Corrupt)?;
    let identity = file_identity(&before, path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    let opened = file.metadata()?;
    if file_identity(&opened, path)? != identity || opened.len() != before.len() {
        return Err(StorageError::Corrupt);
    }
    let mut hasher = Sha256::new();
    let mut observed = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        observed = observed
            .checked_add(u64::try_from(count).map_err(|_| StorageError::Corrupt)?)
            .ok_or(StorageError::Corrupt)?;
        if observed > before.len() {
            return Err(StorageError::Corrupt);
        }
        hasher.update(&buffer[..count]);
    }
    let after = fs::symlink_metadata(path)?;
    if after.file_type().is_symlink()
        || hard_link_count(&after) != 1
        || file_identity(&after, path)? != identity
        || after.len() != before.len()
        || observed != before.len()
    {
        return Err(StorageError::Corrupt);
    }
    Ok((observed, hex_digest(hasher.finalize().as_slice())))
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_backup_manifest(manifest: &BackupManifest) -> Result<(), StorageError> {
    validate_identifier(&manifest.backup_id, "bkp_")?;
    validate_identifier(&manifest.project_id, "prj_")?;
    validate_identifier(&manifest.revision_id, "rev_")?;
    validate_sha256(&manifest.artifact_manifest_sha256)?;
    if manifest.schema_version != STORAGE_SCHEMA
        || manifest.format_version != BACKUP_FORMAT_VERSION
        || manifest.directories.len() + manifest.files.len() > MAX_BACKUP_TREE_ENTRIES
    {
        return Err(StorageError::Corrupt);
    }
    let mut previous_directory: Option<&str> = None;
    for directory in &manifest.directories {
        backup_path(Path::new("."), directory)?;
        if previous_directory.is_some_and(|previous| previous >= directory.as_str()) {
            return Err(StorageError::Corrupt);
        }
        previous_directory = Some(directory);
    }
    let mut previous_file: Option<&str> = None;
    let mut total = 0_u64;
    for file in &manifest.files {
        backup_path(Path::new("."), &file.path)?;
        validate_sha256(&file.sha256)?;
        if file.byte_length > MAX_BACKUP_FILE_BYTES
            || previous_file.is_some_and(|previous| previous >= file.path.as_str())
        {
            return Err(StorageError::Corrupt);
        }
        total = total
            .checked_add(file.byte_length)
            .filter(|value| *value <= MAX_BACKUP_TOTAL_BYTES)
            .ok_or(StorageError::Corrupt)?;
        previous_file = Some(&file.path);
    }
    Ok(())
}

fn verify_backup_root(
    backup_root: &Path,
    journal: Option<&MigrationJournal>,
) -> Result<BackupManifest, StorageError> {
    validate_real_directory(backup_root)?;
    let entries = bounded_directory_entries(backup_root, 3)?;
    let inventory = entries
        .iter()
        .map(|entry| utf8_name(&entry.file_name()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if inventory != BTreeSet::from(["manifest.json".to_owned(), "project".to_owned()]) {
        return Err(StorageError::Corrupt);
    }
    validate_real_directory(&backup_root.join("project"))?;
    let manifest: BackupManifest =
        read_canonical_json_bounded(&backup_root.join("manifest.json"), 8 * 1024 * 1024)?;
    validate_backup_manifest(&manifest)?;
    let directory_name = backup_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StorageError::Corrupt)?;
    let owned_staging_matches = if directory_name.starts_with(".creating-backup-") {
        is_owned_backup_staging(directory_name)
            && directory_name
                .strip_prefix(".creating-backup-")
                .and_then(|value| value.split_once('.'))
                .is_some_and(|(backup_id, _)| backup_id == manifest.backup_id)
    } else if directory_name.starts_with(".deleting-backup-") {
        backup_deleting_staging_parts(directory_name)
            .is_some_and(|(backup_id, _)| backup_id == manifest.backup_id)
    } else {
        false
    };
    if directory_name != manifest.backup_id && !owned_staging_matches {
        return Err(StorageError::Corrupt);
    }
    if let Some(journal) = journal
        && (manifest.backup_id != journal.backup_id || manifest.project_id != journal.project_id)
    {
        return Err(StorageError::Corrupt);
    }
    let (directories, files) = collect_backup_tree(&backup_root.join("project"))?;
    if directories != manifest.directories || files != manifest.files {
        return Err(StorageError::Corrupt);
    }
    Ok(manifest)
}

fn copy_backup_tree(
    source_root: &Path,
    destination_root: &Path,
    manifest: &BackupManifest,
) -> Result<(), StorageError> {
    validate_backup_manifest(manifest)?;
    validate_real_directory(source_root)?;
    validate_real_directory(destination_root)?;
    let (directories, files) = collect_backup_tree(source_root)?;
    if directories != manifest.directories || files != manifest.files {
        return Err(StorageError::Corrupt);
    }
    for directory in &manifest.directories {
        let destination = backup_path(destination_root, directory)?;
        fs::create_dir(&destination)?;
    }
    for file in &manifest.files {
        let source = backup_path(source_root, &file.path)?;
        let destination = backup_path(destination_root, &file.path)?;
        copy_backup_file(&source, &destination, file)?;
    }
    for directory in manifest.directories.iter().rev() {
        sync_directory(&backup_path(destination_root, directory)?)?;
    }
    sync_directory(destination_root)
}

fn copy_backup_file(
    source: &Path,
    destination: &Path,
    expected: &BackupFile,
) -> Result<(), StorageError> {
    let before = fs::symlink_metadata(source)?;
    if before.file_type().is_symlink()
        || !before.is_file()
        || hard_link_count(&before) != 1
        || before.len() != expected.byte_length
    {
        return Err(StorageError::Corrupt);
    }
    let identity = file_identity(&before, source)?;
    let mut source_options = OpenOptions::new();
    source_options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        source_options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut input = source_options.open(source)?;
    let mut destination_options = OpenOptions::new();
    destination_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        destination_options.mode(0o600);
    }
    let mut output = destination_options.open(destination)?;
    let result = (|| -> Result<(), StorageError> {
        let mut hasher = Sha256::new();
        let mut observed = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            observed = observed
                .checked_add(u64::try_from(count).map_err(|_| StorageError::Corrupt)?)
                .ok_or(StorageError::Corrupt)?;
            if observed > expected.byte_length {
                return Err(StorageError::Corrupt);
            }
            hasher.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        if observed != expected.byte_length
            || hex_digest(hasher.finalize().as_slice()) != expected.sha256
        {
            return Err(StorageError::Corrupt);
        }
        output.sync_all()?;
        let after = fs::symlink_metadata(source)?;
        if after.file_type().is_symlink()
            || hard_link_count(&after) != 1
            || file_identity(&after, source)? != identity
            || after.len() != before.len()
        {
            return Err(StorageError::Corrupt);
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(destination);
    }
    result
}

fn validate_migrated_staging(
    staging: &Path,
    backup: &Path,
    manifest: &BackupManifest,
) -> Result<(), StorageError> {
    let verified = verify_backup_root(backup, None)?;
    if &verified != manifest {
        return Err(StorageError::Corrupt);
    }
    validate_project_layout_marker(staging)?;
    validate_stable_project_inventory(staging, true)?;
    let (actual_directories, actual_files) = collect_backup_tree(staging)?;
    let mut expected_directories = manifest
        .directories
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for directory in ["targets", "synapse", "exports", "publications"] {
        expected_directories.insert(directory.to_owned());
    }
    let marker_bytes = canonical_json(&ProjectLayoutMarker {
        schema_version: STORAGE_SCHEMA.to_owned(),
        layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
    })?;
    let mut expected_files = manifest.files.clone();
    expected_files.push(BackupFile {
        path: PROJECT_LAYOUT_MARKER.to_owned(),
        byte_length: u64::try_from(marker_bytes.len()).map_err(|_| StorageError::Corrupt)?,
        sha256: sha256(&marker_bytes),
    });
    expected_files.sort();
    if actual_directories != expected_directories.into_iter().collect::<Vec<_>>()
        || actual_files != expected_files
    {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), StorageError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(StorageError::Corrupt)
    }
}

fn validate_real_directory(path: &Path) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(StorageError::Corrupt)
    } else {
        Ok(())
    }
}

fn ensure_real_directory(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(StorageError::Corrupt),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            validate_real_directory(path)?;
            if let Some(parent) = path.parent() {
                sync_directory(parent)?;
            }
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    serde_json::to_vec(value).map_err(|_| StorageError::Corrupt)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, StorageError> {
    let bytes = read_regular_file_bounded(path, 4 * 1024 * 1024, StorageError::Corrupt)?;
    serde_json::from_slice(&bytes).map_err(|_| StorageError::Corrupt)
}

fn read_canonical_json_bounded<T>(path: &Path, maximum_length: usize) -> Result<T, StorageError>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let bytes = read_regular_file_bounded(path, maximum_length, StorageError::Corrupt)?;
    let value: T = serde_json::from_slice(&bytes).map_err(|_| StorageError::Corrupt)?;
    if canonical_json(&value)? != bytes {
        return Err(StorageError::Corrupt);
    }
    Ok(value)
}

fn write_immutable_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    write_immutable(path, &canonical_json(value)?)
}

fn write_atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    write_atomic(path, &canonical_json(value)?)
}

fn write_immutable(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(StorageError::Corrupt);
        }
        Ok(_) => return verify_immutable_bytes(path, bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let parent = path.parent().ok_or(StorageError::Corrupt)?;
    validate_real_directory(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StorageError::Corrupt)?;
    let temporary = parent.join(format!(".{name}.{}.immutable.tmp", Uuid::new_v4().simple()));
    if let Err(error) = write_new_file(&temporary, bytes) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    // Production holds the process-wide state-root lease and the in-process
    // Store mutex. Rechecking here therefore gives the atomic rename a single
    // writer while still preserving idempotence after interrupted attempts.
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            let _ = fs::remove_file(&temporary);
            Err(StorageError::Corrupt)
        }
        Ok(_) => {
            let _ = fs::remove_file(&temporary);
            verify_immutable_bytes(path, bytes)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Err(error) = rename_internal(&temporary, path) {
                let _ = fs::remove_file(&temporary);
                return Err(error);
            }
            sync_directory(parent)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error.into())
        }
    }
}

fn verify_immutable_bytes(path: &Path, expected: &[u8]) -> Result<(), StorageError> {
    let actual = read_regular_file_exact(path, expected.len(), StorageError::Corrupt)?;
    if actual == expected {
        Ok(())
    } else {
        Err(StorageError::Corrupt)
    }
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    injected_storage_fault(Failpoint::StorageWriteBefore)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    injected_storage_fault(Failpoint::StorageWriteAfter)?;
    injected_storage_fault(Failpoint::StorageFsyncBefore)?;
    file.sync_all()?;
    injected_storage_fault(Failpoint::StorageFsyncAfter)?;
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::Corrupt)?;
    validate_real_directory(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StorageError::Corrupt)?;
    let temporary = parent.join(format!(".{name}.{}.tmp", Uuid::new_v4().simple()));
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(StorageError::Corrupt);
    }
    if let Err(error) = write_new_file(&temporary, bytes) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = rename_internal(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<(), StorageError> {
    injected_storage_fault(Failpoint::StorageFsyncBefore)?;
    File::open(path)?.sync_all()?;
    injected_storage_fault(Failpoint::StorageFsyncAfter)?;
    Ok(())
}

fn rename_internal(source: &Path, destination: &Path) -> Result<(), StorageError> {
    injected_storage_fault(Failpoint::StorageRenameBefore)?;
    fs::rename(source, destination)?;
    injected_storage_fault(Failpoint::StorageRenameAfter)
}

fn injected_storage_fault(failpoint: Failpoint) -> Result<(), StorageError> {
    fault_injection::check(failpoint)
        .map_err(|fault| StorageError::Io(io::Error::new(fault.io_kind(), fault)))
}

fn remove_internal_file_if_present(path: &Path) -> Result<(), StorageError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_internal_tree_if_present(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(StorageError::Corrupt)
        }
        Ok(_) => fs::remove_dir_all(path).map_err(StorageError::Io),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn media_type(path: &str) -> String {
    MimeGuess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_regular_file_exact(
    path: &Path,
    expected_length: usize,
    failure: StorageError,
) -> Result<Vec<u8>, StorageError> {
    if expected_length > MAX_FILE_BYTES {
        return Err(failure);
    }
    let before = fs::symlink_metadata(path).map_err(|_| clone_failure(&failure))?;
    if before.file_type().is_symlink()
        || !before.is_file()
        || hard_link_count(&before) != 1
        || usize::try_from(before.len()).ok() != Some(expected_length)
    {
        return Err(failure);
    }
    let before_identity = file_identity(&before, path).map_err(|_| clone_failure(&failure))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| clone_failure(&failure))?;
    let opened = file.metadata().map_err(|_| clone_failure(&failure))?;
    if file_identity(&opened, path).map_err(|_| clone_failure(&failure))? != before_identity
        || usize::try_from(opened.len()).ok() != Some(expected_length)
    {
        return Err(failure);
    }
    let mut bytes = Vec::with_capacity(expected_length);
    file.take((expected_length + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| clone_failure(&failure))?;
    let after = fs::symlink_metadata(path).map_err(|_| clone_failure(&failure))?;
    if after.file_type().is_symlink()
        || file_identity(&after, path).map_err(|_| clone_failure(&failure))? != before_identity
        || bytes.len() != expected_length
    {
        return Err(failure);
    }
    Ok(bytes)
}

fn read_regular_file_bounded(
    path: &Path,
    maximum_length: usize,
    failure: StorageError,
) -> Result<Vec<u8>, StorageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| clone_failure(&failure))?;
    let length = usize::try_from(metadata.len()).map_err(|_| clone_failure(&failure))?;
    if length > maximum_length {
        return Err(failure);
    }
    read_regular_file_exact_with_limit(path, length, maximum_length, failure)
}

fn read_optional_bounded_file(
    path: &Path,
    maximum_length: usize,
) -> Result<Option<Vec<u8>>, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(StorageError::Corrupt)
        }
        Ok(_) => read_regular_file_bounded(path, maximum_length, StorageError::Corrupt).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn read_regular_file_exact_with_limit(
    path: &Path,
    expected_length: usize,
    maximum_length: usize,
    failure: StorageError,
) -> Result<Vec<u8>, StorageError> {
    if expected_length > maximum_length {
        return Err(failure);
    }
    let before = fs::symlink_metadata(path).map_err(|_| clone_failure(&failure))?;
    if before.file_type().is_symlink()
        || !before.is_file()
        || hard_link_count(&before) != 1
        || usize::try_from(before.len()).ok() != Some(expected_length)
    {
        return Err(failure);
    }
    let before_identity = file_identity(&before, path).map_err(|_| clone_failure(&failure))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| clone_failure(&failure))?;
    let opened = file.metadata().map_err(|_| clone_failure(&failure))?;
    if file_identity(&opened, path).map_err(|_| clone_failure(&failure))? != before_identity
        || usize::try_from(opened.len()).ok() != Some(expected_length)
    {
        return Err(failure);
    }
    let mut bytes = Vec::with_capacity(expected_length);
    file.take((expected_length + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| clone_failure(&failure))?;
    let after = fs::symlink_metadata(path).map_err(|_| clone_failure(&failure))?;
    if after.file_type().is_symlink()
        || file_identity(&after, path).map_err(|_| clone_failure(&failure))? != before_identity
        || bytes.len() != expected_length
    {
        return Err(failure);
    }
    Ok(bytes)
}

fn clone_failure(error: &StorageError) -> StorageError {
    match error {
        StorageError::Drift => StorageError::Drift,
        StorageError::UnsafeImport => StorageError::UnsafeImport,
        StorageError::ImportLimit => StorageError::ImportLimit,
        StorageError::Corrupt | StorageError::Io(_) => StorageError::Corrupt,
    }
}

fn utf8_name(name: &std::ffi::OsStr) -> Result<String, StorageError> {
    name.to_str()
        .map(str::to_owned)
        .ok_or(StorageError::Corrupt)
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct FileIdentity(u64, u64);

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata, _path: &Path) -> Result<FileIdentity, StorageError> {
    use std::os::unix::fs::MetadataExt as _;
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[cfg(unix)]
fn hard_link_count(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    metadata.nlink()
}

#[cfg(not(unix))]
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct FileIdentity(PathBuf);

#[cfg(not(unix))]
fn file_identity(_metadata: &fs::Metadata, path: &Path) -> Result<FileIdentity, StorageError> {
    Ok(FileIdentity(fs::canonicalize(path)?))
}

#[cfg(not(unix))]
fn hard_link_count(_metadata: &fs::Metadata) -> u64 {
    1
}

#[cfg(test)]
pub(super) fn materialized_site_path(state_root: &Path, project_id: &str) -> PathBuf {
    state_root
        .join(STORAGE_DIRECTORY)
        .join("projects")
        .join(project_id)
        .join("site")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT_ID: &str = "prj_11111111111111111111111111111111";
    const REVISION_ID: &str = "rev_22222222222222222222222222222222";
    const ARTIFACT_SHA: &str = "3333333333333333333333333333333333333333333333333333333333333333";
    const EXPORT_ID: &str = "exp_44444444444444444444444444444444";
    const PUBLICATION_ID: &str = "pub_55555555555555555555555555555555";
    const NEXT_REVISION_ID: &str = "rev_66666666666666666666666666666666";
    const NEXT_ARTIFACT_SHA: &str =
        "7777777777777777777777777777777777777777777777777777777777777777";
    const SECOND_PROJECT_ID: &str = "prj_88888888888888888888888888888888";
    const SECOND_REVISION_ID: &str = "rev_99999999999999999999999999999999";
    const SECOND_ARTIFACT_SHA: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn storage_with_project() -> (tempfile::TempDir, ManagedStorage) {
        let root = tempfile::tempdir().unwrap();
        let (storage, projects) = ManagedStorage::open(root.path()).unwrap();
        assert!(projects.is_empty());
        storage
            .create_project(
                PROJECT_ID,
                "Generated artifact persistence",
                REVISION_ID,
                ARTIFACT_SHA,
                &BTreeMap::from([("index.html".into(), b"<h1>safe</h1>".to_vec())]),
            )
            .unwrap();
        (root, storage)
    }

    fn make_legacy_project(storage: &ManagedStorage) {
        let project_root = storage.project_root(PROJECT_ID);
        fs::remove_file(project_root.join(PROJECT_LAYOUT_MARKER)).unwrap();
        for directory in ["targets", "exports", "publications"] {
            fs::remove_dir(project_root.join(directory)).unwrap();
        }
        fs::write(
            project_root.join("synapse").join("review-journal.sqlite3"),
            b"legacy synapse journal",
        )
        .unwrap();
    }

    fn staged_migration_fixture(
        storage: &ManagedStorage,
    ) -> (MigrationJournal, PathBuf, PathBuf, PathBuf, PathBuf) {
        storage.validate_legacy_project(PROJECT_ID).unwrap();
        let nonce = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let journal = MigrationJournal {
            schema_version: STORAGE_SCHEMA.to_owned(),
            target_layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
            migration_id: format!("mig_{nonce}"),
            backup_id: format!("bkp_{nonce}"),
            project_id: PROJECT_ID.to_owned(),
        };
        let migrations_root = storage.root.join(MIGRATIONS_DIRECTORY);
        write_immutable_json(
            &migrations_root.join(format!("{PROJECT_ID}.json")),
            &journal,
        )
        .unwrap();
        let projects_root = storage.root.join("projects");
        let active = storage.project_root(PROJECT_ID);
        let staging = projects_root.join(format!(".migrating-{PROJECT_ID}.{nonce}"));
        let old = projects_root.join(format!(".migration-old-{PROJECT_ID}.{nonce}"));
        let backup = storage
            .root
            .join(RECOVERY_BACKUPS_DIRECTORY)
            .join(&journal.backup_id);
        let backup_staging = storage
            .root
            .join(RECOVERY_BACKUPS_DIRECTORY)
            .join(format!(".creating-backup-{}.{}", journal.backup_id, nonce));
        let manifest = storage
            .create_backup(&active, &backup_staging, &backup, &journal)
            .unwrap();
        storage
            .create_migrated_staging(&staging, &backup, &manifest)
            .unwrap();
        (journal, active, staging, old, backup)
    }

    #[test]
    fn legacy_layout_migrates_via_verified_external_backup_and_recovery_reader() {
        let (root, storage) = storage_with_project();
        make_legacy_project(&storage);

        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        let project_root = storage.project_root(PROJECT_ID);
        validate_project_layout_marker(&project_root).unwrap();
        for directory in ["targets", "synapse", "exports", "publications"] {
            validate_real_directory(&project_root.join(directory)).unwrap();
        }
        assert_eq!(
            fs::read(project_root.join("synapse").join("review-journal.sqlite3")).unwrap(),
            b"legacy synapse journal"
        );

        let backups_root = storage.root.join(RECOVERY_BACKUPS_DIRECTORY);
        let backups = bounded_directory_entries(&backups_root, 2).unwrap();
        assert_eq!(backups.len(), 1);
        let manifest = verify_backup_root(&backups[0].path(), None).unwrap();
        let paths = manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<BTreeSet<_>>();
        assert!(paths.contains("site/index.html"));
        assert!(paths.contains("synapse/review-journal.sqlite3"));
        assert!(paths.contains(format!("revisions/{REVISION_ID}.json").as_str()));

        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        let points = reader.list_points().unwrap();
        assert_eq!(points.len(), 2);
        assert!(points.iter().all(|point| point.diagnostic.verified));
        let backup_point = points
            .iter()
            .find(|point| point.kind == RecoveryPointKind::VersionedBackup)
            .unwrap();
        assert_eq!(backup_point.diagnostic.code, "backup_verified");
        let snapshot = reader.open_snapshot(&backup_point.id).unwrap();
        assert_eq!(
            snapshot.files(),
            &BTreeMap::from([("index.html".to_owned(), b"<h1>safe</h1>".to_vec())])
        );
    }

    #[test]
    fn migration_interruption_states_converge_to_fully_validated_new_layout() {
        for publish_stage in 0..3 {
            let (root, storage) = storage_with_project();
            make_legacy_project(&storage);
            let (_journal, active, staging, old, backup) = staged_migration_fixture(&storage);
            if publish_stage >= 1 {
                fs::rename(&active, &old).unwrap();
            }
            if publish_stage >= 2 {
                fs::rename(&staging, &active).unwrap();
            }

            let (_, projects) = ManagedStorage::open(root.path()).unwrap();
            assert_eq!(projects.len(), 1);
            validate_project_layout_marker(&active).unwrap();
            assert!(!staging.exists());
            assert!(!old.exists());
            assert!(backup.exists());
            assert_eq!(
                fs::read_dir(storage.root.join(MIGRATIONS_DIRECTORY))
                    .unwrap()
                    .count(),
                0
            );
        }
    }

    #[test]
    fn migration_rebuilds_only_owned_partial_staging_and_preserves_unknown_or_corrupt_legacy() {
        let (root, storage) = storage_with_project();
        make_legacy_project(&storage);
        let nonce = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let journal = MigrationJournal {
            schema_version: STORAGE_SCHEMA.to_owned(),
            target_layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
            migration_id: format!("mig_{nonce}"),
            backup_id: format!("bkp_{nonce}"),
            project_id: PROJECT_ID.to_owned(),
        };
        write_immutable_json(
            &storage
                .root
                .join(MIGRATIONS_DIRECTORY)
                .join(format!("{PROJECT_ID}.json")),
            &journal,
        )
        .unwrap();
        let partial = storage
            .root
            .join(RECOVERY_BACKUPS_DIRECTORY)
            .join(format!(".creating-backup-{}.{}", journal.backup_id, nonce));
        fs::create_dir(&partial).unwrap();
        fs::write(partial.join("partial"), b"partial").unwrap();
        assert_eq!(ManagedStorage::open(root.path()).unwrap().1.len(), 1);
        assert!(!partial.exists());

        let (root, storage) = storage_with_project();
        make_legacy_project(&storage);
        let unknown = storage.project_root(PROJECT_ID).join("unknown-state");
        fs::create_dir(&unknown).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(unknown.exists());
        assert!(
            !storage
                .project_root(PROJECT_ID)
                .join(PROJECT_LAYOUT_MARKER)
                .exists()
        );
        assert_eq!(
            fs::read_dir(storage.root.join(RECOVERY_BACKUPS_DIRECTORY))
                .unwrap()
                .count(),
            0
        );
        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        assert!(reader.list_points().unwrap().iter().any(|point| point.kind
            == RecoveryPointKind::LastAccepted
            && point.diagnostic.verified));

        let (root, storage) = storage_with_project();
        make_legacy_project(&storage);
        let revision = storage
            .project_root(PROJECT_ID)
            .join("revisions")
            .join(format!("{REVISION_ID}.json"));
        fs::write(&revision, b"corrupt").unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(revision).unwrap(), b"corrupt");
        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        assert!(
            reader
                .list_points()
                .unwrap()
                .iter()
                .any(|point| !point.diagnostic.verified)
        );
    }

    #[test]
    fn unknown_layout_and_tampered_backup_are_preserved_for_read_only_diagnostics() {
        let (root, storage) = storage_with_project();
        write_atomic_json(
            &storage.project_root(PROJECT_ID).join(PROJECT_LAYOUT_MARKER),
            &ProjectLayoutMarker {
                schema_version: STORAGE_SCHEMA.to_owned(),
                layout_version: "999".to_owned(),
            },
        )
        .unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        let marker: ProjectLayoutMarker =
            read_json(&storage.project_root(PROJECT_ID).join(PROJECT_LAYOUT_MARKER)).unwrap();
        assert_eq!(marker.layout_version, "999");
        assert_eq!(
            fs::read_dir(storage.root.join(RECOVERY_BACKUPS_DIRECTORY))
                .unwrap()
                .count(),
            0
        );

        let (root, storage) = storage_with_project();
        make_legacy_project(&storage);
        storage.migrate_legacy_project(PROJECT_ID).unwrap();
        let backup = bounded_directory_entries(&storage.root.join(RECOVERY_BACKUPS_DIRECTORY), 2)
            .unwrap()
            .remove(0)
            .path();
        let tampered = backup
            .join("project")
            .join("synapse")
            .join("review-journal.sqlite3");
        fs::write(&tampered, b"tampered").unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(&tampered).unwrap(), b"tampered");
        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        let points = reader.list_points().unwrap();
        assert!(points.iter().any(|point| {
            point.kind == RecoveryPointKind::VersionedBackup
                && !point.diagnostic.verified
                && point.diagnostic.code == "backup_corrupt"
        }));
        let accepted = points
            .iter()
            .find(|point| {
                point.kind == RecoveryPointKind::LastAccepted && point.diagnostic.verified
            })
            .unwrap();
        assert_eq!(
            reader.open_snapshot(&accepted.id).unwrap().files(),
            &BTreeMap::from([("index.html".to_owned(), b"<h1>safe</h1>".to_vec())])
        );
    }

    #[test]
    fn recovery_reader_isolates_unknown_and_interrupted_entries_without_mutating_sources() {
        let (root, storage) = storage_with_project();
        let backups = storage.root.join(RECOVERY_BACKUPS_DIRECTORY);
        let corrupt_id = "bkp_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let corrupt = backups.join(corrupt_id);
        fs::create_dir(&corrupt).unwrap();
        fs::write(corrupt.join("manifest.json"), b"not canonical json").unwrap();

        let unknown = backups.join("opaque-backup-source");
        fs::create_dir(&unknown).unwrap();
        let sentinel = unknown.join("sentinel");
        fs::write(&sentinel, b"must remain byte-for-byte unchanged").unwrap();
        let sentinel_before = fs::read(&sentinel).unwrap();
        let identity_before =
            file_identity(&fs::symlink_metadata(&sentinel).unwrap(), &sentinel).unwrap();

        let interrupted = backups.join(
            ".deleting-backup-bkp_cccccccccccccccccccccccccccccccc.dddddddddddddddddddddddddddddddd",
        );
        fs::create_dir(&interrupted).unwrap();
        let unknown_project = storage.root.join("projects").join("unknown-project-source");
        fs::write(&unknown_project, b"opaque project diagnostic source").unwrap();
        let interrupted_project = storage.root.join("projects").join(
            ".deleting-project-prj_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee.ffffffffffffffffffffffffffffffff",
        );
        fs::create_dir(&interrupted_project).unwrap();

        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        let first = reader.list_points().unwrap();
        let second = reader.list_points().unwrap();
        assert_eq!(first, second);
        assert!(first.iter().any(|point| {
            point.id == corrupt_id
                && !point.diagnostic.verified
                && point.diagnostic.code == "backup_corrupt"
        }));
        assert!(first.iter().any(|point| {
            point.id.starts_with("bkp_")
                && !point.diagnostic.verified
                && point.diagnostic.code == "backup_unknown_entry"
        }));
        assert!(first.iter().any(|point| {
            !point.diagnostic.verified && point.diagnostic.code == "backup_interrupted"
        }));
        assert!(first.iter().any(|point| {
            !point.diagnostic.verified && point.diagnostic.code == "project_unknown_entry"
        }));
        assert!(first.iter().any(|point| {
            !point.diagnostic.verified && point.diagnostic.code == "project_interrupted"
        }));
        let accepted = first
            .iter()
            .find(|point| {
                point.kind == RecoveryPointKind::LastAccepted && point.diagnostic.verified
            })
            .unwrap();
        assert_eq!(
            reader.open_snapshot(&accepted.id).unwrap().files(),
            &BTreeMap::from([("index.html".to_owned(), b"<h1>safe</h1>".to_vec())])
        );
        let unverified = first
            .iter()
            .find(|point| !point.diagnostic.verified)
            .unwrap();
        assert!(matches!(
            reader.open_snapshot(&unverified.id),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(&sentinel).unwrap(), sentinel_before);
        assert_eq!(
            file_identity(&fs::symlink_metadata(&sentinel).unwrap(), &sentinel).unwrap(),
            identity_before
        );
        assert!(corrupt.exists());
        assert!(interrupted.exists());
        assert!(unknown_project.exists());
        assert!(interrupted_project.exists());
    }

    #[test]
    fn recovery_reader_fails_closed_above_the_72_point_contract_boundary() {
        let (root, storage) = storage_with_project();
        let backups = storage.root.join(RECOVERY_BACKUPS_DIRECTORY);
        for index in 0..MAX_RECOVERY_POINTS {
            fs::create_dir(backups.join(format!("unknown-recovery-source-{index:02}"))).unwrap();
        }
        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        assert!(matches!(reader.list_points(), Err(StorageError::Corrupt)));
        assert_eq!(fs::read_dir(&backups).unwrap().count(), MAX_RECOVERY_POINTS);
        assert!(storage.project_root(PROJECT_ID).exists());
    }

    #[cfg(unix)]
    #[test]
    fn recovery_reader_reports_symlinks_without_following_or_removing_them() {
        use std::os::unix::ffi::OsStringExt as _;
        use std::os::unix::fs::symlink;

        let (root, storage) = storage_with_project();
        let external = tempfile::tempdir().unwrap();
        let sentinel = external.path().join("sentinel");
        fs::write(&sentinel, b"external recovery source").unwrap();
        let backup_link = storage.root.join(RECOVERY_BACKUPS_DIRECTORY).join(
            ".deleting-backup-bkp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        symlink(external.path(), &backup_link).unwrap();
        let project_link = storage
            .root
            .join("projects")
            .join("prj_cccccccccccccccccccccccccccccccc");
        symlink(external.path(), &project_link).unwrap();
        let non_utf8 =
            storage
                .root
                .join(RECOVERY_BACKUPS_DIRECTORY)
                .join(std::ffi::OsString::from_vec(vec![
                    b'o', b'p', b'a', b'q', b'u', b'e', b'-', 0xff,
                ]));
        fs::create_dir(&non_utf8).unwrap();

        let reader = ManagedStorage::open_recovery(root.path()).unwrap();
        let points = reader.list_points().unwrap();
        assert!(points.iter().any(|point| {
            !point.diagnostic.verified && point.diagnostic.code == "backup_interrupted"
        }));
        assert!(points.iter().any(|point| {
            !point.diagnostic.verified && point.diagnostic.code == "project_unsafe_entry"
        }));
        assert!(points.iter().any(|point| {
            point.id.starts_with("bkp_")
                && !point.diagnostic.verified
                && point.diagnostic.code == "backup_unknown_entry"
        }));
        assert_eq!(fs::read(&sentinel).unwrap(), b"external recovery source");
        assert!(
            fs::symlink_metadata(&backup_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(&project_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(non_utf8.exists());
    }

    fn fill_target_capacity(storage: &ManagedStorage) -> BTreeMap<String, Vec<u8>> {
        let mut retained = BTreeMap::new();
        for index in 0..MAX_PERSISTED_TARGETS {
            let id = format!("tgt_{index:032x}");
            let bytes = format!(r#"{{"targetId":"{id}","value":{index}}}"#).into_bytes();
            storage.persist_target(PROJECT_ID, &id, &bytes).unwrap();
            retained.insert(id, bytes);
        }
        retained
    }

    fn target_replace_fixture(
        storage: &ManagedStorage,
    ) -> (TargetReplaceJournal, PathBuf, PathBuf, PathBuf) {
        let retained = fill_target_capacity(storage);
        let old_target_id = "tgt_00000000000000000000000000000000";
        let old_bytes = retained.get(old_target_id).unwrap();
        let new_target_id = "tgt_ffffffffffffffffffffffffffffffff";
        let new_bytes = br#"{"targetId":"tgt_ffffffffffffffffffffffffffffffff","value":"new"}"#;
        let nonce = "cccccccccccccccccccccccccccccccc";
        let journal = TargetReplaceJournal {
            schema_version: STORAGE_SCHEMA.to_owned(),
            project_id: PROJECT_ID.to_owned(),
            old_target_id: old_target_id.to_owned(),
            old_sha256: sha256(old_bytes),
            old_byte_length: old_bytes.len(),
            new_target_id: new_target_id.to_owned(),
            new_sha256: sha256(new_bytes),
            new_byte_length: new_bytes.len(),
            nonce: nonce.to_owned(),
        };
        let project_root = storage.project_root(PROJECT_ID);
        let targets_root = project_root.join("targets");
        let old_path = targets_root.join(format!("{old_target_id}.json"));
        let new_path = targets_root.join(format!("{new_target_id}.json"));
        let staging = targets_root.join(format!(".replacing-target-{new_target_id}.{nonce}.tmp"));
        write_new_file(&staging, new_bytes).unwrap();
        write_immutable_json(&project_root.join("target-transaction.json"), &journal).unwrap();
        (journal, old_path, new_path, staging)
    }

    #[test]
    fn target_replace_is_journaled_atomic_and_capacity_safe() {
        let (root, storage) = storage_with_project();
        let retained = fill_target_capacity(&storage);
        let old_target_id = "tgt_00000000000000000000000000000000";
        let old_bytes = retained.get(old_target_id).unwrap();
        let new_target_id = "tgt_ffffffffffffffffffffffffffffffff";
        let new_bytes = br#"{"targetId":"tgt_ffffffffffffffffffffffffffffffff","value":"new"}"#;
        storage
            .replace_target(
                PROJECT_ID,
                old_target_id,
                old_bytes,
                new_target_id,
                new_bytes,
            )
            .unwrap();
        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects[0].targets.len(), MAX_PERSISTED_TARGETS);
        assert!(
            !projects[0]
                .targets
                .iter()
                .any(|(id, _)| id == old_target_id)
        );
        assert!(
            projects[0]
                .targets
                .iter()
                .any(|(id, bytes)| id == new_target_id && bytes == new_bytes)
        );
    }

    #[test]
    fn target_replace_restart_converges_from_every_published_state_including_33_files() {
        for stage in 0..3 {
            let (root, storage) = storage_with_project();
            let (journal, old_path, new_path, staging) = target_replace_fixture(&storage);
            if stage >= 1 {
                fs::rename(&staging, &new_path).unwrap();
                assert_eq!(
                    fs::read_dir(storage.project_root(PROJECT_ID).join("targets"))
                        .unwrap()
                        .count(),
                    33
                );
            }
            if stage >= 2 {
                fs::remove_file(&old_path).unwrap();
            }
            let (_, projects) = ManagedStorage::open(root.path()).unwrap();
            assert_eq!(projects[0].targets.len(), MAX_PERSISTED_TARGETS);
            assert!(!old_path.exists());
            assert!(!staging.exists());
            assert_eq!(fs::read(&new_path).unwrap().len(), journal.new_byte_length);
            assert!(
                !storage
                    .project_root(PROJECT_ID)
                    .join("target-transaction.json")
                    .exists()
            );
        }
    }

    #[test]
    fn target_recovery_removes_only_exact_orphan_staging_and_rejects_lookalikes() {
        let (root, storage) = storage_with_project();
        fill_target_capacity(&storage);
        let targets_root = storage.project_root(PROJECT_ID).join("targets");
        let exact = targets_root.join(
            ".replacing-target-tgt_ffffffffffffffffffffffffffffffff.dddddddddddddddddddddddddddddddd.tmp",
        );
        fs::write(&exact, b"orphan").unwrap();
        assert_eq!(ManagedStorage::open(root.path()).unwrap().1.len(), 1);
        assert!(!exact.exists());

        let lookalike = targets_root
            .join(".replacing-target-tgt_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee.not-a-nonce.tmp");
        fs::write(&lookalike, b"unknown").unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(lookalike.exists());
    }

    #[test]
    fn corrupt_target_journal_and_migration_lookalike_are_preserved_fail_closed() {
        let (root, storage) = storage_with_project();
        let (_journal, old_path, _new_path, staging) = target_replace_fixture(&storage);
        let journal_path = storage
            .project_root(PROJECT_ID)
            .join("target-transaction.json");
        let mut noncanonical = fs::read(&journal_path).unwrap();
        noncanonical.push(b'\n');
        fs::write(&journal_path, noncanonical).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(old_path.exists());
        assert!(staging.exists());
        assert!(journal_path.exists());

        let (root, storage) = storage_with_project();
        let lookalike = storage
            .root
            .join("projects")
            .join(format!(".migrating-{PROJECT_ID}.not-a-nonce"));
        fs::create_dir(&lookalike).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(lookalike.exists());
    }

    #[test]
    fn generated_artifact_deletion_requires_exact_binding_and_recovers_owned_staging() {
        let (root, storage) = storage_with_project();
        let export_bytes = b"PK\x03\x04deletable export";
        storage
            .persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                export_bytes,
                br#"{"receipt":"delete"}"#,
            )
            .unwrap();
        let export_sha = sha256(export_bytes);
        assert!(matches!(
            storage.delete_export(PROJECT_ID, EXPORT_ID, &export_sha, b"wrong"),
            Err(StorageError::Corrupt)
        ));
        storage
            .delete_export(PROJECT_ID, EXPORT_ID, &export_sha, export_bytes)
            .unwrap();
        assert!(
            ManagedStorage::open(root.path()).unwrap().1[0]
                .exports
                .is_empty()
        );

        let publication_bytes = b"PK\x03\x04deletable publication";
        storage
            .persist_publication(
                PROJECT_ID,
                PUBLICATION_ID,
                REVISION_ID,
                publication_bytes,
                br#"{"files":[]}"#,
            )
            .unwrap();
        let publications = storage.project_root(PROJECT_ID).join("publications");
        let deleting = publications.join(format!(
            ".deleting-publication-{PUBLICATION_ID}.eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        ));
        fs::rename(publications.join(PUBLICATION_ID), &deleting).unwrap();
        assert!(
            ManagedStorage::open(root.path()).unwrap().1[0]
                .publications
                .is_empty()
        );
        assert!(!deleting.exists());

        let lookalike = publications.join(format!(
            ".deleting-publication-{PUBLICATION_ID}.not-a-nonce"
        ));
        fs::create_dir(&lookalike).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(lookalike.exists());
    }

    #[test]
    fn proposal_deletion_is_exact_and_preserves_synapse_history() {
        let (root, storage) = storage_with_project();
        let proposal_id = "pro_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let metadata = br#"{"proposal":"failed"}"#;
        let binding = br#"{"binding":"immutable"}"#;
        let shared = b"<h1>safe</h1>";
        let unique = b"failed proposal unique CAS canary";
        let shared_object = storage.object_path(&sha256(shared));
        let unique_object = storage.object_path(&sha256(unique));
        storage
            .prepare_proposal(
                PROJECT_ID,
                proposal_id,
                &BTreeMap::from([
                    ("index.html".to_owned(), shared.to_vec()),
                    ("proposal-canary.txt".to_owned(), unique.to_vec()),
                ]),
                metadata,
            )
            .unwrap();
        assert!(shared_object.exists());
        assert!(unique_object.exists());
        storage
            .persist_proposal_binding(PROJECT_ID, proposal_id, binding)
            .unwrap();
        let synapse_history = storage
            .project_root(PROJECT_ID)
            .join("synapse")
            .join("immutable-history");
        fs::write(&synapse_history, b"preserve").unwrap();
        assert!(matches!(
            storage.delete_proposal(
                PROJECT_ID,
                proposal_id,
                &sha256(metadata),
                metadata,
                Some((&sha256(binding), b"wrong")),
            ),
            Err(StorageError::Corrupt)
        ));
        storage
            .delete_proposal(
                PROJECT_ID,
                proposal_id,
                &sha256(metadata),
                metadata,
                Some((&sha256(binding), binding)),
            )
            .unwrap();
        assert!(shared_object.exists());
        assert!(!unique_object.exists());
        assert_eq!(fs::read(&synapse_history).unwrap(), b"preserve");
        assert!(
            ManagedStorage::open(root.path()).unwrap().1[0]
                .proposals
                .is_empty()
        );

        let proposal_id = "pro_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let restart_unique = b"restart proposal unique CAS canary";
        let restart_unique_object = storage.object_path(&sha256(restart_unique));
        storage
            .prepare_proposal(
                PROJECT_ID,
                proposal_id,
                &BTreeMap::from([("restart.txt".to_owned(), restart_unique.to_vec())]),
                metadata,
            )
            .unwrap();
        let proposals_root = storage.project_root(PROJECT_ID).join("proposals");
        let deleting = proposals_root.join(format!(
            ".deleting-proposal-{proposal_id}.ffffffffffffffffffffffffffffffff"
        ));
        fs::rename(proposals_root.join(proposal_id), &deleting).unwrap();
        assert!(
            ManagedStorage::open(root.path()).unwrap().1[0]
                .proposals
                .is_empty()
        );
        assert!(!deleting.exists());
        assert!(!restart_unique_object.exists());

        let creating_id = "pro_cccccccccccccccccccccccccccccccc";
        let creating_unique = b"creating proposal known orphan";
        let creating_unique_object = storage.object_path(&sha256(creating_unique));
        storage
            .prepare_proposal(
                PROJECT_ID,
                creating_id,
                &BTreeMap::from([("creating.txt".to_owned(), creating_unique.to_vec())]),
                metadata,
            )
            .unwrap();
        let creating = proposals_root.join(format!(
            ".creating-proposal-{creating_id}.eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        ));
        fs::rename(proposals_root.join(creating_id), &creating).unwrap();
        assert_eq!(ManagedStorage::open(root.path()).unwrap().1.len(), 1);
        assert!(!creating.exists());
        assert!(!creating_unique_object.exists());

        let lookalike =
            proposals_root.join(format!(".deleting-proposal-{proposal_id}.not-a-nonce"));
        fs::create_dir(&lookalike).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(lookalike.exists());
    }

    #[cfg(unix)]
    #[test]
    fn proposal_cas_cleanup_rejects_links_without_touching_external_data() {
        use std::os::unix::fs::symlink;

        let (root, storage) = storage_with_project();
        let proposal_id = "pro_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let metadata = br#"{"proposal":"linked-cas"}"#;
        let unique = b"hard-linked proposal CAS canary";
        storage
            .prepare_proposal(
                PROJECT_ID,
                proposal_id,
                &BTreeMap::from([("canary.txt".to_owned(), unique.to_vec())]),
                metadata,
            )
            .unwrap();
        let object = storage.object_path(&sha256(unique));
        let external = tempfile::tempdir().unwrap();
        let sentinel = external.path().join("hard-link-sentinel");
        fs::hard_link(&object, &sentinel).unwrap();
        assert!(matches!(
            storage.delete_proposal(PROJECT_ID, proposal_id, &sha256(metadata), metadata, None,),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(&sentinel).unwrap(), unique);
        assert!(object.exists());
        fs::remove_file(&sentinel).unwrap();
        assert_eq!(ManagedStorage::open(root.path()).unwrap().1.len(), 1);
        assert!(!object.exists());

        let (_root, storage) = storage_with_project();
        let proposal_id = "pro_ffffffffffffffffffffffffffffffff";
        let unique = b"symlinked proposal CAS canary";
        storage
            .prepare_proposal(
                PROJECT_ID,
                proposal_id,
                &BTreeMap::from([("canary.txt".to_owned(), unique.to_vec())]),
                metadata,
            )
            .unwrap();
        let object = storage.object_path(&sha256(unique));
        let external = tempfile::tempdir().unwrap();
        let sentinel = external.path().join("symlink-sentinel");
        fs::write(&sentinel, unique).unwrap();
        fs::remove_file(&object).unwrap();
        symlink(&sentinel, &object).unwrap();
        assert!(matches!(
            storage.delete_proposal(PROJECT_ID, proposal_id, &sha256(metadata), metadata, None,),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(&sentinel).unwrap(), unique);
        assert!(
            fs::symlink_metadata(&object)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn project_deletion_removes_verified_external_backups_and_recovers_after_rename() {
        let (root, storage) = storage_with_project();
        make_legacy_project(&storage);
        ManagedStorage::open(root.path()).unwrap();
        assert_eq!(
            fs::read_dir(storage.root.join(RECOVERY_BACKUPS_DIRECTORY))
                .unwrap()
                .count(),
            1
        );
        storage
            .delete_project(PROJECT_ID, REVISION_ID, ARTIFACT_SHA)
            .unwrap();
        assert!(!storage.project_root(PROJECT_ID).exists());
        assert_eq!(
            fs::read_dir(storage.root.join(RECOVERY_BACKUPS_DIRECTORY))
                .unwrap()
                .count(),
            0
        );
        assert!(ManagedStorage::open(root.path()).unwrap().1.is_empty());

        let (root, storage) = storage_with_project();
        let shared = b"<h1>safe</h1>";
        let revision_unique = b"deleted project revision CAS canary";
        let proposal_unique = b"deleted project proposal CAS canary";
        storage
            .commit_revision(
                PROJECT_ID,
                NEXT_REVISION_ID,
                NEXT_ARTIFACT_SHA,
                &BTreeMap::from([
                    ("index.html".to_owned(), shared.to_vec()),
                    ("revision-canary.txt".to_owned(), revision_unique.to_vec()),
                ]),
            )
            .unwrap();
        storage
            .prepare_proposal(
                PROJECT_ID,
                "pro_dddddddddddddddddddddddddddddddd",
                &BTreeMap::from([("proposal-canary.txt".to_owned(), proposal_unique.to_vec())]),
                br#"{"proposal":"project-delete"}"#,
            )
            .unwrap();
        storage
            .create_project(
                SECOND_PROJECT_ID,
                "Shared CAS survivor",
                SECOND_REVISION_ID,
                SECOND_ARTIFACT_SHA,
                &BTreeMap::from([("index.html".to_owned(), shared.to_vec())]),
            )
            .unwrap();
        let survivor_nonce = "abababababababababababababababab";
        let survivor_journal = MigrationJournal {
            schema_version: STORAGE_SCHEMA.to_owned(),
            target_layout_version: PROJECT_LAYOUT_VERSION.to_owned(),
            migration_id: format!("mig_{survivor_nonce}"),
            backup_id: format!("bkp_{survivor_nonce}"),
            project_id: SECOND_PROJECT_ID.to_owned(),
        };
        let backups_root = storage.root.join(RECOVERY_BACKUPS_DIRECTORY);
        let survivor_backup = backups_root.join(&survivor_journal.backup_id);
        let survivor_staging = backups_root.join(format!(
            ".creating-backup-{}.{}",
            survivor_journal.backup_id, survivor_nonce
        ));
        storage
            .create_backup(
                &storage.project_root(SECOND_PROJECT_ID),
                &survivor_staging,
                &survivor_backup,
                &survivor_journal,
            )
            .unwrap();
        let shared_object = storage.object_path(&sha256(shared));
        let revision_unique_object = storage.object_path(&sha256(revision_unique));
        let proposal_unique_object = storage.object_path(&sha256(proposal_unique));
        make_legacy_project(&storage);
        ManagedStorage::open(root.path()).unwrap();
        let nonce = "99999999999999999999999999999999";
        let deleting = storage
            .root
            .join("projects")
            .join(format!(".deleting-project-{PROJECT_ID}.{nonce}"));
        fs::rename(storage.project_root(PROJECT_ID), &deleting).unwrap();
        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, SECOND_PROJECT_ID);
        assert!(!deleting.exists());
        assert!(shared_object.exists());
        assert!(!revision_unique_object.exists());
        assert!(!proposal_unique_object.exists());
        assert_eq!(
            fs::read_dir(storage.root.join(RECOVERY_BACKUPS_DIRECTORY))
                .unwrap()
                .count(),
            1
        );
        assert_eq!(
            verify_backup_root(&survivor_backup, None)
                .unwrap()
                .project_id,
            SECOND_PROJECT_ID
        );
    }

    #[test]
    fn project_deletion_preserves_lookalikes_and_refuses_corrupt_backup() {
        let (root, storage) = storage_with_project();
        let lookalike = storage
            .root
            .join("projects")
            .join(format!(".deleting-project-{PROJECT_ID}.not-a-nonce"));
        fs::create_dir(&lookalike).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(lookalike.exists());

        let (_root, storage) = storage_with_project();
        make_legacy_project(&storage);
        storage.migrate_legacy_project(PROJECT_ID).unwrap();
        let backup = bounded_directory_entries(&storage.root.join(RECOVERY_BACKUPS_DIRECTORY), 2)
            .unwrap()
            .remove(0)
            .path();
        fs::write(
            backup.join("project").join("site").join("index.html"),
            b"evil",
        )
        .unwrap();
        assert!(matches!(
            storage.delete_project(PROJECT_ID, REVISION_ID, ARTIFACT_SHA),
            Err(StorageError::Corrupt)
        ));
        assert!(storage.project_root(PROJECT_ID).exists());
        assert!(backup.exists());
    }

    #[cfg(unix)]
    #[test]
    fn manual_deletion_rejects_nested_symlinks_without_touching_external_data() {
        use std::os::unix::fs::symlink;

        let (_root, storage) = storage_with_project();
        let external = tempfile::tempdir().unwrap();
        let sentinel = external.path().join("sentinel");
        fs::write(&sentinel, b"preserve").unwrap();
        symlink(
            external.path(),
            storage
                .project_root(PROJECT_ID)
                .join("synapse")
                .join("outside"),
        )
        .unwrap();
        assert!(matches!(
            storage.delete_project(PROJECT_ID, REVISION_ID, ARTIFACT_SHA),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
        assert!(storage.project_root(PROJECT_ID).exists());
    }

    #[test]
    fn retained_revision_verification_is_canonical_and_binds_expected_artifact() {
        let (_root, storage) = storage_with_project();
        storage
            .commit_revision(
                PROJECT_ID,
                NEXT_REVISION_ID,
                NEXT_ARTIFACT_SHA,
                &BTreeMap::from([("index.html".into(), b"<h1>next</h1>".to_vec())]),
            )
            .unwrap();

        storage
            .verify_retained_revision(PROJECT_ID, REVISION_ID, ARTIFACT_SHA)
            .unwrap();
        assert!(matches!(
            storage.verify_retained_revision(PROJECT_ID, REVISION_ID, NEXT_ARTIFACT_SHA),
            Err(StorageError::Corrupt)
        ));

        let original = b"<h1>safe</h1>";
        let object = storage.object_path(&sha256(original));
        fs::write(&object, b"<h1>evil</h1>").unwrap();
        assert!(matches!(
            storage.verify_retained_revision(PROJECT_ID, REVISION_ID, ARTIFACT_SHA),
            Err(StorageError::Corrupt)
        ));
        fs::write(&object, original).unwrap();
        storage
            .verify_retained_revision(PROJECT_ID, REVISION_ID, ARTIFACT_SHA)
            .unwrap();

        let manifest_path = storage
            .project_root(PROJECT_ID)
            .join("revisions")
            .join(format!("{REVISION_ID}.json"));
        let mut noncanonical = fs::read(&manifest_path).unwrap();
        noncanonical.push(b'\n');
        fs::write(manifest_path, noncanonical).unwrap();
        assert!(matches!(
            storage.verify_retained_revision(PROJECT_ID, REVISION_ID, ARTIFACT_SHA),
            Err(StorageError::Corrupt)
        ));
    }

    #[test]
    fn proposal_admission_enforces_128_record_boundary_without_bricking_restart() {
        let (root, storage) = storage_with_project();
        let files = BTreeMap::new();
        for index in 0..MAX_PERSISTED_PROPOSALS {
            storage
                .prepare_proposal(
                    PROJECT_ID,
                    &format!("pro_{index:032x}"),
                    &files,
                    br#"{"schemaVersion":"1"}"#,
                )
                .unwrap();
        }

        let overflow_id = "pro_ffffffffffffffffffffffffffffffff";
        assert!(matches!(
            storage.prepare_proposal(PROJECT_ID, overflow_id, &files, br#"{"schemaVersion":"1"}"#,),
            Err(StorageError::Corrupt)
        ));
        let proposals_root = storage.project_root(PROJECT_ID).join("proposals");
        assert!(!proposals_root.join(overflow_id).exists());
        assert_eq!(fs::read_dir(&proposals_root).unwrap().count(), 128);

        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].proposals.len(), MAX_PERSISTED_PROPOSALS);
    }

    #[test]
    fn proposal_admission_recovers_only_exact_owned_staging() {
        let (root, storage) = storage_with_project();
        let proposals_root = storage.project_root(PROJECT_ID).join("proposals");
        let owned = proposals_root.join(
            ".creating-proposal-pro_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.11111111111111111111111111111111",
        );
        fs::create_dir(&owned).unwrap();
        fs::create_dir(owned.join("site")).unwrap();
        let partial = b"known materialized proposal orphan";
        fs::write(owned.join("site").join("partial.txt"), partial).unwrap();
        let partial_digest = sha256(partial);
        storage.write_object(&partial_digest, partial).unwrap();
        storage
            .prepare_proposal(
                PROJECT_ID,
                "pro_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                &BTreeMap::new(),
                br#"{"schemaVersion":"1"}"#,
            )
            .unwrap();
        assert!(!owned.exists());
        assert!(!storage.object_path(&partial_digest).exists());
        assert_eq!(ManagedStorage::open(root.path()).unwrap().1.len(), 1);

        let (_root, storage) = storage_with_project();
        let proposals_root = storage.project_root(PROJECT_ID).join("proposals");
        let foreign = proposals_root.join("foreign-entry");
        fs::create_dir(&foreign).unwrap();
        assert!(matches!(
            storage.prepare_proposal(
                PROJECT_ID,
                "pro_cccccccccccccccccccccccccccccccc",
                &BTreeMap::new(),
                br#"{"schemaVersion":"1"}"#,
            ),
            Err(StorageError::Corrupt)
        ));
        assert!(foreign.exists());

        let (_root, storage) = storage_with_project();
        let proposals_root = storage.project_root(PROJECT_ID).join("proposals");
        let lookalike = proposals_root.join(
            ".creating-proposal-pro_dddddddddddddddddddddddddddddddd.2222222222222222222222222222222x",
        );
        fs::create_dir(&lookalike).unwrap();
        assert!(matches!(
            storage.prepare_proposal(
                PROJECT_ID,
                "pro_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                &BTreeMap::new(),
                br#"{"schemaVersion":"1"}"#,
            ),
            Err(StorageError::Corrupt)
        ));
        assert!(lookalike.exists());
    }

    #[test]
    fn proposal_staging_enumeration_is_bounded_before_recovery() {
        let (_root, storage) = storage_with_project();
        let proposals_root = storage.project_root(PROJECT_ID).join("proposals");
        for index in 0..=MAX_PROPOSAL_STAGING_ENTRIES {
            fs::create_dir(proposals_root.join(format!(
                ".creating-proposal-pro_{index:032x}.11111111111111111111111111111111"
            )))
            .unwrap();
        }
        assert!(matches!(
            storage.prepare_proposal(
                PROJECT_ID,
                "pro_ffffffffffffffffffffffffffffffff",
                &BTreeMap::new(),
                br#"{"schemaVersion":"1"}"#,
            ),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read_dir(proposals_root).unwrap().count(), 17);
    }

    #[test]
    fn generated_artifacts_are_atomic_idempotent_and_rehydrate_exact_bytes() {
        let (root, storage) = storage_with_project();
        let export_zip = b"PK\x03\x04exact static export";
        let export_receipt = b"{\"schema\":\"export-receipt\",\"version\":1}\n";
        let publication_zip = b"PK\x03\x04exact publication bundle";
        let publication_metadata =
            b"{\"files\":[{\"path\":\"projection.json\"}],\"networkWrites\":false}";

        storage
            .persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                export_zip,
                export_receipt,
            )
            .unwrap();
        storage
            .persist_publication(
                PROJECT_ID,
                PUBLICATION_ID,
                REVISION_ID,
                publication_zip,
                publication_metadata,
            )
            .unwrap();
        storage
            .persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                export_zip,
                export_receipt,
            )
            .unwrap();
        assert!(matches!(
            storage.persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                b"PK\x03\x04different",
                export_receipt,
            ),
            Err(StorageError::Corrupt)
        ));

        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].exports,
            vec![PersistedExport {
                id: EXPORT_ID.to_owned(),
                project_id: PROJECT_ID.to_owned(),
                revision_id: REVISION_ID.to_owned(),
                archive_sha256: sha256(export_zip),
                archive_zip: export_zip.to_vec(),
                receipt_json: export_receipt.to_vec(),
            }]
        );
        assert_eq!(
            projects[0].publications,
            vec![PersistedPublication {
                id: PUBLICATION_ID.to_owned(),
                project_id: PROJECT_ID.to_owned(),
                revision_id: REVISION_ID.to_owned(),
                archive_sha256: sha256(publication_zip),
                archive_zip: publication_zip.to_vec(),
                bundle_metadata_json: publication_metadata.to_vec(),
            }]
        );
        let export_root = storage
            .project_root(PROJECT_ID)
            .join("exports")
            .join(EXPORT_ID);
        assert_eq!(
            fs::read_dir(export_root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "archive.zip".to_owned(),
                "manifest.json".to_owned(),
                "metadata.json".to_owned(),
            ])
        );
    }

    #[test]
    fn generated_artifact_load_rejects_byte_metadata_and_inventory_tampering() {
        let (root, storage) = storage_with_project();
        storage
            .persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                b"PK\x03\x04archive-a",
                br#"{"receipt":"safe"}"#,
            )
            .unwrap();
        let export_root = storage
            .project_root(PROJECT_ID)
            .join("exports")
            .join(EXPORT_ID);
        fs::write(export_root.join("archive.zip"), b"PK\x03\x04archive-b").unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));

        let (root, storage) = storage_with_project();
        storage
            .persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                b"PK\x03\x04archive",
                br#"{"receipt":"safe"}"#,
            )
            .unwrap();
        let export_root = storage
            .project_root(PROJECT_ID)
            .join("exports")
            .join(EXPORT_ID);
        let duplicate_keys = br#"{"receipt":"first","receipt":"second"}"#;
        fs::write(export_root.join("metadata.json"), duplicate_keys).unwrap();
        let mut manifest: GeneratedArtifactManifest =
            read_json(&export_root.join("manifest.json")).unwrap();
        manifest.metadata_sha256 = sha256(duplicate_keys);
        manifest.metadata_byte_length = duplicate_keys.len();
        fs::write(
            export_root.join("manifest.json"),
            canonical_json(&manifest).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));

        let (root, storage) = storage_with_project();
        storage
            .persist_publication(
                PROJECT_ID,
                PUBLICATION_ID,
                REVISION_ID,
                b"PK\x03\x04publication",
                br#"{"files":[]}"#,
            )
            .unwrap();
        fs::write(
            storage
                .project_root(PROJECT_ID)
                .join("publications")
                .join(PUBLICATION_ID)
                .join("unexpected.txt"),
            b"not part of the immutable record",
        )
        .unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
    }

    #[test]
    fn generated_artifact_staging_recovery_is_exact_and_fail_closed() {
        let (root, storage) = storage_with_project();
        let exports = storage.project_root(PROJECT_ID).join("exports");
        let publications = storage.project_root(PROJECT_ID).join("publications");
        let export_staging = exports.join(
            ".creating-export-exp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.11111111111111111111111111111111",
        );
        let publication_staging = publications.join(
            ".creating-publication-pub_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.22222222222222222222222222222222",
        );
        fs::create_dir(&export_staging).unwrap();
        fs::write(export_staging.join("partial.zip"), b"partial").unwrap();
        fs::create_dir(&publication_staging).unwrap();
        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(!export_staging.exists());
        assert!(!publication_staging.exists());

        fs::create_dir(
            exports.join(".creating-export-exp_cccccccccccccccccccccccccccccccc.not-a-valid-nonce"),
        )
        .unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
    }

    #[test]
    fn generated_artifact_inputs_and_enumeration_are_strictly_bounded() {
        let (_root, storage) = storage_with_project();
        for (id, archive, metadata) in [
            ("exp_bad", b"PK".as_slice(), br#"{"ok":true}"#.as_slice()),
            (EXPORT_ID, b"".as_slice(), br#"{"ok":true}"#.as_slice()),
            (EXPORT_ID, b"PK".as_slice(), b"[]".as_slice()),
            (
                EXPORT_ID,
                b"PK".as_slice(),
                br#"{"duplicate":1,"duplicate":2}"#.as_slice(),
            ),
        ] {
            assert!(matches!(
                storage.persist_export(PROJECT_ID, id, REVISION_ID, archive, metadata),
                Err(StorageError::Corrupt)
            ));
        }
        assert!(matches!(
            storage.persist_export(
                PROJECT_ID,
                EXPORT_ID,
                "rev_99999999999999999999999999999999",
                b"PK",
                br#"{"ok":true}"#,
            ),
            Err(StorageError::Corrupt)
        ));
        assert!(matches!(
            storage.persist_export(
                PROJECT_ID,
                EXPORT_ID,
                REVISION_ID,
                b"PK",
                &vec![b' '; MAX_GENERATED_METADATA_BYTES + 1],
            ),
            Err(StorageError::Corrupt)
        ));
        for index in 0..MAX_PERSISTED_EXPORTS {
            storage
                .persist_export(
                    PROJECT_ID,
                    &format!("exp_{index:032x}"),
                    REVISION_ID,
                    b"PK",
                    br#"{"ok":true}"#,
                )
                .unwrap();
        }
        assert!(matches!(
            storage.persist_export(
                PROJECT_ID,
                "exp_ffffffffffffffffffffffffffffffff",
                REVISION_ID,
                b"PK",
                br#"{"ok":true}"#,
            ),
            Err(StorageError::Corrupt)
        ));
    }

    #[test]
    fn initial_transaction_without_pointer_is_completed_on_open() {
        let root = tempfile::tempdir().unwrap();
        let (storage, projects) = ManagedStorage::open(root.path()).unwrap();
        assert!(projects.is_empty());
        let files = BTreeMap::from([("index.html".into(), b"<h1>safe</h1>".to_vec())]);
        storage
            .create_project(
                PROJECT_ID,
                "Crash recovery",
                REVISION_ID,
                ARTIFACT_SHA,
                &files,
            )
            .unwrap();

        let project_root = storage.project_root(PROJECT_ID);
        let pointer: CurrentPointer = read_json(&project_root.join("current.json")).unwrap();
        write_atomic_json(
            &project_root.join("transaction.json"),
            &TransactionMarker {
                schema_version: STORAGE_SCHEMA.into(),
                target_revision_id: pointer.revision_id.clone(),
                canonical_manifest_sha256: pointer.canonical_manifest_sha256.clone(),
                artifact_manifest_sha256: pointer.artifact_manifest_sha256.clone(),
            },
        )
        .unwrap();
        write_immutable_json(
            &project_root.join("creating.json"),
            &CreationMarker {
                schema_version: STORAGE_SCHEMA.into(),
            },
        )
        .unwrap();
        fs::remove_file(project_root.join("current.json")).unwrap();

        let (reopened, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, PROJECT_ID);
        assert_eq!(projects[0].files, files);
        reopened
            .verify_project(PROJECT_ID, REVISION_ID, ARTIFACT_SHA)
            .unwrap();
        assert!(!project_root.join("transaction.json").exists());
        assert!(!project_root.join("creating.json").exists());
    }

    #[test]
    fn restart_removes_only_exact_private_atomic_and_revision_temporaries() {
        let (root, storage) = storage_with_project();
        let project_root = storage.project_root(PROJECT_ID);
        let current_temp = project_root.join(".current.json.11111111111111111111111111111111.tmp");
        let transaction_temp =
            project_root.join(".transaction.json.22222222222222222222222222222222.tmp");
        let revision_temp = project_root.join("revisions").join(format!(
            ".{NEXT_REVISION_ID}.json.33333333333333333333333333333333.immutable.tmp"
        ));
        for temporary in [&current_temp, &transaction_temp, &revision_temp] {
            write_new_file(temporary, b"owned interrupted bytes").unwrap();
        }

        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(!current_temp.exists());
        assert!(!transaction_temp.exists());
        assert!(!revision_temp.exists());

        let unknown_atomic = project_root.join(".current.json.not-a-nonce.tmp");
        let unknown_revision = project_root.join("revisions").join(format!(
            ".{NEXT_REVISION_ID}.json.not-a-nonce.immutable.tmp"
        ));
        fs::write(&unknown_atomic, b"unknown bytes").unwrap();
        fs::write(&unknown_revision, b"unknown revision bytes").unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(unknown_atomic).unwrap(), b"unknown bytes");
        assert_eq!(
            fs::read(unknown_revision).unwrap(),
            b"unknown revision bytes"
        );
    }

    #[test]
    fn project_metadata_update_is_atomic_cas_and_restart_durable() {
        let (root, storage) = storage_with_project();
        assert_eq!(
            storage
                .update_project_display_name(
                    PROJECT_ID,
                    "Generated artifact persistence",
                    "Campaign LP",
                )
                .unwrap(),
            ProjectMetadataUpdate::Updated
        );
        assert_eq!(
            storage
                .update_project_display_name(
                    PROJECT_ID,
                    "Generated artifact persistence",
                    "Campaign LP",
                )
                .unwrap(),
            ProjectMetadataUpdate::Idempotent
        );
        assert_eq!(
            storage
                .update_project_display_name(PROJECT_ID, "stale", "Overwrite")
                .unwrap(),
            ProjectMetadataUpdate::Stale
        );

        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, PROJECT_ID);
        assert_eq!(projects[0].display_name, "Campaign LP");
        assert_eq!(projects[0].revision_id, REVISION_ID);
        assert_eq!(projects[0].artifact_manifest_sha256, ARTIFACT_SHA);
        assert_eq!(projects[0].files["index.html"], b"<h1>safe</h1>");
    }

    #[test]
    fn project_metadata_outcome_ambiguous_replace_is_exactly_reconciled() {
        for (failpoint, occurrence) in [
            (Failpoint::StorageRenameAfter, 1),
            // File fsync is occurrence one; the parent fsync after replace is two.
            (Failpoint::StorageFsyncAfter, 2),
        ] {
            let (root, storage) = storage_with_project();
            let _scenario = fault_injection::testing::Scenario::begin();
            fault_injection::testing::fail_on(failpoint, occurrence);
            assert_eq!(
                storage
                    .update_project_display_name(
                        PROJECT_ID,
                        "Generated artifact persistence",
                        "Durably reconciled",
                    )
                    .unwrap(),
                ProjectMetadataUpdate::Updated
            );
            drop(_scenario);
            let (_, projects) = ManagedStorage::open(root.path()).unwrap();
            assert_eq!(projects[0].display_name, "Durably reconciled");
            assert_eq!(projects[0].revision_id, REVISION_ID);
            assert_eq!(projects[0].artifact_manifest_sha256, ARTIFACT_SHA);
        }

        let (_root, storage) = storage_with_project();
        let _scenario = fault_injection::testing::Scenario::begin();
        fault_injection::testing::fail_on(Failpoint::StorageRenameBefore, 1);
        assert!(matches!(
            storage.update_project_display_name(
                PROJECT_ID,
                "Generated artifact persistence",
                "Must not appear",
            ),
            Err(StorageError::Io(_))
        ));
        let metadata: ProjectMetadata =
            read_json(&storage.project_root(PROJECT_ID).join("project.json")).unwrap();
        assert_eq!(metadata.display_name, "Generated artifact persistence");
    }

    #[test]
    fn project_metadata_restart_cleanup_is_exact_and_fail_closed() {
        let (root, storage) = storage_with_project();
        let project_root = storage.project_root(PROJECT_ID);
        let owned = project_root.join(".project.json.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.tmp");
        write_new_file(&owned, b"interrupted canonical replacement").unwrap();
        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert!(!owned.exists());

        let unknown = project_root.join(".project.json.not-a-nonce.tmp");
        fs::write(&unknown, b"unknown metadata temporary").unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(fs::read(&unknown).unwrap(), b"unknown metadata temporary");
    }

    #[cfg(unix)]
    #[test]
    fn project_metadata_symlink_and_hardlink_aliases_fail_closed() {
        use std::os::unix::fs::symlink;

        let (root, storage) = storage_with_project();
        let project_root = storage.project_root(PROJECT_ID);
        let outside = root.path().join("outside-metadata");
        fs::write(&outside, b"outside").unwrap();
        let owned = project_root.join(".project.json.bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.tmp");
        symlink(&outside, &owned).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(owned.exists());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");

        fs::remove_file(&owned).unwrap();
        fs::hard_link(project_root.join("project.json"), &outside).unwrap_err();
        let alias = root.path().join("project-metadata-alias");
        fs::hard_link(project_root.join("project.json"), &alias).unwrap();
        assert!(matches!(
            storage.update_project_display_name(
                PROJECT_ID,
                "Generated artifact persistence",
                "Hard link rejected",
            ),
            Err(StorageError::Corrupt)
        ));
        assert_eq!(
            fs::read(project_root.join("project.json")).unwrap(),
            fs::read(alias).unwrap()
        );
    }

    #[test]
    fn injected_storage_failures_retain_static_os_error_kinds() {
        for (failpoint, failure_kind, expected) in [
            (
                Failpoint::StorageWriteBefore,
                fault_injection::InjectedFailureKind::StorageFull,
                io::ErrorKind::StorageFull,
            ),
            (
                Failpoint::StorageRenameBefore,
                fault_injection::InjectedFailureKind::PermissionDenied,
                io::ErrorKind::PermissionDenied,
            ),
        ] {
            let _scenario = fault_injection::testing::Scenario::begin();
            fault_injection::testing::fail_on_kind(failpoint, 1, failure_kind);
            let error = injected_storage_fault(failpoint).unwrap_err();
            let StorageError::Io(error) = error else {
                panic!("injected storage fault must remain an I/O error");
            };
            assert_eq!(error.kind(), expected);
            assert!(!error.to_string().contains('/'));
            assert!(!error.to_string().contains("rationale"));
        }
    }

    #[test]
    fn recovery_rejects_same_revision_with_different_pointer_hashes() {
        let root = tempfile::tempdir().unwrap();
        let (storage, _) = ManagedStorage::open(root.path()).unwrap();
        let files = BTreeMap::from([("index.html".into(), b"<h1>safe</h1>".to_vec())]);
        storage
            .create_project(
                PROJECT_ID,
                "Pointer binding",
                REVISION_ID,
                ARTIFACT_SHA,
                &files,
            )
            .unwrap();
        let project_root = storage.project_root(PROJECT_ID);
        let pointer: CurrentPointer = read_json(&project_root.join("current.json")).unwrap();
        write_atomic_json(
            &project_root.join("transaction.json"),
            &TransactionMarker {
                schema_version: STORAGE_SCHEMA.into(),
                target_revision_id: pointer.revision_id,
                canonical_manifest_sha256: pointer.canonical_manifest_sha256,
                artifact_manifest_sha256:
                    "4444444444444444444444444444444444444444444444444444444444444444".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
    }

    #[test]
    fn runtime_and_restart_reject_tampered_current_artifact_hash() {
        let root = tempfile::tempdir().unwrap();
        let (storage, _) = ManagedStorage::open(root.path()).unwrap();
        let files = BTreeMap::from([("index.html".into(), b"<h1>safe</h1>".to_vec())]);
        storage
            .create_project(
                PROJECT_ID,
                "Current pointer binding",
                REVISION_ID,
                ARTIFACT_SHA,
                &files,
            )
            .unwrap();
        let current_path = storage.project_root(PROJECT_ID).join("current.json");
        let mut pointer: CurrentPointer = read_json(&current_path).unwrap();
        pointer.artifact_manifest_sha256 =
            "5555555555555555555555555555555555555555555555555555555555555555".into();
        write_atomic_json(&current_path, &pointer).unwrap();

        assert!(matches!(
            storage.verify_project(PROJECT_ID, REVISION_ID, ARTIFACT_SHA),
            Err(StorageError::Corrupt)
        ));
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
    }

    #[test]
    fn interrupted_immutable_temp_never_occupies_the_cas_digest_path() {
        let root = tempfile::tempdir().unwrap();
        let (storage, _) = ManagedStorage::open(root.path()).unwrap();
        let complete = b"complete immutable object";
        let digest = sha256(complete);
        let final_path = storage.object_path(&digest);
        let parent = final_path.parent().unwrap();
        ensure_real_directory(parent).unwrap();
        let stale = parent.join(format!(".{digest}.deadbeef.immutable.tmp"));
        fs::write(&stale, b"truncated").unwrap();

        storage.write_object(&digest, complete).unwrap();
        assert_eq!(fs::read(&final_path).unwrap(), complete);
        assert_eq!(fs::read(&stale).unwrap(), b"truncated");
        storage.write_object(&digest, complete).unwrap();
    }

    #[test]
    fn target_metadata_is_immutable_bounded_and_round_trips_in_sorted_order() {
        let root = tempfile::tempdir().unwrap();
        let (storage, _) = ManagedStorage::open(root.path()).unwrap();
        let files = BTreeMap::from([("index.html".into(), b"<h1>safe</h1>".to_vec())]);
        storage
            .create_project(
                PROJECT_ID,
                "Target persistence",
                REVISION_ID,
                ARTIFACT_SHA,
                &files,
            )
            .unwrap();
        let later_target = "tgt_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let first_target = "tgt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let first = br#"{"schemaVersion":1,"targetId":"tgt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        let later = br#"{"schemaVersion":1,"targetId":"tgt_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#;

        storage
            .persist_target(PROJECT_ID, later_target, later)
            .unwrap();
        storage
            .persist_target(PROJECT_ID, first_target, first)
            .unwrap();
        storage
            .persist_target(PROJECT_ID, first_target, first)
            .unwrap();
        assert!(matches!(
            storage.persist_target(PROJECT_ID, first_target, b"different"),
            Err(StorageError::Corrupt)
        ));
        assert!(matches!(
            storage.persist_target(PROJECT_ID, "tgt_cccccccccccccccccccccccccccccccc", b""),
            Err(StorageError::Corrupt)
        ));
        assert!(matches!(
            storage.persist_target(
                PROJECT_ID,
                "tgt_dddddddddddddddddddddddddddddddd",
                &vec![b'x'; MAX_TARGET_METADATA_BYTES + 1],
            ),
            Err(StorageError::Corrupt)
        ));

        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(
            projects[0].targets,
            vec![
                (first_target.to_owned(), first.to_vec()),
                (later_target.to_owned(), later.to_vec())
            ]
        );
    }

    #[test]
    fn owned_partial_creations_are_removed_but_unknown_empty_projects_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let (storage, _) = ManagedStorage::open(root.path()).unwrap();
        let incomplete_id = "prj_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let incomplete = storage.project_root(incomplete_id);
        fs::create_dir(&incomplete).unwrap();
        fs::create_dir(incomplete.join("revisions")).unwrap();
        write_immutable_json(
            &incomplete.join("creating.json"),
            &CreationMarker {
                schema_version: STORAGE_SCHEMA.into(),
            },
        )
        .unwrap();
        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert!(projects.is_empty());
        assert!(!incomplete.exists());

        let owned_staging = root.path().join(STORAGE_DIRECTORY).join("projects").join(
            ".creating-prj_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.11111111111111111111111111111111",
        );
        fs::create_dir(&owned_staging).unwrap();
        let (_, projects) = ManagedStorage::open(root.path()).unwrap();
        assert!(projects.is_empty());
        assert!(!owned_staging.exists());

        let root_only_id = "prj_cccccccccccccccccccccccccccccccc";
        let root_only = storage.project_root(root_only_id);
        fs::create_dir(&root_only).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
        assert!(root_only.exists());
    }

    #[test]
    fn reviewed_projection_changes_when_only_an_excluded_path_changes() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("index.html"), b"safe").unwrap();
        fs::write(source.path().join(".env"), b"first secret").unwrap();
        let first = scan_import(source.path()).unwrap();
        fs::write(source.path().join(".env.local"), b"second secret").unwrap();
        let second = scan_import(source.path()).unwrap();
        assert_ne!(first.manifest_sha256, second.manifest_sha256);
        assert_eq!(first.files, second.files);
    }

    #[test]
    fn import_excludes_common_package_and_key_credentials() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("index.html"), b"safe").unwrap();
        for name in [
            ".npmrc",
            ".netrc",
            ".pypirc",
            ".envrc",
            "certificate.pem",
            "private.key",
        ] {
            fs::write(source.path().join(name), b"credential").unwrap();
        }
        let scan = scan_import(source.path()).unwrap();
        assert_eq!(scan.files.len(), 1);
        assert_eq!(scan.excluded.len(), 6);
        assert!(
            scan.excluded
                .iter()
                .all(|entry| entry.reason == "credential_material")
        );
    }

    #[test]
    fn import_rejects_casefold_windows_and_backslash_collisions_and_limits() {
        for names in [
            vec!["A.txt", "a.txt"],
            vec!["Maße.txt", "MASSE.txt"],
            vec!["CON"],
            vec!["bad\\name.txt"],
        ] {
            let source = tempfile::tempdir().unwrap();
            fs::write(source.path().join("index.html"), b"safe").unwrap();
            for name in names {
                fs::write(source.path().join(name), b"unsafe").unwrap();
            }
            assert!(matches!(
                scan_import(source.path()),
                Err(StorageError::UnsafeImport)
            ));
        }

        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("index.html"), b"safe").unwrap();
        File::create(source.path().join("too-large.bin"))
            .unwrap()
            .set_len((MAX_FILE_BYTES + 1) as u64)
            .unwrap();
        assert!(matches!(
            scan_import(source.path()),
            Err(StorageError::ImportLimit)
        ));
    }

    #[test]
    fn import_and_materialized_walks_bound_all_visited_entries_and_directory_depth() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("index.html"), b"safe").unwrap();
        for index in 0..MAX_FILES {
            fs::write(source.path().join(format!(".env.{index:04}")), b"excluded").unwrap();
        }
        assert!(matches!(
            scan_import(source.path()),
            Err(StorageError::ImportLimit)
        ));

        let root = tempfile::tempdir().unwrap();
        let (storage, _) = ManagedStorage::open(root.path()).unwrap();
        let files = BTreeMap::from([("index.html".into(), b"safe".to_vec())]);
        storage
            .create_project(PROJECT_ID, "Walk budget", REVISION_ID, ARTIFACT_SHA, &files)
            .unwrap();
        let site = materialized_site_path(root.path(), PROJECT_ID);
        for index in 0..MAX_FILES {
            fs::create_dir(site.join(format!("extra-{index:04}"))).unwrap();
        }
        assert!(matches!(scan_materialized(&site), Err(StorageError::Drift)));

        let deep = tempfile::tempdir().unwrap();
        fs::write(deep.path().join("index.html"), b"safe").unwrap();
        let mut directory = deep.path().to_path_buf();
        for index in 0..=MAX_DEPTH {
            directory = directory.join(format!("d{index}"));
            fs::create_dir(&directory).unwrap();
        }
        assert!(matches!(
            scan_import(deep.path()),
            Err(StorageError::ImportLimit)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn import_and_managed_storage_reject_links_and_symlinked_internal_directories() {
        use std::os::unix::fs::symlink;

        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("index.html"), b"safe").unwrap();
        symlink("index.html", source.path().join("alias.html")).unwrap();
        assert!(matches!(
            scan_import(source.path()),
            Err(StorageError::UnsafeImport)
        ));

        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("index.html"), b"safe").unwrap();
        fs::hard_link(
            source.path().join("index.html"),
            source.path().join("alias.html"),
        )
        .unwrap();
        assert!(matches!(
            scan_import(source.path()),
            Err(StorageError::UnsafeImport)
        ));

        let root = tempfile::tempdir().unwrap();
        ManagedStorage::open(root.path()).unwrap();
        let objects = root.path().join(STORAGE_DIRECTORY).join("objects");
        fs::remove_dir(&objects).unwrap();
        symlink(root.path(), &objects).unwrap();
        assert!(matches!(
            ManagedStorage::open(root.path()),
            Err(StorageError::Corrupt)
        ));
    }
}
