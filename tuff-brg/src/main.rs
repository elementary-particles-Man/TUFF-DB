use async_trait::async_trait;
use axum::{
    extract::{State, WebSocketUpgrade},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use chrono::Utc;
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use transformer_neo::db::{OpKind, TuffEngine};
use transformer_neo::models::{AgentIdentity, Claim, Evidence, Id, IsoDateTime, ManualOverride, VerificationStatus};
use transformer_neo::pipeline::{
    AbstractGenerator, ClaimVerifier, DummyAbstractGenerator, DummySplitter, DummyVerifier,
    FactFetcher, GapResolver, IngestPipeline, LlmAbstractor, LlmGapResolver, LlmVerifier,
    WebFetcher,
};

mod api;
use api::message::{
    ControlCommand, ControlCommandPayload, ControlTrigger, JudgeResultPayload, Message,
    StreamFragmentPayload, VerificationStatus as ProtoStatus,
};

enum Verifier {
    Dummy(DummyVerifier),
    Llm(LlmVerifier),
}

#[async_trait]
impl ClaimVerifier for Verifier {
    async fn verify(
        &self,
        fragment: &str,
        facts: &[transformer_neo::models::RequiredFact],
    ) -> anyhow::Result<transformer_neo::pipeline::traits::VerificationResult> {
        match self {
            Verifier::Dummy(v) => v.verify(fragment, facts).await,
            Verifier::Llm(v) => v.verify(fragment, facts).await,
        }
    }
}

enum Abstractor {
    Dummy(DummyAbstractGenerator),
    Llm(LlmAbstractor),
}

#[async_trait]
impl AbstractGenerator for Abstractor {
    async fn generate(
        &self,
        fragment: &str,
        facts: &[transformer_neo::models::RequiredFact],
        status: VerificationStatus,
    ) -> anyhow::Result<transformer_neo::models::Abstract> {
        match self {
            Abstractor::Dummy(a) => a.generate(fragment, facts, status).await,
            Abstractor::Llm(a) => a.generate(fragment, facts, status).await,
        }
    }
}

fn valid_api_key(key: &str) -> bool {
    let trimmed = key.trim();
    !trimmed.is_empty() && !trimmed.contains("...")
}

fn to_proto_status(status: VerificationStatus) -> ProtoStatus {
    match status {
        VerificationStatus::Smoke => ProtoStatus::Smoke,
        VerificationStatus::GrayBlack => ProtoStatus::GrayBlack,
        VerificationStatus::GrayMid => ProtoStatus::GrayMid,
        VerificationStatus::GrayWhite => ProtoStatus::GrayWhite,
        VerificationStatus::White => ProtoStatus::White,
    }
}

#[derive(Clone)]
struct AppState {
    pipeline: Arc<
        IngestPipeline<
            DummySplitter,
            WebFetcher,
            Verifier,
            Abstractor,
            TuffEngine,
        >,
    >,
    gap_resolver: Option<Arc<LlmGapResolver>>,
    stop_threshold: f32,
    history_dir: PathBuf,
    history_html: Arc<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let _identity = AgentIdentity::current();

    let wal_dir = PathBuf::from("_tuffdb");
    fs::create_dir_all(&wal_dir)?;
    let wal_path = wal_dir.join("tuff.wal");

    let engine = TuffEngine::new(
        wal_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("invalid wal path"))?,
    )?;

    let api_key = env::var("OPENAI_API_KEY").ok();
    let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());

    let verifier = match api_key.as_deref() {
        Some(key) if valid_api_key(key) => Verifier::Llm(LlmVerifier::new(key, &model)),
        _ => Verifier::Dummy(DummyVerifier),
    };

    let abstractor = match api_key.as_deref() {
        Some(key) if valid_api_key(key) => Abstractor::Llm(LlmAbstractor::new(key, &model)),
        _ => Abstractor::Dummy(DummyAbstractGenerator),
    };

    let gap_resolver = match api_key.as_deref() {
        Some(key) if valid_api_key(key) => Some(Arc::new(LlmGapResolver::new(key, &model))),
        _ => None,
    };

    let pipeline = IngestPipeline {
        splitter: DummySplitter,
        fetcher: WebFetcher::new(),
        verifier,
        generator: abstractor,
        db: engine,
    };

    let stop_threshold = env::var("TUFF_STOP_CONFIDENCE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.35);

    let history_dir = PathBuf::from(env::var("TUFF_HISTORY_OUT").unwrap_or_else(|_| "history_out".to_string()));
    let history_html_path = env::var("TUFF_HISTORY_HTML")
        .unwrap_or_else(|_| "tuff-brg/assets/history_viewer.html".to_string());
    let history_html = fs::read_to_string(&history_html_path)
        .unwrap_or_else(|_| "<h1>History Viewer not found</h1>".to_string());

    let state = AppState {
        pipeline: Arc::new(pipeline),
        gap_resolver,
        stop_threshold,
        history_dir,
        history_html: Arc::new(history_html),
    };

    let app = Router::new()
        .route("/", get(ws_handler))
        .route("/history", get(history_page))
        .route("/history/api/latest", get(history_latest))
        .route("/history/api/timeline", get(history_timeline))
        .with_state(state);

    let addr: SocketAddr = "127.0.0.1:8787".parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("TUFF-BRG listening on {}", addr);
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    while let Some(msg) = socket.next().await {
        let Ok(msg) = msg else { break };
        if !msg.is_text() {
            continue;
        }
        let text = msg.to_text().unwrap_or("");
        let parsed: Message = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => {
                let stop = Message::ControlCommand {
                    id: "system".to_string(),
                    ts: Utc::now().to_rfc3339(),
                    payload: ControlCommandPayload {
                        command: ControlCommand::Stop,
                        trigger: ControlTrigger::ManualOverride,
                        detail: "JSON parse error".to_string(),
                        manual_override: None,
                    },
                };
                let _ = socket
                    .send(WsMessage::Text(serde_json::to_string(&stop).unwrap_or_default()))
                    .await;
                continue;
            }
        };

        if let Message::ControlCommand { payload, .. } = parsed {
            if payload.command == ControlCommand::Continue
                && payload.trigger == ControlTrigger::ManualOverride
            {
                let meta = payload.manual_override;
                let note = meta
                    .as_ref()
                    .and_then(|m| m.note.clone())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "No reason provided".to_string());
                let conversation_id = meta.as_ref().and_then(|m| m.conversation_id.clone());
                let abstract_id = meta
                    .as_ref()
                    .and_then(|m| m.abstract_id.as_ref())
                    .and_then(|s| s.parse::<Id>().ok());
                let override_ = ManualOverride {
                    override_id: Id::new(),
                    observed_at: IsoDateTime::now(),
                    agent: AgentIdentity::current(),
                    conversation_id,
                    abstract_id,
                    note: Some(note),
                };
                let _ = state.pipeline.db.append_override(override_);
            }
            continue;
        }

        if let Message::StreamFragment { id, payload, .. } = parsed {
            let StreamFragmentPayload { fragment, .. } = payload;

            let ops = match state.pipeline.ingest(&fragment).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let mut status = VerificationStatus::GrayMid;
            let mut confidence = 0.4_f32;
            let mut evidence_count = 0usize;
            let mut reason = "ok".to_string();
            let mut abstract_id: Option<String> = None;
            if let Some(outcome) = ops.first() {
                status = outcome.status;
                confidence = outcome.confidence;
                evidence_count = outcome.evidence_count;
                reason = outcome.reason.clone();
                if let OpKind::InsertAbstract { abstract_ } = &outcome.op.kind {
                    abstract_id = Some(abstract_.id.to_string());
                }
            }

            let judge = Message::JudgeResult {
                id,
                ts: Utc::now().to_rfc3339(),
                payload: JudgeResultPayload {
                    status: to_proto_status(status),
                    reason,
                    confidence,
                    claim: fragment.clone(),
                    evidence_count: evidence_count as u32,
                    abstract_id,
                },
            };
            let _ = socket
                .send(WsMessage::Text(serde_json::to_string(&judge).unwrap_or_default()))
                .await;

            if status == VerificationStatus::Smoke {
                let stop = Message::ControlCommand {
                    id: "system".to_string(),
                    ts: Utc::now().to_rfc3339(),
                    payload: ControlCommandPayload {
                        command: ControlCommand::Stop,
                        trigger: ControlTrigger::SmokeDetected,
                        detail: "Smoke detected".to_string(),
                        manual_override: None,
                    },
                };
                let _ = socket
                    .send(WsMessage::Text(serde_json::to_string(&stop).unwrap_or_default()))
                    .await;
            } else if confidence < state.stop_threshold {
                let stop = Message::ControlCommand {
                    id: "system".to_string(),
                    ts: Utc::now().to_rfc3339(),
                    payload: ControlCommandPayload {
                        command: ControlCommand::Stop,
                        trigger: ControlTrigger::LowConfidence,
                        detail: "Low confidence".to_string(),
                        manual_override: None,
                    },
                };
                let _ = socket
                    .send(WsMessage::Text(serde_json::to_string(&stop).unwrap_or_default()))
                    .await;
            }

            if let Some(resolver) = &state.gap_resolver {
                let internal_state = "Current Prime Minister is Shigeru Ishiba";
                if let Ok(facts) = state.pipeline.fetcher.fetch(&fragment).await {
                    let evidences: Vec<Evidence> =
                        facts.iter().flat_map(|f| f.evidence.clone()).collect();

                    let claim = Claim {
                        statement: fragment.to_string(),
                        sources: Vec::new(),
                    };

                    if let Ok(Some(transition)) = resolver
                        .resolve(&claim, internal_state, &evidences)
                        .await
                    {
                        let _ = state.pipeline.db.append_transition(transition);
                    }
                }
            }
        }
    }
}

async fn history_page(State(state): State<AppState>) -> Html<String> {
    Html(state.history_html.as_ref().to_string())
}

async fn history_latest(State(state): State<AppState>) -> Response {
    serve_history_json(&state, "latest_facts.json")
}

async fn history_timeline(State(state): State<AppState>) -> Response {
    serve_history_json(&state, "timeline.json")
}

fn serve_history_json(state: &AppState, file: &str) -> Response {
    let path = state.history_dir.join(file);
    match fs::read_to_string(&path) {
        Ok(body) => (StatusCode::OK, body).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, format!(\"missing {}\", file)).into_response(),
    }
}
