// Compact, dismissible media handoff. Dismissal lasts for this page session.
(() => {
  if (window.__idgInitialized) return;
  window.__idgInitialized = true;
  document.addEventListener("idg-post-submit", event => {
    const form = event.target;
    const request = globalThis.__idgReadDownloadForm(form);
    if (!request) return;
    event.preventDefault(); event.stopImmediatePropagation();
    (async () => {
      let accepted = false;
      try {
        const response = await chrome.runtime.sendMessage({ type: "download", ...request });
        accepted = response?.ok === true;
      } catch { /* Restore the original form submission if integration fails. */ }
      form.dispatchEvent(new CustomEvent("idg-post-result", { detail: accepted ? "accepted" : "fallback" }));
    })();
  }, true);
  // Delegation also catches buttons inserted after a site's waiting countdown.
  // Opaque scripts/forms must run in the page so their tokens and redirects work.
  document.addEventListener("click", async event => {
    if (!event.isTrusted || event.defaultPrevented || event.button !== 0 ||
        event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const control = event.target.closest?.("#button_dm");
    if (!control || control.disabled || control.getAttribute("aria-disabled") === "true") return;
    const script = control.getAttribute("onclick");
    const directScript = script?.match(/^\s*(?:(?:window\.)?location(?:\.href)?\s*=\s*|(?:window\.)?location\.(?:assign|replace)\(\s*)["']([^"']+)["']\s*\)?\s*;?\s*(?:return\s+false\s*;?)?\s*$/);
    if (script && !directScript) return;
    const value = directScript?.[1] || control.getAttribute("href") || control.getAttribute("data-href") || control.getAttribute("data-url");
    if (!value || value.startsWith("#")) return;
    let url;
    try { url = new URL(value, document.baseURI); } catch { return; }
    if (!["http:", "https:"].includes(url.protocol) || url.href === location.href ||
        /\/(?:waiting-resource\.php|your-file-is-loading-please-wait)\/?$/i.test(url.pathname)) return;
    event.preventDefault(); event.stopImmediatePropagation();
    try {
      const response = await chrome.runtime.sendMessage({ type: "download", url: url.href, filename: control.getAttribute("download") || null });
      if (response?.ok) return;
    } catch { /* Keep the browser download available if IDG is unavailable. */ }
    location.assign(url.href);
  }, true);
  const buttons = new WeakMap();
  const dismissed = new Set();
  const watched = new WeakSet();
  function youtubePage() {
    try {
      if (!["www.youtube.com", "youtube.com", "m.youtube.com", "music.youtube.com", "www.youtube-nocookie.com"].includes(location.hostname)) return false;
      return (location.pathname === "/watch" && /^[A-Za-z0-9_-]{11}$/.test(new URL(location.href).searchParams.get("v") || "")) || /^\/(shorts|live|embed)\/[A-Za-z0-9_-]{11}/.test(location.pathname);
    } catch { return false; }
  }
  function mediaKey(media) {
    const social = socialPage(media);
    if (social) return social;
    const x = xPage(media);
    if (x) return x;
    if (youtubePage()) {
      const url = new URL(location.href);
      return "youtube:" + (url.searchParams.get("v") || url.pathname);
    }
    return media.currentSrc || media.src || media.querySelector("source[src]")?.src || location.href;
  }
  function xPage(media) {
    if (!["x.com", "www.x.com", "twitter.com", "www.twitter.com", "mobile.twitter.com", "m.twitter.com"].includes(location.hostname)) return null;
    const postUrl = value => {
      try {
        const url = new URL(value, location.href);
        if (url.origin !== new URL(location.href).origin || !/^\/(?:[A-Za-z0-9_]+|i\/web)\/status\/[0-9]+(?:\/video\/[1-9][0-9]*)?\/?$/.test(url.pathname)) return null;
        return url.origin + url.pathname;
      } catch { return null; }
    };
    // Timelines contain many posts: resolve inside this player's article first.
    const article = media.closest?.('article');
    if (article) {
      const links = [...article.querySelectorAll('a[href]')];
      const video = links.find(link => /\/video\/[1-9][0-9]*\/?$/.test(link.pathname || "") && postUrl(link.href));
      if (video) return postUrl(video.href);
      const permalink = links.find(link => link.querySelector?.('time') && postUrl(link.href));
      return permalink ? postUrl(permalink.href) : null;
    }
    return postUrl(location.href);
  }
  function socialPage(media) {
    const instagram = ["instagram.com", "www.instagram.com", "m.instagram.com"].includes(location.hostname);
    const tiktok = ["tiktok.com", "www.tiktok.com", "m.tiktok.com"].includes(location.hostname);
    if (!instagram && !tiktok) return null;
    const canonical = value => {
      try {
        const url = new URL(value, location.href);
        if (url.origin !== new URL(location.href).origin) return null;
        if (instagram && /^\/(?:[A-Za-z0-9_.]+\/)?(p|reel|reels|tv)\/[A-Za-z0-9_-]+\/?$/.test(url.pathname)) return url.origin + url.pathname;
        if (tiktok && /^\/@[^/]+\/video\/[0-9]+\/?$/.test(url.pathname)) return url.origin + url.pathname;
      } catch {}
      return null;
    };
    const container = media.closest?.('article, [data-e2e="recommend-list-item-container"], [data-e2e="browse-video"]');
    if (container) {
      for (const link of container.querySelectorAll('a[href]')) {
        const url = canonical(link.href);
        if (url) return url;
      }
    }
    return canonical(location.href);
  }
  function attach(media) {
    if (!media.parentElement) return;
    const key = mediaKey(media);
    const previous = buttons.get(media);
    if (dismissed.has(key)) {
      previous?.overlay.remove();
      buttons.delete(media);
      return;
    }
    if (previous) {
      if (previous.key !== key) {
        previous.key = key;
        previous.notice?.remove(); previous.notice = null;
        previous.button.textContent = "Download with IDG";
        previous.button.title = "Choose a format and save location in IDG";
        previous.button.disabled = false;
      }
      return;
    }
    const overlay = document.createElement("div");
    overlay.setAttribute("data-idg-overlay", "");
    Object.assign(overlay.style, {
      all: "initial", position: "absolute", top: "6px", right: "6px", zIndex: "2147483647",
      display: "inline-flex", alignItems: "center", height: "28px", maxWidth: "calc(100% - 12px)",
      boxSizing: "border-box", background: "#126e65", color: "#fff", border: "1px solid #60d5c280",
      borderRadius: "5px", font: "500 11px/16px system-ui, sans-serif", boxShadow: "0 2px 8px #0003",
    });
    const button = document.createElement("button");
    button.textContent = "Download with IDG";
    button.type = "button";
    button.title = "Choose a format and save location in IDG";
    button.setAttribute("aria-label", "Download this media with IDG");
    Object.assign(button.style, {
      all: "initial", boxSizing: "border-box", height: "26px", padding: "4px 8px", margin: "0",
      color: "#fff", background: "transparent", border: "0", borderRadius: "4px 0 0 4px",
      cursor: "pointer", font: "inherit", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
    });
    const close = document.createElement("button");
    close.textContent = "×";
    close.type = "button";
    close.title = "Hide for this video";
    close.setAttribute("aria-label", "Hide IDG download button for this video");
    Object.assign(close.style, {
      all: "initial", boxSizing: "border-box", display: "inline-flex", alignItems: "center", justifyContent: "center",
      flex: "0 0 26px", width: "26px", height: "26px", margin: "0", padding: "0",
      color: "#fff", background: "transparent", border: "0", borderLeft: "1px solid #ffffff40",
      borderRadius: "0 4px 4px 0", cursor: "pointer", font: "400 17px/1 system-ui, sans-serif",
    });
    for (const control of [button, close]) {
      control.addEventListener("focus", () => { control.style.outline = "2px solid #fff"; control.style.outlineOffset = "-2px"; });
      control.addEventListener("blur", () => { control.style.outline = "none"; });
      control.addEventListener("pointerdown", event => event.stopPropagation());
    }
    const state = { overlay, button, notice: null, key };
    const dismiss = () => { dismissed.add(mediaKey(media)); overlay.remove(); buttons.delete(media); };
    close.addEventListener("click", event => {
      if (!event.isTrusted) return;
      event.preventDefault(); event.stopPropagation(); dismiss();
    });
    overlay.addEventListener("keydown", event => {
      if (event.isTrusted && event.key === "Escape") { event.preventDefault(); event.stopPropagation(); dismiss(); }
    });
    button.addEventListener("click", async event => {
      if (!event.isTrusted || button.disabled) return;
      event.preventDefault(); event.stopPropagation();
      if (["open.spotify.com", "play.spotify.com"].includes(location.hostname)) {
        button.textContent = "Spotify offline only";
        button.title = "Spotify tracks are DRM-protected. Use Spotify's Download feature for offline listening in Spotify.";
        return;
      }
      const youtube = youtubePage();
      const url = socialPage(media) || xPage(media) || (youtube ? location.href : media.currentSrc || media.src || media.querySelector("source[src]")?.src);
      if (!url || !/^https?:/.test(url)) {
        button.textContent = "No direct link";
        button.title = "Open the X, Instagram or TikTok post containing this video, or use an HTTP/HTTPS media link.";
        return;
      }
      const requestKey = mediaKey(media);
      state.key = requestKey;
      state.notice?.remove(); state.notice = null;
      button.disabled = true; button.textContent = "Sending…";
      try {
        const response = await chrome.runtime.sendMessage({ type: "download", url, filename: youtube ? document.title.replace(/\s*[-–]\s*YouTube$/, "").trim() + ".mp4" : null });
        // Closing or navigating while the handoff runs must not recreate the overlay.
        if (buttons.get(media) !== state || state.key !== requestKey) return;
        button.textContent = response?.ok ? "Opened in IDG" : "IDG · retry";
        button.title = response?.error || "Choose a filename and start in IDG";
        if (!response?.ok) {
          if (!state.notice) {
            state.notice = document.createElement("div");
            state.notice.setAttribute("role", "alert");
            Object.assign(state.notice.style, {
              position: "absolute", top: "32px", right: "0", zIndex: "2147483647", width: "240px", maxWidth: "calc(100vw - 24px)",
              padding: "9px 10px", boxSizing: "border-box", borderRadius: "5px", background: "#19262d", color: "#fff",
              font: "12px/1.5 system-ui, sans-serif", boxShadow: "0 2px 8px #0004", whiteSpace: "normal",
            });
            overlay.appendChild(state.notice);
          }
          state.notice.textContent = response?.error || "Open IDG and retry.";
        } else { state.notice?.remove(); state.notice = null; }
      } catch {
        if (buttons.get(media) === state && state.key === requestKey) {
          button.textContent = "IDG · reconnect";
          button.title = "Reload this page to reconnect to IDG.";
        }
      } finally {
        if (buttons.get(media) === state && state.key === requestKey) button.disabled = false;
      }
    });
    overlay.appendChild(button); overlay.appendChild(close);
    const parent = media.parentElement;
    if (getComputedStyle(parent).position === "static") parent.style.position = "relative";
    buttons.set(media, state);
    parent.appendChild(overlay);
    if (!watched.has(media)) {
      watched.add(media);
      media.addEventListener("loadedmetadata", () => attach(media));
    }
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
        if (node.nodeType === 1 && !node.hasAttribute("data-idg-overlay") && node.tagName !== "BUTTON") scan(node);
      }
    }).observe(document.documentElement, { childList: true, subtree: true });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init, { once: true });
  else init();
})();
