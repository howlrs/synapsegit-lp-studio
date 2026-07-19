use mime_guess::MimeGuess;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use unicase::UniCase;
use unicode_normalization::UnicodeNormalization as _;
use uuid::Uuid;

pub(super) const MAX_FILES: usize = 1_000;
pub(super) const MAX_TOTAL_BYTES: usize = 200 * 1024 * 1024;
pub(super) const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
pub(super) const MAX_PATH_BYTES: usize = 512;
pub(super) const MAX_DEPTH: usize = 16;

const STORAGE_SCHEMA: &str = "1";
const STORAGE_DIRECTORY: &str = "managed-v1";

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
        ensure_real_directory(&root)?;
        ensure_real_directory(&root.join("objects"))?;
        ensure_real_directory(&root.join("projects"))?;
        let storage = Self { root };
        let projects = storage.load_projects()?;
        Ok((storage, projects))
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
            fs::rename(&staging, &project_root)?;
            sync_directory(&projects_root)
        })();
        if result.is_err() {
            let _ = remove_internal_tree_if_present(&staging);
        }
        result
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
        self.materialize_to(&project_root.join("site.staging"), &manifest)?;
        let site = project_root.join("site");
        let backup = project_root.join("site.backup");
        remove_internal_tree_if_present(&backup)?;
        if site.exists() {
            fs::rename(&site, &backup)?;
        }
        if let Err(error) = fs::rename(project_root.join("site.staging"), &site) {
            if backup.exists() && !site.exists() {
                let _ = fs::rename(&backup, &site);
            }
            return Err(StorageError::Io(error));
        }
        write_atomic_json(&project_root.join("current.json"), &pointer)?;
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

    pub fn proposal_repository(
        &self,
        project_id: &str,
        proposal_id: &str,
    ) -> Result<PathBuf, StorageError> {
        validate_identifier(project_id, "prj_")?;
        validate_identifier(proposal_id, "pro_")?;
        let parent = self
            .project_root(project_id)
            .join("proposals")
            .join(proposal_id);
        validate_real_directory(&self.project_root(project_id).join("proposals"))?;
        fs::create_dir(&parent)?;
        Ok(parent.join("repository"))
    }

    fn load_projects(&self) -> Result<Vec<PersistedProject>, StorageError> {
        let mut project_dirs =
            fs::read_dir(self.root.join("projects"))?.collect::<Result<Vec<_>, _>>()?;
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
            remove_internal_file_if_present(&project_root.join("creating.json"))?;
            validate_real_directory(&project_root.join("site"))?;
            let metadata: ProjectMetadata = read_json(&project_root.join("project.json"))?;
            if metadata.schema_version != STORAGE_SCHEMA
                || metadata.id != directory_name
                || metadata.display_name.is_empty()
            {
                return Err(StorageError::Corrupt);
            }
            let pointer: CurrentPointer = read_json(&project_root.join("current.json"))?;
            validate_pointer(&pointer)?;
            let manifest = self.read_manifest(&metadata.id, &pointer)?;
            let files = self.load_manifest_files(&manifest)?;
            result.push(PersistedProject {
                id: metadata.id,
                display_name: metadata.display_name,
                revision_id: pointer.revision_id,
                artifact_manifest_sha256: pointer.artifact_manifest_sha256,
                files,
            });
        }
        Ok(result)
    }

    fn recover_project(&self, project_root: &Path) -> Result<(), StorageError> {
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
                fs::rename(project_root.join("site.staging"), &site)?;
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
                fs::rename(project_root.join("site.staging"), &site)?;
            }
            write_atomic_json(&project_root.join("current.json"), &target_pointer)?;
        } else if backup.exists() {
            remove_internal_tree_if_present(&site)?;
            fs::rename(&backup, &site)?;
        } else if let Some(pointer) = current {
            validate_pointer(&pointer)?;
            let manifest = self.read_manifest_from_root(project_root, &pointer)?;
            self.materialize_to(&project_root.join("site.staging"), &manifest)?;
            remove_internal_tree_if_present(&site)?;
            fs::rename(project_root.join("site.staging"), &site)?;
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
    validate_real_directory(root).map_err(|_| StorageError::Drift)?;
    let mut files = BTreeMap::new();
    let mut budget = ScanBudget::default();
    scan_materialized_directory(root, root, 0, &mut files, &mut budget)?;
    stored_files(&files).map_err(|_| StorageError::Drift)
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

fn validate_canonical_path(path: &str) -> Result<(), StorageError> {
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

fn validate_pointer(pointer: &CurrentPointer) -> Result<(), StorageError> {
    if pointer.schema_version != STORAGE_SCHEMA {
        return Err(StorageError::Corrupt);
    }
    validate_identifier(&pointer.revision_id, "rev_")?;
    validate_sha256(&pointer.canonical_manifest_sha256)?;
    validate_sha256(&pointer.artifact_manifest_sha256)
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
    write_new_file(&temporary, bytes)?;

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
            if let Err(error) = fs::rename(&temporary, path) {
                let _ = fs::remove_file(&temporary);
                return Err(error.into());
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
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
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
    write_new_file(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<(), StorageError> {
    File::open(path)?.sync_all()?;
    Ok(())
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
