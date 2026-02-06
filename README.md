# TUFF-DB / Transformer-NEO

ハルシネーションを物理的に拒絶するための型安全・決定論パイプライン実装です。
このリポジトリは **TUFF-DB (WAL + Index)** と **Ingest Core** を中心に、
LLM検証・抽象化・ギャップ解決（Transition生成）までの実装ベースを提供します。

## クイックスタート

```bash
# 依存取得とビルド
cargo run
```

`.env` を使って実弾モードで動作させる場合は、以下を設定してください。

```bash
export OPENAI_API_KEY="sk-..."
export OPENAI_MODEL="gpt-4o"
export TARGET_URL="https://www.kantei.go.jp/jp/rekidai/index.html"

cargo run
```

## 主要機能

- WAL (Write Ahead Log) への追記型永続化
- InMemoryIndex による検索
- Evidence 取得と HTML サニタイズ
- LLM Verifier / LLM Abstractor / LLM GapResolver
- Transition 生成（履歴の編纂）

## ディレクトリ

- `src/models/` : ドメインモデル
- `src/db/` : TUFF-DB (WAL + Index)
- `src/pipeline/` : Ingest / Verifier / Abstractor / GapResolver
- `docs/` : 資料・設計ノート

## 重要な環境変数

- `OPENAI_API_KEY` : LLM 実弾接続用
- `OPENAI_MODEL` : 使用モデル
- `TARGET_URL` : Evidence 取得元
- `AGENT_ROLE` : AgentIdentity の role に反映

## ライセンス

- `LICENSE` を参照してください。
