#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

LOG_FILE="/tmp/tuff_brg_regression.log"
WAL_FILE="_tuffdb/tuff.wal"
PID=""

cleanup() {
  if [[ -n "${PID}" ]] && kill -0 "${PID}" 2>/dev/null; then
    kill -TERM "${PID}" 2>/dev/null || true
    wait "${PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

mkdir -p _tuffdb
before_size=0
if [[ -f "${WAL_FILE}" ]]; then
  before_size=$(wc -c < "${WAL_FILE}" | tr -d ' ')
fi

: >"${LOG_FILE}"
RUST_LOG=debug cargo build -p tuff_brg >>"${LOG_FILE}" 2>&1
RUST_LOG=debug ./target/debug/tuff_brg >>"${LOG_FILE}" 2>&1 &
PID=$!

for _ in $(seq 1 240); do
  if (echo > /dev/tcp/127.0.0.1/8787) >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done

if ! (echo > /dev/tcp/127.0.0.1/8787) >/dev/null 2>&1; then
  echo "ERROR: tuff_brg did not start on 127.0.0.1:8787"
  exit 1
fi

node <<'NODE'
const payload = {
  type: "StreamFragment",
  id: "regression-1",
  ts: new Date().toISOString(),
  payload: {
    conversation_id: "regression-suite",
    sequence_number: 1,
    url: "https://example.test",
    selector: "#content",
    fragment: "Regression smoke fragment",
    context: {
      page_title: "Regression",
      locale: "ja-JP"
    }
  }
};

function openSocket(url) {
  if (typeof WebSocket !== "undefined") {
    return new WebSocket(url);
  }
  try {
    const Ws = require("ws");
    return new Ws(url);
  } catch (e) {
    throw new Error("WebSocket client is unavailable in this Node runtime");
  }
}

const ws = openSocket("ws://127.0.0.1:8787/");
const timeout = setTimeout(() => {
  console.error("ws timeout");
  process.exit(1);
}, 5000);

ws.onopen = () => {
  ws.send(JSON.stringify(payload));
  setTimeout(() => ws.close(), 250);
};

ws.onerror = (err) => {
  console.error("ws error", err && err.message ? err.message : err);
  process.exit(1);
};

ws.onclose = () => {
  clearTimeout(timeout);
  process.exit(0);
};
NODE

sleep 1

if [[ ! -f "${WAL_FILE}" ]]; then
  echo "ERROR: WAL file not created: ${WAL_FILE}"
  exit 1
fi

after_size=$(wc -c < "${WAL_FILE}" | tr -d ' ')
if [[ "${after_size}" -le "${before_size}" ]]; then
  echo "ERROR: WAL not appended (before=${before_size}, after=${after_size})"
  exit 1
fi

kill -INT "${PID}"
set +e
wait "${PID}"
exit_code=$?
set -e
PID=""

if [[ "${exit_code}" -ne 0 ]]; then
  echo "ERROR: process exited with code ${exit_code}"
  exit 1
fi

echo "OK: regression check passed"
