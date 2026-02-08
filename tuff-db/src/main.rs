use async_trait::async_trait;
use dotenv::dotenv;
use std::env;
use std::fs;
use std::path::PathBuf;
use transformer_neo::db::TuffEngine;
use transformer_neo::models::{Claim, VerificationStatus};
use transformer_neo::pipeline::{
    AbstractGenerator, ClaimVerifier, DummyAbstractGenerator, DummySplitter, DummyVerifier,
    FactFetcher, GapResolver, IngestPipeline, LlmAbstractor, LlmGapResolver, LlmVerifier,
    WebFetcher,
};
use transformer_neo::pipeline::traits::VerificationResult;

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
    ) -> anyhow::Result<VerificationResult> {
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
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains("...") {
        return false;
    }
    true
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

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
        Some(key) if valid_api_key(key) => Some(LlmGapResolver::new(key, &model)),
        _ => None,
    };

    let fetcher = WebFetcher::new();

    // Run pipeline
    let pipeline = IngestPipeline {
        splitter: DummySplitter,
        fetcher: WebFetcher::new(),
        verifier,
        generator: abstractor,
        db: engine,
    };

    let input = "高市早苗は首相である";
    let ops = pipeline.ingest(input).await?;
    if let Some(outcome) = ops.first() {
        println!("op_id={}", outcome.op.op_id);
    }

    let all = pipeline.select_all().await?;
    println!("stored={}", all.len());

    // Gap resolver integration (mock internal state)
    if let Some(resolver) = gap_resolver {
        let internal_state = "Current Prime Minister is Shigeru Ishiba";
        let facts = fetcher.fetch(input).await?;
        let evidences: Vec<transformer_neo::models::Evidence> =
            facts.iter().flat_map(|f| f.evidence.clone()).collect();

        let claim = Claim {
            statement: input.to_string(),
            sources: Vec::new(),
        };

        if let Some(transition) = resolver
            .resolve(&claim, internal_state, &evidences)
            .await?
        {
            let json = serde_json::to_string(&transition)?;
            println!("[TRANSITION RECORD GENERATED] {}", json);
        }
    }

    Ok(())
}
