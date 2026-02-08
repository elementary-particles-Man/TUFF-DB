use crate::models::{Abstract, ManualOverride, Transition, VerificationStatus};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OpKind {
    InsertAbstract { abstract_: Abstract },
    InsertTransition { transition: Transition },
    AppendOverride { override_: ManualOverride },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpLog {
    pub op_id: Uuid,
    pub kind: OpKind,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub struct SelectQuery {
    pub tag_key: Option<String>,
    pub min_verification: Option<VerificationStatus>,
}

#[async_trait]
pub trait TuffDb: Send + Sync {
    async fn append_abstract(&self, abstract_: Abstract) -> anyhow::Result<OpLog>;
    async fn append_transition(&self, transition: Transition) -> anyhow::Result<OpLog>;
    async fn append_override(&self, override_: ManualOverride) -> anyhow::Result<OpLog>;
    async fn select(&self, query: SelectQuery) -> anyhow::Result<Vec<Abstract>>;
}
