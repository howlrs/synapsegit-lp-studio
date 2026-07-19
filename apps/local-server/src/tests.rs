use super::*;
use axum::body::to_bytes;
use axum::http::Request;
use serde_json::Value;
use std::io::Read;
use tempfile::TempDir;
use tower::ServiceExt;

const EDITOR_ORIGIN: &str = "http://editor.test:4321";
const EDITOR_HOST: &str = "editor.test:4321";
const PREVIEW_ORIGIN: &str = "http://localhost:4322";
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

async fn get_preview_for(state: &StudioState, url: &str) -> Response {
    get_preview_with_binding(state, preview_host(url), preview_path(url)).await
}

async fn get_preview_with_binding(state: &StudioState, host: &str, path: &str) -> Response {
    preview_router(state.clone())
        .oneshot(
            Request::builder()
                .uri(path)
                .header(HOST, host)
                .body(Body::empty())
                .expect("preview request"),
        )
        .await
        .expect("preview response")
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

fn test_target_id(seed: u128) -> String {
    format!("tgt_{seed:032x}")
}

fn target_request(revision_id: &str, seed: u128, kind: &str, element_id: &str) -> Value {
    target_request_for_source(revision_id, seed, kind, element_id, "accepted", None)
}

fn target_request_for_source(
    revision_id: &str,
    seed: u128,
    kind: &str,
    element_id: &str,
    capture_source: &str,
    capture_proposal_id: Option<&str>,
) -> Value {
    let (tag_name, label, accessible_name) = match element_id {
        "hero" => ("section", "ヒーローブロック", "ヒーローブロック"),
        "hero-copy" => (
            "p",
            "ヒーロー説明文",
            "伝えたいことを選び、AIとの対話から最初の一歩をつくります。",
        ),
        "hero-cta" => ("a", "ヒーローCTA", "構想を始める"),
        _ => ("h1", "ヒーロー見出し", "まだ、白紙です。"),
    };
    let element_anchor = json!({
        "tagName":tag_name,
        "uniqueElementId":element_id,
        "accessibleName":accessible_name,
        "domPath":format!("main/{tag_name}#{element_id}")
    });
    let geometry = json!({
        "documentCssPixelRect":{"x":80.0,"y":120.0,"width":640.0,"height":96.0},
        "viewportCssPixelRect":{"x":80.0,"y":120.0,"width":640.0,"height":96.0},
        "viewportNormalizedRect":{"x":0.05,"y":0.1,"width":0.4,"height":0.08}
    });
    let mut target = json!({
        "schemaVersion":1,
        "targetId":test_target_id(seed),
        "captureRevisionId":revision_id,
        "captureSource":capture_source,
        "pagePath":"index.html",
        "kind":kind,
        "label":label,
        "viewport":{
            "cssWidth":1600.0,
            "cssHeight":1200.0,
            "scrollX":0.0,
            "scrollY":0.0,
            "devicePixelRatio":1.0,
            "visualViewportScale":1.0,
            "previewScale":1.0
        },
        "document":{"cssWidth":1600.0,"cssHeight":2400.0,"layoutEpoch":1}
    });
    if let Some(proposal_id) = capture_proposal_id {
        target["captureProposalId"] = json!(proposal_id);
    }
    match kind {
        "page" => {}
        "block" => {
            target["geometry"] = geometry;
            target["elementAnchor"] = element_anchor;
            target["block"] = json!({"source":"semantic","level":1});
        }
        "element" => {
            target["geometry"] = geometry;
            target["elementAnchor"] = element_anchor;
        }
        "text" => {
            target["geometry"] = geometry;
            target["elementAnchor"] = element_anchor;
            target["textAnchor"] = json!({
                "exact":accessible_name,
                "startOffset":0,
                "endOffset":accessible_name.encode_utf16().count()
            });
        }
        "point" => {
            target["point"] = json!({
                "documentCssPixel":{"x":120.0,"y":160.0},
                "viewportNormalized":{"x":0.075,"y":0.1333333333}
            });
            target["regionAnchor"] = json!({"containingBlock":element_anchor,"layoutMode":"flow"});
        }
        "region" => {
            target["geometry"] = geometry;
            target["regionAnchor"] = json!({"containingBlock":element_anchor,"layoutMode":"flow"});
        }
        _ => panic!("unsupported test target kind: {kind}"),
    }
    json!({"schemaVersion":"1","target":target})
}

fn element_target_request(revision_id: &str, seed: u128, element_id: &str) -> Value {
    target_request(revision_id, seed, "element", element_id)
}

fn oversized_canonical_target_request(revision_id: &str, seed: u128) -> Value {
    const BODY_LIMIT: usize = 64 * 1024 - 1;
    for token_count in 1..=8 {
        for token_chars in 3..=128 {
            let class_tokens = (0..token_count)
                .map(|index| {
                    let prefix = format!("c{index:02}");
                    format!("{prefix}{}", "界".repeat(token_chars - prefix.len()))
                })
                .collect::<Vec<_>>();
            let anchor = json!({
                "tagName":"section",
                "uniqueElementId":"界".repeat(MAX_TARGET_ANCHOR_LENGTH),
                "role":"界".repeat(MAX_TARGET_CLASS_TOKEN_LENGTH),
                "accessibleName":"界".repeat(512),
                "domPath":"界".repeat(MAX_TARGET_ANCHOR_LENGTH),
                "classTokens":class_tokens,
                "ancestorFingerprint":"界".repeat(MAX_TARGET_ANCHOR_LENGTH)
            });
            let mut request = target_request(revision_id, seed, "region", "hero-heading");
            request["target"]["label"] = json!("x");
            request["target"]["viewport"] = json!({
                "cssWidth":8,
                "cssHeight":8,
                "scrollX":0,
                "scrollY":0,
                "devicePixelRatio":1,
                "visualViewportScale":1,
                "previewScale":1
            });
            request["target"]["document"] = json!({"cssWidth":8,"cssHeight":8,"layoutEpoch":1});
            request["target"]["geometry"] = json!({
                "documentCssPixelRect":{"x":0,"y":0,"width":8,"height":8},
                "viewportCssPixelRect":{"x":0,"y":0,"width":8,"height":8},
                "viewportNormalizedRect":{"x":0,"y":0,"width":1,"height":1}
            });
            request["target"]["regionAnchor"] = json!({
                "containingBlock":anchor,
                "previousVisibleSibling":anchor,
                "nextVisibleSibling":anchor,
                "layoutMode":"flow"
            });

            let initial_length = serde_json::to_vec(&request).unwrap().len();
            if initial_length > BODY_LIMIT {
                continue;
            }
            let label_padding = BODY_LIMIT - initial_length;
            if label_padding >= MAX_TARGET_LABEL_LENGTH {
                continue;
            }
            request["target"]["label"] = json!("x".repeat(label_padding + 1));
            assert_eq!(serde_json::to_vec(&request).unwrap().len(), BODY_LIMIT);
            let parsed: CreateTargetRequest = serde_json::from_value(request.clone()).unwrap();
            assert!(validate_target_shape(&parsed.target).is_ok());
            assert!(
                serde_json::to_vec(&parsed.target).unwrap().len()
                    > STORAGE_MAX_TARGET_METADATA_BYTES
            );
            return request;
        }
    }
    panic!("failed to construct a shape-valid request with oversized canonical metadata");
}

fn context_request(revision_id: &str, target_response: &Value, instruction: &str) -> Value {
    let target_id = string_at(target_response, "/target/targetId");
    json!({
        "schemaVersion":"1",
        "attemptId":format!("attempt-{target_id}"),
        "revisionId":revision_id,
        "targetId":target_id,
        "resolutionId":string_at(target_response, "/resolutionId"),
        "providerId":"fake",
        "requestedModel":"deterministic-v1",
        "instruction":instruction
    })
}

async fn create_blank_fake_proposal(
    harness: &Harness,
    seed: u128,
) -> (String, String, String, String) {
    let project = harness
        .post(
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await;
    assert_eq!(project.status(), StatusCode::CREATED);
    let project = response_json(project).await;
    let project_id = string_at(&project, "/project/id");
    let revision_id = string_at(&project, "/project/revisionId");
    let target = harness
        .post(
            &format!("/api/v1/projects/{project_id}/targets"),
            element_target_request(&revision_id, seed, "hero-heading"),
        )
        .await;
    assert_eq!(target.status(), StatusCode::CREATED);
    let target = response_json(target).await;
    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(&revision_id, &target, "見出しを変更してください。"),
        )
        .await;
    assert_eq!(context.status(), StatusCode::CREATED);
    let context = response_json(context).await;
    let proposal = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":string_at(&context, "/context/id"),
                "contextSha256":string_at(&context, "/context/sha256")
            }),
        )
        .await;
    assert_eq!(proposal.status(), StatusCode::CREATED);
    let proposal = response_json(proposal).await;
    (
        project_id,
        revision_id,
        string_at(&proposal, "/proposal/id"),
        string_at(&proposal, "/proposal/reviewId"),
    )
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
    let accepted_preview_url = string_at(&create_body, "/project/previewUrl");
    assert_eq!(
        create_body["project"]["displayName"],
        "Untitled landing page"
    );
    assert_eq!(create_body["project"]["files"].as_array().unwrap().len(), 2);

    let target = harness
        .post(
            &format!("/api/v1/projects/{project_id}/targets"),
            element_target_request(&original_revision, 1, "hero-heading"),
        )
        .await;
    assert_eq!(target.status(), StatusCode::CREATED);
    let target = response_json(target).await;

    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(
                &original_revision,
                &target,
                "見出しを公開に向けた言葉へ変更してください。",
            ),
        )
        .await;
    assert_eq!(context.status(), StatusCode::CREATED);
    let context_body = response_json(context).await;
    let context_id = string_at(&context_body, "/context/id");
    let context_sha = string_at(&context_body, "/context/sha256");
    let canonical = string_at(&context_body, "/context/canonicalJson");
    assert_eq!(context_body["context"]["providerId"], "fake");
    assert_eq!(
        context_body["context"]["requestedModel"],
        "deterministic-v1"
    );
    assert_eq!(context_body["context"]["provider"]["external"], false);
    assert!(
        context_body["context"]["manifest"]["entries"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty())
    );
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
    let proposal_preview_url = string_at(&proposal_body, "/proposal/previewUrl");
    assert_ne!(accepted_preview_url, proposal_preview_url);
    assert_ne!(
        preview_host(&accepted_preview_url),
        preview_host(&proposal_preview_url),
        "each snapshot must receive an isolated origin"
    );
    assert_ne!(proposal_digest, original_manifest);
    assert_eq!(proposal_body["proposal"]["executionVerified"], false);
    assert_eq!(proposal_body["proposal"]["validation"]["status"], "passed");
    let validation_checks = proposal_body["proposal"]["validation"]["checks"]
        .as_array()
        .unwrap();
    assert!(validation_checks.iter().any(|check| {
        check["id"] == "target-reresolution"
            && check["status"] == "passed"
            && check["blocking"] == false
    }));
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
                .header(HOST, preview_host(&proposal_preview_url))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(proposal_preview.status(), StatusCode::OK);
    let preview_headers = proposal_preview.headers();
    assert_eq!(preview_headers.get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(
        preview_headers.get("clear-site-data").unwrap(),
        "\"cache\", \"cookies\", \"storage\""
    );
    assert_eq!(
        preview_headers.get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        preview_headers.get("referrer-policy").unwrap(),
        "no-referrer"
    );
    assert_eq!(
        preview_headers.get("x-dns-prefetch-control").unwrap(),
        "off"
    );
    assert!(
        preview_headers
            .get("permissions-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("camera=()")
    );
    let preview_csp = preview_headers
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(preview_csp.contains("frame-ancestors http://editor.test:4321"));
    assert!(preview_csp.contains("script-src 'self' 'nonce-"));
    assert!(preview_csp.contains("worker-src 'none'"));
    assert!(preview_csp.contains("frame-src 'none'"));
    assert!(preview_csp.contains("connect-src 'none'"));
    assert!(preview_csp.contains("webrtc 'block'"));
    assert!(preview_csp.contains("form-action 'none'"));
    assert!(!preview_csp.contains("script-src 'unsafe-inline'"));
    let preview_html = response_text(proposal_preview).await;
    assert!(preview_html.contains("対話から、公開できるLPへ。"));
    assert!(preview_html.contains("synapsegit-lp.selection"));
    assert!(preview_html.contains("synapsegit-lp.diagnostic"));
    assert!(preview_html.contains("securitypolicyviolation"));
    assert!(preview_html.contains("csp_blocked"));
    assert!(preview_html.contains("site_error"));
    assert!(preview_html.contains("unhandled_rejection"));
    assert!(preview_html.contains("sourceUnavailable:true"));
    assert!(preview_html.contains("window.name=\"\""));
    assert!(preview_html.contains("pending.splice(0).forEach(send)"));
    assert!(preview_html.contains("nav.addEventListener(\"navigate\""));
    assert!(preview_html.contains("e.preventDefault()"));
    assert!(preview_html.contains("Object.defineProperty(owner,name"));
    assert!(preview_html.contains("RTCPeerConnection"));
    assert!(!preview_html.contains("e.message"));
    assert!(!preview_html.contains("e.filename"));
    assert!(!preview_html.contains("e.stack"));
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
    assert_uniform_preview_not_found(get_preview_for(&harness.state, &proposal_preview_url).await)
        .await;

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
    assert!(!index.contains("synapsegit-lp.diagnostic"));
    assert!(!index.contains("window.name=\"\""));
}

#[tokio::test]
async fn fake_ai_proposal_is_bound_to_each_selected_element() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let cases = [
        (
            "hero-heading",
            "まだ、白紙です。",
            "対話から、公開できるLPへ。",
            "選択したヒーロー見出し",
        ),
        (
            "hero-copy",
            "伝えたいことを選び、AIとの対話から最初の一歩をつくります。",
            "要望を選び、AIとの対話から公開できるLPへ育てます。",
            "選択したヒーロー説明文",
        ),
        (
            "hero-cta",
            "構想を始める",
            "公開LPをつくる",
            "選択したヒーローCTA",
        ),
    ];

    for (case_index, (element_id, accepted_text, proposed_text, summary_fragment)) in
        cases.into_iter().enumerate()
    {
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
                element_target_request(&revision_id, case_index as u128 + 1, element_id),
            )
            .await;
        assert_eq!(target.status(), StatusCode::CREATED, "target {element_id}");
        let target = response_json(target).await;

        let context = harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                context_request(
                    &revision_id,
                    &target,
                    "選択した要素だけを更新してください。",
                ),
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
        assert_eq!(
            canonical["target"]["elementAnchor"]["uniqueElementId"],
            element_id
        );
        assert_eq!(
            canonical["targetResolution"]["targetId"],
            target["target"]["targetId"]
        );
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
        let proposal_preview_url = string_at(&proposal_body, "/proposal/previewUrl");

        let preview = preview_router(harness.state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/preview/{project_id}/{proposal_id}/"))
                    .header(HOST, preview_host(&proposal_preview_url))
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
async fn blocking_warnings_prevent_adoption_but_allow_reject_and_defer() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    for (index, disposition) in ["adopted_unchanged", "rejected", "deferred"]
        .into_iter()
        .enumerate()
    {
        let harness = Harness::new().await;
        let (project_id, revision_id, proposal_id, review_id) =
            create_blank_fake_proposal(&harness, 100 + index as u128).await;
        {
            let mut store = harness.state.store().unwrap();
            let proposal = store
                .projects
                .get_mut(&project_id)
                .and_then(|project| project.proposal.as_mut())
                .expect("pending proposal");
            proposal.validation.status = "warning";
            proposal.validation.checks[0].blocking = true;
        }
        let intent_id = format!("warning-disposition-{index}");
        let approval = harness
            .post(
                &format!("/api/v1/reviews/{review_id}/approvals"),
                json!({
                    "schemaVersion":"1",
                    "proposalId":proposal_id,
                    "expectedRevisionId":revision_id,
                    "disposition":disposition,
                    "intentId":intent_id
                }),
            )
            .await;
        if disposition == "adopted_unchanged" {
            assert_eq!(approval.status(), StatusCode::CONFLICT);
            assert_eq!(
                response_json(approval).await["error"]["code"],
                "proposal_blocked_by_validation"
            );
            continue;
        }

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
                    "disposition":disposition,
                    "intentId":intent_id,
                    "rationale":"警告を確認し、採用せずに終了します。"
                }),
            )
            .await;
        assert_eq!(decision.status(), StatusCode::OK);
        assert_eq!(
            response_json(decision).await["project"]["revisionId"],
            revision_id,
            "non-adopt dispositions must not change Accepted"
        );
    }
}

