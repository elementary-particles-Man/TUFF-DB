// GPT-Booster v5.2 ultra-passive — ChatGPT output aware (standalone, body observer)

const DEBUG = true;
const OBS_CONFIG = { childList: true, subtree: true };
const TIMESTAMP_STYLE_ID = "gpt-booster-visible-timestamp-style";
const BASE_SELECTORS = {
  default: {
    assistant: '[data-message-author-role="assistant"], .model-response, [data-test-id="model-response"], message-content',
    rootFallback: '[role="article"]',
    turns: '[data-message-author-role], .user-query, .model-response, [data-test-id="user-query"], [data-test-id="model-response"], user-query, message-content'
  },
  gemini: {
    assistant: [
      'div[data-message-author-role="model"]',
      'div[data-message-author-role="assistant"]',
      'div.message-content',
      'div[class*="message-content"]',
      'div[class*="model-response"]',
      'div[class*="response"]',
      'div[aria-label*="Model response"]',
      'div[aria-live="polite"]'
    ].join(", "),
    turns: [
      'div[data-message-author-role]',
      'div.message-content',
      'div[class*="message-content"]',
      'div[class*="response"]'
    ].join(", ")
  }
};
const STABLE_TEXT_MS = 900;
const SAFE_MODEL_RE = /\bgpt-?5\b/i;
const WS_URL = "ws://127.0.0.1:8787";
const WS_RETRY_BASE_MS = 800;
const WS_RETRY_MAX_MS = 8000;
const WS_SEND_DEBOUNCE_MS = 180;
const SEND_DELAY = 600;
const STOP_OVERLAY_ID = "tuff-brg-stop-overlay";
const CONTINUE_BUTTON_ID = "tuff-brg-continue-btn";
const META_COPY_ID = "tuff-brg-meta-copy";
const RETRY_BUTTON_ID = "tuff-brg-retry-btn";
const SELECTOR_WARNING_ID = "tuff-brg-selector-warning";

let lastScrollTop = null;
let activeAssistant = null;
let lastAssistantText = "";
let lastAssistantChange = 0;
let scrollClampEnabled = false;
let mutationDebounceTimer = null;
let finalizeTimer = null;
let responseSafeMode = false;
let ws = null;
let wsRetryTimer = null;
let wsRetryDelay = WS_RETRY_BASE_MS;
let wsReady = false;
let pendingFragments = [];
let convoId = crypto.randomUUID();
let seq = 0;
let lastSentText = "";
let sendTimer = null;
let lastAbstractId = null;
let stopActive = false;
let aiOrigin = "Gemini";
let tuffWebBase = "";
let lastJudgeStatus = null;
let lastJudgeReason = null;
let lastJudgeClaim = null;
let lastJudgeTs = null;
let lastDebugFragment = "";
let lastDebugTs = 0;
let activeSelectors = buildSelectorConfig();
let selectorMissSince = 0;

window.__GPT_BOOSTER_NEW__ = true;

function nowRfc3339() {
  return new Date().toISOString();
}

function isGeminiPage() {
  return location.hostname.includes("gemini.google.com");
}

function validSelectorString(value, fallback) {
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}

function buildSelectorConfig(overrideRaw) {
  const o = overrideRaw && typeof overrideRaw === "object" ? overrideRaw : {};
  const od = o.default && typeof o.default === "object" ? o.default : {};
  const og = o.gemini && typeof o.gemini === "object" ? o.gemini : {};
  return {
    default: {
      assistant: validSelectorString(od.assistant, BASE_SELECTORS.default.assistant),
      rootFallback: validSelectorString(od.rootFallback, BASE_SELECTORS.default.rootFallback),
      turns: validSelectorString(od.turns, BASE_SELECTORS.default.turns)
    },
    gemini: {
      assistant: validSelectorString(og.assistant, BASE_SELECTORS.gemini.assistant),
      turns: validSelectorString(og.turns, BASE_SELECTORS.gemini.turns)
    }
  };
}

