use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Agent Identity: Origin is constant, Role is variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentIdentity {
    /// Origin: コードレベルで固定された不可侵の識別子
    pub origin: String,

    /// Role: ユーザ要請等による一時的な役割
    #[serde(default)]
    pub role: Option<String>,

    /// Build Info
    pub build: String,
}

impl AgentIdentity {
    pub fn current() -> Self {
        static ORIGIN: OnceLock<String> = OnceLock::new();
        let origin = ORIGIN
            .get_or_init(|| std::env::var("AI_ORIGIN").unwrap_or_else(|_| "Gemini".to_string()))
            .clone();
        Self {
            origin,
            role: std::env::var("AGENT_ROLE").ok(),
            build: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
