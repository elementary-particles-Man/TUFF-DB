use crate::models::{Abstract, TagBits, TagGroupId, TopicId, VerificationStatus};
use crate::pipeline::traits::AbstractGenerator;
use anyhow::{Context, Result};
use async_trait::async_trait;
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs,
};
use async_openai::{config::OpenAIConfig, Client};
use serde::Deserialize;

pub struct LlmAbstractor {
    client: Client<OpenAIConfig>,
    model: String,
}

impl LlmAbstractor {
    pub fn new(api_key: &str, model: &str) -> Self {
        let config = OpenAIConfig::new().with_api_key(api_key);
        let client = Client::with_config(config);
        Self {
            client,
            model: model.to_string(),
        }
    }

    fn normalize_tags(tags: Vec<String>) -> TagBits {
        let mut cleaned: Vec<String> = tags
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        cleaned.sort();
        cleaned.dedup();
        TagBits { tags: cleaned }
    }
}

#[derive(Deserialize)]
struct LlmAbstractResponse {
    summary: String,
    tags: Vec<String>,
}

#[async_trait]
impl AbstractGenerator for LlmAbstractor {
    async fn generate(
        &self,
        fragment: &str,
        facts: &[crate::models::RequiredFact],
        status: VerificationStatus,
    ) -> Result<Abstract> {
        let mut evidence_blocks = Vec::new();
        for fact in facts {
            for evidence in &fact.evidence {
                let snippet: String = evidence.snippet.chars().take(400).collect();
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

        let system_prompt = "You are a strict abstractor for a fact-checking database. \
Given CLAIM, EVIDENCE, and STATUS, output JSON with keys: summary, tags. \
summary must be brief and neutral. tags must be 3-8 short tags.";

        let user_prompt = format!(
            "CLAIM:\n{}\n\nSTATUS:\n{:?}\n\nEVIDENCE:\n{}",
            fragment, status, evidence_text
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
            .context("LLM response missing content")?;

        let parsed: LlmAbstractResponse = serde_json::from_str(&content).unwrap_or_else(|_| {
            LlmAbstractResponse {
                summary: format!("LLM parse error. Raw: {}", content.chars().take(80).collect::<String>()),
                tags: vec!["UNKNOWN".to_string()],
            }
        });

        let tags = Self::normalize_tags(parsed.tags);

        let mut abstract_ = Abstract::new(TopicId::new(), TagGroupId::new(), tags);
        abstract_.summary = parsed.summary;
        abstract_.verification = status;
        Ok(abstract_)
    }
}