function selectorFor(kind) {
  if (isGeminiPage() && activeSelectors.gemini[kind]) {
    return activeSelectors.gemini[kind];
  }
  return activeSelectors.default[kind];
}

function debugLogFragment(source, text) {
  if (!DEBUG) return;
  const now = Date.now();
  if (text === lastDebugFragment && now - lastDebugTs < 1000) return;
  lastDebugFragment = text;
  lastDebugTs = now;
  const snippet = text.length > 160 ? `${text.slice(0, 160)}...` : text;
  console.log(`[TUFF][${source}] ${snippet}`);
}

function wsScheduleReconnect() {
  if (wsRetryTimer) return;
  wsRetryTimer = setTimeout(() => {
    wsRetryTimer = null;
    wsRetryDelay = Math.min(WS_RETRY_MAX_MS, wsRetryDelay * 2);
    wsConnect();
  }, wsRetryDelay);
}

function wsConnect() {
  try {
    ws = new WebSocket(WS_URL);
  } catch (_) {
    wsScheduleReconnect();
    return;
  }

  ws.addEventListener("open", () => {
    wsReady = true;
    wsRetryDelay = WS_RETRY_BASE_MS;
    flushPendingFragments();
  });

  ws.addEventListener("close", () => {
    wsReady = false;
    wsScheduleReconnect();
  });

  ws.addEventListener("error", () => {
    wsReady = false;
    wsScheduleReconnect();
  });

  ws.addEventListener("message", (ev) => {
    if (typeof ev.data !== "string") return;
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch (_) {
      return;
    }

    if (msg.type === "JudgeResult" && msg.payload) {
      if (msg.payload.abstract_id) {
        lastAbstractId = msg.payload.abstract_id;
      }
      lastJudgeStatus = msg.payload.status || null;
      lastJudgeReason = msg.payload.reason || null;
      lastJudgeClaim = msg.payload.claim || null;
      lastJudgeTs = msg.ts || nowRfc3339();
    }

    if (msg.type === "ControlCommand" && msg.payload) {
      const cmd = msg.payload.command;
      if (cmd === "STOP") {
        activateStopOverlay(msg.payload.detail || "STOP");
      } else if (cmd === "CONTINUE") {
        deactivateStopOverlay();
      }
    }
  });
}

function loadAddonSettings() {
  if (typeof chrome === "undefined" || !chrome.storage || !chrome.storage.local) {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    chrome.storage.local.get(["TUFF_WEB_BASE", "AI_ORIGIN", "OVERRIDE_SELECTORS"], (res) => {
      if (res && typeof res.TUFF_WEB_BASE === "string") {
        tuffWebBase = res.TUFF_WEB_BASE.trim();
      }
      if (res && typeof res.AI_ORIGIN === "string" && res.AI_ORIGIN.trim()) {
        aiOrigin = res.AI_ORIGIN.trim();
      }
      if (res && res.OVERRIDE_SELECTORS) {
        try {
          const raw =
            typeof res.OVERRIDE_SELECTORS === "string"
              ? JSON.parse(res.OVERRIDE_SELECTORS)
              : res.OVERRIDE_SELECTORS;
          activeSelectors = buildSelectorConfig(raw);
        } catch (_) {
          activeSelectors = buildSelectorConfig();
        }
      } else {
        activeSelectors = buildSelectorConfig();
      }
      resolve();
    });
  });
}

function ensureStopOverlayStyle() {
  if (document.getElementById("tuff-brg-stop-style")) return;
  const style = document.createElement("style");
  style.id = "tuff-brg-stop-style";
  style.textContent = `
#${STOP_OVERLAY_ID} {
  position: fixed;
  inset: 0;
  background: rgba(8, 8, 8, 0.85);
  color: #f6f6f6;
  z-index: 2147483647;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 24px;
  font-family: system-ui, -apple-system, Segoe UI, sans-serif;
}
#${STOP_OVERLAY_ID} .card {
  max-width: 520px;
  background: #111;
  border: 1px solid #333;
  border-radius: 12px;
  padding: 18px 20px;
  box-shadow: 0 10px 40px rgba(0,0,0,0.4);
}
#${STOP_OVERLAY_ID} .title { font-size: 18px; margin-bottom: 10px; }
#${STOP_OVERLAY_ID} .reason { font-size: 13px; opacity: 0.85; }
#${STOP_OVERLAY_ID} .link { margin-top: 12px; font-size: 12px; color: #7dd3fc; }
#${META_COPY_ID} { margin-top: 10px; font-size: 12px; color: #e5e7eb; user-select: text; cursor: copy; }
#${RETRY_BUTTON_ID} { margin-left: 8px; }
`;
  document.head.appendChild(style);
}