#[tokio::test]
async fn target_v1_all_six_kinds_resolve_and_bind_context_receipts() {
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
    let cases = [
        ("page", ""),
        ("block", "hero"),
        ("element", "hero-heading"),
        ("text", "hero-heading"),
        ("point", "hero"),
        ("region", "hero"),
    ];

    for (index, (kind, element_id)) in cases.into_iter().enumerate() {
        let response = harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                target_request(&revision_id, index as u128 + 1, kind, element_id),
            )
            .await;
        let status = response.status();
        let target = response_json(response).await;
        assert_eq!(status, StatusCode::CREATED, "{kind} target: {target}");
        assert_eq!(target["target"]["kind"], kind);
        assert_eq!(target["target"]["schemaVersion"], 1);
        assert_eq!(target["resolution"]["schemaVersion"], 1);
        assert_eq!(target["resolution"]["resolverVersion"], 1);
        assert_eq!(target["resolution"]["status"], "resolved");
        assert_eq!(target["resolution"]["selectedCandidateId"], "candidate-1");
        assert_eq!(
            target["resolution"]["targetId"],
            target["target"]["targetId"]
        );
        assert_eq!(target["resolution"]["captureRevisionId"], revision_id);
        assert_eq!(target["resolution"]["resolvedRevisionId"], revision_id);
        let resolution_id = string_at(&target, "/resolutionId");
        assert!(resolution_id.starts_with("res_"));
        assert_eq!(resolution_id.len(), 36);

        let context = harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                context_request(&revision_id, &target, &format!("{kind} target only")),
            )
            .await;
        let context_status = context.status();
        let context = response_json(context).await;
        assert_eq!(
            context_status,
            StatusCode::CREATED,
            "{kind} context: {context}"
        );
        assert_eq!(context["context"]["targetResolutionId"], resolution_id);
        let canonical: Value =
            serde_json::from_str(context["context"]["canonicalJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            canonical["numericEncoding"],
            "serde-json-shortest-decimal-string-v1"
        );
        assert_eq!(canonical["target"]["kind"], kind);
        assert_eq!(canonical["targetResolution"]["status"], "resolved");
        assert!(canonical["target"]["viewport"]["cssWidth"].is_string());
        assert!(canonical["targetResolution"]["candidates"][0]["score"].is_string());
    }

    let page_target = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                target_request(&revision_id, 99, "page", ""),
            )
            .await,
    )
    .await;
    let mut mismatched = context_request(&revision_id, &page_target, "wrong receipt");
    mismatched["resolutionId"] = json!("res_00000000000000000000000000000000");
    let response = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            mismatched,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "target_resolution_mismatch"
    );
}

