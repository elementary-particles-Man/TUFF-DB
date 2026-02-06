use async_trait::async_trait;
use dotenv::dotenv;
use std::env;
use std::fs;
use std::path::PathBuf;
use transformer_neo::db::TuffEngine;
use transformer_neo::models::VerificationStatus;
use transformer_neo::pipeline::{
    ClaimVerifier, DummyAbstractGenerator, DummySplitter, DummyVerifier, IngestPipeline, LlmVerifier,
    WebFetcher,
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
    ) -> anyhow::Result<VerificationStatus> {
        match self {
            Verifier::Dummy(v) => v.verify(fragment, facts).await,
            Verifier::Llm(v) => v.verify(fragment, facts).await,
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
    )?;

    let verifier = match env::var("OPENAI_API_KEY") {
        Ok(key) if valid_api_key(&key) => {
            let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string());
            Verifier::Llm(LlmVerifier::new(&key, &model))
        }
        _ => Verifier::Dummy(DummyVerifier),
    };

    let pipeline = IngestPipeline {
        splitter: DummySplitter,
        fetcher: WebFetcher::new(),
        verifier,
        generator: DummyAbstractGenerator,
        db: engine,
    };

    let ops = pipeline.ingest("高市早苗は首相である").await?;
    if let Some(op) = ops.first() {
        println!("op_id={}", op.op_id);
    }

    let all = pipeline.select_all()?;
    println!("stored={}", all.len());
    Ok(())
}
