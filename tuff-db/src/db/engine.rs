use crate::db::api::{OpKind, OpLog, SelectQuery, TuffDb};
use crate::db::index::InMemoryIndex;
use crate::models::{Abstract, AgentIdentity, Transition};
use chrono::Utc;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use uuid::Uuid;

pub struct TuffEngine {
    index: Mutex<InMemoryIndex>,
    wal: Mutex<BufWriter<std::fs::File>>,
}

impl TuffEngine {
    pub fn new(wal_path: &str) -> anyhow::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(wal_path)?;
        Ok(Self {
            index: Mutex::new(InMemoryIndex::default()),
            wal: Mutex::new(BufWriter::new(file)),
        })
    }

    fn write_wal(&self, op: &OpLog) -> anyhow::Result<()> {
        let mut guard = self.wal.lock().expect("wal lock");
        let line = serde_json::to_string(op)?;
        guard.write_all(line.as_bytes())?;
        guard.write_all(b"\n")?;
        guard.flush()?;
        Ok(())
    }
}

impl TuffDb for TuffEngine {
    fn append_abstract(&self, abstract_: Abstract) -> anyhow::Result<OpLog> {
        let op = OpLog {
            op_id: Uuid::new_v4(),
            kind: OpKind::InsertAbstract { abstract_ },
            created_at: Utc::now(),
        };
        self.write_wal(&op)?;

        let OpKind::InsertAbstract { abstract_ } = op.kind.clone();
        let mut index = self.index.lock().expect("index lock");
        index.insert(abstract_);

        Ok(op)
    }

    fn append_transition(&self, mut transition: Transition) -> anyhow::Result<OpLog> {
        transition.agent = AgentIdentity::current();
        let op = OpLog {
            op_id: Uuid::new_v4(),
            kind: OpKind::InsertTransition { transition },
            created_at: Utc::now(),
        };
        self.write_wal(&op)?;
        Ok(op)
    }

    fn select(&self, query: SelectQuery) -> anyhow::Result<Vec<Abstract>> {
        let index = self.index.lock().expect("index lock");
        Ok(index.select(
            query.tag_key.as_deref(),
            query.min_verification,
        ))
    }
}
