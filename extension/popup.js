/**
 * rust-idm popup script
 * Communicates with background.js and the local API server.
 */

const API_BASE = "http://127.0.0.1:1001";
const LIST_INTERVAL = 2000; // ms

// ── Toggle intercept ────────────────────────────────────────────────────────
const toggle = document.getElementById("toggleIntercept");

chrome.runtime.sendMessage({ type: "getStatus" }, (resp) => {
  if (resp) toggle.checked = resp.interceptEnabled;
});

toggle.addEventListener("change", () => {
  chrome.runtime.sendMessage({ type: "toggleIntercept", enabled: toggle.checked });
});

// ── Manual URL form ─────────────────────────────────────────────────────────
document.getElementById("add-btn").addEventListener("click", async () => {
  const url = document.getElementById("manual-url").value.trim();
  if (!url) return;

  try {
    const resp = await fetch(`${API_BASE}/downloads`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url }),
    });
    if (resp.ok) {
      document.getElementById("manual-url").value = "";
      await refreshList();
    } else {
      alert("Failed to add download: " + resp.status);
    }
  } catch (e) {
    alert("Cannot reach rust-idm server.\nMake sure rust-idm server is running.");
  }
});

// ── Downloads list ──────────────────────────────────────────────────────────
async function refreshList() {
  const container = document.getElementById("downloads-list");
  try {
    const resp = await fetch(`${API_BASE}/downloads`);
    if (!resp.ok) throw new Error(resp.status);
    const list = await resp.json();

    if (!list.length) {
      container.innerHTML = '<p style="opacity:.5;font-size:12px;padding:4px;">No active downloads.</p>';
      return;
    }

    // Sort by creation date, newest first
    list.sort((a, b) => new Date(b.created_at) - new Date(a.created_at));

    container.innerHTML = list.map(dl => {
      const name = dl.filename || dl.url.split("/").pop().split("?")[0] || "unknown";
      const pct  = dl.total_bytes ? Math.round(dl.bytes_downloaded / dl.total_bytes * 100) : 0;
      const speed = formatSpeed(dl.speed_bps);
      const total = dl.total_bytes ? formatBytes(dl.total_bytes) : "?";
      const done  = formatBytes(dl.bytes_downloaded);
      const stateClass = `state-${dl.state}`;

      return `
        <div class="dl-item">
          <div class="name" title="${dl.url}">${name}</div>
          <progress max="100" value="${pct}"></progress>
          <div class="meta">
            <span>${done} / ${total}</span>
            <span>${speed}</span>
            <span class="state-badge ${stateClass}">${dl.state}</span>
          </div>
        </div>`;
    }).join("");
  } catch (_) {
    container.innerHTML = '<p style="opacity:.5;font-size:12px;padding:4px;color:#f88;">Server offline</p>';
  }
}

function formatBytes(n) {
  if (n >= 1e9) return (n / 1e9).toFixed(2) + " GB";
  if (n >= 1e6) return (n / 1e6).toFixed(2) + " MB";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + " KB";
  return n + " B";
}

function formatSpeed(bps) {
  if (!bps) return "";
  return formatBytes(bps) + "/s";
}

refreshList();
setInterval(refreshList, LIST_INTERVAL);
