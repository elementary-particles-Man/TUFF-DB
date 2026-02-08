use crate::models::claim::Claim;
use crate::models::ids::{AbstractId, TagGroupId, TopicId};
use crate::models::verify::VerificationStatus;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagBits {
    pub tags: Vec<String>,
}

impl TagBits {
    pub fn canonical(&self) -> TagBits {
        let mut tags: Vec<String> = self
            .tags
            .iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        tags.sort();
        tags.dedup();
        TagBits { tags }
    }

    pub fn to_key(&self) -> String {
        self.canonical().tags.join("|")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Abstract {
    pub id: AbstractId,
    pub topic_id: TopicId,
    pub tag_group_id: TagGroupId,
    pub tags: TagBits,
    pub claims: Vec<Claim>,
    pub summary: String,
    pub verification: VerificationStatus,
    pub created_at: DateTime<Utc>,
}

impl Abstract {
    pub fn new(topic_id: TopicId, tag_group_id: TagGroupId, tags: TagBits) -> Self {
        Self {
            id: AbstractId::new(),
            topic_id,
            tag_group_id,
            tags,
            claims: Vec::new(),
            summary: String::new(),
            verification: VerificationStatus::GrayMid,
            created_at: Utc::now(),
        }
    }
}
