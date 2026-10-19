/**
 * rust-idm content script
 *
 * Injected into every page at document_start.  Monitors <a>, <video>,
 * <audio> and <source> elements so the extension can show a download
 * button overlay – similar to IDM's "Download This Video" button.
 */

(function () {
  "use strict";

  if (window.__rustIdmInit__) return;
  window.__rustIdmInit__ = true;

  const DOWNLOAD_EXTS = new Set([
    "exe","msi","zip","rar","7z","tar","gz","bz2","iso","dmg","pkg","deb","rpm","apk",
    "pdf","doc","docx","xls","xlsx","ppt","pptx",
    "mp3","mp4","avi","mkv","mov","wmv","flv","webm","m4v","m4a",
    "wav","ogg","flac","aac",
    "torrent",
  ]);

  function extOf(url) {
    try {
      const path = new URL(url, location.href).pathname;
      return path.split(".").pop().toLowerCase();
    } catch (_) { return ""; }
  }

  function isDownloadable(url) {
    return DOWNLOAD_EXTS.has(extOf(url));
  }

  /** Send a download request to the background service worker. */
  function requestDownload(url) {
    chrome.runtime.sendMessage({
      type:      "download",
      url:       url,
      referrer:  location.href,
      userAgent: navigator.userAgent,
    });
  }

  /** Attach a floating download button to a media element. */
  function attachButton(el, url) {
    if (el.__rustIdmBtn__) return;
    el.__rustIdmBtn__ = true;

    const btn = document.createElement("button");
    btn.textContent = "⬇ rust-idm";
    btn.title = "Download with rust-idm";
    Object.assign(btn.style, {
      position: "absolute",
      top: "8px",
      right: "8px",
      zIndex: "2147483647",
      padding: "4px 10px",
      background: "#1976d2",
      color: "#fff",
      border: "none",
      borderRadius: "4px",
      cursor: "pointer",
      fontSize: "12px",
      fontFamily: "sans-serif",
      boxShadow: "0 2px 6px rgba(0,0,0,.4)",
    });

    btn.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      requestDownload(url);
    });

    // Make the parent relatively positioned so the button is anchored.
    const parent = el.parentElement;
    if (parent) {
      const pos = getComputedStyle(parent).position;
      if (pos === "static") parent.style.position = "relative";
      parent.appendChild(btn);
    }
  }

  /** Scan existing elements and intercept future ones. */
  function scan(root) {
    // <a href="*.zip"> links
    root.querySelectorAll("a[href]").forEach((a) => {
      if (isDownloadable(a.href)) attachButton(a, a.href);
    });

    // <video src>, <audio src>, <source src>
    root.querySelectorAll("video[src], audio[src], source[src]").forEach((el) => {
      const src = el.src || el.getAttribute("src");
      if (src && isDownloadable(src)) attachButton(el.closest("video,audio") || el, src);
    });
  }

  // Run once DOM is interactive.
  function init() {
    scan(document);

    // Watch for dynamically added elements (SPA sites).
    const observer = new MutationObserver((mutations) => {
      for (const m of mutations) {
        for (const node of m.addedNodes) {
          if (node.nodeType === 1) scan(node);
        }
      }
    });
    observer.observe(document.body || document.documentElement, {
      childList: true,
      subtree: true,
    });
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
