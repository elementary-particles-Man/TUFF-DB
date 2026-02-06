use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    // コンパイル時に Origin を決定する
    // (TUFF-DBのビルドごとに、どのAIとして振る舞うかを固定)
    const ORIGIN_CONST: &'static str = "GPT-5"; // ★ここを書き換えない限り不変

    pub fn current() -> Self {
        Self {
            origin: Self::ORIGIN_CONST.to_string(),
            role: std::env::var("AGENT_ROLE").ok(),
            build: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}