#[tokio::test]
async fn ambiguous_and_detached_targets_are_preserved_but_cannot_create_contexts() {
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

    let mut ambiguous_request = element_target_request(&revision_id, 1, "hero-heading");
    ambiguous_request["target"]["elementAnchor"]["uniqueElementId"] =
        json!("missing-but-text-matches");
    let ambiguous = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                ambiguous_request,
            )
            .await,
    )
    .await;
    assert_eq!(ambiguous["resolution"]["status"], "ambiguous");
    assert!(
        ambiguous["resolution"]["selectedCandidateId"].is_null(),
        "an ambiguous result must never select a candidate"
    );
    assert_eq!(
        ambiguous["resolution"]["candidates"][0]["reasons"][0],
        "text_quote"
    );
    let ambiguous_context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(&revision_id, &ambiguous, "must not guess"),
        )
        .await;
    assert_eq!(ambiguous_context.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(ambiguous_context).await["error"]["code"],
        "target_ambiguous"
    );

    let mut detached_request = element_target_request(&revision_id, 2, "hero-heading");
    detached_request["target"]["elementAnchor"]["uniqueElementId"] = json!("missing-detached");
    detached_request["target"]["elementAnchor"]["accessibleName"] =
        json!("content that is not in the document");
    let detached = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                detached_request,
            )
            .await,
    )
    .await;
    assert_eq!(detached["resolution"]["status"], "detached");
    assert!(
        detached["resolution"]["candidates"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let detached_context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(&revision_id, &detached, "must fail closed"),
        )
        .await;
    assert_eq!(detached_context.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(detached_context).await["error"]["code"],
        "target_detached"
    );
}

