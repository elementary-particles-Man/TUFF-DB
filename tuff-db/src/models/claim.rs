use crate::models::evidence::Evidence;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceRef {
    pub url: Url,
    pub retrieved_at_rfc3339: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequiredFact {
    pub key: String,
    pub value: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Claim {
    pub statement: String,
    pub sources: Vec<SourceRef>,
}