function buildMetaLine() {
  const ts = lastJudgeTs || nowRfc3339();
  const status = lastJudgeStatus || "UNKNOWN";
  const reason = lastJudgeReason ? ` (${lastJudgeReason})` : "";
  const claim = lastJudgeClaim || "(unknown claim)";
  return `[${ts}] [AI: ${aiOrigin}] Claim: ${claim} | Result: ${status}${reason}`;
}

function buildAbstractLink() {
  if (!lastAbstractId) return null;
  if (!tuffWebBase) return null;
  const base = tuffWebBase.replace(/\/+$/, "");
  return `${base}/abstract/${lastAbstractId}`;
}

function activateStopOverlay(detail) {
  stopActive = true;
  ensureStopOverlayStyle();
  if (document.getElementById(STOP_OVERLAY_ID)) return;
  const overlay = document.createElement("div");
  overlay.id = STOP_OVERLAY_ID;
  const abstractLink = buildAbstractLink();
  const link = abstractLink
    ? `Abstract: <a href="${abstractLink}" target="_blank" rel="noreferrer">${lastAbstractId}</a>`
    : lastAbstractId
      ? `Abstract ID: ${lastAbstractId}`
      : "Abstract ID: (pending)";
  overlay.innerHTML = `
    <div class="card">
      <div class="title">STOP: 検証失敗を検知</div>
      <div class="reason">${detail || "Smoke detected"}</div>
      <div class="link">${link}</div>
      <div id="${META_COPY_ID}">${buildMetaLine()}</div>
      <div style="margin-top:12px;">
        <button id="${CONTINUE_BUTTON_ID}" style="padding:6px 12px;border-radius:8px;border:1px solid #444;background:#222;color:#eee;cursor:pointer;">
          自己責任で続行
        </button>
      </div>
    </div>
  `;
  document.body.appendChild(overlay);
  const btn = document.getElementById(CONTINUE_BUTTON_ID);
  if (btn) {
    btn.addEventListener("click", () => {
      sendManualOverride();
    });
  }
  const meta = document.getElementById(META_COPY_ID);
  if (meta) {
    meta.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(meta.textContent || "");
      } catch (_) {
        // Fallback: user can still select text manually.
      }
    });
  }
}

function deactivateStopOverlay() {
  stopActive = false;
  const overlay = document.getElementById(STOP_OVERLAY_ID);
  if (overlay) overlay.remove();
}

function ensureSelectorWarning(message) {
  if (document.getElementById(SELECTOR_WARNING_ID)) return;
  const box = document.createElement("div");
  box.id = SELECTOR_WARNING_ID;
  box.textContent = message;
  box.style.cssText = [
    "position:fixed",
    "right:12px",
    "bottom:12px",
    "z-index:2147483646",
    "padding:10px 12px",
    "border:1px solid #8b5e34",
    "background:#fff6e5",
    "color:#6b3f1d",
    "font-size:12px",
    "border-radius:8px",
    "max-width:320px",
    "line-height:1.4"
  ].join(";");
  document.body.appendChild(box);
}

function clearSelectorWarning() {
  const box = document.getElementById(SELECTOR_WARNING_ID);
  if (box) box.remove();
}

function buildStreamFragmentPayload(fragment) {
  return {
    type: "StreamFragment",
    id: crypto.randomUUID(),
    ts: nowRfc3339(),
    payload: {
      conversation_id: convoId,
      sequence_number: seq++,
      url: window.location.href,
      selector: selectorFor("assistant"),
      fragment,
      context: {
        page_title: document.title || "",
        locale: navigator.language || ""
      }
    }
  };
}

