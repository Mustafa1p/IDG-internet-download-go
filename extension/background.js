/**
 * IDG – Internet Download GO — browser extension background service worker
 *
 * Intercepts downloads and sends them to the IDG desktop app via its local
 * HTTP API on http://127.0.0.1:1001.
 *
 * MV3-compatible approach:
 *  1. chrome.downloads.onCreated  — catches all browser download starts
 *  2. Cancels the browser download
 *  3. POSTs the URL to IDG's local API
 */

const API_BASE        = "http://127.0.0.1:1001";
const MIN_SIZE_BYTES  = 1024 * 100;   // ignore files < 100 KB
const CONNECT_RETRY   = 5_000;        // ms between connection retries

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
let serverOnline     = false;

// ── Connectivity ─────────────────────────────────────────────────────────────

async function pingServer() {
  try {
    const r = await fetch(`${API_BASE}/health`, {
      signal: AbortSignal.timeout(2000)
    });
    return r.ok;
  } catch {
    return false;
  }
}

async function updateServerStatus() {
  serverOnline = await pingServer();
  // Update extension badge
  chrome.action.setBadgeText({ text: serverOnline ? "" : "OFF" });
  chrome.action.setBadgeBackgroundColor({ color: "#c62828" });
}

// Check on startup and every 5 seconds
updateServerStatus();
setInterval(updateServerStatus, CONNECT_RETRY);

// ── Intercept logic ───────────────────────────────────────────────────────────

function shouldIntercept(url, mime, filesize) {
  if (!interceptEnabled) return false;

  // Always intercept Content-Disposition attachment via mime signal
  if (mime && INTERCEPT_MIME.has(mime.split(";")[0].trim().toLowerCase())) {
    if (!filesize || filesize >= MIN_SIZE_BYTES) return true;
  }

  // Check by extension
  try {
    const path = new URL(url).pathname.split("?")[0];
    const ext  = path.split(".").pop().toLowerCase();
    if (INTERCEPT_EXT.has(ext)) return true;
  } catch {}

  return false;
}

async function sendDownloadToIDG(url, filename, referrer, cookies) {
  if (!serverOnline) {
    serverOnline = await pingServer();
    if (!serverOnline) {
      console.warn("[IDG ext] server offline — cannot intercept:", url);
      return false;
    }
  }

  try {
    const resp = await fetch(`${API_BASE}/downloads`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ url, filename, referrer, cookies }),
      signal:  AbortSignal.timeout(3000),
    });
    return resp.ok || resp.status === 201;
  } catch (e) {
    console.error("[IDG ext] API call failed:", e);
    serverOnline = false;
    return false;
  }
}

// ── chrome.downloads.onCreated (MV3-compatible interception) ─────────────────
//
// In Manifest V3, blocking webRequest is not allowed for extensions.
// The correct approach is to listen for the download starting, cancel it
// immediately, and send it to IDG instead.

chrome.downloads.onCreated.addListener(async (item) => {
  if (!interceptEnabled) return;
  if (!shouldIntercept(item.url, item.mime, item.fileSize)) return;

  console.log("[IDG ext] intercepting download:", item.url);

  // Cancel the browser's download
  chrome.downloads.cancel(item.id, () => {
    // Erase the entry from the download shelf
    chrome.downloads.erase({ id: item.id });
  });

  // Get cookies for this URL
  let cookies = null;
  try {
    const jar    = await chrome.cookies.getAll({ url: item.url });
    if (jar.length) cookies = jar.map(c => `${c.name}=${c.value}`).join("; ");
  } catch {}

  // Get referrer from current tab
  let referrer = null;
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (tab) referrer = tab.url;
  } catch {}

  const sent = await sendDownloadToIDG(item.url, item.filename || null, referrer, cookies);
  if (!sent) {
    // IDG unavailable — re-open the download in browser as fallback
    chrome.downloads.download({ url: item.url });
  }
});

// ── Context menu ─────────────────────────────────────────────────────────────

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "idg-download-link",  title: "Download with IDG", contexts: ["link"],
  });
  chrome.contextMenus.create({
    id: "idg-download-image", title: "Download image with IDG", contexts: ["image"],
  });
  chrome.contextMenus.create({
    id: "idg-download-video", title: "Download video with IDG", contexts: ["video","audio"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;
  let cookies  = null;
  try {
    const jar = await chrome.cookies.getAll({ url });
    if (jar.length) cookies = jar.map(c => `${c.name}=${c.value}`).join("; ");
  } catch {}
  await sendDownloadToIDG(url, null, tab?.url ?? null, cookies);
});

// ── Messages from popup ───────────────────────────────────────────────────────

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  (async () => {
    switch (msg.type) {
      case "getStatus":
        sendResponse({ interceptEnabled, serverOnline });
        break;
      case "toggleIntercept":
        interceptEnabled = msg.enabled;
        sendResponse({ interceptEnabled });
        break;
      case "download":
        const ok = await sendDownloadToIDG(msg.url, null, msg.referrer ?? null, null);
        sendResponse({ ok });
        break;
      case "ping":
        serverOnline = await pingServer();
        sendResponse({ serverOnline });
        break;
    }
  })();
  return true; // keep channel open for async
});

console.log("[IDG ext] background service worker ready");
