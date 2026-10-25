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
  "torrent","eml",
]);

// MIME types to intercept
const INTERCEPT_MIME = new Set([
  "message/rfc822",
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
let lastHealth = null;
let activeHandoffs = 0;
const settingsReady = chrome.storage.local.get({ interceptEnabled: true, statusOnlyPopup: false }).then(async settings => {
  interceptEnabled = settings.statusOnlyPopup ? settings.interceptEnabled : true;
  if (!settings.statusOnlyPopup) await chrome.storage.local.set({ interceptEnabled: true, statusOnlyPopup: true });
});

function validUrl(value) {
  try { return ["http:", "https:"].includes(new URL(value).protocol); } catch { return false; }
}

function isMailExport(value) {
  try {
    const url = new URL(value);
    return validUrl(value) && url.hostname === "mail.hostinger.com" &&
      /^\/bridge\/api\/v3\/mailboxes\/[^/]+\/messages\/sources\/export$/.test(url.pathname);
  } catch { return false; }
}

function isTokenDownload(value) {
  try {
    const url = new URL(value);
    return /^[a-z0-9-]+\.safefilehosting\.com$/i.test(url.hostname) && url.pathname === "/files/download.php";
  } catch { return false; }
}

// Never convert an unrecognized browser POST download into a tokenless GET.
// This stores only method/URL metadata, not form contents or credentials.
const browserPosts = new Map();
chrome.webRequest.onBeforeRequest.addListener(details => {
  if (details.method !== "POST" || details.tabId < 0) return;
  browserPosts.delete(details.url);
  browserPosts.set(details.url, Date.now());
  for (const [url, time] of browserPosts) {
    if (Date.now() - time > 120000 || browserPosts.size > 256) browserPosts.delete(url);
  }
}, { urls: ["<all_urls>"], types: ["main_frame", "sub_frame", "other"] });
function browserPosted(url) {
  return Date.now() - (browserPosts.get(url) || 0) < 120000;
}

// Some webmail sessions use authorization/CSRF headers as well as cookies.
// Keep only the exact export request's credentials, in memory for two minutes.
const exportRequests = new Map();
chrome.webRequest.onBeforeSendHeaders.addListener(details => {
  if (!isMailExport(details.url) || details.method !== "GET" || details.tabId < 0) return;
  const headers = {};
  for (const header of details.requestHeaders || []) {
    const name = header.name.toLowerCase();
    if (["authorization", "cookie", "referer", "origin", "user-agent", "x-csrf-token", "x-xsrf-token", "x-requested-with"].includes(name) && typeof header.value === "string") headers[name] = header.value;
  }
  exportRequests.delete(details.url);
  exportRequests.set(details.url, { headers, time: Date.now() });
  for (const [url, request] of exportRequests) {
    if (Date.now() - request.time > 120000 || exportRequests.size > 64) exportRequests.delete(url);
  }
}, { urls: ["https://mail.hostinger.com/bridge/api/v3/mailboxes/*/messages/sources/export*", "http://mail.hostinger.com/bridge/api/v3/mailboxes/*/messages/sources/export*"] }, ["requestHeaders", "extraHeaders"]);

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
    const valid = response.ok && health.status === "ok" && typeof health.version === "string";
    lastHealth = valid ? health : null;
    return valid;
  } catch { return false; }
}

async function reportIntegration() {
  try {
    const self = await chrome.management.getSelf();
    const response = await fetch(`${API_BASE}/extension/heartbeat`, {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ version: chrome.runtime.getManifest().version, install_type: self.installType }),
      signal: AbortSignal.timeout(4000),
    });
    if (response.ok) {
      const update = await response.json();
      if (update.reload === true && self.installType === "development" && activeHandoffs === 0) {
        const disk = await fetch(chrome.runtime.getURL("manifest.json"), { cache: "no-store" });
        const manifest = await disk.json();
        if (manifest.version === update.version && chrome.runtime.getManifest().version !== update.version && activeHandoffs === 0) chrome.runtime.reload();
      }
    }
  } catch { /* IDG may be closed; polling never launches it. */ }
}
chrome.alarms.onAlarm.addListener(alarm => {
  if (alarm.name === "idg-integration") void reportIntegration();
});
chrome.alarms.create("idg-integration", { periodInMinutes: 1 });
chrome.runtime.onStartup.addListener(() => void reportIntegration());
void reportIntegration();

