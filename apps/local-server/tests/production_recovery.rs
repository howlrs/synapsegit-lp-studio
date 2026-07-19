#![forbid(unsafe_code)]

use reqwest::{Client, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead as _, BufReader, Cursor, Read as _},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct RunningServer {
    child: Child,
    editor_origin: String,
    operating_mode: String,
}

impl RunningServer {
    fn start(state_root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_synapsegit-lp-local-server"))
            .env("LP_STUDIO_STATE_ROOT", state_root)
            .env("LP_STUDIO_EDITOR_PORT", "0")
            .env("LP_STUDIO_PREVIEW_PORT", "0")
            .env("LP_STUDIO_WEB_DIST", state_root.join("missing-web-dist"))
            .env("RUST_LOG", "off")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("production local-server binary starts");
        let stdout = child.stdout.take().expect("child stdout");
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break Err("server exited before READY".to_owned()),
                    Ok(_) if line.starts_with("LP_STUDIO_READY ") => {
                        break serde_json::from_str::<Value>(
                            line.trim_start_matches("LP_STUDIO_READY ").trim(),
                        )
                        .map_err(|error| format!("invalid READY payload: {error}"));
                    }
                    Ok(_) => {}
                    Err(error) => break Err(format!("could not read READY: {error}")),
                }
            };
            let _ = sender.send(result);
        });
        let ready = receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("production binary emitted READY in time")
            .expect("production binary emitted valid READY");
        Self {
            child,
            editor_origin: ready["editorOrigin"]
                .as_str()
                .expect("READY editor origin")
                .to_owned(),
            operating_mode: ready["operatingMode"]
                .as_str()
                .expect("READY operating mode")
                .to_owned(),
        }
    }

    fn stop(mut self) {
        let _ = self.child.kill();
        self.child.wait().expect("production binary stopped");
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn bootstrap(client: &Client, server: &RunningServer) -> Value {
    let response = client
        .get(format!("{}/api/v1/bootstrap", server.editor_origin))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .expect("bootstrap response");
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.expect("bootstrap JSON")
}

fn source_fingerprint(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, current: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries = fs::read_dir(current)
            .expect("fingerprint directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("fingerprint entries");
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).expect("fingerprint metadata");
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                visit(root, &path, output);
            } else {
                assert!(metadata.is_file());
                let relative = path
                    .strip_prefix(root)
                    .expect("relative fingerprint path")
                    .to_string_lossy()
                    .replace('\\', "/");
                output.insert(relative, fs::read(path).expect("fingerprint file"));
            }
        }
    }

    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

