// Direct media downloads. Blob streams and protected media stay in the browser.
(() => {
  if (window.__idgInitialized) return;
  window.__idgInitialized = true;
  const buttons = new WeakMap();
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
      event.preventDefault(); event.stopPropagation();
      const url = media.currentSrc || media.src || media.querySelector("source[src]")?.src;
      if (!url || !/^https?:/.test(url)) {
        button.textContent = "Direct download unavailable";
        return;
      }
      button.disabled = true;
      button.textContent = "Sending to IDG…";
      try {
        const response = await chrome.runtime.sendMessage({ type: "download", url });
        button.textContent = response?.ok ? "Opened in IDG" : "IDG unavailable · retry";
        button.title = response?.error || "Choose a filename and start in IDG";
      } catch { button.textContent = "Reload this page to reconnect"; }
      button.disabled = false;
    });
    const parent = media.parentElement;
    if (getComputedStyle(parent).position === "static") parent.style.position = "relative";
    parent.appendChild(button);
    buttons.set(media, button);
  }
  function scan(root) {
    if (root.matches?.("video,audio")) attach(root);
    root.querySelectorAll("video,audio").forEach(attach);
  }
  function init() {
    scan(document);
    new MutationObserver(mutations => {
      for (const mutation of mutations) for (const node of mutation.addedNodes) {
        if (node.nodeType === 1 && node.tagName !== "BUTTON") scan(node);
      }
    }).observe(document.documentElement, { childList: true, subtree: true });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init, { once: true });
  else init();
})();
