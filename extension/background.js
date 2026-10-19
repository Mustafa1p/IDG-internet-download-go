/**
 * rust-idm background service worker (Manifest V3)
 *
 * Architecture mirrors IDM's IDMGCExt / IDMMsgHost design:
 *
 *  Browser network stack
 *       │  webRequest.onHeadersReceived
 *       ▼
 *  background.js  ─── nativeMessaging ──► rust-idm (native host)
 *       │                                      │
 *       │  cancel browser download             │  POST /downloads  →  parallel segments
 *       ▼                                      ▼
 *  (download suppressed)              127.0.0.1:1001 API
 *
 * Evidence from IDMGCExt.crx:
 *   - webRequest + declarativeNetRequest permissions
 *   - nativeMessaging to "com.tonec.idm"  (we use "com.rust_idm.host")
 *   - Extensive MIME-type → extension mapping (za{} in background.js)
 *   - Ka = ["127.0.0.1:1001"] local API addresses
 *   - Content-Disposition / Content-Length / Accept-Ranges header inspection
 */

const NATIVE_HOST_NAME = "com.rust_idm.host";
const API_BASE = "http://127.0.0.1:1001";

// MIME types that should always be intercepted (mirrors IDM's za{} table)
const INTERCEPT_MIME = new Set([
  "application/octet-stream",
  "application/zip", "application/x-zip", "application/x-zip-compressed",
  "application/x-rar", "application/x-rar-compressed",
  "application/x-7z-compressed",
  "application/x-tar", "application/gzip", "application/x-gzip",
  "application/pdf",
  "application/msword",
  "application/vnd.ms-excel",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  "application/x-dosexec", "application/x-msdos-program",
  "application/x-msi",
  "application/x-bittorrent", "application/torrent",
  "video/mp4", "video/x-mp4", "video/mpeg", "video/webm", "video/avi",
  "video/x-msvideo", "video/quicktime", "video/x-flv", "video/flv",
  "video/x-ms-wmv", "video/3gpp",
  "audio/mpeg", "audio/mp3", "audio/mp4", "audio/ogg", "audio/wav",
  "audio/x-ms-wma",
  "image/gif",
]);

// File extensions that should always be intercepted
const INTERCEPT_EXT = new Set([
  "exe","msi","dmg","pkg","deb","rpm","apk",
  "zip","rar","7z","tar","gz","bz2","xz","cab","iso",
  "pdf","doc","docx","xls","xlsx","ppt","pptx",
  "mp3","mp4","avi","mkv","mov","wmv","flv","webm","m4v","m4a",
  "wav","ogg","flac","aac",
  "jpg","jpeg","png","gif","webp","bmp","tiff",
  "torrent",
]);

// Minimum size (bytes) to trigger interception (avoid tiny API calls)
const MIN_SIZE = 1024 * 100; // 100 KB

let nativePort = null;
let interceptEnabled = true;

// ── Native messaging ────────────────────────────────────────────────────────

function connectNative() {
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST_NAME);
    nativePort.onDisconnect.addListener(() => {
      console.warn("[rust-idm] native host disconnected, will reconnect on next download");
      nativePort = null;
    });
  } catch (e) {
    console.error("[rust-idm] cannot connect to native host:", e.message);
  }
}

function sendToNativeHost(msg) {
  if (!nativePort) connectNative();
  if (nativePort) {
    nativePort.postMessage(msg);
  } else {
    // Fallback: hit the API directly from the extension (no native host needed).
    sendToApi(msg);
  }
}

async function sendToApi(msg) {
  if (msg.type !== "download") return;
  try {
    await fetch(`${API_BASE}/downloads`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: msg.url,
        filename: msg.filename,
        referrer: msg.referrer,
        cookies: msg.cookies,
        user_agent: msg.userAgent,
      }),
    });
  } catch (e) {
    console.error("[rust-idm] API call failed:", e);
  }
}

// ── Intercept logic ─────────────────────────────────────────────────────────

