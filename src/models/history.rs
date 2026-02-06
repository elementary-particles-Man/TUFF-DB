use crate::models::{AgentIdentity, Id, IsoDateTime};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Transition {
    pub transition_id: Id,
    pub observed_at: IsoDateTime,

    // 誰がこの遷移を認定したか (Origin固定)
    pub agent: AgentIdentity,

    // 遷移元と遷移先のState (例: "PM: Ishiba" -> "PM: Takaichi")
    pub from_state: String,
    pub to_state: String,

    // 遷移を引き起こしたイベント (例: "General Election 2025")
    pub event: String,

    // イベント発生時期 (Gapを埋める期間)
    pub occurred_at: Option<IsoDateTime>,

    // 根拠となるEvidence ID
    pub evidence_ids: Vec<Id>,
}
