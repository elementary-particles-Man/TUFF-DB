# TUFF-BRG-ECO (The Unfiltered Fact Finder Bridge Ecosystem)

## 概要
- LLMのハルシネーションと現在バイアスを物理的・構造的に排除するための次世代情報検証・中継基盤。
- 計算コストの激減、事実の不動点化、およびAIの知能自壊（モデル崩壊）防止を目的とする。

## TUFF-BRG-ECO とは
ECO (Ecosystem) は、tuff-db（コア）と tuff-brg（ブリッジ）が単一のワークスペースで連携し、事実の不動点を共有する統合運用環境を指します。

## アーキテクチャ層
- TUFF-DB (Core): 事実の保管庫。WALと型による歴史的整合性の管理。
- TUFF-BRG (Middleware): アドオンとDBを繋ぐブリッジ。リアルタイム検証と計算代行の中枢。
- TUFF-BRG-Extension (Add-on): `tuff-brg/addon/` に配置。現在はミドルウェアに内包されているが、将来的に独立配布・マルチプラットフォーム展開が可能。

## 主要メカニズム
- Physical Identity Protocol: AIのOrigin（起源）を固定し責任帰属を明確化。
- Identity Lock: 環境変数 `AI_ORIGIN` を参照（未指定時のデフォルトは `Gemini`）。本番運用や特定モデルでの検証時は、責任帰属を明確にするため明示的な指定を推奨する。内部的には `OnceLock` 等を用いて初期化を一度に限定し、プロセス実行中の再定義を物理的に防止する。
- Gap Resolver: 内部知識と外部事実の乖離を特定し、Transition（遷移）として編纂。
- Semantic Caching: 既知の事実をMIDで即答し、LLMの推論コストをスキップ。

## ロードマップ
- Phase 1: Foundation: ディレクトリ再配置、ワークスペース化、Originの動的固定実装。
- Phase 2: Connectivity: WebSocketプロトコル実装、アドオンのストリーミング送信対応。
- WebSocketプロトコル実装（仕様策定中。API型定義および `docs/protocol.md` は順次実装予定）。
- Phase 3: Verification: リアルタイム事実照合、ハルシネーション時のSTOP命令、WALへの遷移記録。
- Phase 4: Optimization: セマンティック・キャッシュによる低コスト化、プロバンス追跡による偏向拒絶。

## クイックスタート

### 実行コマンド
```bash
# ミドルウェア(MID)の起動
export AI_ORIGIN="Gemini" # 未指定時のデフォルトは "Gemini"
cargo run -p tuff_brg
```

### 環境変数
| 変数名 | 説明 | 例 |
| :--- | :--- | :--- |
| `AI_ORIGIN` | AIの自己識別子。Transitionに刻印される。 | `Gemini`, `GPT-4o` |
| `AGENT_ROLE` | 一時的な役割（任意）。 | `Verifier`, `Coder` |
| `OPENAI_API_KEY` | 検証用LLMのAPIキー。 | `sk-...` |

## 商用利用
- 商用利用の方は `COMMERCIAL.md` をご確認ください。

## ディレクトリ構造と命名規則
本プロジェクトはワークスペース構成をとります。ディレクトリ名は `kebab-case` を採用しています。
- `tuff-db/`: コアエンジン
- `tuff-brg/`: ミドルウェア & アドオン

## 技術的背景と根拠
- 計算コストの激減: Semantic Caching により、検証済み事実を MID が即答することで LLM の推論ステップをスキップする（実装は `tuff-brg` 側で順次追加予定）。
- 知能自壊（モデル崩壊）の防止: `docs/architecture.md` を参照。

## 開発者向け注記
- ユニットテストは `cargo test -p tuff_db` で実行可能です（※現在、一部の基盤テストを順次拡充中のため、環境により未実装項目がある場合があります）。