#[tokio::test]
async fn proposal_captured_target_becomes_stale_after_the_proposal_is_closed() {
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
    let accepted_target = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                element_target_request(&revision_id, 1, "hero-heading"),
            )
            .await,
    )
    .await;
    let context = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                context_request(&revision_id, &accepted_target, "make a proposal"),
            )
            .await,
    )
    .await;
    let proposal = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/proposals"),
                json!({
                    "schemaVersion":"1",
                    "contextId":string_at(&context, "/context/id"),
                    "contextSha256":string_at(&context, "/context/sha256")
                }),
            )
            .await,
    )
    .await;
    let proposal_id = string_at(&proposal, "/proposal/id");
    let review_id = string_at(&proposal, "/proposal/reviewId");
    let proposed_target_request = target_request_for_source(
        &revision_id,
        2,
        "element",
        "hero-heading",
        "proposal",
        Some(&proposal_id),
    );
    let proposed_target_response = harness
        .post(
            &format!("/api/v1/projects/{project_id}/targets"),
            proposed_target_request,
        )
        .await;
    let proposed_target_status = proposed_target_response.status();
    let proposed_target = response_json(proposed_target_response).await;
    assert_eq!(
        proposed_target_status,
        StatusCode::CREATED,
        "proposal target: {proposed_target}"
    );
    assert_eq!(proposed_target["resolution"]["status"], "resolved");
    assert_eq!(proposed_target["target"]["captureProposalId"], proposal_id);

    let intent = "close-proposal-target-test";
    let approval = response_json(
        harness
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
            .await,
    )
    .await;
    let decision = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            json!({
                "schemaVersion":"1",
                "approvalToken":string_at(&approval, "/approval/token"),
                "proposalId":proposal_id,
                "expectedRevisionId":revision_id,
                "disposition":"rejected",
                "intentId":intent,
                "rationale":"close the proposal fixture"
            }),
        )
        .await;
    assert_eq!(decision.status(), StatusCode::OK);

    let stale_context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(
                &revision_id,
                &proposed_target,
                "this proposal target is no longer authoritative",
            ),
        )
        .await;
    assert_eq!(stale_context.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(stale_context).await["error"]["code"],
        "target_source_proposal_stale"
    );
}

