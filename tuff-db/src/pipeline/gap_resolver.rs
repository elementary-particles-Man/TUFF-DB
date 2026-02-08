use async_trait::async_trait;
use anyhow::{Context, Result};
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_openai::{config::OpenAIConfig, Client};
use serde::Deserialize;

use crate::models::{AgentIdentity, Claim, Evidence, Id, IsoDateTime, Transition};
use crate::pipeline::traits::GapResolver;

pub struct LlmGapResolver {
    client: Client<OpenAIConfig>,
    model: String,
}

impl LlmGapResolver {
    pub fn new(api_key: &str, model: &str) -> Self {
        let mut config = OpenAIConfig::new().with_api_key(api_key);
        if let Ok(base_url) = std::env::var("OPENAI_API_BASE") {
            config = config.with_api_base(base_url);
        }
        Self {
            client: Client::with_config(config),
            model: model.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct LlmGapResponse {
    event_name: String,
    occurred_at: Option<String>,
    from_state: String,
    to_state: String,
}

#[async_trait]
impl GapResolver for LlmGapResolver {
    async fn resolve(
        &self,
        claim: &Claim,
        internal_state: &str,
        external_evidence: &[Evidence],
    ) -> Result<Option<Transition>> {
        if external_evidence.is_empty() {
            return Ok(None);
        }

        let evidence_text = external_evidence
            .iter()
            .map(|e| e.snippet.chars().take(200).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");

        let system_prompt = r#"You are a Historian AI.
Identify the EVENT that caused a change from the Internal State to the External Evidence.
Output JSON only: { \"event_name\": string, \"occurred_at\": string(ISO8601 or null), \"from_state\": string, \"to_state\": string }"#;

        let user_prompt = format!(
            "Internal State: {}\nExternal Evidence: {}\nClaim: {}\n\nWhat event connects these states?",
            internal_state, evidence_text, claim.statement
        );

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages([
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(system_prompt)
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(user_prompt)
                    .build()?
                    .into(),
            ])
            .build()?;

        let response = self.client.chat().create(request).await?;
        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .context("No content")?;

        let res: LlmGapResponse = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        let occurred_at = res
            .occurred_at
            .as_deref()
            .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
            .map(IsoDateTime);

        Ok(Some(Transition {
            transition_id: Id::new(),
            observed_at: IsoDateTime::now(),
            agent: AgentIdentity::current(),
            from_state: res.from_state,
            to_state: res.to_state,
            event: res.event_name,
            occurred_at,
            evidence_ids: external_evidence
                .iter()
                .map(|e| e.evidence_id.clone())
                .collect(),
        }))
    }
}
