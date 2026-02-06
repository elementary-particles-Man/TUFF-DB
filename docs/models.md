# モデル定義（実装ベース）

## 核心モデル

- `Abstract` : 生成結果
- `Evidence` : 根拠（`evidence_id` を必須化）
- `Transition` : Gap Resolver による遷移記録
- `AgentIdentity` : Origin 固定 / Role 可変

## 重要な型

- `Id` : UUID ベースの汎用 ID
- `IsoDateTime` : UTC 時刻
- `VerificationStatus` : Smoke/Gray*/White

## 位置

- `src/models/*`