#[tokio::test]
async fn target_is_immutable_private_metadata_and_rehydrates_across_restart() {
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
    let created = response_json(
        post_for(
            &first_state,
            &first_token,
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await,
    )
    .await;
    let project_id = string_at(&created, "/project/id");
    let revision_id = string_at(&created, "/project/revisionId");
    let request = element_target_request(&revision_id, 1, "hero-heading");
    let expected_target = request["target"].clone();
    let response = post_for(
        &first_state,
        &first_token,
        &format!("/api/v1/projects/{project_id}/targets"),
        request.clone(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let target = response_json(response).await;
    let target_id = string_at(&target, "/target/targetId");
    let target_path = root
        .path()
        .join("managed-v1/projects")
        .join(&project_id)
        .join("targets")
        .join(format!("{target_id}.json"));
    let persisted_before = std::fs::read(&target_path).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&persisted_before).unwrap(),
        expected_target
    );
    assert!(!String::from_utf8_lossy(&persisted_before).contains("runtimeHandle"));

    let duplicate = post_for(
        &first_state,
        &first_token,
        &format!("/api/v1/projects/{project_id}/targets"),
        request,
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(duplicate).await["error"]["code"],
        "target_id_exists"
    );
    assert_eq!(std::fs::read(&target_path).unwrap(), persisted_before);

    let mut runtime_handle = element_target_request(&revision_id, 2, "hero-heading");
    runtime_handle["target"]["runtimeHandle"] = json!("node-123");
    let rejected = post_for(
        &first_state,
        &first_token,
        &format!("/api/v1/projects/{project_id}/targets"),
        runtime_handle,
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    assert!(
        !target_path
            .with_file_name(format!("{}.json", test_target_id(2)))
            .exists()
    );

    drop(first_state);
    let second_state = StudioState::new(config()).unwrap();
    let second_token = bootstrap_token(&second_state).await;
    let context = post_for(
        &second_state,
        &second_token,
        &format!("/api/v1/projects/{project_id}/contexts"),
        context_request(&revision_id, &target, "restored target"),
    )
    .await;
    let context_status = context.status();
    let context = response_json(context).await;
    assert_eq!(
        context_status,
        StatusCode::CREATED,
        "restored target context: {context}"
    );
    assert_eq!(
        context["context"]["targetResolutionId"],
        target["resolutionId"]
    );
    assert_eq!(std::fs::read(&target_path).unwrap(), persisted_before);
}

#[tokio::test]
async fn target_capacity_reuses_only_unreferenced_persisted_metadata() {
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
    let created = response_json(
        post_for(
            &first_state,
            &first_token,
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await,
    )
    .await;
    let project_id = string_at(&created, "/project/id");
    let revision_id = string_at(&created, "/project/revisionId");
    let targets_uri = format!("/api/v1/projects/{project_id}/targets");
    let contexts_uri = format!("/api/v1/projects/{project_id}/contexts");

    let protected = response_json(
        post_for(
            &first_state,
            &first_token,
            &targets_uri,
            element_target_request(&revision_id, 1, "hero-heading"),
        )
        .await,
    )
    .await;
    let protected_context = post_for(
        &first_state,
        &first_token,
        &contexts_uri,
        context_request(&revision_id, &protected, "retain referenced target"),
    )
    .await;
    assert_eq!(protected_context.status(), StatusCode::CREATED);

    let mut recyclable = Value::Null;
    let mut retained_unreferenced = Vec::new();
    for seed in 2..=MAX_TARGETS_PER_PROJECT as u128 {
        let response = post_for(
            &first_state,
            &first_token,
            &targets_uri,
            element_target_request(&revision_id, seed, "hero-heading"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED, "seed {seed}");
        let response = response_json(response).await;
        if seed == 2 {
            recyclable = response;
        } else {
            retained_unreferenced.push(response);
        }
    }
    assert_ne!(recyclable, Value::Null);

    let targets_root = root
        .path()
        .join("managed-v1/projects")
        .join(&project_id)
        .join("targets");
    let recyclable_path = targets_root.join(format!("{}.json", test_target_id(2)));
    let recyclable_bytes = std::fs::read(&recyclable_path).unwrap();
    let oversized_response = post_for(
        &first_state,
        &first_token,
        &targets_uri,
        oversized_canonical_target_request(&revision_id, 99),
    )
    .await;
    assert_eq!(oversized_response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read(&recyclable_path).unwrap(), recyclable_bytes);
    {
        let store = first_state.store().unwrap();
        let project = store.projects.get(&project_id).unwrap();
        assert!(project.targets.contains_key(&test_target_id(2)));
        assert!(!project.targets.contains_key(&test_target_id(99)));
    }

    let replacement_response = post_for(
        &first_state,
        &first_token,
        &targets_uri,
        element_target_request(&revision_id, 33, "hero-heading"),
    )
    .await;
    assert_eq!(replacement_response.status(), StatusCode::CREATED);
    let replacement = response_json(replacement_response).await;
    retained_unreferenced.push(replacement.clone());

    {
        let store = first_state.store().unwrap();
        let project = store.projects.get(&project_id).unwrap();
        assert_eq!(project.targets.len(), MAX_TARGETS_PER_PROJECT);
        assert!(project.targets.contains_key(&test_target_id(1)));
        assert!(!project.targets.contains_key(&test_target_id(2)));
        assert!(project.targets.contains_key(&test_target_id(33)));
        assert!(
            project
                .contexts
                .values()
                .any(|context| context.target_id == test_target_id(1))
        );
    }
    assert!(
        targets_root
            .join(format!("{}.json", test_target_id(1)))
            .is_file()
    );
    assert!(
        !targets_root
            .join(format!("{}.json", test_target_id(2)))
            .exists()
    );
    assert!(
        targets_root
            .join(format!("{}.json", test_target_id(33)))
            .is_file()
    );

    let evicted_context = post_for(
        &first_state,
        &first_token,
        &contexts_uri,
        context_request(&revision_id, &recyclable, "evicted target must be detached"),
    )
    .await;
    assert_eq!(evicted_context.status(), StatusCode::NOT_FOUND);

    for target in &retained_unreferenced {
        let context = post_for(
            &first_state,
            &first_token,
            &contexts_uri,
            context_request(&revision_id, target, "protect retained target"),
        )
        .await;
        assert_eq!(context.status(), StatusCode::CREATED);
    }
    let fully_referenced_overflow = post_for(
        &first_state,
        &first_token,
        &targets_uri,
        element_target_request(&revision_id, 34, "hero-heading"),
    )
    .await;
    assert_eq!(
        fully_referenced_overflow.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        response_json(fully_referenced_overflow).await["error"]["code"],
        "local_capacity_reached"
    );
    assert!(
        !targets_root
            .join(format!("{}.json", test_target_id(34)))
            .exists()
    );

    drop(first_state);
    let restarted = StudioState::new(config()).unwrap();
    let restarted_token = bootstrap_token(&restarted).await;
    let restored_protected = post_for(
        &restarted,
        &restarted_token,
        &contexts_uri,
        context_request(
            &revision_id,
            &protected,
            "protected target survives restart",
        ),
    )
    .await;
    assert_eq!(restored_protected.status(), StatusCode::CREATED);
    let restored_replacement = post_for(
        &restarted,
        &restarted_token,
        &contexts_uri,
        context_request(&revision_id, &replacement, "replacement survives restart"),
    )
    .await;
    assert_eq!(restored_replacement.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn restart_rejects_a_target_whose_filename_and_canonical_id_disagree() {
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
    let state = StudioState::new(config()).unwrap();
    let token = bootstrap_token(&state).await;
    let created = response_json(
        post_for(
            &state,
            &token,
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await,
    )
    .await;
    let project_id = string_at(&created, "/project/id");
    let revision_id = string_at(&created, "/project/revisionId");
    let target = response_json(
        post_for(
            &state,
            &token,
            &format!("/api/v1/projects/{project_id}/targets"),
            element_target_request(&revision_id, 1, "hero-heading"),
        )
        .await,
    )
    .await;
    let target_id = string_at(&target, "/target/targetId");
    let target_path = root
        .path()
        .join("managed-v1/projects")
        .join(project_id)
        .join("targets")
        .join(format!("{target_id}.json"));
    drop(state);

    let mut mismatched = target["target"].clone();
    mismatched["targetId"] = json!(test_target_id(2));
    std::fs::write(target_path, serde_json::to_vec(&mismatched).unwrap()).unwrap();

    let error = StudioState::new(config()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
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
    let first_preview_url = string_at(&created, "/project/previewUrl");
    let first_secret_bound_url =
        scoped_preview_url(&first_state, "ses_fixed", &project_id, &revision_id);
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
    assert_ne!(
        listed["projects"][0]["previewUrl"], first_preview_url,
        "a restarted server must not reproduce a prior session origin"
    );
    assert_ne!(
        scoped_preview_url(&second_state, "ses_fixed", &project_id, &revision_id),
        first_secret_bound_url,
        "the process preview secret must rotate across state reconstruction"
    );

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
        br#"<!doctype html><h1>Imported</h1><a href="docs/">Nested documentation</a>"#,
    )
    .unwrap();
    std::fs::create_dir(source.path().join("docs")).unwrap();
    std::fs::write(
        source.path().join("docs/index.html"),
        br#"<!doctype html><h1 data-lp-id="nested-heading">Nested documentation</h1><a href="../">Back to project root</a>"#,
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
    assert_eq!(capability["previewOrigin"], PREVIEW_ORIGIN);
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
        3
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
    assert_eq!(confirmed["project"]["files"].as_array().unwrap().len(), 3);
    let preview_url = string_at(&confirmed, "/project/previewUrl");
    let nested_response = get_preview_for(&harness.state, &format!("{preview_url}docs/")).await;
    assert_eq!(nested_response.status(), StatusCode::OK);
    assert_eq!(
        nested_response.headers().get(CONTENT_TYPE).unwrap(),
        "text/html"
    );
    let nested_html = String::from_utf8(response_bytes(nested_response).await).unwrap();
    assert!(nested_html.contains("Nested documentation"));
    assert!(nested_html.contains("href=\"../\""));
    assert!(nested_html.contains(",\"docs/index.html\");</script>"));
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
    let restarted_preview_url = string_at(&listed, "/projects/0/previewUrl");
    let restarted_nested =
        get_preview_for(&restarted, &format!("{restarted_preview_url}docs/")).await;
    assert_eq!(restarted_nested.status(), StatusCode::OK);
    assert!(
        String::from_utf8(response_bytes(restarted_nested).await)
            .unwrap()
            .contains("Nested documentation")
    );
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
                element_target_request(&revision_id, 1, "hero-heading"),
            )
            .await,
    )
    .await;
    let context = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                context_request(&revision_id, &target, "change heading"),
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
    assert!(csp.contains("frame-src http://*.localhost:4322"));
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("style-src 'self' 'unsafe-inline'"));
    assert!(!csp.contains("script-src 'self' 'unsafe-inline'"));
}

#[tokio::test]
async fn preview_origins_are_session_and_route_bound_with_uniform_not_found_responses() {
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
    let first_url = string_at(&created, "/project/previewUrl");
    let first_host = preview_host(&first_url).to_owned();
    assert!(first_url.starts_with("http://pv-"));
    assert!(first_url.ends_with(&format!(
        ".localhost:4322/preview/{project_id}/{revision_id}/"
    )));
    let first_label = first_host
        .strip_suffix(".localhost:4322")
        .unwrap()
        .strip_prefix("pv-")
        .unwrap();
    assert_eq!(first_label.len(), 32);
    assert!(
        first_label
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert!(!first_url.contains('@'));
    assert!(!first_url.contains('?'));
    assert!(!first_url.contains('#'));
    assert_eq!(
        get_preview_for(&harness.state, &first_url).await.status(),
        StatusCode::OK
    );

    let second_token = bootstrap_token(&harness.state).await;
    let second_project_view = response_json(
        get_for(
            &harness.state,
            &second_token,
            &format!("/api/v1/projects/{project_id}"),
        )
        .await,
    )
    .await;
    let second_url = string_at(&second_project_view, "/project/previewUrl");
    assert_ne!(first_url, second_url);
    assert_ne!(preview_host(&first_url), preview_host(&second_url));
    assert_eq!(
        get_preview_for(&harness.state, &second_url).await.status(),
        StatusCode::OK
    );

    let second_project = response_json(
        post_for(
            &harness.state,
            &second_token,
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await,
    )
    .await;
    let second_project_id = string_at(&second_project, "/project/id");
    let second_project_url = string_at(&second_project, "/project/previewUrl");
    assert_ne!(preview_host(&second_url), preview_host(&second_project_url));

    let invalid_bindings = [
        (
            "localhost:4322".to_owned(),
            format!("/preview/{project_id}/{revision_id}/"),
        ),
        (
            "127.0.0.1:4322".to_owned(),
            format!("/preview/{project_id}/{revision_id}/"),
        ),
        (
            first_host.replace(":4322", ":4323"),
            format!("/preview/{project_id}/{revision_id}/"),
        ),
        (
            first_host.clone(),
            format!("/preview/{second_project_id}/{revision_id}/"),
        ),
        (
            first_host.clone(),
            format!("/preview/{project_id}/rev_foreign/"),
        ),
        (
            first_host.clone(),
            format!("/preview/{project_id}/{revision_id}/missing.css"),
        ),
        (first_host.clone(), "/not-a-preview-route".to_owned()),
    ];
    for (host, path) in invalid_bindings {
        assert_uniform_preview_not_found(
            get_preview_with_binding(&harness.state, &host, &path).await,
        )
        .await;
    }

    {
        let mut store = harness.state.store().unwrap();
        store
            .sessions
            .get_mut(&token_hash(&harness.token))
            .unwrap()
            .expires_at = now_unix().saturating_sub(1);
    }
    assert_uniform_preview_not_found(get_preview_for(&harness.state, &first_url).await).await;
    assert_eq!(
        get_preview_for(&harness.state, &second_url).await.status(),
        StatusCode::OK,
        "expiring one session must not revoke another session's origin"
    );
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
fn active_attempt_claim_rejects_without_overwriting_the_in_flight_attempt() {
    let mut active_attempts = HashMap::new();
    claim_ai_attempt(&mut active_attempts, "prj_one", "attempt-one").unwrap();

    let error = claim_ai_attempt(&mut active_attempts, "prj_one", "attempt-two").unwrap_err();

    assert_eq!(error.status, StatusCode::CONFLICT);
    assert_eq!(error.code, "ai_attempt_in_progress");
    assert_eq!(
        active_attempts.get("prj_one").map(String::as_str),
        Some("attempt-one")
    );
}

#[tokio::test]
async fn active_attempt_guard_releases_cancelled_attempt_without_touching_a_later_one() {
    let harness = Harness::new().await;
    {
        let mut store = harness.state.store().unwrap();
        claim_ai_attempt(&mut store.active_attempts, "prj_one", "attempt-one").unwrap();
    }
    drop(ActiveAttemptGuard::new(
        harness.state.clone(),
        "prj_one",
        "attempt-one",
    ));
    assert!(
        !harness
            .state
            .store()
            .unwrap()
            .active_attempts
            .contains_key("prj_one")
    );

    {
        let mut store = harness.state.store().unwrap();
        claim_ai_attempt(&mut store.active_attempts, "prj_one", "attempt-two").unwrap();
    }
    drop(ActiveAttemptGuard::new(
        harness.state.clone(),
        "prj_one",
        "attempt-one",
    ));
    assert_eq!(
        harness
            .state
            .store()
            .unwrap()
            .active_attempts
            .get("prj_one")
            .map(String::as_str),
        Some("attempt-two")
    );
}

#[test]
fn proposal_target_reresolution_is_evidenced_and_fails_closed() {
    let revision_id = "rev_11111111111111111111111111111111";
    let target: TargetRecord = serde_json::from_value(
        element_target_request(revision_id, 1, "hero-heading")["target"].clone(),
    )
    .unwrap();

    let resolved =
        proposal_target_reresolution_check(&target, &blank_files(), revision_id).unwrap();
    assert_eq!(resolved.id, "target-reresolution");
    assert_eq!(resolved.status, StaticCheckStatus::Passed);

    let mut ambiguous_files = blank_files();
    let ambiguous_html = String::from_utf8(ambiguous_files["index.html"].clone())
        .unwrap()
        .replace(
            "</main>",
            "<h1 data-lp-id=\"hero-heading\">duplicate</h1></main>",
        );
    ambiguous_files.insert("index.html".into(), ambiguous_html.into_bytes());
    let ambiguous =
        proposal_target_reresolution_check(&target, &ambiguous_files, revision_id).unwrap_err();
    assert_eq!(ambiguous.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(ambiguous.code, "proposal_target_ambiguous");

    let mut detached_files = blank_files();
    let detached_html = String::from_utf8(detached_files["index.html"].clone())
        .unwrap()
        .replace("hero-heading", "removed-heading")
        .replace("まだ、白紙です。", "削除済み");
    detached_files.insert("index.html".into(), detached_html.into_bytes());
    let detached =
        proposal_target_reresolution_check(&target, &detached_files, revision_id).unwrap_err();
    assert_eq!(detached.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detached.code, "proposal_target_detached");

    let comment_spoof_html = String::from_utf8(detached_files["index.html"].clone())
        .unwrap()
        .replace(
            "</body>",
            "<!-- <h1 data-lp-id=\"hero-heading\">まだ、白紙です。</h1> --></body>",
        );
    detached_files.insert("index.html".into(), comment_spoof_html.into_bytes());
    let comment_spoof =
        proposal_target_reresolution_check(&target, &detached_files, revision_id).unwrap_err();
    assert_eq!(comment_spoof.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(comment_spoof.code, "proposal_target_ambiguous");

    detached_files.remove("index.html");
    let missing_page =
        proposal_target_reresolution_check(&target, &detached_files, revision_id).unwrap_err();
    assert_eq!(missing_page.code, "proposal_target_detached");
}

#[test]
fn provider_context_redacts_common_credential_forms_case_insensitively() {
    let source = concat!(
        "Authorization: bearer very-secret-token\n",
        "AWS_ACCESS_KEY_ID=AKIA1234567890ABCDEF\n",
        "github_token=ghp_1234567890abcdef\n",
        "const client_secret = \"SK-1234567890abcdef\";\n",
        "path=/Users/alice/private-project\n",
    );

    let (redacted, categories) = redact_sensitive_text(source);

    for secret in [
        "very-secret-token",
        "AKIA1234567890ABCDEF",
        "ghp_1234567890abcdef",
        "SK-1234567890abcdef",
        "/Users/alice/private-project",
    ] {
        assert!(!redacted.contains(secret), "credential leaked: {secret}");
    }
    for category in [
        "absolute_path",
        "aws_access_key",
        "bearer_token",
        "credential_assignment",
        "github_token",
        "openai_key",
    ] {
        assert!(
            categories.iter().any(|value| value == category),
            "missing redaction category: {category}"
        );
    }
}

#[tokio::test]
async fn redacted_site_context_is_reviewable_but_generation_fails_before_provider_execution() {
    let source = tempfile::tempdir().unwrap();
    let secret = "Bearer very-secret-site-token";
    let source_html =
        BLANK_INDEX.replace("<body>", &format!("<body data-private-note=\"{secret}\">"));
    std::fs::write(source.path().join("index.html"), source_html.as_bytes()).unwrap();
    std::fs::write(source.path().join("styles.css"), BLANK_STYLES.as_bytes()).unwrap();
    let harness = Harness::with_import(source).await;

    let preview = response_json(
        harness
            .post("/api/v1/imports/previews", json!({"schemaVersion":"1"}))
            .await,
    )
    .await;
    let confirmed = harness
        .post(
            &format!(
                "/api/v1/imports/{}/confirm",
                string_at(&preview, "/importPreview/id")
            ),
            json!({
                "schemaVersion":"1",
                "expectedManifestSha256":string_at(&preview, "/importPreview/manifestSha256")
            }),
        )
        .await;
    assert_eq!(confirmed.status(), StatusCode::CREATED);
    let confirmed = response_json(confirmed).await;
    let project_id = string_at(&confirmed, "/project/id");
    let revision_id = string_at(&confirmed, "/project/revisionId");
    let target = harness
        .post(
            &format!("/api/v1/projects/{project_id}/targets"),
            element_target_request(&revision_id, 501, "hero-heading"),
        )
        .await;
    assert_eq!(target.status(), StatusCode::CREATED);
    let target = response_json(target).await;
    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(&revision_id, &target, "見出しを変更してください。"),
        )
        .await;
    assert_eq!(context.status(), StatusCode::CREATED);
    let context = response_json(context).await;
    let canonical_text = context["context"]["canonicalJson"].as_str().unwrap();
    assert!(!canonical_text.contains(secret));
    let canonical: Value = serde_json::from_str(canonical_text).unwrap();
    let entry = &canonical["manifest"]["entries"][0];
    let content = canonical["untrustedSiteContent"][0]["content"]
        .as_str()
        .unwrap();
    let source_sha256 = canonical["untrustedSiteContent"][0]["sourceSha256"]
        .as_str()
        .unwrap();
    let included_sha256 = canonical["untrustedSiteContent"][0]["includedSha256"]
        .as_str()
        .unwrap();
    assert_eq!(source_sha256, raw_sha256(source_html.as_bytes()));
    assert_eq!(included_sha256, raw_sha256(content.as_bytes()));
    assert_ne!(source_sha256, included_sha256);
    assert_eq!(entry["sha256"], included_sha256);
    assert_eq!(entry["redacted"], true);

    let forged = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":string_at(&context, "/context/id"),
                "contextSha256":"0".repeat(64)
            }),
        )
        .await;
    assert_eq!(forged.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(forged).await["error"]["code"],
        "context_digest_mismatch"
    );

    let proposal = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":string_at(&context, "/context/id"),
                "contextSha256":string_at(&context, "/context/sha256")
            }),
        )
        .await;
    assert_eq!(proposal.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let proposal = response_json(proposal).await;
    assert_eq!(
        proposal["error"]["code"],
        "provider_context_redacted_source_unsupported"
    );
    assert!(!serde_json::to_string(&proposal).unwrap().contains(secret));
    let store = harness.state.store().unwrap();
    assert!(!store.active_attempts.contains_key(&project_id));
    assert!(
        store
            .projects
            .get(&project_id)
            .is_some_and(|project| project.proposal.is_none())
    );
}

#[tokio::test]
async fn redacted_instruction_with_clean_site_files_can_generate_a_proposal() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let harness = Harness::new().await;
    let project = response_json(
        harness
            .post(
                "/api/v1/projects",
                json!({"schemaVersion":"1","template":"blank"}),
            )
            .await,
    )
    .await;
    let project_id = string_at(&project, "/project/id");
    let revision_id = string_at(&project, "/project/revisionId");
    let target = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/targets"),
                element_target_request(&revision_id, 502, "hero-heading"),
            )
            .await,
    )
    .await;
    let secret = "Bearer very-secret-instruction-token";
    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(
                &revision_id,
                &target,
                &format!("Authorization: {secret}\n見出しを変更してください。"),
            ),
        )
        .await;
    assert_eq!(context.status(), StatusCode::CREATED);
    let context = response_json(context).await;
    let canonical_text = context["context"]["canonicalJson"].as_str().unwrap();
    assert!(!canonical_text.contains(secret));
    let canonical: Value = serde_json::from_str(canonical_text).unwrap();
    assert!(
        canonical["manifest"]["entries"]
            .as_array()
            .is_some_and(|entries| entries.iter().all(|entry| entry["redacted"] == false))
    );
    assert!(
        canonical["instructionRedactions"]
            .as_array()
            .is_some_and(|redactions| !redactions.is_empty())
    );

    let proposal = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":string_at(&context, "/context/id"),
                "contextSha256":string_at(&context, "/context/sha256")
            }),
        )
        .await;
    assert_eq!(proposal.status(), StatusCode::CREATED);
    let proposal = response_json(proposal).await;
    assert_eq!(proposal["proposal"]["attribution"]["providerId"], "fake");
    assert!(!serde_json::to_string(&proposal).unwrap().contains(secret));
}

