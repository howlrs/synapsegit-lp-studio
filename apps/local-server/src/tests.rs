use super::*;
use axum::body::to_bytes;
use axum::http::Request;
use serde_json::Value;
use std::io::Read;
use tempfile::TempDir;
use tower::ServiceExt;

const EDITOR_ORIGIN: &str = "http://editor.test:4321";
const EDITOR_HOST: &str = "editor.test:4321";
const PREVIEW_ORIGIN: &str = "http://preview.test:4322";
static SYNAPSEGIT_WORKFLOW_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Harness {
    _root: TempDir,
    _import: Option<TempDir>,
    state: StudioState,
    token: String,
}

impl Harness {
    async fn new() -> Self {
        Self::with_optional_import(None).await
    }

    async fn with_import(import: TempDir) -> Self {
        Self::with_optional_import(Some(import)).await
    }

    async fn with_optional_import(import: Option<TempDir>) -> Self {
        let root = tempfile::tempdir().expect("temp root");
        let config = ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            root.path(),
            root.path().join("missing-web-dist"),
        )
        .with_import_root(import.as_ref().map(|source| source.path().to_path_buf()));
        let state = StudioState::new(config).expect("state");
        let token = bootstrap_token(&state).await;
        Self {
            _root: root,
            _import: import,
            state,
            token,
        }
    }

    async fn post(&self, uri: &str, payload: Value) -> Response {
        self.post_with_token(uri, payload, &self.token).await
    }

    async fn post_with_token(&self, uri: &str, payload: Value, token: &str) -> Response {
        post_for(&self.state, token, uri, payload).await
    }

    async fn get(&self, uri: &str) -> Response {
        editor_router(self.state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(HOST, EDITOR_HOST)
                    .header(AUTHORIZATION, format!("Bearer {}", self.token))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response")
    }
}

