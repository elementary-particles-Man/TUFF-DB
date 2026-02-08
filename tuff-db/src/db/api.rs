use crate::models::{Abstract, Transition, VerificationStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OpKind {
    InsertAbstract { abstract_: Abstract },
    InsertTransition { transition: Transition },
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

pub trait TuffDb: Send + Sync {
    fn append_abstract(&self, abstract_: Abstract) -> anyhow::Result<OpLog>;
    fn append_transition(&self, transition: Transition) -> anyhow::Result<OpLog>;
    fn select(&self, query: SelectQuery) -> anyhow::Result<Vec<Abstract>>;
}