#[test]
fn target_page_path_matches_contract_safety_and_utf8_boundaries() {
    let ascii_boundary = format!("{}.html", "a".repeat(STORAGE_MAX_PATH_BYTES - 5));
    let multibyte_boundary = format!("{}.html", "界".repeat(169));
    assert_eq!(ascii_boundary.len(), STORAGE_MAX_PATH_BYTES);
    assert_eq!(multibyte_boundary.len(), STORAGE_MAX_PATH_BYTES);
    for accepted in [
        "index.html",
        "INDEX.HTML",
        "pages/Café/ランディング.HtM",
        "docs/a#b.html",
        &ascii_boundary,
        &multibyte_boundary,
    ] {
        assert!(is_safe_page_path(accepted), "{accepted}");
        assert_eq!(
            mime_guess::from_path(accepted)
                .first_or_octet_stream()
                .essence_str(),
            "text/html",
            "{accepted}",
        );
    }

    let over_ascii_boundary = format!("{}.html", "a".repeat(STORAGE_MAX_PATH_BYTES - 4));
    let over_multibyte_boundary = format!("{}.html", "界".repeat(170));
    for rejected in [
        "",
        "/",
        "/index.html",
        "C:/index.html",
        "pages/",
        "pages//index.html",
        "pages/./index.html",
        "pages/../index.html",
        "index.html?draft=1",
        "pages\\index.html",
        "bad./index.html",
        "con/index.html",
        "C:index.html",
        "pages/line\nbreak.html",
        ".html",
        "docs/.html",
        "index.css",
        "pages/Cafe\u{301}.HTML",
        &over_ascii_boundary,
        &over_multibyte_boundary,
    ] {
        assert!(!is_safe_page_path(rejected), "{rejected}");
    }
}

