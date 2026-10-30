// Compact, dismissible media handoff. Dismissal lasts for this page session.
(() => {
  if (window.__idgInitialized) return;
  window.__idgInitialized = true;
  const contentBuild = "spotify-reader-20261025";
  let siteEnabled = false, siteStateKnown = false;
  function applySiteState(state) {
    siteStateKnown = true;
    siteEnabled = state?.enabled === true;
    if (!siteEnabled) {
      for (const overlay of document.querySelectorAll("[data-idg-overlay]")) overlay.remove();
      // Invalidate in-flight handoffs as well as detached overlays.
      buttons = new WeakMap();
      activeOverlays.clear();
    } else if (document.readyState !== "loading") scan(document);
  }
  chrome.runtime.onMessage.addListener((message, _sender, respond) => {
    if (message.type === "getContentStatus") respond({ build: contentBuild, enabled: siteEnabled, reader: document.documentElement?.getAttribute("data-idg-spotify-reader") === "1", rows: document.querySelectorAll('[data-testid="tracklist-row"], [role="row"]').length, resolvedRows: spotifyRowCount, libraryButtons: document.querySelectorAll("[data-idg-spotify-library]").length });
    if (message.type === "siteStateChanged") applySiteState(message);
  });
  const siteStateReady = chrome.runtime.sendMessage({ type: "getSiteStatus" }).then(applySiteState).catch(() => { siteStateKnown = true; });
  document.addEventListener("idg-post-submit", event => {
    if (siteStateKnown && !siteEnabled) return;
    const form = event.target;
    const request = globalThis.__idgReadDownloadForm(form);
    if (!request) return;
    event.preventDefault(); event.stopImmediatePropagation();
    (async () => {
      let accepted = false;
      try {
        await siteStateReady;
        if (siteEnabled) {
          const response = await chrome.runtime.sendMessage({ type: "download", ...request });
          accepted = response?.ok === true;
        }
      } catch { /* Restore the original form submission if integration fails. */ }
      form.dispatchEvent(new CustomEvent("idg-post-result", { detail: accepted ? "accepted" : "fallback" }));
    })();
  }, true);
  // Delegation also catches buttons inserted after a site's waiting countdown.
  // Opaque scripts/forms must run in the page so their tokens and redirects work.
  document.addEventListener("click", async event => {
    if (!siteEnabled) return;
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
  let buttons = new WeakMap();
  const dismissed = new Set();
  const watched = new WeakSet();
  const activeOverlays = new Set();
  const spotifyRows = new WeakMap();
  let spotifyRowCount = 0;
  let spotifyPlayback = { page: null, identity: null, track: null, identityTrack: null, sawPrimaryPlayer: false, staleHint: false, stalePlayerTrack: null, hintUsed: false };
  let pageMusicTarget = null, pageMusicHost = null;
  function youtubePage() {
    try {
      if (!["www.youtube.com", "youtube.com", "m.youtube.com", "music.youtube.com", "www.youtube-nocookie.com"].includes(location.hostname)) return false;
      return (location.pathname === "/watch" && /^[A-Za-z0-9_-]{11}$/.test(new URL(location.href).searchParams.get("v") || "")) || /^\/(shorts|live|embed)\/[A-Za-z0-9_-]{11}/.test(location.pathname);
    } catch { return false; }
  }
  function musicSite() {
    return location.hostname === "open.spotify.com" || /^[a-z0-9-]+\.bandcamp\.com$/.test(location.hostname) ||
      ["deezer.com", "www.deezer.com", "tidal.com", "www.tidal.com", "listen.tidal.com", "music.apple.com", "qobuz.com", "www.qobuz.com", "play.qobuz.com", "open.qobuz.com"].includes(location.hostname);
  }
  function musicPage() {
    const host = location.hostname, path = location.pathname;
    if (host === "open.spotify.com") {
      const playing = spotifyPlayingTrack();
      // Explicit albums/playlists remain collection downloads. Track actions
      // follow current playback, which can change without browser navigation.
      if (/^\/(?:intl-[A-Za-z]+\/)?(?:album|playlist|show)\/[A-Za-z0-9]{22}\/?$/.test(path)) return location.href;
      if (playing) return playing;
      if (spotifyPlayback.staleHint) return null;
      const pageTrack = spotifyTrackUrl(location.href);
      if (pageTrack) spotifyPlayback.hintUsed = true;
      return pageTrack || (/^\/(?:intl-[A-Za-z]+\/)?episode\/[A-Za-z0-9]{22}\/?$/.test(path) ? location.href : null);
    }
    return ((/^[a-z0-9-]+\.bandcamp\.com$/.test(host) && /^\/(track|album)\//.test(path)) ||
      (["deezer.com", "www.deezer.com", "tidal.com", "www.tidal.com", "listen.tidal.com", "music.apple.com", "qobuz.com", "www.qobuz.com", "play.qobuz.com", "open.qobuz.com"].includes(host) && /\/(track|album|playlist)\//.test(path))) ? location.href : null;
  }
  function spotifyTrackUrl(value) {
    if (typeof value !== "string") return null;
    const uri = value.match(/^spotify:track:([A-Za-z0-9]{22})$/);
    if (uri) return `https://open.spotify.com/track/${uri[1]}`;
    try {
      const url = new URL(value, location.href);
      if (!["http:", "https:"].includes(url.protocol) || url.hostname !== "open.spotify.com" || url.username || url.password) return null;
      const track = url.pathname.match(/^\/(?:intl-[A-Za-z]+\/)?track\/([A-Za-z0-9]{22})\/?$/);
      if (track) return `https://open.spotify.com/track/${track[1]}`;
      if (/^\/(?:intl-[A-Za-z]+\/)?collection\/tracks\/?$/.test(url.pathname)) {
        const values = url.searchParams.getAll("uri");
        return values.length === 1 && /^spotify:track:[A-Za-z0-9]{22}$/.test(values[0]) ? spotifyTrackUrl(values[0]) : null;
      }
    } catch {}
    return null;
  }
  function spotifyPublicSource(root) {
    root.dispatchEvent(new CustomEvent("idg-resolve-spotify-source", { bubbles: true }));
    return root.getAttribute("data-idg-spotify-uri");
  }
  function spotifyRowSource(row) {
    const publicSource = spotifyPublicSource(row);
    const candidates = new Map();
    const add = (value, title) => {
      const url = spotifyTrackUrl(value);
      if (url && (!candidates.has(url) || candidates.get(url) === "Spotify track")) candidates.set(url, title?.trim() || "Spotify track");
    };
    add(row.getAttribute("data-uri"), row.querySelector('[data-testid="internal-track-link"], [data-testid="tracklist-row-title"]')?.textContent);
    add(row.getAttribute("data-track-uri"), row.getAttribute("aria-label"));
    for (const element of row.querySelectorAll('a[href], [data-uri], [data-track-uri]')) {
      add(element.getAttribute("href") || element.getAttribute("data-uri") || element.getAttribute("data-track-uri"), element.textContent);
    }
    if (!candidates.size) add(publicSource, row.querySelector('[data-testid="internal-track-link"], [data-testid="tracklist-row-title"]')?.textContent || row.getAttribute("aria-label"));
    if (candidates.size !== 1) return null;
    const [url, title] = [...candidates][0];
    return { url, title: title.slice(0, 160) };
  }
  const spotifyLibraryScope = '[data-testid="your-library"], [data-testid="library-list"], [data-testid="rootlist"], [data-testid="rootlist-container"], #Desktop_LeftSidebar_Id, [data-testid="Desktop_LeftSidebar_Id"], [aria-label="Your Library"], [aria-label="Your library"]';
  function spotifyLibraryRow(element) {
    const row = element.closest?.('[data-testid="rootlist-item"], [role="listitem"], [data-encore-id="listRow"], [role="row"]');
    return row && (row.matches?.('[data-testid="rootlist-item"]') || row.closest(spotifyLibraryScope)) ? row : null;
  }
  function spotifyLibraryUrl(value) {
    try {
      const url = new URL(value, location.href);
      if (!["http:", "https:"].includes(url.protocol) || url.hostname !== "open.spotify.com" || url.username || url.password) return null;
      if (/^\/(?:intl-[A-Za-z]+\/)?collection\/tracks\/?$/.test(url.pathname)) return 'https://open.spotify.com/collection/tracks';
      const playlist = url.pathname.match(/^\/(?:intl-[A-Za-z]+\/)?playlist\/([A-Za-z0-9]{22})\/?$/);
      return playlist ? `https://open.spotify.com/playlist/${playlist[1]}` : null;
    } catch { return null; }
  }
  function spotifyLibrarySource(row) {
    const publicSource = spotifyPublicSource(row);
    const candidates = new Map();
    for (const link of [row, ...row.querySelectorAll('a[href]')]) {
      const url = spotifyLibraryUrl(link.getAttribute('href'));
      if (!url) continue;
      const label = link.getAttribute('aria-label') || row.querySelector('[data-testid="listrow-title"], [data-testid="entityTitle"]')?.textContent || (link.innerText || link.textContent || '').trim().split(/\r?\n/)[0];
      candidates.set(url, label.trim().slice(0,160) || (url.endsWith('/collection/tracks') ? 'Liked Songs' : 'Playlist'));
    }
    if (!candidates.size) {
      const url = spotifyLibraryUrl(publicSource);
      if (url) candidates.set(url, row.querySelector('[data-testid="listrow-title"], [data-testid="entityTitle"]')?.textContent?.trim().slice(0,160) || "Playlist");
    }
    if (candidates.size !== 1) return null;
    const [url,title] = [...candidates][0];
    return {url,title,kind:'library',collection:true};
  }
  function spotifyActionSource(row) {
    return spotifyRows.get(row)?.kind === 'library' ? spotifyLibrarySource(row) : spotifyRowSource(row);
  }
  function syncSpotifyLibrary() {
    if (!siteEnabled || location.hostname !== 'open.spotify.com' || (window.top && window.top !== window)) return;
    const rows = new Set();
    for (const link of document.querySelectorAll?.('a[href]') || []) {
      if (!link.getAttribute || !spotifyLibraryUrl(link.getAttribute('href'))) continue;
      const row = spotifyLibraryRow(link);
      if (row) rows.add(row);
    }
    for (const scope of document.querySelectorAll(spotifyLibraryScope)) for (const row of scope.querySelectorAll('[data-testid="rootlist-item"], [role="listitem"], [data-encore-id="listRow"], [role="row"]')) rows.add(row);
    const seen = new Set();
    for (const row of rows) {
      const source = spotifyLibrarySource(row);
      if (!source) continue;
      seen.add(row); spotifyRows.set(row,source); attach(row);
      const state = buttons.get(row);
      if (state) styleSpotifyRow(state,source);
    }
    for (const state of [...activeOverlays]) {
      if (spotifyRows.get(state.media)?.kind === 'library' && !seen.has(state.media)) {
        spotifyRows.delete(state.media); removeOverlay(state);
      }
    }
  }
  function styleSpotifyRow(state, source) {
    state.overlay.setAttribute("data-idg-spotify-row", "");
    if (source.collection) state.overlay.setAttribute("data-idg-spotify-library", "");
    state.close.remove();
    if (state.rowUrl !== source.url) { state.button.textContent = "\u2193"; state.rowUrl = source.url; }
    const action = source.collection ? 'Download all' : 'Download';
    state.button.setAttribute("aria-label", `${action} ${source.title} with IDG`);
    state.button.title = source.collection ? `Download all tracks from ${source.title} using the connected IDG Spotify account` : `Download ${source.title} with IDG`;
    Object.assign(state.button.style, { width: "34px", height: "34px", padding: "0", font: "700 23px/1 system-ui, sans-serif", borderRadius: "50%", color: "#1ed760" });
    for (const [name, value] of Object.entries({ background: "#172b20", border: "1px solid #1ed760", height: "34px", "border-radius": "50%" })) state.overlay.style.setProperty(name, value, "important");
    placeOverlay(state);
  }
  function syncSpotifyRows() {
    spotifyRowCount = 0;
    if (!siteEnabled || location.hostname !== "open.spotify.com" || (window.top && window.top !== window)) return;
    const rows = document.querySelectorAll?.('[data-testid="tracklist-row"], [role="row"]') || [];
    for (const row of rows) {
      if (!row.getAttribute || spotifyLibraryRow(row) || row.querySelector?.('[data-testid="tracklist-row"], [role="row"]')) continue;
      const source = spotifyRowSource(row);
      const previous = buttons.get(row);
      if (!source) {
        spotifyRows.delete(row);
        if (previous) removeOverlay(previous);
        continue;
      }
      spotifyRows.set(row, source);
      spotifyRowCount++;
      attach(row);
      const state = buttons.get(row);
      if (state) styleSpotifyRow(state, source);
    }
  }
  function spotifyPlayingTrack() {
    const groups = [
      '[data-testid="now-playing-widget"], [data-testid="nowplaying-widget"], [data-testid="now-playing-bar"] [data-testid="context-item-info"]',
      'footer [data-testid="context-item-info-title"], footer a[data-testid="context-item-link"]',
      '[data-testid="context-item-info-title"], a[data-testid="context-item-link"]',
      '[data-testid="NPV_Panel_OpenDiv"], [data-testid="now-playing-view"]',
    ];
    let resolved = null, identity = null;
    let sawPrimaryPlayer = spotifyPlayback.page === location.href && spotifyPlayback.sawPrimaryPlayer;
    let primaryPresent = false;
    for (const [groupIndex, selector] of groups.entries()) {
      if (groupIndex >= 2 && sawPrimaryPlayer) break;
      const roots = [...(document.querySelectorAll?.(selector) || [])].filter(root => !root.closest?.('[data-testid="tracklist-row"], [role="row"]'));
      if (!roots.length) continue;
      if (groupIndex < 2) { sawPrimaryPlayer = true; primaryPresent = true; }
      const candidates = new Set(), publicCandidates = new Set();
      const text = [];
      for (const root of roots) {
        const publicUrl = spotifyTrackUrl(spotifyPublicSource(root));
        if (publicUrl) publicCandidates.add(publicUrl);
        const title = root.querySelector?.('[data-testid="context-item-info-title"]') || root.querySelector?.('a[data-testid="context-item-link"]') || root;
        if (title.textContent?.trim()) text.push(title.textContent.trim());
        for (const element of [root, ...root.querySelectorAll?.('a[href], [data-uri], [data-track-uri]') || []]) {
          for (const attribute of ["href", "data-uri", "data-track-uri"]) {
            const url = spotifyTrackUrl(element.getAttribute?.(attribute));
            if (url) candidates.add(url);
          }
        }
      }
      if (!candidates.size) for (const uri of publicCandidates) candidates.add(uri);
      if (text.length && !identity) identity = text.join("\n");
      if (candidates.size === 1) { resolved = [...candidates][0]; break; }
      // A player panel with conflicting tracks is not a trustworthy source.
      if (candidates.size > 1) break;
    }
    const metadata = typeof navigator !== "undefined" ? navigator.mediaSession?.metadata : null;
    if (metadata?.title) identity = JSON.stringify([metadata.title, metadata.artist || ""]);
    if (spotifyPlayback.page !== location.href) {
      spotifyPlayback = { page: location.href, identity, track: resolved, identityTrack: resolved, sawPrimaryPlayer, staleHint: false, stalePlayerTrack: null, hintUsed: false };
    } else {
      if (identity && spotifyPlayback.identity && identity !== spotifyPlayback.identity) {
        spotifyPlayback.staleHint = true;
        // Spotify can update the title before replacing the link. Do not send
        // the previous song during that intermediate render.
        spotifyPlayback.stalePlayerTrack = resolved && resolved === (spotifyPlayback.identityTrack || spotifyPlayback.stalePlayerTrack || spotifyPlayback.track) ? resolved : null;
        spotifyPlayback.identityTrack = spotifyPlayback.stalePlayerTrack ? null : resolved;
      } else if (identity && !spotifyPlayback.identity && spotifyPlayback.hintUsed) {
        spotifyPlayback.staleHint = true;
      }
      if (identity) spotifyPlayback.identity = identity;
      if (resolved && resolved !== spotifyPlayback.track) {
        spotifyPlayback.track = resolved;
        if (resolved !== spotifyPlayback.stalePlayerTrack) spotifyPlayback.stalePlayerTrack = null;
      }
      if (!spotifyPlayback.identityTrack && resolved && resolved !== spotifyPlayback.stalePlayerTrack) spotifyPlayback.identityTrack = resolved;
    }
    spotifyPlayback.sawPrimaryPlayer = sawPrimaryPlayer;
    if (sawPrimaryPlayer && !primaryPresent) spotifyPlayback.staleHint = true;
    return resolved && resolved !== spotifyPlayback.stalePlayerTrack ? resolved : null;
  }
  function musicButtonLabel() {
    return location.hostname === "open.spotify.com" && !musicPage() ? "Open a Spotify track" : "Download with IDG";
  }
  function showMediaNotice(state, message) {
    state.notice?.remove();
    state.notice = document.createElement("div");
    state.notice.setAttribute("role", "alert");
    Object.assign(state.notice.style, {
      position: "absolute", bottom: "36px", right: "0", width: "280px", maxWidth: "calc(100vw - 32px)",
      padding: "10px", boxSizing: "border-box", borderRadius: "6px", background: "#19262d", color: "#fff",
      font: "12px/1.5 system-ui, sans-serif", boxShadow: "0 2px 8px #0006", whiteSpace: "normal",
    });
    state.notice.textContent = message;
    state.surface.appendChild(state.notice);
  }
  function mediaKey(media) {
    if (spotifyRows.has(media)) return "spotify-row:" + spotifyRows.get(media).url;
    if (musicSite()) return "music:" + (musicPage() || location.href);
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
  function removeOverlay(state) {
    state.overlay.remove();
    activeOverlays.delete(state);
    if (buttons.get(state.media) === state) buttons.delete(state.media);
  }
  function placeOverlay(state) {
    if (!document.body || !state.media.getBoundingClientRect) return;
    if (!siteEnabled || state.media.isConnected === false) { removeOverlay(state); return; }
    const host = state.overlay;
    if (host.isConnected === false) { removeOverlay(state); return; }
    const music = state.media === pageMusicTarget;
    const rect = music ? null : state.media.getBoundingClientRect();
    const width = window.innerWidth, height = window.innerHeight;
    let visible = music || (rect.width >= 40 && rect.height >= 24 && rect.bottom > 0 && rect.top < height && rect.right > 0 && rect.left < width);
    const row = spotifyRows.get(state.media);
    if (row && visible) {
      const center = rect.top + rect.height / 2;
      for (let parent = state.media.parentElement; parent && parent !== document.body; parent = parent.parentElement) {
        if (/auto|scroll|hidden|clip/.test(getComputedStyle(parent).overflowY)) {
          const clip = parent.getBoundingClientRect();
          if (center < Math.max(0, clip.top) + 17 || center > Math.min(height, clip.bottom) - 17) { visible = false; break; }
        }
      }
    }
    host.style.setProperty("display", visible ? "inline-flex" : "none", "important");
    host.style.setProperty("visibility", visible ? "visible" : "hidden", "important");
    if (!visible) return;
    const buttonWidth = row ? 34 : Math.min(190, music ? width - 24 : Math.max(100, rect.width - 12));
    let left = music ? width - buttonWidth - 16 : Math.max(8, Math.min(rect.right - buttonWidth - 6, width - buttonWidth - 8));
    let top = music ? height - 64 : Math.max(8, Math.min(rect.top + 6, height - 38));
    if (row) {
      const menu = state.media.querySelector('[data-testid="more-button"], button[aria-haspopup="menu"]');
      const menuRect = menu?.getBoundingClientRect();
      left = (menuRect?.width ? menuRect.left : rect.right - (row.kind === 'library' ? 44 : 4)) - buttonWidth - 6;
      top = rect.top + (rect.height - 34) / 2;
    }
    for (const [name, value] of Object.entries({ position: "fixed", inset: "auto", left: `${left}px`, top: `${top}px`, right: "auto", bottom: "auto", margin: "0", width: `${buttonWidth}px`, maxWidth: "calc(100vw - 16px)", pointerEvents: "auto" })) {
      host.style.setProperty(name, value, "important");
    }
  }
  function refreshOverlayPositions() {
    for (const state of [...activeOverlays]) {
      placeOverlay(state);
      if (state.portal && state.media.isConnected !== false && state.overlay.isConnected === false) attach(state.media);
    }
  }
  function attach(media) {
    if (!siteEnabled) return;
    // Music services can play without an HTML audio/video element. Use one page action.
    if (musicSite() && media !== pageMusicTarget && !spotifyRows.has(media)) return;
    if (!media.parentElement) return;
    const key = mediaKey(media);
    const previous = buttons.get(media);
    if (dismissed.has(key)) {
      if (previous) removeOverlay(previous);
      buttons.delete(media);
      return;
    }
    if (previous && previous.overlay.isConnected !== false) {
      if (previous.key !== key) {
        previous.key = key;
        previous.notice?.remove(); previous.notice = null;
        previous.button.textContent = musicButtonLabel();
        previous.button.title = "Choose a format and save location in IDG";
        previous.button.disabled = false;
      }
      placeOverlay(previous);
      return;
    }
    if (previous) removeOverlay(previous);
    const overlay = document.createElement("div");
    overlay.setAttribute("data-idg-overlay", "");
    Object.assign(overlay.style, {
      all: "initial", position: "absolute", top: "6px", right: "6px", zIndex: "2147483647",
      display: "inline-flex", alignItems: "center", height: "28px", maxWidth: "calc(100% - 12px)",
      boxSizing: "border-box", background: "#126e65", color: "#fff", border: "1px solid #60d5c280",
      borderRadius: "5px", font: "500 11px/16px system-ui, sans-serif", boxShadow: "0 2px 8px #0003",
    });
    const surface = overlay.attachShadow ? overlay.attachShadow({ mode: "open" }) : overlay;
    const button = document.createElement("button");
    button.textContent = musicButtonLabel();
    button.type = "button";
    button.title = "Choose a format and save location in IDG";
    button.setAttribute("aria-label", "Download this media with IDG");
    Object.assign(button.style, {
      all: "initial", boxSizing: "border-box", display: "inline-flex", alignItems: "center", justifyContent: "center",
      flex: "1 1 auto", minWidth: "0", height: "26px", padding: "4px 8px", margin: "0",
      color: "#fff", background: "transparent", border: "0", borderRadius: "4px 0 0 4px",
      cursor: "pointer", font: "inherit", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
    });
    const close = document.createElement("button");
    close.textContent = "×";
    close.type = "button";
    close.title = musicSite() ? "Hide for this music link" : "Hide for this video";
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
    const state = { overlay, surface, button, close, notice: null, key, media, portal: !!document.body };
    const dismiss = () => { dismissed.add(mediaKey(media)); removeOverlay(state); };
    close.addEventListener("click", event => {
      if (!event.isTrusted) return;
      event.preventDefault(); event.stopPropagation(); dismiss();
    });
    overlay.addEventListener("keydown", event => {
      if (event.isTrusted && event.key === "Escape") {
        event.preventDefault(); event.stopPropagation();
        if (spotifyRows.has(media)) { state.notice?.remove(); state.notice = null; }
        else dismiss();
      }
    });
    button.addEventListener("click", async event => {
      if (!siteEnabled || !event.isTrusted || button.disabled) return;
      event.preventDefault(); event.stopPropagation();
      if (musicSite()) {
        let rowSource = spotifyRows.get(media);
        if (rowSource) {
          rowSource = spotifyActionSource(media);
          if (!rowSource) {
            showMediaNotice(state, "Spotify is updating this row. Wait for its track link, then retry.");
            return;
          }
          spotifyRows.set(media, rowSource);
          styleSpotifyRow(state, rowSource);
        }
        const source = rowSource?.url || musicPage();
        if (!source) {
          showMediaNotice(state, "Open a Spotify track, album or playlist, or click the playing song's title first. Liked Songs needs a resolvable track link; IDG will not guess a song.");
          return;
        }
        const requestKey = mediaKey(media);
        button.disabled = true;
        if (rowSource) button.textContent = "…";
        try {
          state.key = requestKey;
          state.notice?.remove(); state.notice = null;
          const response = await chrome.runtime.sendMessage({ type: "download", url: source, filename: null });
          if (buttons.get(media) !== state || state.key !== requestKey) return;
          if (rowSource && spotifyActionSource(media)?.url !== source) { syncSpotifyRows(); return; }
          if (!rowSource && mediaKey(media) !== requestKey) { syncMusicAction(); return; }
          button.textContent = rowSource ? (response?.ok ? "\u2193" : "!") : (response?.ok ? "Opened in IDG" : "Set up music in IDG");
          button.title = response?.ok ? "Opened in IDG. Choose a format and start the download there." : response?.error || "Connect this service in IDG Settings > Music.";
          if (!response?.ok) showMediaNotice(state, response?.error || "Connect this service in IDG Settings > Music.");
        } catch {
          if (rowSource && spotifyActionSource(media)?.url !== source) { syncSpotifyRows(); return; }
          if (!rowSource && mediaKey(media) !== requestKey) { syncMusicAction(); return; }
          if (buttons.get(media) === state && state.key === requestKey) {
            button.textContent = rowSource ? "!" : "IDG · reconnect";
            showMediaNotice(state, "Open IDG or reload this page to reconnect.");
          }
        } finally { if (buttons.get(media) === state && state.key === requestKey) button.disabled = false; }
        return;
      }
      const youtube = youtubePage();
      const url = socialPage(media) || xPage(media) || (youtube ? location.href : media.currentSrc || media.src || media.querySelector("source[src]")?.src);
      if (!url || !/^https?:/.test(url)) {
        button.textContent = "No direct link";
        button.title = "Open the X, Instagram or TikTok post containing this video, or use an HTTP/HTTPS media link.";
        showMediaNotice(state, button.title);
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
            surface.appendChild(state.notice);
          }
          state.notice.textContent = response?.error || "Open IDG and retry.";
        } else { state.notice?.remove(); state.notice = null; }
      } catch {
        if (buttons.get(media) === state && state.key === requestKey) {
          button.textContent = "IDG · reconnect";
          button.title = "Reload this page to reconnect to IDG.";
          showMediaNotice(state, "The browser extension disconnected. Reload this page, then retry.");
        }
      } finally {
        if (buttons.get(media) === state && state.key === requestKey) button.disabled = false;
      }
    });
    surface.appendChild(button); surface.appendChild(close);
    const parent = document.body || media.parentElement;
    if (!document.body && getComputedStyle(parent).position === "static") parent.style.position = "relative";
    buttons.set(media, state);
    activeOverlays.add(state);
    if (document.body) {
      overlay.setAttribute("popover", "manual");
      // Keep the portal above site content without a modal backdrop or focus grab.
      for (const [name, value] of Object.entries({ display: "inline-flex", alignItems: "center", height: "28px", boxSizing: "border-box", background: "#126e65", color: "#fff", border: "1px solid #60d5c280", borderRadius: "5px", zIndex: "2147483647", font: "500 11px/16px system-ui, sans-serif", padding: "0" })) overlay.style.setProperty(name, value, "important");
    }
    parent.appendChild(overlay);
    if (overlay.showPopover) { try { overlay.showPopover(); } catch { overlay.removeAttribute("popover"); } }
    else overlay.removeAttribute?.("popover");
    placeOverlay(state);
    if (!watched.has(media)) {
      watched.add(media);
      media.addEventListener("loadedmetadata", () => attach(media));
    }
  }
  function syncMusicAction() {
    if (!siteEnabled || spotifyRowCount > 0 || (!musicPage() && location.hostname !== "open.spotify.com") || (window.top && window.top !== window)) {
      if (pageMusicTarget) {
        const previous = buttons.get(pageMusicTarget);
        if (previous) removeOverlay(previous);
      }
      pageMusicHost?.remove(); pageMusicHost = null; pageMusicTarget = null;
      return;
    }
    if (!document.body) return;
    if (!pageMusicHost?.parentElement) {
      pageMusicHost = document.createElement("div");
      pageMusicHost.setAttribute("data-idg-overlay", "");
      pageMusicHost.setAttribute("data-idg-music-action", "");
      Object.assign(pageMusicHost.style, {
        all: "initial", position: "fixed", bottom: "24px", right: "16px", width: "190px", height: "40px", zIndex: "2147483647",
        pointerEvents: "none",
      });
      pageMusicTarget = document.createElement("span");
      pageMusicHost.appendChild(pageMusicTarget);
      document.body.appendChild(pageMusicHost);
    }
    attach(pageMusicTarget);
    const state = buttons.get(pageMusicTarget);
    if (state) { state.overlay.style.pointerEvents = "auto"; placeOverlay(state); }
  }
  function scan(root) {
    syncSpotifyRows();
    syncSpotifyLibrary();
    syncMusicAction();
    if (root.matches?.("video,audio")) attach(root);
    root.querySelectorAll("video,audio").forEach(attach);
    refreshOverlayPositions();
  }
  function init() {
    scan(document);
    document.addEventListener("yt-navigate-finish", () => scan(document));
    window.addEventListener?.("popstate", () => scan(document));
    window.addEventListener?.("scroll", refreshOverlayPositions, { passive: true, capture: true });
    window.addEventListener?.("resize", refreshOverlayPositions, { passive: true });
    document.addEventListener("fullscreenchange", () => {
      for (const state of activeOverlays) {
        if (state.overlay.showPopover) {
          try { state.overlay.hidePopover(); state.overlay.showPopover(); } catch {}
        }
      }
      refreshOverlayPositions();
    });
    if (typeof setInterval === "function") {
      let previousUrl = location.href;
      setInterval(() => {
        if (location.href !== previousUrl) { previousUrl = location.href; scan(document); }
        else if (location.hostname === "open.spotify.com") scan(document);
        else refreshOverlayPositions();
      }, 750);
    }
    let scanPending = false;
    const requestScan = () => {
      if (scanPending) return;
      if (typeof requestAnimationFrame !== "function") { scan(document); return; }
      scanPending = true;
      requestAnimationFrame(() => { scanPending = false; scan(document); });
    };
    new MutationObserver(mutations => {
      if (mutations.some(mutation => mutation.type === "attributes" || mutation.type === "characterData" || mutation.removedNodes?.length ||
          [...mutation.addedNodes].some(node => node.nodeType === 1 && !node.hasAttribute("data-idg-overlay") && node.tagName !== "BUTTON"))) requestScan();
    }).observe(document.documentElement, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["src", "href", "data-uri", "data-track-uri", "data-idg-spotify-uri", "aria-rowindex", "aria-current", "data-playing"] });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init, { once: true });
  else init();
})();