async fn post_for(state: &StudioState, token: &str, uri: &str, payload: Value) -> Response {
    editor_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(HOST, EDITOR_HOST)
                .header(ORIGIN, EDITOR_ORIGIN)
                .header("sec-fetch-site", "same-origin")
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(serde_json::to_vec(&payload).expect("json")))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn get_for(state: &StudioState, token: &str, uri: &str) -> Response {
    editor_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(HOST, EDITOR_HOST)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn bootstrap_token(state: &StudioState) -> String {
    let response = editor_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/bootstrap")
                .header(HOST, EDITOR_HOST)
                .header("sec-fetch-site", "same-origin")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("bootstrap response");
    assert_eq!(response.status(), StatusCode::OK);
    response_json(response).await["session"]["token"]
        .as_str()
        .expect("session token")
        .to_owned()
}

#[tokio::test]
async fn complete_real_synapsegit_flow_adopts_only_after_one_shot_approval() {
    // Production proposal creation is serialized by the single Store mutex.
    // Mirror that boundary across otherwise-independent test StudioState values.
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let harness = Harness::new().await;
    let create = harness
        .post(
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await;
    assert_eq!(create.status(), StatusCode::CREATED);
    let create_body = response_json(create).await;
    let project_id = string_at(&create_body, "/project/id");
    let original_revision = string_at(&create_body, "/project/revisionId");
    let original_manifest = string_at(&create_body, "/project/acceptedManifestSha256");
    assert_eq!(
        create_body["project"]["displayName"],
        "Untitled landing page"
    );
    assert_eq!(create_body["project"]["files"].as_array().unwrap().len(), 2);

    let target = harness
        .post(
            &format!("/api/v1/projects/{project_id}/targets"),
            json!({
                "schemaVersion":"1",
                "revisionId":original_revision,
                "kind":"element",
                "elementId":"hero-heading"
            }),
        )
        .await;
    assert_eq!(target.status(), StatusCode::CREATED);
    let target_id = string_at(&response_json(target).await, "/target/id");

    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            json!({
                "schemaVersion":"1",
                "revisionId":original_revision,
                "targetId":target_id,
                "instruction":"見出しを公開に向けた言葉へ変更してください。"
            }),
        )
        .await;
    assert_eq!(context.status(), StatusCode::CREATED);
    let context_body = response_json(context).await;
    let context_id = string_at(&context_body, "/context/id");
    let context_sha = string_at(&context_body, "/context/sha256");
    let canonical = string_at(&context_body, "/context/canonicalJson");
    assert_eq!(
        review_context_sha256(canonical.as_bytes()).unwrap(),
        context_sha
    );

    let proposal = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":context_id,
                "contextSha256":context_sha
            }),
        )
        .await;
    assert_eq!(proposal.status(), StatusCode::CREATED);
    let proposal_body = response_json(proposal).await;
    let proposal_id = string_at(&proposal_body, "/proposal/id");
    let review_id = string_at(&proposal_body, "/proposal/reviewId");
    let proposal_digest = string_at(&proposal_body, "/proposal/artifactManifestSha256");
    assert_ne!(proposal_digest, original_manifest);
    assert_eq!(proposal_body["proposal"]["executionVerified"], false);
    assert_eq!(proposal_body["proposal"]["validation"]["status"], "passed");
    assert!(
        proposal_body["proposal"]["unifiedDiff"]
            .as_str()
            .unwrap()
            .contains("対話から、公開できるLPへ。")
    );
    let public_proposal = serde_json::to_string(&proposal_body).unwrap();
    for forbidden in [
        "proposal/artifact/",
        "decision/artifact/",
        "actor_oid",
        "policy_oid",
        "grant_oid",
        "permit",
        "repository_path",
    ] {
        assert!(!public_proposal.contains(forbidden), "leaked {forbidden}");
    }

    let accepted_before = harness.get(&format!("/api/v1/projects/{project_id}")).await;
    assert_eq!(
        response_json(accepted_before).await["project"]["acceptedManifestSha256"],
        original_manifest
    );

    let proposal_preview = preview_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/preview/{project_id}/{proposal_id}/"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(proposal_preview.status(), StatusCode::OK);
    assert!(
        proposal_preview
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("frame-ancestors http://editor.test:4321")
    );
    let preview_html = response_text(proposal_preview).await;
    assert!(preview_html.contains("対話から、公開できるLPへ。"));
    assert!(preview_html.contains("synapsegit-lp.selection"));
    assert!(preview_html.contains("channelId:channel"));
    assert!(preview_html.contains(&format!(
        "S={}",
        serde_json::to_string(&proposal_id).unwrap()
    )));
    assert!(preview_html.contains(&format!(
        "R={}",
        serde_json::to_string(&original_revision).unwrap()
    )));

    let preview_api = preview_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/bootstrap")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview_api.status(), StatusCode::NOT_FOUND);

    let second = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":context_id,
                "contextSha256":context_sha
            }),
        )
        .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(second).await["error"]["code"],
        "artifact_single_proposal_limit"
    );

    let intent = "intent-e2e-0001";
    let approval = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/approvals"),
            json!({
                "schemaVersion":"1",
                "proposalId":proposal_id,
                "expectedRevisionId":original_revision,
                "disposition":"adopted_unchanged",
                "intentId":intent
            }),
        )
        .await;
    assert_eq!(approval.status(), StatusCode::CREATED);
    let approval_token = string_at(&response_json(approval).await, "/approval/token");

    let decision_request = json!({
        "schemaVersion":"1",
        "approvalToken":approval_token,
        "proposalId":proposal_id,
        "expectedRevisionId":original_revision,
        "disposition":"adopted_unchanged",
        "intentId":intent,
        "rationale":"内容と差分を確認して採用します。"
    });
    let decision = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            decision_request.clone(),
        )
        .await;
    assert_eq!(decision.status(), StatusCode::OK);
    let decision_body = response_json(decision).await;
    let adopted_revision = string_at(&decision_body, "/project/revisionId");
    assert_ne!(adopted_revision, original_revision);
    assert_eq!(
        decision_body["project"]["acceptedManifestSha256"],
        proposal_digest
    );

    let replay = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            decision_request,
        )
        .await;
    assert_eq!(replay.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(replay).await["error"]["code"],
        "approval_invalid_or_consumed"
    );

    let export_one = harness
        .post(
            &format!("/api/v1/projects/{project_id}/exports"),
            json!({"schemaVersion":"1","revisionId":adopted_revision}),
        )
        .await;
    assert_eq!(export_one.status(), StatusCode::CREATED);
    let export_one_body = response_json(export_one).await;
    let first_sha = string_at(&export_one_body, "/export/sha256");
    let first_url = string_at(&export_one_body, "/export/downloadUrl");
    let first_zip = response_bytes(harness.get(&first_url).await).await;

    let export_two = harness
        .post(
            &format!("/api/v1/projects/{project_id}/exports"),
            json!({"schemaVersion":"1","revisionId":adopted_revision}),
        )
        .await;
    let export_two_body = response_json(export_two).await;
    assert_eq!(export_two_body["export"]["sha256"], first_sha);
    let second_zip = response_bytes(
        harness
            .get(export_two_body["export"]["downloadUrl"].as_str().unwrap())
            .await,
    )
    .await;
    assert_eq!(first_zip, second_zip);

    let mut archive = zip::ZipArchive::new(Cursor::new(first_zip)).unwrap();
    assert_eq!(archive.len(), 2);
    assert_eq!(archive.by_index(0).unwrap().name(), "index.html");
    assert_eq!(archive.by_index(1).unwrap().name(), "styles.css");
    let mut index = String::new();
    archive
        .by_name("index.html")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    assert!(index.contains("対話から、公開できるLPへ。"));
    assert!(!index.contains("synapsegit-lp.selection"));
}

