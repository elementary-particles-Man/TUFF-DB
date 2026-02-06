# アーキテクチャ概要

## 目的

- ハルシネーションの物理的拒絶
- 事実と根拠を WAL と型で固定
- Gap Resolver により「点」ではなく「線」の記録を生成

## コンポーネント

- `pipeline` : Ingest / Verifier / Abstractor / GapResolver
- `db` : InMemoryIndex + WAL
- `models` : Abstract, Evidence, Transition, AgentIdentity 等

## データフロー（概要）

1. Input Split
2. Fact Fetch (Evidence)
3. Verify (LLM or Dummy)
4. Abstract (LLM or Dummy)
5. DB Append (WAL)
6. Gap Resolve (任意、LLM)

## 信頼境界

- Evidence のハッシュは「生HTML」から計算
- LLM は Evidence 以外の知識を使わない前提
- Origin (AgentIdentity.origin) はビルド定数で固定
