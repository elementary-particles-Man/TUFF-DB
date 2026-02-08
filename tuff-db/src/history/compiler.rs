use crate::db::{OpKind, OpLog};
use crate::models::{Abstract, ManualOverride, Transition, VerificationStatus};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct LatestFacts {
    pub last_updated: String,
    pub facts: Vec<LatestFact>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestFact {
    pub topic_id: String,
    pub subject: String,
    pub current_value: String,
    pub status: String,
    pub confidence: f32,
    pub confidence_kind: String,
    pub agent_origin: String,
    pub source_op_id: String,
    pub last_event_ts: String,
    pub is_human_overridden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Timeline {
    pub topic_id: String,
    pub events: Vec<TimelineEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub op_id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub agent_origin: String,
    pub status_after: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
}

#[derive(Debug, Clone)]
struct RawEvent {
    topic_id: String,
    timestamp: DateTime<Utc>,
    priority: u8,
    op_id_raw: String,
    event: TimelineEvent,
}

#[derive(Debug, Clone)]
struct LatestState {
    status: String,
    current_value: String,
    confidence: f32,
    confidence_kind: String,
    agent_origin: String,
    source_op_id: String,
    last_event_ts: String,
    is_human_overridden: bool,
    subject: String,
}

pub fn compile(wal_path: impl AsRef<Path>, out_dir: impl AsRef<Path>) -> anyhow::Result<()> {
    let wal_path = wal_path.as_ref();
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;

    let file = File::open(wal_path)?;
    let reader = BufReader::new(file);

    let mut events_by_topic: HashMap<String, Vec<RawEvent>> = HashMap::new();
    let mut abstract_topic: HashMap<Uuid, String> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let op: OpLog = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match op.kind {
            OpKind::InsertAbstract { abstract_ } => {
                let topic_id = topic_id_from_abstract(&abstract_);
                abstract_topic.insert(abstract_.id.0, topic_id.clone());
                let (event, raw) = event_from_abstract(op.op_id, op.created_at, abstract_);
                events_by_topic.entry(topic_id).or_default().push(raw);
            }
            OpKind::InsertTransition { transition } => {
                let topic_id = topic_id_from_transition(&transition);
                let raw = event_from_transition(op.op_id, op.created_at, transition, topic_id.clone());
                events_by_topic.entry(topic_id).or_default().push(raw);
            }
            OpKind::AppendOverride { override_: override_ } => {
                let topic_id = override_
                    .abstract_id
                    .as_ref()
                    .and_then(|id| abstract_topic.get(&id.0).cloned())
                    .unwrap_or_else(|| "override:unmapped".to_string());
                let raw = event_from_override(op.op_id, op.created_at, override_, topic_id.clone());
                events_by_topic.entry(topic_id).or_default().push(raw);
            }
        }
    }

    let mut timelines: Vec<Timeline> = Vec::new();
    let mut latest: Vec<LatestFact> = Vec::new();
    let now = Utc::now().to_rfc3339();

    for (topic_id, mut raws) in events_by_topic {
        raws.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then(a.priority.cmp(&b.priority))
                .then(a.op_id_raw.cmp(&b.op_id_raw))
        });

        let mut latest_state: Option<LatestState> = None;
        let mut events: Vec<TimelineEvent> = Vec::new();
        for raw in raws {
            let event = raw.event.clone();
            latest_state = Some(state_from_event(&event));
            events.push(event);
        }

        timelines.push(Timeline { topic_id: topic_id.clone(), events });
        if let Some(state) = latest_state {
            latest.push(LatestFact {
                topic_id,
                subject: state.subject,
                current_value: state.current_value,
                status: state.status,
                confidence: state.confidence,
                confidence_kind: state.confidence_kind,
                agent_origin: state.agent_origin,
                source_op_id: state.source_op_id,
                last_event_ts: state.last_event_ts,
                is_human_overridden: state.is_human_overridden,
            });
        }
    }

    let latest_facts = LatestFacts {
        last_updated: now,
        facts: latest,
    };

    write_json(out_dir.join("latest_facts.json"), &latest_facts)?;
    write_json(out_dir.join("timeline.json"), &timelines)?;
    Ok(())
}

