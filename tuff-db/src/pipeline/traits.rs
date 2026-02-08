use crate::models::{Abstract, Claim, Evidence, RequiredFact, Transition, VerificationStatus};
use async_trait::async_trait;

#[derive(Clone, Debug)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    pub confidence: f32,
    pub reason: String,
}

pub trait InputSplitter: Send + Sync {
    fn split(&self, input: &str) -> Vec<String>;
}

#[async_trait]
pub trait FactFetcher: Send + Sync {
    async fn fetch(&self, fragment: &str) -> anyhow::Result<Vec<RequiredFact>>;
}

#[async_trait]
pub trait ClaimVerifier: Send + Sync {
    async fn verify(
        &self,
        fragment: &str,
        facts: &[RequiredFact],
    ) -> anyhow::Result<VerificationResult>;
}

#[async_trait]
pub trait AbstractGenerator: Send + Sync {
    async fn generate(
        &self,
        fragment: &str,
        facts: &[RequiredFact],
        status: VerificationStatus,
    ) -> anyhow::Result<Abstract>;
}

#[async_trait]
pub trait GapResolver: Send + Sync {
    /// Resolve the gap between internal knowledge and external evidence.
    /// Returns a signed Transition if an explaining event is found.
    async fn resolve(
        &self,
        claim: &Claim,
        internal_state: &str,
        external_evidence: &[Evidence],
    ) -> anyhow::Result<Option<Transition>>;
}