#[tokio::test]
async fn production_binary_falls_back_to_verified_read_only_recovery_without_mutation() {
    let state_root = tempfile::tempdir().expect("private state root");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(state_root.path(), fs::Permissions::from_mode(0o700))
            .expect("private state root permissions");
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client");

    let normal = RunningServer::start(state_root.path());
    assert_eq!(normal.operating_mode, "normal");
    let normal_bootstrap = bootstrap(&client, &normal).await;
    let normal_token = normal_bootstrap["session"]["token"]
        .as_str()
        .expect("normal session token");
    let created = client
        .post(format!("{}/api/v1/projects", normal.editor_origin))
        .header(header::ORIGIN, &normal.editor_origin)
        .header("sec-fetch-site", "same-origin")
        .bearer_auth(normal_token)
        .json(&json!({"schemaVersion":"1","template":"blank"}))
        .send()
        .await
        .expect("create project response");
    assert_eq!(created.status(), StatusCode::CREATED);
    let created: Value = created.json().await.expect("create project JSON");
    let project_id = created["project"]["id"]
        .as_str()
        .expect("created project ID")
        .to_owned();
    normal.stop();

    let project_root = state_root
        .path()
        .join("managed-v1/projects")
        .join(&project_id);
    let current: Value = serde_json::from_slice(
        &fs::read(project_root.join("current.json")).expect("current pointer bytes"),
    )
    .expect("current pointer JSON");
    fs::write(
        project_root.join("transaction.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": "1",
            "targetRevisionId": current["revisionId"],
            "canonicalManifestSha256": current["canonicalManifestSha256"],
            "artifactManifestSha256": current["artifactManifestSha256"]
        }))
        .expect("transaction marker JSON"),
    )
    .expect("write exact owned transaction marker");
    let owned_temporary = project_root.join(".project.json.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.tmp");
    fs::write(&owned_temporary, b"interrupted owned temporary")
        .expect("write exact owned atomic temporary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&owned_temporary, fs::Permissions::from_mode(0o600))
            .expect("owned temporary permissions");
    }

    let layout_path = project_root.join("layout.json");
    let mut layout: Value =
        serde_json::from_slice(&fs::read(&layout_path).expect("project layout bytes"))
            .expect("project layout JSON");
    layout["schemaVersion"] = Value::String("unknown-integration-schema".to_owned());
    fs::write(
        &layout_path,
        serde_json::to_vec(&layout).expect("corrupt layout JSON"),
    )
    .expect("write explicit unknown-schema fixture");
    let corrupted_source = source_fingerprint(state_root.path());

    let recovery = RunningServer::start(state_root.path());
    assert_eq!(recovery.operating_mode, "read_only_recovery");
    let recovery_bootstrap = bootstrap(&client, &recovery).await;
    assert_eq!(
        recovery_bootstrap["capabilities"]["operatingMode"],
        "read_only_recovery"
    );
    let recovery_token = recovery_bootstrap["session"]["token"]
        .as_str()
        .expect("recovery session token");

    let points = client
        .get(format!("{}/api/v1/recovery", recovery.editor_origin))
        .bearer_auth(recovery_token)
        .send()
        .await
        .expect("recovery list response");
    assert_eq!(points.status(), StatusCode::OK);
    let points: Value = points.json().await.expect("recovery list JSON");
    let point = points["recoveryPoints"]
        .as_array()
        .expect("recovery point array")
        .iter()
        .find(|point| point["diagnostic"]["verified"] == true)
        .expect("verified last-Accepted recovery point");
    assert_eq!(point["projectId"], project_id);
    assert_eq!(point["kind"], "last_accepted");
    let export_url = point["exportUrl"]
        .as_str()
        .expect("verified recovery export URL");

    let mutation = client
        .post(format!("{}/api/v1/projects", recovery.editor_origin))
        .header(header::ORIGIN, &recovery.editor_origin)
        .header("sec-fetch-site", "same-origin")
        .bearer_auth(recovery_token)
        .json(&json!({"schemaVersion":"1","template":"blank"}))
        .send()
        .await
        .expect("recovery mutation response");
    assert_eq!(mutation.status(), StatusCode::LOCKED);
    let mutation: Value = mutation.json().await.expect("recovery mutation JSON");
    assert_eq!(mutation["error"]["code"], "read_only_recovery");

    let export = client
        .get(format!("{}{}", recovery.editor_origin, export_url))
        .bearer_auth(recovery_token)
        .send()
        .await
        .expect("recovery export response");
    assert_eq!(export.status(), StatusCode::OK);
    assert_eq!(export.headers()[header::CONTENT_TYPE], "application/zip");
    let expected_sha256 = export
        .headers()
        .get("x-content-sha256")
        .expect("recovery export digest")
        .to_str()
        .expect("ASCII recovery digest")
        .to_owned();
    let export_bytes = export.bytes().await.expect("recovery ZIP bytes");
    let actual_sha256 = Sha256::digest(&export_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(actual_sha256, expected_sha256);
    let mut archive = zip::ZipArchive::new(Cursor::new(export_bytes)).expect("verified ZIP");
    let mut index = String::new();
    archive
        .by_name("index.html")
        .expect("recovered index.html")
        .read_to_string(&mut index)
        .expect("UTF-8 recovered index.html");
    assert!(index.contains("<!doctype html>"));

    assert_eq!(source_fingerprint(state_root.path()), corrupted_source);
    recovery.stop();
}