fn topic_id_from_abstract(abstract_: &Abstract) -> String {
    let key = abstract_.tags.to_key();
    if key.is_empty() {
        format!("topic:{}", short_id(abstract_.topic_id.0))
    } else {
        format!("tag:{}", key)
    }
}

fn topic_id_from_transition(transition: &Transition) -> String {
    let base = format!("{}|{}", transition.from_state, transition.to_state);
    format!("transition:{}", short_hash(&base))
}

fn event_from_abstract(op_id: Uuid, ts: DateTime<Utc>, abstract_: Abstract) -> (TimelineEvent, RawEvent) {
    let op_id_fmt = op_id_fmt(op_id);
    let status = status_mapping(abstract_.verification);
    let event = TimelineEvent {
        op_id: op_id_fmt.clone(),
        timestamp: ts.to_rfc3339(),
        event_type: "INGEST".to_string(),
        agent_origin: "UNKNOWN".to_string(),
        status_after: status,
        evidence_ids: Vec::new(),
        reason: Some(abstract_.summary.clone()),
        override_id: None,
        user_note: None,
    };
    let raw = RawEvent {
        topic_id: topic_id_from_abstract(&abstract_),
        timestamp: ts,
        priority: 1,
        op_id_raw: op_id.simple().to_string(),
        event,
    };
    (raw.event.clone(), raw)
}

fn event_from_transition(
    op_id: Uuid,
    ts: DateTime<Utc>,
    transition: Transition,
    topic_id: String,
) -> RawEvent {
    let event = TimelineEvent {
        op_id: op_id_fmt(op_id),
        timestamp: ts.to_rfc3339(),
        event_type: "TRANSITION".to_string(),
        agent_origin: transition.agent.origin,
        status_after: "SMOKE".to_string(),
        evidence_ids: transition
            .evidence_ids
            .iter()
            .map(|id| format!("evd_{}", short_id(id.0)))
            .collect(),
        reason: Some(transition.event),
        override_id: None,
        user_note: None,
    };
    RawEvent {
        topic_id,
        timestamp: ts,
        priority: 2,
        op_id_raw: op_id.simple().to_string(),
        event,
    }
}

fn event_from_override(
    op_id: Uuid,
    ts: DateTime<Utc>,
    override_: ManualOverride,
    topic_id: String,
) -> RawEvent {
    let event = TimelineEvent {
        op_id: op_id_fmt(op_id),
        timestamp: ts.to_rfc3339(),
        event_type: "OVERRIDE".to_string(),
        agent_origin: override_.agent.origin,
        status_after: "OVERRIDDEN".to_string(),
        evidence_ids: Vec::new(),
        reason: None,
        override_id: Some(format!("ovr_{}", short_id(override_.override_id.0))),
        user_note: override_.note,
    };
    RawEvent {
        topic_id,
        timestamp: ts,
        priority: 3,
        op_id_raw: op_id.simple().to_string(),
        event,
    }
}

fn state_from_event(event: &TimelineEvent) -> LatestState {
    let is_override = event.event_type == "OVERRIDE";
    LatestState {
        status: event.status_after.clone(),
        current_value: event
            .reason
            .clone()
            .unwrap_or_else(|| "(unknown)".to_string()),
        confidence: 0.0,
        confidence_kind: "UNKNOWN".to_string(),
        agent_origin: event.agent_origin.clone(),
        source_op_id: event.op_id.clone(),
        last_event_ts: event.timestamp.clone(),
        is_human_overridden: is_override,
        subject: event.event_type.clone(),
    }
}

fn status_mapping(status: VerificationStatus) -> String {
    match status {
        VerificationStatus::White => "VERIFIED".to_string(),
        VerificationStatus::Smoke => "SMOKE".to_string(),
        VerificationStatus::GrayBlack | VerificationStatus::GrayMid | VerificationStatus::GrayWhite => {
            "GRAY_*".to_string()
        }
    }
}

fn op_id_fmt(id: Uuid) -> String {
    format!("op_{}", short_id(id))
}

fn short_id(id: Uuid) -> String {
    let s = id.simple().to_string();
    s.chars().take(8).collect()
}

fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex.chars().take(8).collect()
}

fn write_json(path: PathBuf, value: &impl Serialize) -> anyhow::Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}
