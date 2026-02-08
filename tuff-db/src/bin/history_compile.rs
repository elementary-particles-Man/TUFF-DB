use std::env;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let wal_path = env::var("TUFF_WAL_PATH").unwrap_or_else(|_| "_tuffdb/tuff.wal".to_string());
    let out_dir = env::var("TUFF_HISTORY_OUT").unwrap_or_else(|_| "history_out".to_string());
    transformer_neo::history::compiler::compile(PathBuf::from(wal_path), PathBuf::from(out_dir))
}