function sendStreamFragment(fragment) {
  if (!fragment) return false;
  if (!wsReady || !ws || ws.readyState !== WebSocket.OPEN) {
    queuePendingFragment(fragment);
    return false;
  }
  try {
    debugLogFragment("send", fragment);
    const msg = buildStreamFragmentPayload(fragment);
    ws.send(JSON.stringify(msg));
    if (DEBUG) console.log("[TUFF] Sent to server!");
    return true;
  } catch (_) {
    queuePendingFragment(fragment);
    return false;
  }
}

function scheduleSend(fragment) {
  if (fragment === lastSentText || fragment.length < 4) return;
  if (sendTimer) clearTimeout(sendTimer);
  sendTimer = setTimeout(() => {
    sendTimer = null;
    if (Math.abs(fragment.length - lastSentText.length) > 5) {
      if (DEBUG) {
        const snippet = fragment.length > 10 ? `${fragment.slice(0, 10)}...` : fragment;
        console.log("[TUFF][throttled-send]", snippet);
      }
      sendStreamFragment(fragment);
      lastSentText = fragment;
    }
  }, SEND_DELAY);
}

function queuePendingFragment(fragment) {
  if (!fragment) return;
  pendingFragments.push(fragment);
  if (pendingFragments.length > 50) {
    pendingFragments = pendingFragments.slice(-50);
  }
}

function flushPendingFragments() {
  if (!wsReady || !ws || ws.readyState !== WebSocket.OPEN) return;
  if (!pendingFragments.length) return;
  const queue = pendingFragments.slice();
  pendingFragments = [];
  queue.forEach((fragment) => {
    try {
      debugLogFragment("send", fragment);
      const msg = buildStreamFragmentPayload(fragment);
      ws.send(JSON.stringify(msg));
      if (DEBUG) console.log("[TUFF] Sent to server!");
    } catch (_) {
      queuePendingFragment(fragment);
    }
  });
}

function sendManualOverride() {
  if (!wsReady || !ws || ws.readyState !== WebSocket.OPEN) return;
  const msg = {
    type: "ControlCommand",
    id: crypto.randomUUID(),
    ts: nowRfc3339(),
    payload: {
      command: "CONTINUE",
      trigger: "ManualOverride",
      detail: "User override",
      manual_override: {
        conversation_id: convoId,
        abstract_id: lastAbstractId,
        note: "No reason provided"
      }
    }
  };
  ws.send(JSON.stringify(msg));
}

function detectModelTokenFromLocation() {
  try {
    const url = new URL(window.location.href);
    const candidates = [url.searchParams.get("model"), url.searchParams.get("m"), url.hash, url.pathname];
    for (const value of candidates) {
      if (typeof value !== "string") continue;
      if (SAFE_MODEL_RE.test(value)) return value;
    }
  } catch (_) {
    // Ignore URL parsing issues.
  }
  return "";
}

function detectModelTokenFromUi() {
  const modelSwitcher = document.querySelector(
    '[data-testid*="model"], button[aria-haspopup="menu"], [aria-label*="Model"], [aria-label*="model"]'
  );
  if (!modelSwitcher) return "";
  const text = (modelSwitcher.textContent || "").replace(/\s+/g, " ").trim();
  return text;
}

function refreshResponseSafeMode() {
  const token = `${detectModelTokenFromLocation()} ${detectModelTokenFromUi()}`;
  responseSafeMode = SAFE_MODEL_RE.test(token);
}

function isThinking(root) {
  if (!root) return false;
  const testId = root.getAttribute("data-testid") || "";
  if (/thinking|generating|streaming/i.test(testId)) return true;
  const className = typeof root.className === "string" ? root.className : "";
  if (/thinking|generating|streaming/i.test(className)) return true;
  const thinkingNode = root.querySelector(
    '[data-testid*="thinking"], [data-testid*="generating"], [data-testid*="streaming"], .thinking, .generating, .streaming'
  );
  return Boolean(thinkingNode);
}

