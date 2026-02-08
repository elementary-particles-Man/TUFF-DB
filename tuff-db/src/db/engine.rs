use crate::db::api::{OpKind, OpLog, SelectQuery, TuffDb};
use crate::db::index::InMemoryIndex;
use crate::models::{Abstract, AgentIdentity, ManualOverride, Transition};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::Mutex as StdMutex;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

pub struct TuffEngine {
    index: StdMutex<InMemoryIndex>,
    wal: TokioMutex<BufWriter<File>>,
}

impl TuffEngine {
    pub async fn new(wal_path: &str) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(wal_path)
            .await?;
        Ok(Self {
            index: StdMutex::new(InMemoryIndex::default()),
            wal: TokioMutex::new(BufWriter::new(file)),
        })
    }

    async fn write_wal(&self, op: &OpLog) -> anyhow::Result<()> {
        let mut guard = self.wal.lock().await;
        let line = serde_json::to_string(op)?;
        guard.write_all(line.as_bytes()).await?;
        guard.write_all(b"\n").await?;
        guard.flush().await?;
        Ok(())
    }
}

#[async_trait]
impl TuffDb for TuffEngine {
    async fn append_abstract(&self, abstract_: Abstract) -> anyhow::Result<OpLog> {
        let op = OpLog {
            op_id: Uuid::new_v4(),
            kind: OpKind::InsertAbstract { abstract_ },
            created_at: Utc::now(),
        };
        self.write_wal(&op).await?;

        if let OpKind::InsertAbstract { abstract_ } = op.kind.clone() {
            let mut index = self.index.lock().expect("index lock");
            index.insert(abstract_);
        }

        Ok(op)
    }

    async fn append_transition(&self, mut transition: Transition) -> anyhow::Result<OpLog> {
        transition.agent = AgentIdentity::current();
        let op = OpLog {
            op_id: Uuid::new_v4(),
            kind: OpKind::InsertTransition { transition },
            created_at: Utc::now(),
        };
        self.write_wal(&op).await?;
        Ok(op)
    }

    async fn append_override(&self, mut override_: ManualOverride) -> anyhow::Result<OpLog> {
        override_.agent = AgentIdentity::current();
        let op = OpLog {
            op_id: Uuid::new_v4(),
            kind: OpKind::AppendOverride { override_ },
            created_at: Utc::now(),
        };
        self.write_wal(&op).await?;
        Ok(op)
    }

    async fn select(&self, query: SelectQuery) -> anyhow::Result<Vec<Abstract>> {
        let index = self.index.lock().expect("index lock");
        Ok(index.select(
            query.tag_key.as_deref(),
            query.min_verification,
        ))
    }
}
