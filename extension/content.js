// Direct media downloads and YouTube video-page handoff (including blob players).
(() => {
  if (window.__idgInitialized) return;
  window.__idgInitialized = true;
  const buttons = new WeakMap();
  function youtubePage() {
    try {
      if (!["www.youtube.com", "youtube.com", "m.youtube.com", "music.youtube.com", "www.youtube-nocookie.com"].includes(location.hostname)) return false;
      return (location.pathname === "/watch" && /^[A-Za-z0-9_-]{11}$/.test(new URL(location.href).searchParams.get("v") || "")) || /^\/(shorts|live|embed)\/[A-Za-z0-9_-]{11}/.test(location.pathname);
    } catch { return false; }
  }
  function attach(media) {
    if (buttons.has(media) || !media.parentElement) return;
    const button = document.createElement("button");
    button.textContent = "Download with IDG";
    button.type = "button";
    button.setAttribute("aria-label", "Download this media with IDG");
    Object.assign(button.style, {
      position: "absolute", top: "8px", right: "8px", zIndex: "2147483647",
      padding: "10px 14px", background: "#126e65", color: "#fff", border: "1px solid #60d5c2",
      borderRadius: "8px", cursor: "pointer", font: "600 13px system-ui, sans-serif",
      boxShadow: "0 4px 16px #0004",
    });
    button.addEventListener("click", async event => {
      if (!event.isTrusted) return;
      event.preventDefault(); event.stopPropagation();
      const youtube = youtubePage();
      const url = youtube ? location.href : media.currentSrc || media.src || media.querySelector("source[src]")?.src;
      if (!url || !/^https?:/.test(url)) {
        button.textContent = "This stream has no direct download";
        button.title = "Use an HTTP/HTTPS media link. YouTube watch pages are supported separately.";
        return;
      }
      button.disabled = true;
      button.textContent = "Sending to IDG…";
      try {
        const response = await chrome.runtime.sendMessage({ type: "download", url, filename: youtube ? document.title.replace(/\s*[-–]\s*YouTube$/, "").trim() + ".mp4" : null });
        button.textContent = response?.ok ? "Opened in IDG" : "IDG needs attention · retry";
        button.title = response?.error || "Choose a filename and start in IDG";
        if (!response?.ok) {
          let notice = buttons.get(media)?.notice;
          if (!notice) {
            notice = document.createElement("div");
            notice.setAttribute("role", "alert");
            Object.assign(notice.style, { position: "absolute", top: "52px", right: "8px", zIndex: "2147483647", maxWidth: "320px", padding: "12px", borderRadius: "8px", background: "#19262d", color: "#fff", font: "13px/1.5 system-ui", boxShadow: "0 4px 16px #0005" });
            media.parentElement?.appendChild(notice);
            buttons.get(media).notice = notice;
          }
          notice.textContent = response?.error || "Open IDG and retry.";
        } else { buttons.get(media)?.notice?.remove(); buttons.get(media).notice = null; }
      } catch { button.textContent = "Reload this page to reconnect"; }
      button.disabled = false;
    });
    const parent = media.parentElement;
    if (getComputedStyle(parent).position === "static") parent.style.position = "relative";
    parent.appendChild(button);
    buttons.set(media, { button, notice: null });
  }
  function scan(root) {
    if (root.matches?.("video,audio")) attach(root);
    root.querySelectorAll("video,audio").forEach(attach);
  }
  function init() {
    scan(document);
    document.addEventListener("yt-navigate-finish", () => scan(document));
    new MutationObserver(mutations => {
      for (const mutation of mutations) for (const node of mutation.addedNodes) {
        if (node.nodeType === 1 && node.tagName !== "BUTTON") scan(node);
      }
    }).observe(document.documentElement, { childList: true, subtree: true });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init, { once: true });
  else init();
})();