function hasStreamingCursor(root) {
  if (!root) return false;
  return Boolean(
    root.querySelector(
      '[data-testid="cursor"], [data-testid*="cursor"], .cursor, .result-streaming, .typing, [class*="streaming"]'
    )
  );
}

function pickLatestWithText(nodes) {
  const list = Array.from(nodes || []);
  for (let i = list.length - 1; i >= 0; i -= 1) {
    const node = list[i];
    if (!(node instanceof HTMLElement)) continue;
    const text = extractNodeText(node);
    if (text.length > 0) return node;
  }
  return null;
}

function getLatestGeminiAssistant() {
  const nodes = document.querySelectorAll(selectorFor("assistant"));
  return pickLatestWithText(nodes);
}

function getLatestAssistant() {
  if (isGeminiPage()) {
    return getLatestGeminiAssistant() || document.querySelector(selectorFor("rootFallback"));
  }
  const assistants = document.querySelectorAll(selectorFor("assistant"));
  if (assistants.length) return assistants[assistants.length - 1];
  const fallbackNodes = document.querySelectorAll(selectorFor("rootFallback"));
  return fallbackNodes[fallbackNodes.length - 1] || null;
}

function ensureTimestampStyle() {
  if (document.getElementById(TIMESTAMP_STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = TIMESTAMP_STYLE_ID;
  style.textContent = `
    .gpt-booster-ts {
      font-size: 11px;
      line-height: 1.3;
      opacity: 0.72;
      margin: 0 0 8px 0;
      letter-spacing: 0.01em;
      user-select: text;
      pointer-events: auto;
    }
    .gpt-booster-ts-user { color: #2d6a4f; }
    .gpt-booster-ts-assistant { color: #1d3557; }
  `;
  document.head.appendChild(style);
}

function formatLocalTimestamp(ms) {
  const parts = new Intl.DateTimeFormat("ja-JP", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false
  }).formatToParts(new Date(ms));
  const map = Object.create(null);
  parts.forEach((p) => {
    map[p.type] = p.value;
  });
  return `${map.year}-${map.month}-${map.day} ${map.hour}:${map.minute}:${map.second}`;
}

function getLocalTimeZoneLabel() {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "local";
  } catch (_) {
    return "local";
  }
}

const STORAGE_PREFIX = "gpt-booster-ts";

function getStableId(node) {
  if (node.id) return node.id;
  const testId = node.getAttribute("data-testid");
  if (testId) return testId;
  const article = node.closest('article[data-testid]');
  if (article) {
    const articleTestId = article.getAttribute("data-testid");
    if (articleTestId) return articleTestId;
  }
  const childWithId = node.querySelector('[id^="user-query-content-"], [id^="message-content-id-"]');
  if (childWithId && childWithId.id) return childWithId.id;
  return null;
}

function getStorageKey(stableId) {
  // Scope by pathname to prevent collisions for generic IDs (like user-query-content-10)
  // key format: gpt-booster-ts:<pathname>:<elementId>
  const scope = window.location.pathname.replace(/\/+$/, "");
  return `${STORAGE_PREFIX}:${scope}:${stableId}`;
}

function getNodeTextFingerprint(node) {
  // Ignore booster timestamp marker itself to keep fingerprints stable.
  const cloned = node.cloneNode(true);
  if (cloned instanceof HTMLElement) {
    cloned.querySelectorAll(".gpt-booster-ts").forEach((el) => el.remove());
  }
  const text = (cloned.textContent || "").replace(/\s+/g, " ").trim();
  if (!text) return "";
  return text.slice(0, 1800);
}