#[test]
fn preview_directory_route_resolves_only_one_safe_trailing_slash() {
    assert_eq!(
        canonical_preview_file_path("docs/"),
        Some("docs/index.html".into())
    );
    assert_eq!(
        canonical_preview_file_path("docs/guide.html"),
        Some("docs/guide.html".into())
    );
    for rejected in [
        "",
        "/",
        "/docs/",
        "docs//",
        "docs///",
        "docs/./",
        "docs/../",
        "../docs/",
        "docs\\",
        "docs\\index.html",
    ] {
        assert_eq!(canonical_preview_file_path(rejected), None, "{rejected}");
    }
    let oversized_directory = format!("{}/", "a".repeat(STORAGE_MAX_PATH_BYTES - 1));
    assert_eq!(canonical_preview_file_path(&oversized_directory), None);
}

#[test]
fn preview_bridge_is_response_only_first_script_and_privacy_safe_before_handshake() {
    let source = br#"<!doctype html><!-- <script src="comment.js"></script><header> --><html><head><meta charset="utf-8"><script src="app.js"></script></head><body><header>source</header></body></html>"#;
    let original = source.to_vec();
    let injected = inject_preview_bridge(
        source,
        "fixed-nonce",
        EDITOR_ORIGIN,
        "prj_fixed",
        "pro_fixed",
        "rev_fixed",
        "docs/index.html",
    )
    .unwrap();
    assert_eq!(
        source,
        original.as_slice(),
        "injection must not mutate source"
    );
    let injected = String::from_utf8(injected).unwrap();
    let bridge = injected.find("<script nonce=\"fixed-nonce\">").unwrap();
    let source_script = injected.find("<script src=\"app.js\">").unwrap();
    let head = injected.find("<head>").unwrap();
    assert_eq!(bridge, "<!doctype html>".len());
    assert!(bridge < head);
    assert!(
        bridge < source_script,
        "bridge listeners must be installed first"
    );
    assert!(injected.contains("window.name=\"\""));
    assert!(injected.contains("seen=new Set(),pending=[]"));
    assert!(injected.contains("channel?send(code):pending.push(code)"));
    assert!(injected.contains("pending.splice(0).forEach(send)"));
    assert!(injected.contains("securitypolicyviolation"));
    assert!(injected.contains("addEventListener(\"error\",()=>diagnose(\"site_error\"),true)"));
    assert!(injected.contains("nav.addEventListener(\"navigate\""));
    assert!(injected.contains("Function.call.bind(String.prototype.startsWith)"));
    assert!(injected.contains("Function.call.bind(Event.prototype.preventDefault)"));
    assert!(injected.contains("getter(NavigateEvent.prototype,\"destination\")"));
    assert!(injected.contains("getter(NavigationDestination.prototype,\"url\")"));
    assert!(injected.contains("startsWith(destination,A)"));
    assert!(!injected.contains("new URL("));
    assert!(injected.contains("Object.defineProperty(owner,name"));
    assert!(injected.contains("RTCPeerConnection"));
    assert!(injected.contains("function previewTargetRuntime"));
    assert!(injected.contains(",\"docs/index.html\");</script>"));
    assert!(
        injected.contains("\n}))(") && !injected.contains("\n});)("),
        "the extracted target runtime must remain a callable expression"
    );
    assert!(injected.contains("sourceUnavailable:true"));
    for private_field in [
        "error.message",
        "error.filename",
        "error.stack",
        "reason.stack",
    ] {
        assert!(!injected.contains(private_field));
    }

    let no_doctype =
        br#"<template><script src="template.js"></script></template><noscript><script src="fallback.js"></script></noscript>"#;
    let injected_no_doctype = inject_preview_bridge(
        no_doctype,
        "second-nonce",
        EDITOR_ORIGIN,
        "prj_fixed",
        "rev_fixed",
        "rev_fixed",
        "index.html",
    )
    .unwrap();
    assert!(injected_no_doctype.starts_with(b"<script nonce=\"second-nonce\">"));
    assert_eq!(
        preview_bridge_insertion("\u{feff} \n<!DOCTYPE html><html>"),
        20
    );
}

