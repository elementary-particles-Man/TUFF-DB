# サマリー（ここまでの進捗）

## 目的
TUFF-DB / Transformer-NEO は、LLMのハルシネーションと現在バイアスを物理・構造で抑制するための Rust 実装。WAL と型安全モデルを核に、検証・要約・履歴（Transition）生成までの基盤を整備。

## フェーズ進捗
- Phase 1: モデル + WAL + Ingest コア
- Phase 2: LLM Verifier / WebFetcher 実URL / html2text サニタイズ
- Phase 2.5: LLM Abstractor
- Phase 4 Step 1: AgentIdentity / Transition
- Phase 4 Step 2: GapResolver
- Phase 4 Step 2 Wire: main への配線

## 実装済みの主要コンポーネント
- TUFF-DB
  - `src/db/engine.rs`: WAL 追記と InMemoryIndex
  - `src/db/api.rs`: OpLog/OpKind/SelectQuery
- モデル
  - `Abstract`, `TagBits`
  - `Evidence`（`evidence_id` 付き）
  - `AgentIdentity`（Origin固定）
  - `Transition`（履歴/遷移）
  - `Id`, `IsoDateTime`
- パイプライン
  - `WebFetcher`: TARGET_URL 取得 + html2text
  - `LlmVerifier`: Evidenceのみで検証
  - `LlmAbstractor`: 要約・タグ付与
  - `LlmGapResolver`: 遷移生成（Transition）
  - `IngestPipeline`: Split -> Fetch -> Verify -> Abstract -> DB

## 重要な実装方針
- Evidence のハッシュは生HTMLから計算
- 検証は Evidence のみに基づく
- Origin はコード定数（AgentIdentity）

## 主要ファイル
- `src/main.rs`: 実行エントリ（GapResolver含む）
- `src/pipeline/gap_resolver.rs`: LLM GapResolver
- `src/models/agent.rs`, `src/models/history.rs`
- `docs/ARCHITECTURE_SPEC.md`: 仕様概要

## 実行方法（実弾）
```bash
export OPENAI_API_KEY="sk-..."
export OPENAI_MODEL="gpt-4o"
export TARGET_URL="https://www.kantei.go.jp/jp/rekidai/index.html"

cargo run
```

## 期待ログ
- `op_id=...`
- `[TRANSITION RECORD GENERATED] {...}`（APIキー有効時）

## ライセンス/商用
- ライセンス: PolyForm Noncommercial 1.0.0
- 商用条件: `COMMERCIAL.md`（有料サブスク利用時のみロイヤルティ）

## 追加ドキュメント
- `docs/index.md`
- `docs/architecture.md`
- `docs/pipeline.md`
- `docs/models.md`
- `docs/wal.md`
- `docs/phases.md`
