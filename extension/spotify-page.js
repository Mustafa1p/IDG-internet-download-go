// Read public entity identifiers only. Never inspect accounts, stores or credentials.
(() => {
  if (location.hostname !== 'open.spotify.com' || window.__idgSpotifyReader) return;
  window.__idgSpotifyReader = true;
  const attribute = 'data-idg-spotify-uri';
  const rowSelector = '[data-testid="tracklist-row"], [role="row"]';
  const libraryScope = '[data-testid="your-library"], [data-testid="library-list"], [data-testid="rootlist"], [data-testid="rootlist-container"], #Desktop_LeftSidebar_Id, [data-testid="Desktop_LeftSidebar_Id"], [aria-label="Your Library"], [aria-label="Your library"]';
  const librarySelector = '[data-testid="rootlist-item"], [role="listitem"], [data-encore-id="listRow"], [role="row"]';
  const playerSelector = '[data-testid="now-playing-widget"], [data-testid="nowplaying-widget"], [data-testid="context-item-info"], [data-testid="context-item-info-title"], a[data-testid="context-item-link"], [data-testid="NPV_Panel_OpenDiv"], [data-testid="now-playing-view"]';
  function own(object, key) {
    if (!object || typeof object !== 'object') return undefined;
    return Object.getOwnPropertyDescriptor(object, key)?.value;
  }
  function canonical(value, library) {
    if (typeof value !== 'string') return null;
    if (library && /^spotify:(?:collection:tracks|user:[^:]+:collection)$/.test(value)) return 'https://open.spotify.com/collection/tracks';
    const uri = value.match(/^spotify:(track|playlist):([A-Za-z0-9]{22})$/);
    if (uri) return (library ? uri[1] === 'playlist' : uri[1] === 'track') ? `https://open.spotify.com/${uri[1]}/${uri[2]}` : null;
    try {
      const url = new URL(value, location.origin);
      if (url.origin !== location.origin || url.username || url.password) return null;
      if (library && /^\/(?:intl-[A-Za-z]+\/)?collection\/tracks\/?$/.test(url.pathname)) return 'https://open.spotify.com/collection/tracks';
      const match = url.pathname.match(/^\/(?:intl-[A-Za-z]+\/)?(track|playlist)\/([A-Za-z0-9]{22})\/?$/);
      return match && (library ? match[1] === 'playlist' : match[1] === 'track') ? `https://open.spotify.com/${match[1]}/${match[2]}` : null;
    } catch { return null; }
  }
  function identifiers(value, library) {
    const found = new Set(), seen = new Set();
    let budget = 100;
    function visit(object, depth) {
      if (--budget < 0 || depth > 5 || !object || typeof object !== 'object' || seen.has(object)) return;
      seen.add(object);
      for (const key of ['uri', 'href']) {
        const url = canonical(own(object, key), library);
        if (url) found.add(url);
      }
      const type = own(object, 'type'), id = own(object, 'id');
      if ((library ? type === 'playlist' : type === 'track') && typeof id === 'string' && /^[A-Za-z0-9]{22}$/.test(id)) found.add(`https://open.spotify.com/${type}/${id}`);
      // Deliberately omit state, stores, context, session and arbitrary object keys.
      for (const key of ['track', 'item', 'entity', 'playlist', 'data', 'props', 'children']) {
        const child = own(object, key);
        if (Array.isArray(child)) {
          for (let i = 0; i < Math.min(child.length, 12); i++) visit(own(child, String(i)), depth + 1);
        } else visit(child, depth + 1);
      }
    }
    visit(value, 0);
    return found;
  }
  function resolve(root) {
    if (!(root instanceof Element) || !root.isConnected) return;
    const library = root.matches('[data-testid="rootlist-item"]') || !!root.closest(libraryScope);
    if (!(library ? root.matches(librarySelector) : root.matches(`${rowSelector}, ${playerSelector}`))) return;
    const found = new Set();
    // Stay inside the requested row/player; never walk a parent playlist's props.
    for (const element of [root, ...Array.from(root.querySelectorAll('*')).slice(0, 70)]) {
      for (const key of Object.getOwnPropertyNames(element)) {
        if (key.startsWith('__reactProps$')) {
          for (const uri of identifiers(own(element, key), library)) found.add(uri);
        } else if (key.startsWith('__reactFiber$')) {
          const fiber = own(element, key);
          for (const uri of identifiers(own(fiber, 'memoizedProps'), library)) found.add(uri);
        }
      }
    }
    const uri = found.size === 1 ? [...found][0] : null;
    if (uri) { if (root.getAttribute(attribute) !== uri) root.setAttribute(attribute, uri); }
    else root.removeAttribute(attribute);
  }
  document.addEventListener('idg-resolve-spotify-source', event => resolve(event.target), true);
  function scan() {
    if (!document.documentElement) return;
    document.documentElement.setAttribute('data-idg-spotify-reader', '1');
    for (const root of document.querySelectorAll(`${rowSelector}, ${playerSelector}, [data-testid="rootlist-item"]`)) resolve(root);
    for (const scope of document.querySelectorAll(libraryScope)) for (const row of scope.querySelectorAll(librarySelector)) resolve(row);
  }
  document.addEventListener('DOMContentLoaded', scan, { once: true });
  setInterval(scan, 750);
  scan();
})();
