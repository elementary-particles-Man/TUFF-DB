# WAL 仕様

## 概要

- WAL は JSON Lines 形式
- 各行が `OpLog` を表す

## 例

```json
{"op_id":"...","kind":{"InsertAbstract":{"abstract_":{...}}},"created_at":"..."}
```

## 出力先

- 既定: `/_tuffdb/tuff.wal`