#[tokio::test]
async fn fake_ai_proposal_is_bound_to_each_selected_element() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let cases = [
        (
            "hero-heading",
            "まだ、白紙です。",
            "対話から、公開できるLPへ。",
            "selected hero heading",
        ),
        (
            "hero-copy",
            "伝えたいことを選び、AIとの対話から最初の一歩をつくります。",
            "要望を選び、AIとの対話から公開できるLPへ育てます。",
            "selected hero copy",
        ),
        (
            "hero-cta",
            "構想を始める",
            "公開LPをつくる",
            "selected hero CTA",
        ),
    ];

    for (element_id, accepted_text, proposed_text, summary_fragment) in cases {
        let harness = Harness::new().await;
        let create = harness
            .post(
                "/api/v1/projects",
                json!({"schemaVersion":"1","template":"blank"}),
            )
            .await;
        let create_body = response_json(create).await;
        let project_id = string_at(&create_body, "/project/id");
        let revision_id = string_at(&create_body, "/project/revisionId");

        let target = harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                json!({
                    "schemaVersion":"1",
                    "revisionId":revision_id,
                    "kind":"element",
                    "elementId":element_id
                }),
            )
            .await;
        assert_eq!(target.status(), StatusCode::CREATED, "target {element_id}");
        let target_id = string_at(&response_json(target).await, "/target/id");

        let context = harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                json!({
                    "schemaVersion":"1",
                    "revisionId":revision_id,
                    "targetId":target_id,
                    "instruction":"選択した要素だけを更新してください。"
                }),
            )
            .await;
        assert_eq!(
            context.status(),
            StatusCode::CREATED,
            "context {element_id}"
        );
        let context_body = response_json(context).await;
        let context_id = string_at(&context_body, "/context/id");
        let context_sha = string_at(&context_body, "/context/sha256");
        let canonical: Value =
            serde_json::from_str(context_body["context"]["canonicalJson"].as_str().unwrap())
                .unwrap();
        assert_eq!(canonical["target"]["elementId"], element_id);
        assert_eq!(canonical["selectedText"], accepted_text);

        let proposal = harness
            .post(
                &format!("/api/v1/projects/{project_id}/proposals"),
                json!({
                    "schemaVersion":"1",
                    "contextId":context_id,
                    "contextSha256":context_sha
                }),
            )
            .await;
        assert_eq!(
            proposal.status(),
            StatusCode::CREATED,
            "proposal {element_id}"
        );
        let proposal_body = response_json(proposal).await;
        let summary = proposal_body["proposal"]["summary"].as_str().unwrap();
        let unified_diff = proposal_body["proposal"]["unifiedDiff"].as_str().unwrap();
        assert!(summary.contains(summary_fragment), "summary {element_id}");
        assert!(
            unified_diff.contains(accepted_text),
            "diff old {element_id}"
        );
        assert!(
            unified_diff.contains(proposed_text),
            "diff new {element_id}"
        );
        let proposal_id = string_at(&proposal_body, "/proposal/id");

        let preview = preview_router(harness.state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/preview/{project_id}/{proposal_id}/"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let preview_html = response_text(preview).await;
        assert!(
            preview_html.contains(proposed_text),
            "preview new {element_id}"
        );
        assert!(
            !preview_html.contains(accepted_text),
            "preview old {element_id}"
        );
        for other in [
            "対話から、公開できるLPへ。",
            "要望を選び、AIとの対話から公開できるLPへ育てます。",
            "公開LPをつくる",
        ] {
            if other != proposed_text {
                assert!(
                    !preview_html.contains(other),
                    "proposal for {element_id} changed another target to {other}"
                );
            }
        }
        let review_id = string_at(&proposal_body, "/proposal/reviewId");
        let intent = format!("cleanup-{element_id}");
        let approval = harness
            .post(
                &format!("/api/v1/reviews/{review_id}/approvals"),
                json!({
                    "schemaVersion":"1",
                    "proposalId":proposal_id,
                    "expectedRevisionId":revision_id,
                    "disposition":"rejected",
                    "intentId":intent
                }),
            )
            .await;
        assert_eq!(approval.status(), StatusCode::CREATED);
        let approval_token = string_at(&response_json(approval).await, "/approval/token");
        let decision = harness
            .post(
                &format!("/api/v1/reviews/{review_id}/decisions"),
                json!({
                    "schemaVersion":"1",
                    "approvalToken":approval_token,
                    "proposalId":proposal_id,
                    "expectedRevisionId":revision_id,
                    "disposition":"rejected",
                    "intentId":intent,
                    "rationale":"test cleanup"
                }),
            )
            .await;
        assert_eq!(decision.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn retained_state_rehydrates_blank_project_and_immutable_revision() {
    let root = tempfile::tempdir().unwrap();
    let config = || {
        ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            root.path(),
            root.path().join("missing-web-dist"),
        )
    };
    let first_state = StudioState::new(config()).unwrap();
    let first_token = bootstrap_token(&first_state).await;
    let created = post_for(
        &first_state,
        &first_token,
        "/api/v1/projects",
        json!({"schemaVersion":"1","template":"blank"}),
    )
    .await;
    let created = response_json(created).await;
    let project_id = string_at(&created, "/project/id");
    let revision_id = string_at(&created, "/project/revisionId");
    let manifest_sha = string_at(&created, "/project/acceptedManifestSha256");
    drop(first_state);

    let second_state = StudioState::new(config()).unwrap();
    let second_token = bootstrap_token(&second_state).await;
    let listed =
        response_json(get_for(&second_state, &second_token, "/api/v1/projects").await).await;
    assert_eq!(listed["projects"].as_array().unwrap().len(), 1);
    assert_eq!(listed["projects"][0]["id"], project_id);
    assert_eq!(listed["projects"][0]["revisionId"], revision_id);
    assert_eq!(
        listed["projects"][0]["acceptedManifestSha256"],
        manifest_sha
    );
    assert_eq!(listed["projects"][0]["status"], "ready");

    let project_root = root.path().join("managed-v1/projects").join(&project_id);
    assert!(
        project_root
            .join("revisions")
            .join(format!("{revision_id}.json"))
            .is_file()
    );
    let site = storage::materialized_site_path(root.path(), &project_id);
    assert!(site.join("index.html").is_file());
    assert!(site.join("styles.css").is_file());
    assert!(!site.join(".studio").exists());
    assert!(root.path().join("managed-v1/objects").is_dir());
}

#[tokio::test]
async fn import_is_session_bound_rescanned_consumed_persisted_and_source_preserving() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(
        source.path().join("index.html"),
        b"<!doctype html><h1>Imported</h1>",
    )
    .unwrap();
    std::fs::write(source.path().join("styles.css"), b"h1 { color: navy; }").unwrap();
    std::fs::write(source.path().join(".env"), b"SECRET=not-imported").unwrap();
    std::fs::create_dir(source.path().join(".git")).unwrap();
    std::fs::write(source.path().join(".git/config"), b"private git metadata").unwrap();
    let before = source_fingerprint(source.path());
    let harness = Harness::with_import(source).await;

    let capability = response_json(
        editor_router(harness.state.clone())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/bootstrap")
                    .header(HOST, EDITOR_HOST)
                    .header("sec-fetch-site", "same-origin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(capability["capabilities"]["importAvailable"], true);
    assert_eq!(capability["capabilities"]["limits"]["maxFiles"], 1_000);

    let preview = harness
        .post("/api/v1/imports/previews", json!({"schemaVersion":"1"}))
        .await;
    assert_eq!(preview.status(), StatusCode::CREATED);
    let preview = response_json(preview).await;
    let preview_id = string_at(&preview, "/importPreview/id");
    let preview_sha = string_at(&preview, "/importPreview/manifestSha256");
    assert_eq!(preview["importPreview"]["entryPoint"], "index.html");
    assert_eq!(
        preview["importPreview"]["included"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        preview["importPreview"]["excluded"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let second_session = bootstrap_token(&harness.state).await;
    let wrong_session = harness
        .post_with_token(
            &format!("/api/v1/imports/{preview_id}/confirm"),
            json!({"schemaVersion":"1","expectedManifestSha256":preview_sha}),
            &second_session,
        )
        .await;
    assert_eq!(wrong_session.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(wrong_session).await["error"]["code"],
        "import_preview_binding_mismatch"
    );

    let import_root = harness._import.as_ref().unwrap().path();
    std::fs::write(import_root.join(".env.local"), b"CHANGED_REVIEW=1").unwrap();
    let changed = harness
        .post(
            &format!("/api/v1/imports/{preview_id}/confirm"),
            json!({"schemaVersion":"1","expectedManifestSha256":preview_sha}),
        )
        .await;
    assert_eq!(changed.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(changed).await["error"]["code"],
        "import_source_changed"
    );

    let expiring = response_json(
        harness
            .post("/api/v1/imports/previews", json!({"schemaVersion":"1"}))
            .await,
    )
    .await;
    let expiring_id = string_at(&expiring, "/importPreview/id");
    let expiring_sha = string_at(&expiring, "/importPreview/manifestSha256");
    harness
        .state
        .store()
        .unwrap()
        .import_previews
        .get_mut(&expiring_id)
        .unwrap()
        .expires_at = now_unix() - 1;
    let expired_wrong_session = harness
        .post_with_token(
            &format!("/api/v1/imports/{expiring_id}/confirm"),
            json!({"schemaVersion":"1","expectedManifestSha256":expiring_sha.clone()}),
            &second_session,
        )
        .await;
    assert_eq!(
        response_json(expired_wrong_session).await["error"]["code"],
        "import_preview_binding_mismatch"
    );
    assert!(
        harness
            .state
            .store()
            .unwrap()
            .import_previews
            .contains_key(&expiring_id)
    );
    let expired = harness
        .post(
            &format!("/api/v1/imports/{expiring_id}/confirm"),
            json!({"schemaVersion":"1","expectedManifestSha256":expiring_sha}),
        )
        .await;
    assert_eq!(
        response_json(expired).await["error"]["code"],
        "import_preview_expired"
    );

    let final_preview = response_json(
        harness
            .post("/api/v1/imports/previews", json!({"schemaVersion":"1"}))
            .await,
    )
    .await;
    let final_id = string_at(&final_preview, "/importPreview/id");
    let final_sha = string_at(&final_preview, "/importPreview/manifestSha256");
    let confirmed = harness
        .post(
            &format!("/api/v1/imports/{final_id}/confirm"),
            json!({"schemaVersion":"1","expectedManifestSha256":final_sha}),
        )
        .await;
    assert_eq!(confirmed.status(), StatusCode::CREATED);
    let confirmed = response_json(confirmed).await;
    let project_id = string_at(&confirmed, "/project/id");
    assert_eq!(confirmed["project"]["files"].as_array().unwrap().len(), 2);
    let replay = harness
        .post(
            &format!("/api/v1/imports/{final_id}/confirm"),
            json!({"schemaVersion":"1","expectedManifestSha256":final_sha}),
        )
        .await;
    assert_eq!(replay.status(), StatusCode::NOT_FOUND);
    let mut expected_after = before;
    expected_after.insert(".env.local".into(), b"CHANGED_REVIEW=1".to_vec());
    assert_eq!(source_fingerprint(import_root), expected_after);

    let restarted = StudioState::new(
        ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            harness._root.path(),
            harness._root.path().join("missing-web-dist"),
        )
        .with_import_root(Some(import_root.to_path_buf())),
    )
    .unwrap();
    let restarted_token = bootstrap_token(&restarted).await;
    let listed =
        response_json(get_for(&restarted, &restarted_token, "/api/v1/projects").await).await;
    assert_eq!(listed["projects"][0]["id"], project_id);
}

#[tokio::test]
async fn drift_blocks_proposal_decision_and_export_without_burning_approval() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let harness = Harness::new().await;
    let created = response_json(
        harness
            .post(
                "/api/v1/projects",
                json!({"schemaVersion":"1","template":"blank"}),
            )
            .await,
    )
    .await;
    let project_id = string_at(&created, "/project/id");
    let revision_id = string_at(&created, "/project/revisionId");
    let accepted_sha = string_at(&created, "/project/acceptedManifestSha256");
    let target = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                json!({"schemaVersion":"1","revisionId":revision_id,"kind":"element","elementId":"hero-heading"}),
            )
            .await,
    )
    .await;
    let target_id = string_at(&target, "/target/id");
    let context = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                json!({"schemaVersion":"1","revisionId":revision_id,"targetId":target_id,"instruction":"change heading"}),
            )
            .await,
    )
    .await;
    let context_id = string_at(&context, "/context/id");
    let context_sha = string_at(&context, "/context/sha256");
    let site_index =
        storage::materialized_site_path(harness._root.path(), &project_id).join("index.html");
    let accepted_index = std::fs::read(&site_index).unwrap();
    std::fs::write(&site_index, b"external before proposal").unwrap();
    let blocked_proposal = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({"schemaVersion":"1","contextId":context_id,"contextSha256":context_sha}),
        )
        .await;
    assert_eq!(blocked_proposal.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(blocked_proposal).await["error"]["code"],
        "external_changes_detected"
    );
    std::fs::write(&site_index, &accepted_index).unwrap();

    let proposal = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/proposals"),
                json!({"schemaVersion":"1","contextId":context_id,"contextSha256":context_sha}),
            )
            .await,
    )
    .await;
    let proposal_id = string_at(&proposal, "/proposal/id");
    let review_id = string_at(&proposal, "/proposal/reviewId");
    let intent = "drift-decision-test";
    let approval = response_json(
        harness
            .post(
                &format!("/api/v1/reviews/{review_id}/approvals"),
                json!({"schemaVersion":"1","proposalId":proposal_id,"expectedRevisionId":revision_id,"disposition":"adopted_unchanged","intentId":intent}),
            )
            .await,
    )
    .await;
    let approval_token = string_at(&approval, "/approval/token");
    std::fs::write(&site_index, b"external before decision").unwrap();
    let decision_request = json!({"schemaVersion":"1","approvalToken":approval_token,"proposalId":proposal_id,"expectedRevisionId":revision_id,"disposition":"adopted_unchanged","intentId":intent,"rationale":"verified"});
    let blocked_decision = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            decision_request.clone(),
        )
        .await;
    assert_eq!(blocked_decision.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(blocked_decision).await["error"]["code"],
        "external_changes_detected"
    );
    let unchanged =
        response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
    assert_eq!(unchanged["project"]["revisionId"], revision_id);
    assert_eq!(unchanged["project"]["acceptedManifestSha256"], accepted_sha);
    std::fs::write(&site_index, &accepted_index).unwrap();
    let adopted = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            decision_request,
        )
        .await;
    assert_eq!(adopted.status(), StatusCode::OK);
    let adopted = response_json(adopted).await;
    let adopted_revision = string_at(&adopted, "/project/revisionId");
    assert_ne!(adopted_revision, revision_id);

    std::fs::write(&site_index, b"external before export").unwrap();
    let blocked_export = harness
        .post(
            &format!("/api/v1/projects/{project_id}/exports"),
            json!({"schemaVersion":"1","revisionId":adopted_revision}),
        )
        .await;
    assert_eq!(blocked_export.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(blocked_export).await["error"]["code"],
        "external_changes_detected"
    );
}

