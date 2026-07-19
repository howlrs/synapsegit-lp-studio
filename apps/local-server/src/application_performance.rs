//! Production-code performance probe for the bounded ChangeSet application path.
//!
//! The probe is intentionally a thin fixture around the same private parser used
//! by the HTTP application. Keeping it in this crate prevents a benchmark-only
//! reimplementation from being mistaken for application evidence.

use super::change_set::parse_and_apply_change_set;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

pub const CHANGE_SET_PROBE_SCHEMA_VERSION: &str =
    "synapsegit-lp-studio.change-set-performance-probe/1";
pub const CHANGED_TEXT_FILE_COUNT: usize = 10;
pub const TOTAL_CHANGED_TEXT_BYTES: usize = 2 * 1024 * 1024;
const BASE_REVISION_ID: &str = "revision-performance-fixture";
const MAX_PROBE_SAMPLES: usize = 100;

struct ProbeInput {
    accepted: BTreeMap<String, Vec<u8>>,
    raw_change_set: String,
    output_manifest_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetFixtureIdentity {
    pub changed_text_file_count: usize,
    pub total_changed_text_bytes: usize,
    pub operation_count: usize,
    pub change_set_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSetPerformanceProbe {
    pub schema_version: &'static str,
    pub fixture: ChangeSetFixtureIdentity,
    pub warmup_sample_count: usize,
    pub sample_count: usize,
    pub raw_samples_ms: Vec<f64>,
    pub p95_ms: f64,
    pub output_manifest_sha256: String,
}

pub fn run_change_set_performance_probe(
    warmup_sample_count: usize,
    sample_count: usize,
) -> Result<ChangeSetPerformanceProbe, String> {
    if sample_count == 0
        || sample_count > MAX_PROBE_SAMPLES
        || warmup_sample_count > MAX_PROBE_SAMPLES
    {
        return Err("sample_count_invalid".to_owned());
    }
    let fixture = build_fixture()?;
    let accepted = fixture.accepted;
    let raw = fixture.raw_change_set;
    let expected_output_manifest_sha256 = fixture.output_manifest_sha256;
    let change_set_sha256 = sha256(raw.as_bytes());

    for _ in 0..warmup_sample_count {
        let applied = parse_and_apply_change_set(&raw, BASE_REVISION_ID, &accepted)
            .map_err(|_| "change_set_probe_warmup_failed".to_owned())?;
        verify_applied(
            &applied.files,
            applied.changes.len(),
            &expected_output_manifest_sha256,
        )?;
        black_box(applied);
    }

    let mut durations = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let started = Instant::now();
        let applied = parse_and_apply_change_set(&raw, BASE_REVISION_ID, &accepted)
            .map_err(|_| "change_set_probe_sample_failed".to_owned())?;
        let duration = started.elapsed();
        verify_applied(
            &applied.files,
            applied.changes.len(),
            &expected_output_manifest_sha256,
        )?;
        black_box(applied);
        durations.push(duration);
    }

    let p95 = percentile95(&durations).ok_or_else(|| "sample_count_invalid".to_owned())?;
    Ok(ChangeSetPerformanceProbe {
        schema_version: CHANGE_SET_PROBE_SCHEMA_VERSION,
        fixture: ChangeSetFixtureIdentity {
            changed_text_file_count: CHANGED_TEXT_FILE_COUNT,
            total_changed_text_bytes: TOTAL_CHANGED_TEXT_BYTES,
            operation_count: CHANGED_TEXT_FILE_COUNT,
            change_set_sha256,
        },
        warmup_sample_count,
        sample_count,
        raw_samples_ms: durations.into_iter().map(duration_ms).collect(),
        p95_ms: duration_ms(p95),
        output_manifest_sha256: expected_output_manifest_sha256,
    })
}

fn build_fixture() -> Result<ProbeInput, String> {
    let mut accepted = BTreeMap::new();
    let mut operations = Vec::with_capacity(CHANGED_TEXT_FILE_COUNT);
    let base_size = TOTAL_CHANGED_TEXT_BYTES / CHANGED_TEXT_FILE_COUNT;
    let remainder = TOTAL_CHANGED_TEXT_BYTES % CHANGED_TEXT_FILE_COUNT;

    for index in 0..CHANGED_TEXT_FILE_COUNT {
        let (path, media_type, before) = if index == 0 {
            (
                "index.html".to_owned(),
                "text/html",
                b"<!doctype html><html><body>baseline</body></html>".to_vec(),
            )
        } else {
            (
                format!("asset-{index:02}.txt"),
                "text/plain",
                format!("baseline-{index}").into_bytes(),
            )
        };
        let expected_sha256 = sha256(&before);
        accepted.insert(path.clone(), before);
        let requested_size = base_size + usize::from(index < remainder);
        let content = if index == 0 {
            sized_html(requested_size)?
        } else {
            std::iter::repeat_n(char::from(b'a' + index as u8), requested_size).collect()
        };
        if content.len() != requested_size {
            return Err("change_set_probe_fixture_size_failed".to_owned());
        }
        operations.push(json!({
            "op": "replace_text",
            "path": path,
            "expectedSha256": expected_sha256,
            "mediaType": media_type,
            "content": content
        }));
    }

    let raw = serde_json::to_string(&json!({
        "schema": "org.synapsegit-lp-studio.change-set",
        "version": 1,
        "baseRevisionId": BASE_REVISION_ID,
        "summary": "Exact 10-file, 2 MiB production ChangeSet performance fixture.",
        "operations": operations
    }))
    .map_err(|_| "change_set_probe_fixture_json_failed".to_owned())?;
    let applied = parse_and_apply_change_set(&raw, BASE_REVISION_ID, &accepted)
        .map_err(|_| "change_set_probe_fixture_validation_failed".to_owned())?;
    verify_applied_file_sizes(&applied.files, applied.changes.len())?;
    let output_manifest_sha256 = files_manifest_sha256(&applied.files);
    Ok(ProbeInput {
        accepted,
        raw_change_set: raw,
        output_manifest_sha256,
    })
}

fn sized_html(size: usize) -> Result<String, String> {
    const PREFIX: &str = "<!doctype html><html><body><!--";
    const SUFFIX: &str = "--></body></html>";
    let padding = size
        .checked_sub(PREFIX.len() + SUFFIX.len())
        .ok_or_else(|| "change_set_probe_fixture_size_failed".to_owned())?;
    Ok(format!("{PREFIX}{}{SUFFIX}", "x".repeat(padding)))
}

fn verify_applied(
    files: &BTreeMap<String, Vec<u8>>,
    change_count: usize,
    expected_manifest_sha256: &str,
) -> Result<(), String> {
    verify_applied_file_sizes(files, change_count)?;
    if files_manifest_sha256(files) != expected_manifest_sha256 {
        return Err("change_set_probe_output_mismatch".to_owned());
    }
    Ok(())
}

fn verify_applied_file_sizes(
    files: &BTreeMap<String, Vec<u8>>,
    change_count: usize,
) -> Result<(), String> {
    let total = files.values().map(Vec::len).sum::<usize>();
    if files.len() != CHANGED_TEXT_FILE_COUNT
        || change_count != CHANGED_TEXT_FILE_COUNT
        || total != TOTAL_CHANGED_TEXT_BYTES
    {
        return Err("change_set_probe_output_shape_failed".to_owned());
    }
    Ok(())
}

fn files_manifest_sha256(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut digest = Sha256::new();
    for (path, bytes) in files {
        digest.update(path.len().to_be_bytes());
        digest.update(path.as_bytes());
        digest.update(bytes.len().to_be_bytes());
        digest.update(bytes);
    }
    hex_digest(digest.finalize().as_slice())
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn percentile95(values: &[Duration]) -> Option<Duration> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted
        .get(
            sorted
                .len()
                .saturating_mul(95)
                .div_ceil(100)
                .saturating_sub(1),
        )
        .copied()
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_exact_and_exercises_the_production_parser() {
        let result = run_change_set_performance_probe(1, 2).unwrap();
        assert_eq!(result.fixture.changed_text_file_count, 10);
        assert_eq!(result.fixture.total_changed_text_bytes, 2_097_152);
        assert_eq!(result.raw_samples_ms.len(), 2);
        assert!(result.raw_samples_ms.iter().all(|sample| *sample > 0.0));
    }
}
