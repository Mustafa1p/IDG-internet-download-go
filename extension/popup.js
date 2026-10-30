const label = document.getElementById("status-label");
const detail = document.getElementById("connection-detail");
const statusPanel = document.querySelector("main");
const siteHost = document.getElementById("site-host");
const siteLabel = document.getElementById("site-status");
const siteToggle = document.getElementById("site-toggle");
let currentTabId;
let paused = false;
function renderSite(state) {
  paused = state.paused === true;
  siteHost.textContent = state.site || "Open a website to use this control";
  siteLabel.textContent = state.site ? paused ? "Paused on this website" : state.enabled ? "Active on this website" : "IDG interception is disabled" : "";
  siteLabel.className = paused ? "paused" : "";
  siteToggle.textContent = paused ? "Resume on this website" : "Pause on this website";
  siteToggle.setAttribute("aria-pressed", String(paused));
  siteToggle.disabled = !state.site;
}
async function refreshSite() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    currentTabId = tab?.id;
    void refreshPageCheck(tab);
    renderSite(await chrome.runtime.sendMessage({ type: "getSiteStatus", tabId: currentTabId }));
  } catch {
    siteHost.textContent = "Website unavailable";
    siteToggle.disabled = true;
  }
}
siteToggle.addEventListener("click", async () => {
  siteToggle.disabled = true;
  siteLabel.textContent = "Saving…";
  try {
    const state = await chrome.runtime.sendMessage({ type: "setSitePaused", tabId: currentTabId, paused: !paused });
    if (!state?.ok) throw new Error(state?.error || "Could not save. Try again.");
    renderSite(state);
  } catch (error) {
    siteLabel.textContent = error.message;
    siteToggle.disabled = false;
  }
});
void refreshSite();
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

async function refreshPageCheck(tab) {
  const section = document.getElementById("page-check");
  if (!tab?.url?.startsWith("https://open.spotify.com/")) return;
  section.hidden = false;
  const expected = "spotify-reader-20261025";
  let page;
  try { page = await chrome.tabs.sendMessage(tab.id, { type: "getContentStatus" }, { frameId: 0 }); } catch {}
  const stale = page?.build !== expected || page?.reader !== true;
  document.getElementById("page-status").textContent = stale ? "Refresh Spotify to load the updated IDG buttons." : page.enabled ? `${page.resolvedRows} song buttons and ${page.libraryButtons} playlist buttons available.` : "IDG is paused or disabled on Spotify.";
  document.getElementById("page-reload").hidden = !stale;
  document.getElementById("page-diagnostics").textContent = JSON.stringify({ extension: chrome.runtime.id, version: chrome.runtime.getManifest().version, expected, page: page || "No current page script" }, null, 2);
}
document.getElementById("page-reload").addEventListener("click", async () => {
  await chrome.tabs.reload(currentTabId);
  window.close();
});
document.getElementById("copy-diagnostics").addEventListener("click", async event => {
  try {
    await navigator.clipboard.writeText(document.getElementById("page-diagnostics").textContent);
    event.target.textContent = "Copied";
  } catch { event.target.textContent = "Select the diagnostics text to copy"; }
});