async function updateServerStatus() {
  serverOnline = await pingServer();
  if (serverOnline) await reportIntegration();
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
  return { interceptEnabled, serverOnline, installed, serverVersion: lastHealth?.version, youtubeReady: lastHealth?.capabilities?.youtube === true, error: connectionError };
}

function shouldIntercept(url, mime, filesize, filename = "") {
  if (!interceptEnabled || !validUrl(url)) return false;
  if (isMailExport(url) || mime?.split(";")[0].trim().toLowerCase() === "message/rfc822") return true;
  if (mime && INTERCEPT_MIME.has(mime.split(";")[0].trim().toLowerCase()) &&
      (!filesize || filesize < 0 || filesize >= MIN_SIZE_BYTES)) return true;
  return [new URL(url).pathname, filename].some(name => INTERCEPT_EXT.has(name.split(".").pop().toLowerCase()));
}

function youtubeVideoUrl(value) {
  try {
    const url = new URL(value);
    let id;
    if (url.hostname === "youtu.be") id = url.pathname.slice(1);
    else if (["youtube.com", "www.youtube.com", "m.youtube.com", "music.youtube.com", "www.youtube-nocookie.com"].includes(url.hostname)) {
      if (url.pathname === "/watch") id = url.searchParams.get("v");
      else if (/^\/(shorts|embed|live)\//.test(url.pathname)) id = url.pathname.split("/")[2];
    }
    return id && /^[A-Za-z0-9_-]{11}$/.test(id) ? `https://www.youtube.com/watch?v=${id}` : null;
  } catch { return null; }
}

function nativeError(message) {
  if (/not found|missing|forbidden|access.*denied/i.test(message)) {
    return "IDG browser integration is not registered. Install the full IDG package, open IDG once, then reload this extension.";
  }
  return message;
}

async function sendDownloadToIDG(...args) {
  activeHandoffs++;
  try { return await sendDownloadRequest(...args); }
  finally { activeHandoffs--; }
}
function xVideoUrl(value) {
  try {
    const url = new URL(value);
    if (!["http:", "https:"].includes(url.protocol) || !["x.com", "www.x.com", "twitter.com", "www.twitter.com", "mobile.twitter.com", "m.twitter.com"].includes(url.hostname)) return null;
    const match = url.pathname.match(/^\/(?:[A-Za-z0-9_]+|i\/web)\/status\/([0-9]+)(?:\/video\/([0-9]+))?\/?$/);
    if (!match || (match[2] && !(Number(match[2]) > 0 && Number(match[2]) <= 4294967295))) return null;
    return `https://x.com${url.pathname.replace(/\/$/, "")}`;
  } catch { return null; }
}
async function sendDownloadRequest(url, filename, referrer, userAgent, postData) {
  if (!validUrl(url)) return { ok: false, error: "Only HTTP and HTTPS links can be sent to IDG." };
  const youtube = youtubeVideoUrl(url);
  const x = xVideoUrl(url);
  if (youtube || x) url = youtube || x;
  let cookies = null, media_cookies = null;
  try {
    const jar = await chrome.cookies.getAll({ url });
    if (jar.length) {
      cookies = jar.map(cookie => `${cookie.name}=${cookie.value}`).join("; ");
      if (youtube || x) {
        const field = value => String(value ?? "").replace(/[\t\r\n]/g, "");
        media_cookies = "# Netscape HTTP Cookie File\n" + jar.map(cookie => [
          (cookie.httpOnly ? "#HttpOnly_" : "") + field(cookie.domain),
          cookie.domain?.startsWith(".") ? "TRUE" : "FALSE", field(cookie.path || "/"),
          cookie.secure ? "TRUE" : "FALSE", Math.floor(cookie.expirationDate || 0), field(cookie.name), field(cookie.value),
        ].join("\t")).join("\n") + "\n";
      }
    }
  } catch {}
  const download = { url, filename, referrer, cookies, media_cookies, user_agent: userAgent, show_dialog: true };
  if (typeof postData === "string") {
    if (postData.length > 65536) return { ok: false, error: "Download form is too large." };
    download.post_data = postData;
  }
  const captured = exportRequests.get(url);
  if (isMailExport(url) && captured && Date.now() - captured.time <= 120000) {
    download.headers = captured.headers;
    exportRequests.delete(url);
  }
  // Never retry a POST using another transport: a lost response could duplicate it.
  const online = await pingServer();
  try {
    if (download.post_data !== undefined) {
      const capability = online ? lastHealth : await nativeRequest({ type: "ping" });
      if (!capability?.capabilities?.post_downloads) return { ok: false, error: "POST downloads require IDG 1.4.4 or newer." };
    }
    if (youtube || x) {
      const capability = online ? lastHealth : await nativeRequest({ type: "ping" });
      if (x && !capability?.capabilities?.x) {
        return { ok: false, error: "X videos require the complete IDG v1.4.6 package or newer. Quit IDG in the tray, install the update, then reopen IDG." };
      }
      if (!capability?.capabilities?.youtube) {
        return { ok: false, error: "YouTube requires the complete IDG v1.2.0 package or newer. Quit the old IDG in the tray, install the update, then reopen IDG." };
      }
    }
    if (online) {
      const response = await fetch(`${API_BASE}/downloads`, {
        method: "POST", headers: { "Content-Type": "application/json" },
        body: JSON.stringify(download), signal: AbortSignal.timeout(5000),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => ({}));
        throw new Error(body.error || `IDG returned HTTP ${response.status}`);
      }
    } else {
      await nativeRequest({ type: "download", download });
    }
    serverOnline = true;
    return { ok: true };
  } catch (error) {
    connectionError = nativeError(error.message);
    return { ok: false, error: connectionError };
  }
}

// PHP/redirect downloads may acquire their MIME type and filename after onCreated.
// Mark the ID before pausing: onChanged also fires during our own handoff.
const attemptedDownloads = new Set();
async function interceptDownload(item) {
  await settingsReady;
  const url = item.finalUrl || item.url;
  if (item.state === "complete" || item.state === "interrupted" || attemptedDownloads.has(item.id) ||
      // A native fallback has already submitted this token. Never replay it as GET.
      isTokenDownload(item.url) || isTokenDownload(url) || browserPosted(item.url) || browserPosted(url) ||
      item.byExtensionId === chrome.runtime.id || !shouldIntercept(url, item.mime, item.fileSize, item.filename)) return;
  attemptedDownloads.add(item.id);
  if (attemptedDownloads.size > 1024) attemptedDownloads.delete(attemptedDownloads.values().next().value);
  activeHandoffs++;
  try {
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
  } finally { activeHandoffs--; }
}
chrome.downloads.onCreated.addListener(interceptDownload);
chrome.downloads.onChanged.addListener(async delta => {
  if (attemptedDownloads.has(delta.id) ||
      !["filename", "mime", "fileSize", "totalBytes", "finalUrl", "url"].some(key => key in delta)) return;
  try {
    const [item] = await chrome.downloads.search({ id: delta.id });
    if (item) await interceptDownload(item);
  } catch (error) { console.warn("[IDG] Could not inspect updated download", error); }
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
  const url = youtubeVideoUrl(info.linkUrl) || xVideoUrl(info.linkUrl) ||
    (info.menuItemId === "idg-download-video" ? youtubeVideoUrl(tab?.url) || xVideoUrl(tab?.url) : null) || info.linkUrl || info.srcUrl;
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
        return sendDownloadToIDG(message.url, typeof message.filename === "string" ? message.filename : null, sender.tab ? sender.url : message.referrer, navigator.userAgent, message.post_data);
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
