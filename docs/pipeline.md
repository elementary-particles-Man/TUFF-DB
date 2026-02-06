# Ingest パイプライン

## 構成

- `InputSplitter` : 入力分割
- `FactFetcher` : Evidence 取得
- `ClaimVerifier` : 検証
- `AbstractGenerator` : 要約・タグ生成
- `GapResolver` : 状態ギャップの遷移生成

## 現在の実装

- Splitter: `DummySplitter`
- Fetcher: `WebFetcher` (TARGET_URL)
- Verifier: `LlmVerifier` / `DummyVerifier`
- Abstractor: `LlmAbstractor` / `DummyAbstractGenerator`
- GapResolver: `LlmGapResolver` (main で任意起動)

## 実弾運用

```bash
export OPENAI_API_KEY="sk-..."
export OPENAI_MODEL="gpt-4o"
export TARGET_URL="https://www.kantei.go.jp/jp/rekidai/index.html"

cargo run
```