function shouldIntercept(details) {
  if (!interceptEnabled) return false;

  const ct = (details.responseHeaders || [])
    .find(h => h.name.toLowerCase() === "content-type");
  const cl = (details.responseHeaders || [])
    .find(h => h.name.toLowerCase() === "content-length");
  const cd = (details.responseHeaders || [])
    .find(h => h.name.toLowerCase() === "content-disposition");

  const mimeBase = ct ? ct.value.split(";")[0].trim().toLowerCase() : "";
  const size = cl ? parseInt(cl.value, 10) : 0;

  // Always intercept Content-Disposition: attachment
  if (cd && cd.value.toLowerCase().includes("attachment")) return true;

  // Intercept known download MIME types above minimum size
  if (INTERCEPT_MIME.has(mimeBase) && size >= MIN_SIZE) return true;

  // Intercept by file extension
  const urlPath = new URL(details.url).pathname;
  const ext = urlPath.split(".").pop().toLowerCase().split("?")[0];
  if (INTERCEPT_EXT.has(ext) && size >= MIN_SIZE) return true;

  return false;
}

function getFilenameFromHeaders(headers, url) {
  const cd = (headers || []).find(h => h.name.toLowerCase() === "content-disposition");
  if (cd) {
    const match = cd.value.match(/filename\*?=(?:UTF-8'')?["']?([^"';\n]+)/i);
    if (match) return decodeURIComponent(match[1].trim());
  }
  // Fall back to URL path
  try {
    const path = new URL(url).pathname;
    const last = path.split("/").pop();
    if (last && last.includes(".")) return decodeURIComponent(last);
  } catch (_) {}
  return null;
}

async function getCookiesForUrl(url) {
  try {
    const cookies = await chrome.cookies.getAll({ url });
    return cookies.map(c => `${c.name}=${c.value}`).join("; ") || null;
  } catch (_) {
    return null;
  }
}

// ── webRequest listener ─────────────────────────────────────────────────────

chrome.webRequest.onHeadersReceived.addListener(
  async (details) => {
    if (details.tabId < 0) return; // ignore background requests
    if (!shouldIntercept(details)) return;

    // Gather context
    const filename = getFilenameFromHeaders(details.responseHeaders, details.url);
    const cookies  = await getCookiesForUrl(details.url);
    const tab      = await chrome.tabs.get(details.tabId).catch(() => null);
    const referrer = tab ? tab.url : details.initiator || null;

    console.log(`[rust-idm] intercepting: ${details.url}`);

    sendToNativeHost({
      type:      "download",
      url:       details.url,
      filename:  filename,
      referrer:  referrer,
      cookies:   cookies,
      userAgent: navigator.userAgent,
    });

    // Cancel the browser's own download so IDM handles it exclusively.
    return { cancel: true };
  },
  { urls: ["<all_urls>"], types: ["main_frame", "sub_frame", "xmlhttprequest", "media", "object"] },
  ["responseHeaders", "blocking", "extraHeaders"]
);

// ── Context-menu "Download with rust-idm" ───────────────────────────────────

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "rust-idm-download-link",
    title: "Download with rust-idm",
    contexts: ["link"],
  });
  chrome.contextMenus.create({
    id: "rust-idm-download-image",
    title: "Download image with rust-idm",
    contexts: ["image"],
  });
  chrome.contextMenus.create({
    id: "rust-idm-download-video",
    title: "Download video with rust-idm",
    contexts: ["video", "audio"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = info.linkUrl || info.srcUrl;
  if (!url) return;

  const cookies  = await getCookiesForUrl(url);
  const referrer = tab ? tab.url : null;

  sendToNativeHost({
    type:      "download",
    url:       url,
    filename:  null,
    referrer:  referrer,
    cookies:   cookies,
    userAgent: navigator.userAgent,
  });
});

// ── Message from content scripts or popup ───────────────────────────────────

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.type === "download") {
    sendToNativeHost(msg);
    sendResponse({ status: "queued" });
  }
  if (msg.type === "toggleIntercept") {
    interceptEnabled = msg.enabled;
    sendResponse({ interceptEnabled });
  }
  if (msg.type === "getStatus") {
    sendResponse({ interceptEnabled });
  }
  return true; // keep channel open for async
});

connectNative();
console.log("[rust-idm] background service worker ready");
