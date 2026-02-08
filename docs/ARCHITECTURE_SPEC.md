# TUFF-DB Architecture Specification

## 1. System Architecture
本システムは「Ingest Pipeline」と「Gap Resolver」の2つの主要なフローで構成されます。

### 1.1 Ingest Pipeline
- **Fetcher**: Web/Mockから `Evidence` を取得。
- **Verifier**: Claim と Evidence を比較し、`VerificationStatus` (White/Gray/Black/Smoke) を決定。
- **Abstractor**: 検証結果を要約し、検索可能なインデックスを生成。

### 1.2 Gap Resolver (Phase 4)
- **目的**: 内部知識（Internal State）と外部事実（External Evidence）の不整合を解消する。
- **動作**: 差分検知 -> イベント推論 (LLM) -> 歴史記録 (Transition) の生成。

## 2. Data Models

### 2.1 Agent Identity (`src/models/agent.rs`)
- **Origin**: 実行主体の識別子 (例: `GPT-5`, `Gemini`). 実行中に変更不可能。
- **Identity Lock 原則**: 起動時に `AI_ORIGIN` 環境変数で自己を定義し、実行コンテキスト内では固定された Origin として振る舞う。
- **Role**: 一時的な役割 (例: `Gemini`). 文脈に応じて可変。
- **設計思想**: AIの「同一性」を物理的に固定し、なりすましや自己認識の揺らぎを防ぐ。

### 2.2 Transition (`src/models/history.rs`)
- 歴史の「線」を表すレコード。
- `from_state` (過去) -> `event` (変化要因) -> `to_state` (現在)。
- 必ず `evidence_ids` を持ち、事実に基づかない歴史修正を許さない。

## 3. Implementation Details
### Local AI Support
- `async_openai` クレートにより、OpenAI API だけでなく Ollama (Gemma/Llama) 等のローカルLLMエンドポイントにも対応可能。

## 4. Glossary
- **SMOKE**: 官邸HPなどのTier-1ソースと矛盾する情報。検索結果から除外される。
- **Origin**: AIの出自。Roleと混同してはならない。
