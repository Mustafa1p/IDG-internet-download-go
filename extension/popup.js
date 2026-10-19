const label = document.getElementById("status-label");
const detail = document.getElementById("connection-detail");
const statusPanel = document.querySelector("main");
async function refreshStatus() {
  try {
    const status = await chrome.runtime.sendMessage({ type: "getStatus" });
    statusPanel.className = "status " + (status.serverOnline ? "connected" : status.installed ? "ready" : "offline");
    label.textContent = status.serverOnline ? "IDG is connected" : status.installed ? "IDG is ready" : "IDG is not connected";
    detail.textContent = status.serverOnline ? `Desktop v${status.serverVersion || ""}` : status.installed ? "IDG will open when a download starts." : "Open IDG to connect.";
  } catch {
    statusPanel.className = "status offline";
    label.textContent = "IDG is not connected";
    detail.textContent = "Open IDG or reload this extension.";
  }
}
void refreshStatus();
setInterval(refreshStatus, 3000);