#[test]
fn preview_scope_and_length_delimited_labels_are_canonical() {
    assert_eq!(
        validate_preview_scope_origin("http://localhost:4322").unwrap(),
        4322
    );
    for invalid in [
        "http://127.0.0.1:4322",
        "https://localhost:4322",
        "http://localhost:04322",
        "http://localhost:0",
        "http://localhost:80",
        "http://localhost:4322/",
        "http://user@localhost:4322",
        "http://localhost:4322?query=1",
        "http://localhost:4322#fragment",
    ] {
        assert!(
            validate_preview_scope_origin(invalid).is_err(),
            "accepted non-canonical scope: {invalid}"
        );
    }

    let root = tempfile::tempdir().unwrap();
    let state = StudioState::new(ServerConfig::new(
        EDITOR_ORIGIN,
        EDITOR_HOST,
        PREVIEW_ORIGIN,
        root.path(),
        root.path().join("missing-web-dist"),
    ))
    .unwrap();
    let split_one = scoped_preview_label(&state, "ses_ab", "c", "snapshot");
    let split_two = scoped_preview_label(&state, "ses_a", "bc", "snapshot");
    assert_ne!(
        split_one, split_two,
        "field boundaries must affect the digest"
    );
    assert_eq!(
        split_one,
        scoped_preview_label(&state, "ses_ab", "c", "snapshot")
    );
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

async fn assert_uniform_preview_not_found(response: Response) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(
        response.headers().get("clear-site-data").unwrap(),
        "\"cache\", \"cookies\", \"storage\""
    );
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        response.headers().get("referrer-policy").unwrap(),
        "no-referrer"
    );
    assert_eq!(
        response.headers().get("x-dns-prefetch-control").unwrap(),
        "off"
    );
    assert!(response.headers().contains_key("permissions-policy"));
    assert_eq!(response_bytes(response).await, b"Not found");
}

fn string_at(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string at {pointer}: {value}"))
        .to_owned()
}

fn preview_host(url: &str) -> &str {
    url.strip_prefix("http://")
        .and_then(|remainder| remainder.split_once('/'))
        .map(|(host, _)| host)
        .unwrap_or_else(|| panic!("invalid preview URL: {url}"))
}

fn preview_path(url: &str) -> &str {
    url.find("/preview/")
        .map(|index| &url[index..])
        .unwrap_or_else(|| panic!("invalid preview URL: {url}"))
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
