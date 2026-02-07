# Transformer-NEO on TUFF-DB

## 概要 (Overview)
**TUFF-DB (The Unfiltered Fact Finder Database)** は、LLMのハルシネーション（幻覚）と現在バイアスを物理的・構造的に排除するために設計された、Rust製の次世代情報検証基盤です。


## 商用利用
- 商用利用の方は `COMMERCIAL.md` をご確認ください。

## 主な機能 (Core Features)
1. **Physical Identity Protocol**:
   - AIの「Origin（起源）」をコードレベルで固定し、役割（Role）と分離。
   - ログの責任帰属を暗号的に保証。
2. **Verification Floor System**:
   - 外部事実と矛盾する主張を「SMOKE」層へ物理的に隔離。
3. **Gap Resolver (The Historian)**:
   - 内部知識と外部事実の差分を検知し、その原因（イベント）をWebから特定。
   - 「いつ、なぜ変わったか」を `Transition` レコードとして編纂。
4. **Determinism over Probability**:
   - 確率的な生成よりも、Rustの型システムと検証ロジックによる決定論的処理を優先。

## クイックスタート
```bash
export OPENAI_API_KEY="sk-..."
cargo run
```
