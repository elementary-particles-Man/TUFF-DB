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
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};
use futures_util::SinkExt;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::signal::unix::{signal as unix_signal, SignalKind};
use tokio::time::{timeout, Duration};
use transformer_neo::db::{OpKind, TuffDb, TuffEngine};
use transformer_neo::lightweight::{LightweightVerifier, MeaningDb, MeaningMatchMode};
use transformer_neo::models::{AgentIdentity, Id, IsoDateTime, ManualOverride, VerificationStatus};
use transformer_neo::pipeline::{
    AbstractGenerator, ClaimVerifier, DummyAbstractGenerator, DummySplitter, DummyVerifier,
    IngestPipeline, LlmAbstractor, LlmGapResolver, LlmVerifier,
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
    lightweight_verifier: Option<Arc<LightweightVerifier>>,
    gap_resolver: Option<Arc<LlmGapResolver>>,
    stop_threshold: f32,
    history_dir: PathBuf,
    history_html: Arc<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();
    let _identity = AgentIdentity::current();
    log_line("TUFF-BRG boot: main() start");

    let wal_dir = PathBuf::from("_tuffdb");
    fs::create_dir_all(&wal_dir)?;
    let wal_path = wal_dir.join("tuff.wal");

    let engine = TuffEngine::new(
        wal_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("invalid wal path"))?,
    ).await?;

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

    let lightweight_verifier = init_lightweight_verifier(&wal_dir);

    let stop_threshold = env::var("TUFF_STOP_CONFIDENCE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.35);

    let history_dir = PathBuf::from(env::var("TUFF_HISTORY_OUT").unwrap_or_else(|_| "history_out".to_string()));
    let history_html = include_str!("../assets/history_viewer.html").to_string();

    let state = AppState {
        pipeline: Arc::new(pipeline),
        lightweight_verifier,
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
    log_line(&format!("TUFF-BRG listening on {}", addr));
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn parse_meaning_env_pairs() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Ok(raw) = env::var("TUFF_MEANING_DB") {
        for item in raw.split(';') {
            let mut parts = item.splitn(2, '=');
            if let (Some(tag), Some(meaning)) = (parts.next(), parts.next()) {
                if !tag.trim().is_empty() && !meaning.trim().is_empty() {
                    map.insert(tag.trim().to_string(), meaning.trim().to_string());
                }
            }
        }
    }
    map
}

fn init_lightweight_verifier(wal_dir: &PathBuf) -> Option<Arc<LightweightVerifier>> {
    let enabled = env::var("TUFF_FAST_PATH")
        .map(|v| v.trim() != "0")
        .unwrap_or(true);
    if !enabled {
        return None;
    }

    let default_path = wal_dir.join("lightweight").join("meaning.db");
    let path = env::var("TUFF_LIGHTWEIGHT_MEANING_PATH")
        .ok()
        .map(PathBuf::from)
        .unwrap_or(default_path);

    let mut merged = if path.exists() {
        MeaningDb::from_path(&path).unwrap_or_else(|_| MeaningDb::new(std::collections::HashMap::new()))
    } else {
        MeaningDb::new(std::collections::HashMap::new())
    };
    merged.merge(parse_meaning_env_pairs());
    let verifier = LightweightVerifier::new(merged);
    Some(Arc::new(verifier))
}

async fn shutdown_signal() {
    let mut sigint = unix_signal(SignalKind::interrupt()).ok();
    let mut sigterm = unix_signal(SignalKind::terminate()).ok();

    tokio::select! {
        _ = async {
            if let Err(err) = signal::ctrl_c().await {
                eprintln!("Failed to listen for shutdown signal: {}", err);
            }
        } => {
            log_line("SIGINT received. Shutting down...");
        }
        _ = async {
            if let Some(sig) = sigint.as_mut() { sig.recv().await; }
        } => {
            log_line("SIGINT (unix) received. Shutting down...");
        }
        _ = async {
            if let Some(sig) = sigterm.as_mut() { sig.recv().await; }
        } => {
            log_line("SIGTERM received. Shutting down...");
        }
    }

    // force exit if runtime is wedged
    std::process::exit(0);
}

fn log_line(msg: &str) {
    println!("{}", msg);
    let _ = io::stdout().flush();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    log_line("WS: client connected");
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::channel::<WsMessage>(256);
    let (frag_tx, mut frag_rx) = watch::channel(String::new());

    // outbound pump
    let tx_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    // ingest worker (decouple from WS receive loop)
    let state_for_worker = state.clone();
    let tx_for_worker = tx.clone();
    let ingest_task = tokio::spawn(async move {
        while frag_rx.changed().await.is_ok() {
            let fragment = frag_rx.borrow().clone();
            if fragment.is_empty() {
                continue;
            }
            log_line("INGEST: start");

            if let Some(lightweight) = state_for_worker.lightweight_verifier.as_ref() {
                if let Some(hit) = lightweight.verify_fragment(&fragment) {
                    let mode = match hit.mode {
                        MeaningMatchMode::Exact => "exact",
                        MeaningMatchMode::Contains => "contains",
                    };
                    let judge = Message::JudgeResult {
                        id: Id::new().to_string(),
                        ts: Utc::now().to_rfc3339(),
                        payload: JudgeResultPayload {
                            status: ProtoStatus::White,
                            reason: format!("source=Cache tag={} mode={}", hit.tag, mode),
                            confidence: 1.0,
                            claim: fragment.clone(),
                            evidence_count: 0,
                            abstract_id: None,
                        },
                    };
                    let _ = tx_for_worker
                        .send(WsMessage::Text(serde_json::to_string(&judge).unwrap_or_default()))
                        .await;
                    log_line("INGEST: cache hit");
                    continue;
                }
            }

            let ingest_result = timeout(
                Duration::from_secs(3),
                state_for_worker.pipeline.ingest(&fragment),
            )
            .await;

            let ops = match ingest_result {
                Ok(Ok(v)) => v,
                Ok(Err(_)) => {
                    log_line("INGEST: error");
                    continue;
                }
                Err(_) => {
                    log_line("INGEST: timeout");
                    continue;
                }
            };
            log_line("INGEST: end");

            let mut status = VerificationStatus::GrayMid;
            let mut confidence = 0.4_f32;
            let mut evidence_count = 0usize;
            let mut reason = "ok".to_string();
            let mut abstract_id: Option<String> = None;
            if let Some(outcome) = ops.first() {
                status = outcome.status;
                confidence = outcome.confidence;
                evidence_count = outcome.evidence_count;
                reason = format!("source=LLM {}", outcome.reason);
                if let OpKind::InsertAbstract { abstract_ } = &outcome.op.kind {
                    abstract_id = Some(abstract_.id.to_string());
                }
            } else {
                reason = "source=LLM ok".to_string();
            }

            let judge = Message::JudgeResult {
                id: Id::new().to_string(),
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
            let _ = tx_for_worker
                .send(WsMessage::Text(serde_json::to_string(&judge).unwrap_or_default()))
                .await;
            log_line("WS: JudgeResult sent");
        }
    });

    // inbound loop
    while let Some(msg) = ws_rx.next().await {
        let Ok(msg) = msg else { break };
        let text = match msg {
            WsMessage::Text(t) => t,
            _ => continue,
        };
        log_line("WS: received text frame");
        let parsed: Message = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                log_line("WS: JSON parse error");
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
                let _ = tx
                    .send(WsMessage::Text(serde_json::to_string(&stop).unwrap_or_default()))
                    .await;
                continue;
            }
        };

        // handle control command inline
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
                let _ = state.pipeline.db.append_override(override_).await;
            }
            continue;
        }

        if let Message::StreamFragment { payload, .. } = parsed {
            log_line("WS: StreamFragment received");
            let StreamFragmentPayload { fragment, .. } = payload;
            let _ = frag_tx.send(fragment);
        }
    }

    tx_task.abort();
    ingest_task.abort();
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
        Err(_) => (StatusCode::NOT_FOUND, format!("missing {}", file)).into_response(),
    }
}
