use crate::models::{Abstract, RequiredFact, TagBits, TagGroupId, TopicId, VerificationStatus};
use crate::pipeline::traits::{
    AbstractGenerator, ClaimVerifier, FactFetcher, InputSplitter, VerificationResult,
};
use async_trait::async_trait;

pub struct DummySplitter;

impl InputSplitter for DummySplitter {
    fn split(&self, input: &str) -> Vec<String> {
        input
            .split('\n')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

pub struct DummyFetcher;

#[async_trait]
impl FactFetcher for DummyFetcher {
    async fn fetch(&self, fragment: &str) -> anyhow::Result<Vec<RequiredFact>> {
        Ok(vec![RequiredFact {
            key: "mock".to_string(),
            value: fragment.to_string(),
            evidence: Vec::new(),
        }])
    }
}

pub struct DummyVerifier;

#[async_trait]
impl ClaimVerifier for DummyVerifier {
    async fn verify(
        &self,
        _fragment: &str,
        facts: &[RequiredFact],
    ) -> anyhow::Result<VerificationResult> {
        if facts.is_empty() {
            Ok(VerificationResult {
                status: VerificationStatus::GrayMid,
                confidence: 0.4,
                reason: "no evidence".to_string(),
            })
        } else {
            Ok(VerificationResult {
                status: VerificationStatus::White,
                confidence: 0.8,
                reason: "dummy verifier".to_string(),
            })
        }
    }
}

pub struct DummyAbstractGenerator;

#[async_trait]
impl AbstractGenerator for DummyAbstractGenerator {
    async fn generate(
        &self,
        fragment: &str,
        _facts: &[RequiredFact],
        status: VerificationStatus,
    ) -> anyhow::Result<Abstract> {
        let mut abstract_ = Abstract::new(
            TopicId::new(),
            TagGroupId::new(),
            TagBits {
                tags: vec!["smoke".to_string(), "sanity".to_string()],
            },
        );
        abstract_.summary = format!("SMOKE: {}", fragment);
        abstract_.verification = status;
        Ok(abstract_)
    }
}
