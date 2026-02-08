use crate::models::Id;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceMeta {
    pub url: Url,
    pub retrieved_at_rfc3339: String,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_id: Id,
    pub source: SourceMeta,
    pub snippet: String,
}
