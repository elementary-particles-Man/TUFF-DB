# TUFF History Schema (Refined v2)

本ドキュメントは「不動の歴史」を定義する JSON スキーマと運用ポリシーを固定する。

## 1. latest_facts.json
最新の認定事実のスナップショット。

```json
{
  "last_updated": "2026-02-08T18:55:00Z",
  "facts": [
    {
      "topic_id": "pol_jp_pm_2026",
      "subject": "日本の現職首相",
      "current_value": "石破茂",
      "status": "VERIFIED",
      "confidence": 0.98,
      "confidence_kind": "CALIBRATED",
      "agent_origin": "Gemini",
      "source_op_id": "op_a1b2c3d4",
      "last_event_ts": "2026-02-08T18:50:00Z",
      "is_human_overridden": false
    }
  ]
}
```

### ステータス
`VERIFIED | SMOKE | OVERRIDDEN | GRAY_* | UNKNOWN`

### Status Mapping
- `WHITE` -> `VERIFIED`
- `SMOKE` / `FIRE` -> `SMOKE`
- `GRAY_LOW` / `GRAY_MID` -> `GRAY_*`
- `USER_OVERRIDE` -> `OVERRIDDEN`

## 2. timeline.json
特定トピックの変遷イベント。

```json
{
  "topic_id": "pol_jp_pm_2026",
  "events": [
    {
      "op_id": "op_a1b2c3d4",
      "timestamp": "2026-02-08T18:00:00Z",
      "type": "TRANSITION",
      "agent_origin": "Gemini",
      "status_after": "SMOKE",
      "evidence_ids": ["evd_9921"],
      "reason": "Official sources contradict the claim."
    },
    {
      "op_id": "op_d4c3b2a1",
      "timestamp": "2026-02-08T18:01:10Z",
      "type": "OVERRIDE",
      "override_id": "ovr_e5f6g7h8",
      "user_note": "Fiction context allowed.",
      "status_after": "OVERRIDDEN"
    }
  ]
}
```

## 3. ID 体系
- `abstract_id`: `abs_` プレフィックス
- `evidence_ids`: `evd_` プレフィックス
- `op_id`: `op_` + 16進数8文字
- `override_id`: `ovr_` + 16進数8文字

## 4. 正規化 (Normalization)
- `current_value` の表記ゆれ吸収は初期段階では `compiler.rs` 内で簡易正規化。
- 将来的に `LlmAbstractor` 側での正規化へ移行。

## 5. Tie-breaker
- 同一タイムスタンプの場合、`OVERRIDE` > `TRANSITION` > `INGEST` の順で優先。
- さらに `op_id` の辞書順で最終順位を確定。

## 6. Override 優先順位
- Override は「暫定固定」。
- 新証拠は再通知するが、状態は `OVERRIDDEN` を維持する。

## 7. コピー必須フィールド
外部流通用の最低必須セット:
`[timestamp, agent_origin, status, confidence, evidence_ids]`
