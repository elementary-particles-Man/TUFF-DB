const input = document.getElementById("tuff-web-base");
const status = document.getElementById("status");
const save = document.getElementById("save");

function load() {
  chrome.storage.local.get(["TUFF_WEB_BASE"], (res) => {
    if (res && typeof res.TUFF_WEB_BASE === "string") {
      input.value = res.TUFF_WEB_BASE;
    }
  });
}

function normalizeUrl(raw) {
  const val = (raw || "").trim();
  if (!val) return "";
  try {
    const url = new URL(val);
    return url.toString().replace(/\/$/, "");
  } catch (_) {
    return "";
  }
}

save.addEventListener("click", () => {
  const normalized = normalizeUrl(input.value);
  if (!normalized) {
    status.textContent = "URL が無効です";
    return;
  }
  chrome.storage.local.set({ TUFF_WEB_BASE: normalized }, () => {
    status.textContent = "保存しました";
  });
});

load();