#[tokio::test]
async fn mutation_security_fails_closed_with_stable_redacted_errors() {
    let harness = Harness::new().await;
    let app = editor_router(harness.state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(HOST, EDITOR_HOST)
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {}", harness.token))
                .body(Body::from(r#"{"schemaVersion":"1","template":"blank"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response_json(response).await;
    assert_eq!(body["schemaVersion"], "1");
    assert_eq!(body["error"]["code"], "request_origin_rejected");
    assert_eq!(
        body["error"]["message"],
        "Request origin validation failed."
    );
    assert!(
        body["error"]["requestId"]
            .as_str()
            .unwrap()
            .starts_with("req_")
    );
    assert_eq!(body["error"]["retryable"], false);

    let wrong_host = editor_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/bootstrap")
                .header(HOST, "evil.test")
                .header("sec-fetch-site", "same-origin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_host.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn editor_responses_deny_framing_and_allow_only_the_preview_origin() {
    let harness = Harness::new().await;
    let response = editor_router(harness.state)
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    let csp = response
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(csp.contains("frame-src http://preview.test:4322"));
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("style-src 'self' 'unsafe-inline'"));
    assert!(!csp.contains("script-src 'self' 'unsafe-inline'"));
}

#[tokio::test]
async fn project_quota_returns_a_stable_429_without_replacing_existing_state() {
    let harness = Harness::new().await;
    for _ in 0..MAX_PROJECTS {
        let response = harness
            .post(
                "/api/v1/projects",
                json!({"schemaVersion":"1","template":"blank"}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let overflow = harness
        .post(
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await;
    assert_eq!(overflow.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = response_json(overflow).await;
    assert_eq!(body["error"]["code"], "local_capacity_reached");
    assert_eq!(body["error"]["retryable"], false);
    assert_eq!(harness.state.store().unwrap().projects.len(), MAX_PROJECTS);
}

#[test]
fn quota_boundaries_fail_closed_and_expired_authority_is_swept() {
    assert!(ensure_capacity(MAX_TARGETS_PER_PROJECT - 1, MAX_TARGETS_PER_PROJECT).is_ok());
    let count_error =
        ensure_capacity(MAX_TARGETS_PER_PROJECT, MAX_TARGETS_PER_PROJECT).unwrap_err();
    assert_eq!(count_error.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(count_error.code, "local_capacity_reached");
    assert!(
        ensure_byte_capacity(MAX_RETAINED_EXPORT_BYTES - 1, 1, MAX_RETAINED_EXPORT_BYTES).is_ok()
    );
    assert_eq!(
        ensure_byte_capacity(MAX_RETAINED_EXPORT_BYTES, 1, MAX_RETAINED_EXPORT_BYTES)
            .unwrap_err()
            .code,
        "local_capacity_reached"
    );

    let mut store = Store::default();
    store.sessions.insert(
        token_hash("expired-session"),
        Session {
            id: "ses_expired".into(),
            expires_at: 99,
        },
    );
    store.sessions.insert(
        token_hash("live-session"),
        Session {
            id: "ses_live".into(),
            expires_at: 100,
        },
    );
    store.approvals.insert(
        token_hash("expired-approval"),
        ApprovalGrant {
            binding: sample_binding(),
            expires_at: 99,
        },
    );
    store.approvals.insert(
        token_hash("live-approval"),
        ApprovalGrant {
            binding: sample_binding(),
            expires_at: 100,
        },
    );
    sweep_expired_sessions(&mut store, 100);
    sweep_expired_approvals(&mut store, 100);
    assert_eq!(store.sessions.len(), 1);
    assert!(store.sessions.contains_key(&token_hash("live-session")));
    assert_eq!(store.approvals.len(), 1);
    assert!(store.approvals.contains_key(&token_hash("live-approval")));
}

#[test]
fn approval_is_hashed_bound_expiring_and_atomically_one_shot() {
    let token = "never-store-this-approval-token";
    let binding = sample_binding();
    let mut approvals = HashMap::new();
    approvals.insert(
        token_hash(token),
        ApprovalGrant {
            binding: binding.clone(),
            expires_at: 200,
        },
    );
    assert!(!format!("{:?}", approvals.keys().next().unwrap()).contains(token));

    let mut mismatch = binding.clone();
    mismatch.session_id = "ses_other".into();
    let error = consume_approval(&mut approvals, token, &mismatch, 100).unwrap_err();
    assert_eq!(error.code, "approval_binding_mismatch");
    assert_eq!(
        approvals.len(),
        1,
        "binding mismatch must not burn authority"
    );

    consume_approval(&mut approvals, token, &binding, 100).unwrap();
    assert!(approvals.is_empty());
    let replay = consume_approval(&mut approvals, token, &binding, 100).unwrap_err();
    assert_eq!(replay.code, "approval_invalid_or_consumed");

    approvals.insert(
        token_hash(token),
        ApprovalGrant {
            binding: binding.clone(),
            expires_at: 99,
        },
    );
    let expired = consume_approval(&mut approvals, token, &binding, 100).unwrap_err();
    assert_eq!(expired.code, "approval_expired");
    assert!(approvals.is_empty());

    for mutate in [
        |value: &mut ApprovalBinding| value.project_id = "prj_other".into(),
        |value: &mut ApprovalBinding| value.review_id = "revw_other".into(),
        |value: &mut ApprovalBinding| value.proposal_id = "pro_other".into(),
        |value: &mut ApprovalBinding| value.proposal_digest = "00".repeat(32),
        |value: &mut ApprovalBinding| value.expected_revision_id = "rev_other".into(),
        |value: &mut ApprovalBinding| value.disposition = DispositionDto::Rejected,
        |value: &mut ApprovalBinding| value.intent_id = "intent_other".into(),
    ] {
        let mut wrong = binding.clone();
        mutate(&mut wrong);
        approvals.insert(
            token_hash(token),
            ApprovalGrant {
                binding: binding.clone(),
                expires_at: 200,
            },
        );
        assert_eq!(
            consume_approval(&mut approvals, token, &wrong, 100)
                .unwrap_err()
                .code,
            "approval_binding_mismatch"
        );
        approvals.clear();
    }
}

#[test]
fn zip_is_pure_lexical_stored_and_has_fixed_metadata() {
    let files = blank_files();
    let first = deterministic_zip(&files).unwrap();
    let second = deterministic_zip(&files).unwrap();
    assert_eq!(first, second);
    assert_eq!(&first[0..4], &[0x50, 0x4b, 0x03, 0x04]);
    assert_eq!(u16::from_le_bytes([first[8], first[9]]), 0);
    assert_eq!(u16::from_le_bytes([first[10], first[11]]), 0);
    assert_eq!(u16::from_le_bytes([first[12], first[13]]), 0x21);
    let mut archive = zip::ZipArchive::new(Cursor::new(first)).unwrap();
    assert_eq!(archive.by_index(0).unwrap().name(), "index.html");
    assert_eq!(archive.by_index(1).unwrap().name(), "styles.css");
    assert_eq!(archive.by_index(0).unwrap().unix_mode(), Some(0o100644));
    assert_eq!(archive.by_index(1).unwrap().unix_mode(), Some(0o100644));
}

fn sample_binding() -> ApprovalBinding {
    ApprovalBinding {
        session_id: "ses_one".into(),
        project_id: "prj_one".into(),
        review_id: "revw_one".into(),
        proposal_id: "pro_one".into(),
        proposal_digest: "ab".repeat(32),
        expected_revision_id: "rev_one".into(),
        disposition: DispositionDto::AdoptedUnchanged,
        intent_id: "intent_one".into(),
    }
}

async fn response_json(response: Response) -> Value {
    serde_json::from_slice(&response_bytes(response).await).expect("JSON response")
}

async fn response_text(response: Response) -> String {
    String::from_utf8(response_bytes(response).await).expect("UTF-8 response")
}

async fn response_bytes(response: Response) -> Vec<u8> {
    to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .expect("response body")
        .to_vec()
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string at {pointer}: {value}"))
        .to_owned()
}

fn source_fingerprint(root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(
        root: &std::path::Path,
        directory: &std::path::Path,
        output: &mut BTreeMap<String, Vec<u8>>,
    ) {
        let mut entries = std::fs::read_dir(directory)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), output);
            } else {
                let path = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                output.insert(path, std::fs::read(entry.path()).unwrap());
            }
        }
    }

    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}
