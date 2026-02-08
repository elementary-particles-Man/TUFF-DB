use crate::models::{RequiredFact, VerificationStatus};
use crate::pipeline::traits::ClaimVerifier;
use anyhow::{Context, Result};
use async_trait::async_trait;
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_openai::{config::OpenAIConfig, Client};
use serde::Deserialize;

pub struct LlmVerifier {
    client: Client<OpenAIConfig>,
    model: String,
}

impl LlmVerifier {
    pub fn new(api_key: &str, model: &str) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);
        Self {
            client,
            model: model.to_string(),
        }
    }

    fn parse_status(raw: &str) -> VerificationStatus {
        match raw.trim().to_uppercase().as_str() {
            "SMOKE" => VerificationStatus::Smoke,
            "GRAY_BLACK" => VerificationStatus::GrayBlack,
            "GRAY_MID" => VerificationStatus::GrayMid,
            "GRAY_WHITE" => VerificationStatus::GrayWhite,
            "WHITE" => VerificationStatus::White,
            _ => VerificationStatus::GrayMid,
        }
    }
}

#[derive(Deserialize)]
struct LlmResponse {
    status: String,
    reasoning: String,
}

#[async_trait]
impl ClaimVerifier for LlmVerifier {
    async fn verify(&self, fragment: &str, facts: &[RequiredFact]) -> Result<VerificationStatus> {
        if facts.is_empty() {
            return Ok(VerificationStatus::GrayMid);
        }

        let mut evidence_blocks = Vec::new();
        for fact in facts {
            for evidence in &fact.evidence {
                let snippet: String = evidence.snippet.chars().take(300).collect();
                evidence_blocks.push(format!(
                    "[URL: {}] [SHA256: {}]\n{}",
                    evidence.source.url, evidence.source.sha256_hex, snippet
                ));
            }
        }

        let evidence_text = if evidence_blocks.is_empty() {
            "(no evidence snippets)".to_string()
        } else {
            evidence_blocks.join("\n\n")
        };

        let system_prompt = "You are a strict verification engine. Compare CLAIM to EVIDENCE only. \
Output JSON with keys: status, reasoning. status must be one of SMOKE, GRAY_BLACK, GRAY_MID, GRAY_WHITE, WHITE. \
SMOKE if evidence contradicts claim. WHITE if evidence supports claim. Use GRAY_* if insufficient.";

        let user_prompt = format!("CLAIM:\n{}\n\nEVIDENCE:\n{}", fragment, evidence_text);

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
            .context("LLM response missing content")?;

        let parsed: LlmResponse = serde_json::from_str(&content).unwrap_or(LlmResponse {
            status: "GRAY_MID".to_string(),
            reasoning: format!("Parse error: {}", content),
        });

        let _reasoning = parsed.reasoning;
        let status = Self::parse_status(&parsed.status);
        Ok(status)
    }
}