function hashText(text) {
  // FNV-1a 32-bit (compact, fast, stable in plain JS).
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = (h * 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function getFallbackStorageKey(node, role) {
  const scope = window.location.pathname.replace(/\/+$/, "");
  const fp = getNodeTextFingerprint(node);
  if (!fp) return null;
  return `${STORAGE_PREFIX}:${scope}:fp:${role}:${hashText(fp)}`;
}

function getNodeStorageBaseKey(node, role) {
  const stableId = getStableId(node);
  if (stableId) return getStorageKey(stableId);
  return getFallbackStorageKey(node, role);
}

function upsertVisibleTimestamp(node, role, options = {}) {
  if (!(node instanceof HTMLElement)) return;
  const markAssistantDone = Boolean(options.markAssistantDone) && role === "assistant";
  const storageKey = getNodeStorageBaseKey(node, role);
  const endStorageKey = storageKey ? `${storageKey}:end` : null;
  let ts;
  let endTs;

  if (storageKey) {
    try {
      const stored = localStorage.getItem(storageKey);
      if (stored) {
        ts = Number(stored);
      }
      if (endStorageKey) {
        const storedEnd = localStorage.getItem(endStorageKey);
        if (storedEnd) endTs = Number(storedEnd);
      }
    } catch (_) {
      // Ignore storage failures and keep DOM-only timestamp.
    }
  }

  // Fallback if not in storage or no stable ID
  if (!Number.isFinite(ts) || ts <= 0) {
    ts = Number(node.getAttribute("data-gpt-booster-ts"));
    if (!Number.isFinite(ts) || ts <= 0) {
      ts = Number(node.getAttribute("data-gpt-booster-jst-ts"));
    }
    if (!Number.isFinite(ts) || ts <= 0) {
      ts = Date.now();
    }
  }

  if (!Number.isFinite(endTs) || endTs <= 0) {
    endTs = Number(node.getAttribute("data-gpt-booster-ts-end"));
  }
  if (markAssistantDone) {
    const now = Date.now();
    if (!Number.isFinite(endTs) || endTs <= 0 || endTs < now) {
      endTs = now;
    }
  }

  // Save to storage if we have a valid key
  if (storageKey) {
    try {
      localStorage.setItem(storageKey, String(ts));
      if (endStorageKey && Number.isFinite(endTs) && endTs > 0) {
        localStorage.setItem(endStorageKey, String(endTs));
      }
    } catch (_) {
      // Storage quota or privacy mode can block writes.
    }
  }
  node.setAttribute("data-gpt-booster-ts", String(ts));
  if (Number.isFinite(endTs) && endTs > 0) {
    node.setAttribute("data-gpt-booster-ts-end", String(endTs));
  }

  // Double-check attribute consistency
  if (ts && node.getAttribute("data-gpt-booster-ts") !== String(ts)) {
    node.setAttribute("data-gpt-booster-ts", String(ts));
  }

  let badge = node.querySelector(":scope > .gpt-booster-ts");
  if (!badge) {
    badge = document.createElement("div");
    badge.className = "gpt-booster-ts";
    badge.setAttribute("aria-hidden", "true");
    node.prepend(badge);
  }
  const roleLabel = role === "user" ? "USER" : "AI";
  badge.className = `gpt-booster-ts gpt-booster-ts-${role}`;
  const tz = getLocalTimeZoneLabel();
  const label =
    role === "assistant" && Number.isFinite(endTs) && endTs > ts
      ? `${roleLabel}: ${formatLocalTimestamp(ts)} -> ${formatLocalTimestamp(endTs)} ${tz}`
      : `${roleLabel}: ${formatLocalTimestamp(ts)} ${tz}`;
  badge.setAttribute("data-label", label);
  badge.textContent = label;
}

function detectRole(node) {
  const roleAttr = node.getAttribute("data-message-author-role");
  if (roleAttr) return roleAttr.toLowerCase();

  if (node.matches(selectorFor("assistant"))) return "assistant";
  const tagName = node.tagName.toLowerCase();
  const testId = node.getAttribute("data-test-id") || "";

  if (tagName === "user-query" || testId === "user-query" || node.classList.contains("user-query")) return "user";
  if (tagName === "message-content" || testId === "model-response" || node.classList.contains("model-response")) return "assistant";
  if (isGeminiPage() && node.classList.contains("message-content")) return "assistant";
  
  return null;
}

function annotateTurns() {
  if (responseSafeMode) return;
  const turns = document.querySelectorAll(selectorFor("turns"));
  turns.forEach((node) => {
    if (!(node instanceof HTMLElement)) return;
    const role = detectRole(node);
    if (!role) return;

    if (role === "user") {
      upsertVisibleTimestamp(node, "user");
      return;
    }
    if (role === "assistant") {
      upsertVisibleTimestamp(node, "assistant");
    }
  });
}

function markAssistantState(node) {
  const text = node ? extractNodeText(node) : "";
  if (node !== activeAssistant) {
    activeAssistant = node;
    lastAssistantText = text;
    lastAssistantChange = Date.now();
    return false;
  }
  if (text !== lastAssistantText) {
    lastAssistantText = text;
    lastAssistantChange = Date.now();
    return false;
  }
  return true;
}

function enableClamp() {
  if (scrollClampEnabled) return;
  window.addEventListener("scroll", clampScroll, { passive: true });
  scrollClampEnabled = true;
}

function disableClamp() {
  if (!scrollClampEnabled) return;
  window.removeEventListener("scroll", clampScroll, { passive: true });
  scrollClampEnabled = false;
}

function scheduleFinalizeCheck() {
  if (finalizeTimer) return;
  finalizeTimer = setTimeout(() => {
    finalizeTimer = null;
    handleMutations();
  }, STABLE_TEXT_MS);
}

function handleMutations() {
  refreshResponseSafeMode();
  if (mutationDebounceTimer) {
    clearTimeout(mutationDebounceTimer);
    mutationDebounceTimer = null;
  }
  if (stopActive) {
    return;
  }
  if (responseSafeMode) {
    disableClamp();
    return;
  }
  const latest = getLatestAssistant();
  annotateTurns();
  if (!latest) {
    if (!selectorMissSince) selectorMissSince = Date.now();
    if (Date.now() - selectorMissSince > 3000) {
      ensureSelectorWarning("TUFF-BRG: セレクタが一致せず監視対象を検出できません。OVERRIDE_SELECTORS を確認してください。");
    }
    return;
  }
  selectorMissSince = 0;
  clearSelectorWarning();

  const latestText = extractNodeText(latest);
  if (latestText) debugLogFragment("pick", latestText);

  if (isThinking(latest)) {
    disableClamp();
    scheduleFinalizeCheck();
    return;
  }

  const stableText = markAssistantState(latest);
  const streaming = hasStreamingCursor(latest);
  const sinceChange = Date.now() - lastAssistantChange;
  const isStable = stableText && !streaming && sinceChange >= STABLE_TEXT_MS;
  scheduleSend(latestText);

  if (isStable) {
    upsertVisibleTimestamp(latest, "assistant", { markAssistantDone: true });
    enableClamp();
  } else {
    scheduleFinalizeCheck();
  }
}

function scheduleMutationHandling() {
  if (mutationDebounceTimer) clearTimeout(mutationDebounceTimer);
  mutationDebounceTimer = setTimeout(handleMutations, 150);
}

function extractNodeText(node) {
  if (!node) return "";
  const text = (node.innerText || node.textContent || "").replace(/\s+/g, " ").trim();
  return text;
}

function clampScroll() {
  const scroller = document.scrollingElement || document.documentElement;
  if (!scroller) return;
  const current = scroller.scrollTop;
  if (lastScrollTop === null) {
    lastScrollTop = current;
    return;
  }
  const delta = current - lastScrollTop;
  if (delta < -800) {
    scroller.scrollTop = lastScrollTop - 800;
    lastScrollTop = scroller.scrollTop;
    return;
  }
  lastScrollTop = current;
}

function start() {
  if (!document.body) return;
  refreshResponseSafeMode();
  ensureTimestampStyle();
  loadAddonSettings().finally(() => {
    wsConnect();
  });
  const observer = new MutationObserver(scheduleMutationHandling);
  observer.observe(document.body, OBS_CONFIG);
  annotateTurns();
  handleMutations();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", start, { once: true });
} else {
  start();
}
