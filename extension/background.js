/**
 * IDG – Internet Download GO — browser extension background service worker
 *
 * Sends downloads over the loopback API, using native messaging to launch IDG when closed.
 *
 * MV3-compatible approach:
 *  1. chrome.downloads.onCreated  — catches all browser download starts
 *  2. Pauses the browser download while IDG acknowledges the handoff
 *  3. Cancels only after acceptance; resumes on failure
 */

const API_BASE        = "http://127.0.0.1:17843";
const MIN_SIZE_BYTES  = 1024 * 100;   // ignore files < 100 KB

// File extensions that should always be intercepted
const INTERCEPT_EXT = new Set([
  "exe","msi","dmg","pkg","deb","rpm","apk",
  "zip","rar","7z","tar","gz","bz2","xz","cab","iso",
  "pdf","doc","docx","xls","xlsx","ppt","pptx",
  "mp3","mp4","avi","mkv","mov","wmv","flv","webm","m4v","m4a",
  "wav","ogg","flac","aac","mp2","wma",
  "torrent",
]);

// MIME types to intercept
const INTERCEPT_MIME = new Set([
  "application/octet-stream",
  "application/zip","application/x-zip","application/x-zip-compressed",
  "application/x-rar","application/x-rar-compressed","application/x-7z-compressed",
  "application/x-tar","application/gzip","application/x-gzip",
  "application/pdf","application/msword",
  "application/x-dosexec","application/x-msdos-program","application/x-msi",
  "application/x-bittorrent","application/torrent",
  "video/mp4","video/x-mp4","video/mpeg","video/webm","video/avi",
  "video/x-msvideo","video/quicktime","video/x-flv","video/flv",
  "video/x-ms-wmv","video/3gpp","video/mkv",
  "audio/mpeg","audio/mp3","audio/mp4","audio/ogg","audio/wav","audio/x-ms-wma",
]);

let interceptEnabled = true;
let serverOnline = false;
let installed = false;
let connectionError = "";
const settingsReady = chrome.storage.local.get({ interceptEnabled: true }).then(settings => {
  interceptEnabled = settings.interceptEnabled;
});

function validUrl(value) {
  try { return ["http:", "https:"].includes(new URL(value).protocol); } catch { return false; }
}

function nativeRequest(message) {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendNativeMessage("com.rust_idm.host", message, response => {
      const error = chrome.runtime.lastError;
      if (error) reject(new Error(error.message));
      else if (!response?.ok) reject(new Error(response?.error || "IDG did not accept the request"));
      else resolve(response);
    });
  });
}

async function pingServer() {
  try {
    const response = await fetch(`${API_BASE}/health`, { signal: AbortSignal.timeout(1500) });
    const health = await response.json();
    return response.ok && health.status === "ok" && typeof health.version === "string";
  } catch { return false; }
}

async function updateServerStatus() {
  serverOnline = await pingServer();
  try {
    const response = await nativeRequest({ type: "ping" });
    installed = response.installed;
    connectionError = "";
  } catch (error) {
    installed = false;
    connectionError = error.message;
  }
  await chrome.action.setBadgeText({ text: serverOnline || installed ? "" : "OFF" });
  await chrome.action.setBadgeBackgroundColor({ color: "#dc5662" });
  return { interceptEnabled, serverOnline, installed, error: connectionError };
}

function shouldIntercept(url, mime, filesize) {
  if (!interceptEnabled || !validUrl(url)) return false;
  if (mime && INTERCEPT_MIME.has(mime.split(";")[0].trim().toLowerCase()) &&
      (!filesize || filesize < 0 || filesize >= MIN_SIZE_BYTES)) return true;
  return INTERCEPT_EXT.has(new URL(url).pathname.split(".").pop().toLowerCase());
}

async function sendDownloadToIDG(url, filename, referrer, userAgent) {
  if (!validUrl(url)) return { ok: false, error: "Only HTTP and HTTPS links can be sent to IDG." };
  let cookies = null;
  try {
    const jar = await chrome.cookies.getAll({ url });
    if (jar.length) cookies = jar.map(cookie => `${cookie.name}=${cookie.value}`).join("; ");
  } catch {}
  const download = { url, filename, referrer, cookies, user_agent: userAgent, show_dialog: true };
  // Never retry a POST using another transport: a lost response could duplicate it.
  const online = await pingServer();
  try {
    if (online) {
      const response = await fetch(`${API_BASE}/downloads`, {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify(download), signal: AbortSignal.timeout(5000),
      });
      if (!response.ok) throw new Error(`IDG returned HTTP ${response.status}`);
    } else {
      await nativeRequest({ type: "download", download });
    }
    serverOnline = true;
    return { ok: true };
  } catch (error) {
    connectionError = error.message;
    return { ok: false, error: error.message };
  }
}

chrome.downloads.onCreated.addListener(async item => {
  await settingsReady;
  const url = item.finalUrl || item.url;
  if (item.byExtensionId === chrome.runtime.id || !shouldIntercept(url, item.mime, item.fileSize)) return;
  // Leave the original browser download intact if IDG cannot be reached.
  let paused = false;
  try { await chrome.downloads.pause(item.id); paused = true; } catch { return; }
  const result = await sendDownloadToIDG(url, item.filename || null, item.referrer || null, navigator.userAgent);
  if (result.ok) {
    try {
      await chrome.downloads.cancel(item.id);
      await chrome.downloads.erase({ id: item.id });
    } catch (error) { console.warn("[IDG] Could not clear browser download", error); }
  } else if (paused) {
    try { await chrome.downloads.resume(item.id); } catch (error) { console.warn("[IDG] Resume the download in Chrome", error); }
  }
});

chrome.runtime.onInstalled.addListener(async () => {
  await chrome.contextMenus.removeAll();
  for (const [id, title, contexts] of [
    ["idg-download-link", "Download with IDG", ["link"]],
    ["idg-download-image", "Download image with IDG", ["image"]],
    ["idg-download-video", "Download media with IDG", ["video", "audio"]],
  ]) chrome.contextMenus.create({ id, title, contexts });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  const result = await sendDownloadToIDG(url, null, info.pageUrl || tab?.url || null, navigator.userAgent);
  if (!result.ok) {
    await chrome.action.setBadgeText({ text: "!" });
    await chrome.action.setTitle({ title: `IDG: ${result.error}` });
  }
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  (async () => {
    await settingsReady;
    // Content scripts can request downloads only; settings and list access belong to the popup.
    if (sender.tab && message.type !== "download") return { ok: false };
    switch (message.type) {
      case "getStatus":
      case "ping": return updateServerStatus();
      case "toggleIntercept":
        interceptEnabled = message.enabled === true;
        await chrome.storage.local.set({ interceptEnabled });
        return { interceptEnabled };
      case "download":
        if (sender.tab && !interceptEnabled) return { ok: false, error: "IDG interception is disabled." };
        return sendDownloadToIDG(message.url, null, sender.tab ? sender.url : message.referrer, navigator.userAgent);
      case "list": {
        // Polling must not start the desktop app.
        if (!await pingServer()) return { ok: false, error: "Open IDG to view your downloads." };
        const response = await fetch(`${API_BASE}/downloads`, { signal: AbortSignal.timeout(2000) });
        return { ok: response.ok, data: await response.json() };
      }
      default: return { ok: false, error: "Unknown request" };
    }
  })().then(sendResponse).catch(error => sendResponse({ ok: false, error: error.message }));
  return true;
});
