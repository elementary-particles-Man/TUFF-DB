use async_trait::async_trait;
use chrono::Utc;
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
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
        Some(key) if valid_api_key(key) => Some(LlmGapResolver::new(key, &model)),
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

    let addr: SocketAddr = "127.0.0.1:8787".parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("TUFF-BRG listening on {}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let mut ws = accept_async(stream).await?;

        while let Some(msg) = ws.next().await {
            let msg = msg?;
            if !msg.is_text() {
                continue;
            }

            let text = msg.to_text()?;
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
                    ws.send(tokio_tungstenite::tungstenite::Message::Text(
                        serde_json::to_string(&stop)?,
                    ))
                    .await?;
                    continue;
                }
            };

            if let Message::ControlCommand { id: _, ts: _, payload } = parsed {
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
                    let _ = pipeline.db.append_override(override_)?;
                }
                continue;
            }

            if let Message::StreamFragment { id, ts: _, payload } = parsed {
                let StreamFragmentPayload {
                    fragment,
                    conversation_id: _,
                    sequence_number: _,
                    ..
                } = payload;

                let ops = pipeline.ingest(&fragment).await?;
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
                ws.send(tokio_tungstenite::tungstenite::Message::Text(
                    serde_json::to_string(&judge)?,
                ))
                .await?;

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
                    ws.send(tokio_tungstenite::tungstenite::Message::Text(
                        serde_json::to_string(&stop)?,
                    ))
                    .await?;
                } else if confidence < stop_threshold {
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
                    ws.send(tokio_tungstenite::tungstenite::Message::Text(
                        serde_json::to_string(&stop)?,
                    ))
                    .await?;
                }

                if let Some(resolver) = &gap_resolver {
                    let internal_state = "Current Prime Minister is Shigeru Ishiba";
                    let facts = pipeline.fetcher.fetch(&fragment).await?;
                    let evidences: Vec<Evidence> =
                        facts.iter().flat_map(|f| f.evidence.clone()).collect();

                    let claim = Claim {
                        statement: fragment.to_string(),
                        sources: Vec::new(),
                    };

                    if let Some(transition) = resolver
                        .resolve(&claim, internal_state, &evidences)
                        .await?
                    {
                        let _ = pipeline.db.append_transition(transition)?;
                    }
                }
            }
        }
    }
}
