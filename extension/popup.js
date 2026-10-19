const toggle = document.getElementById("toggleIntercept");
const feedback = document.getElementById("feedback");
async function refreshStatus() {
  try {
    const status = await chrome.runtime.sendMessage({ type: "getStatus" });
    toggle.checked = status.interceptEnabled;
    document.getElementById("connection-status").textContent = status.serverOnline ? "IDG is connected" : status.installed ? "IDG is ready" : "IDG is not connected";
    document.getElementById("connection-detail").textContent = status.serverOnline ? "Downloads open in the desktop app." : status.installed ? "The app will open when you send a download." : "Open IDG, or reinstall it to register browser integration. " + (status.error || "");
  } catch { feedback.textContent = "Reload the extension to reconnect."; }
}
toggle.addEventListener("change", async () => {
  await chrome.runtime.sendMessage({ type: "toggleIntercept", enabled: toggle.checked });
});
document.getElementById("url-form").addEventListener("submit", async event => {
  event.preventDefault();
  const input = document.getElementById("manual-url");
  const button = document.getElementById("add-btn");
  button.disabled = true;
  feedback.textContent = "Opening the IDG download window…";
  try {
    const result = await chrome.runtime.sendMessage({ type: "download", url: input.value.trim() });
    feedback.textContent = result.ok ? "Ready. Start your download in IDG." : result.error;
    if (result.ok) input.value = "";
    await refreshStatus();
    await refreshList();
  } catch { feedback.textContent = "Could not connect. Open IDG and retry."; }
  finally { button.disabled = false; }
});
function formatBytes(n) {
  n = Number(n) || 0;
  if (n >= 1e9) return (n / 1e9).toFixed(2) + " GB";
  if (n >= 1e6) return (n / 1e6).toFixed(2) + " MB";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + " KB";
  return n + " B";
}
function formatSpeed(bytesPerSecond) { return ((Number(bytesPerSecond) || 0) * 8 / 1e6).toFixed(2) + " Mbps"; }
async function refreshList() {
  const container = document.getElementById("downloads-list");
  try {
    const result = await chrome.runtime.sendMessage({ type: "list" });
    container.replaceChildren();
    if (!result.ok || !result.data.length) {
      const empty = document.createElement("p"); empty.className = "empty";
      empty.textContent = result.ok ? "Your next download starts here." : "Open IDG to view recent downloads.";
      container.append(empty); return;
    }
    for (const download of result.data.slice().reverse().slice(0, 20)) {
      const item = document.createElement("div"); item.className = "dl-item";
      const name = document.createElement("div"); name.className = "name";
      name.textContent = download.filename || "Download"; name.title = download.url;
      const progress = document.createElement("progress"); progress.max = 100;
      progress.value = download.total_bytes ? Math.min(100, download.downloaded_bytes / download.total_bytes * 100) : 0;
      progress.setAttribute("aria-label", `${name.textContent} progress`);
      const meta = document.createElement("div"); meta.className = "meta";
      for (const text of [formatBytes(download.downloaded_bytes) + " / " + (download.total_bytes == null ? "Unknown size" : formatBytes(download.total_bytes)), formatSpeed(download.speed_bps), download.status]) {
        const span = document.createElement("span"); span.textContent = text; meta.append(span);
      }
      item.append(name, progress, meta); container.append(item);
    }
  } catch { container.textContent = "Could not load downloads."; }
}
refreshStatus(); refreshList();
setInterval(() => { refreshStatus(); refreshList(); }, 3000);
