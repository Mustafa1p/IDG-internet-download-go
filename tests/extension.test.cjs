const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const crypto = require('node:crypto');

function worker({ online = true, accepts = true, enabled = true, migrated = true, native = true, youtube = true, reload = false, diskVersion = "1.3.1" } = {}) {
  const events = {}, calls = [], reports = [];
  const event = name => ({ addListener: listener => { events[name] = listener; } });
  const chrome = {
    runtime: {
      id: 'idg-test', onInstalled: event('installed'), onMessage: event('message'), onStartup: event('startup'),
      getManifest: () => ({ version: '1.3.0' }), getURL: path => `chrome-extension://idg-test/${path}`, reload: () => reports.push('reload'),
      sendNativeMessage(name, message, callback) {
        calls.push(['native', message]);
        if (!native) this.lastError = { message: 'Native host missing' };
        callback(native ? { ok: accepts, installed: true, serverOnline: online, capabilities: { youtube } } : undefined);
        delete this.lastError;
      },
    },
    management: { getSelf: async () => ({ installType: 'development' }) },
    alarms: { onAlarm: event('alarm'), create() {} },
    storage: { local: { get: async () => ({ interceptEnabled: enabled, statusOnlyPopup: migrated }), set: async value => calls.push(['settings', value]) } },
    action: { setBadgeText: async () => {}, setBadgeBackgroundColor: async () => {}, setTitle: async () => {} },
    cookies: { getAll: async () => [{ name: 'session', value: 'test-cookie' }] },
    webRequest: { onBeforeSendHeaders: event('requestHeaders') },
    downloads: {
      onCreated: event('created'),
      pause: async id => calls.push(['pause', id]),
      resume: async id => calls.push(['resume', id]),
      cancel: async id => calls.push(['cancel', id]),
      erase: async filter => calls.push(['erase', filter]),
    },
    contextMenus: { onClicked: event('context'), create() {}, removeAll: async () => {} },
  };
  const context = vm.createContext({ chrome, console, URL, AbortSignal, navigator: { userAgent: 'test-agent' }, fetch: async (url, options) => {
    if (url.endsWith('/extension/heartbeat')) {
      reports.push(JSON.parse(options.body));
      return { ok: online, json: async () => ({ reload, version: '1.3.1' }) };
    }
    if (url.startsWith('chrome-extension://')) return { ok: true, json: async () => ({ version: diskVersion }) };
    if (url.endsWith('/health')) {
      if (!online) throw new Error('Offline');
      return { ok: true, json: async () => ({ status: 'ok', version: '1.2.0', capabilities: { youtube } }) };
    }
    calls.push(['post', JSON.parse(options.body)]);
    return { ok: accepts, status: accepts ? 201 : 500 };
  } });
  vm.runInContext(fs.readFileSync('extension/background.js', 'utf8'), context);
  const message = value => new Promise(resolve => events.message(value, { id: 'idg-test' }, resolve));
  return { events, calls, message, context, reports };
}
const download = { id: 17, url: 'https://example.com/file.zip', filename: 'C:\\Downloads\\file.zip', referrer: 'https://example.com/source', mime: 'application/zip', fileSize: 200000 };

test('small Hostinger exports preserve repeated Inbox and Sent parameters and captured authentication', async () => {
  for (const query of ['m=2464-INBOX', 'm=534-INBOX.Sent&m=533-INBOX.Sent']) {
    const { events, calls } = worker();
    const url = `https://mail.hostinger.com/bridge/api/v3/mailboxes/test/messages/sources/export?${query}`;
    events.requestHeaders({ url, method: 'GET', tabId: 4, requestHeaders: [
      { name: 'Authorization', value: 'Bearer test-token' },
      { name: 'Cookie', value: 'partitioned-session=abc' },
      { name: 'X-CSRF-Token', value: 'csrf-test' },
      { name: 'Range', value: 'bytes=0-' },
    ] });
    await events.created({ ...download, url, filename: '', mime: 'application/octet-stream', fileSize: 512 });
    assert.deepEqual(calls.map(call => call[0]), ['pause', 'post', 'cancel', 'erase']);
    const request = calls.find(call => call[0] === 'post')[1];
    assert.equal(request.url, url);
    assert.deepEqual(request.headers, { authorization: 'Bearer test-token', cookie: 'partitioned-session=abc', 'x-csrf-token': 'csrf-test' });
  }
});

test('EML downloads are captured even below the usual file size threshold', async () => {
  const { events, calls } = worker();
  await events.created({ ...download, url: 'https://example.com/message', mime: 'message/rfc822; charset=utf-8', fileSize: 500 });
  assert.ok(calls.some(call => call[0] === 'post'));
});

test('expired or unrelated captured mail credentials are never forwarded', async () => {
  for (const stale of [false, true]) {
    const { events, calls, message, context } = worker();
    const url = 'https://mail.hostinger.com/bridge/api/v3/mailboxes/test/messages/sources/export?m=1-INBOX';
    events.requestHeaders({ url, method: 'GET', tabId: 1, requestHeaders: [{ name: 'Authorization', value: 'Bearer private' }] });
    if (stale) vm.runInContext('for (const request of exportRequests.values()) request.time = 0', context);
    await message({ type: 'download', url: stale ? url : 'https://example.com/file.zip' });
    assert.equal(calls.find(call => call[0] === 'post')[1].headers, undefined);
  }
});

test('handoff pauses, forwards credentials, then cancels after acceptance', async () => {
  const { events, calls } = worker();
  await events.created(download);
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'post', 'cancel', 'erase']);
  assert.equal(calls[1][1].cookies, 'session=test-cookie');
  assert.equal(calls[1][1].referrer, download.referrer);
  assert.equal(calls[1][1].user_agent, 'test-agent');
  assert.equal(calls[1][1].show_dialog, true);
});
test('rejected handoff resumes original download without cancellation or duplicate POST', async () => {
  const { events, calls } = worker({ accepts: false });
  await events.created(download);
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'post', 'resume']);
});
test('offline app uses native host to launch and accept download', async () => {
  const { events, calls } = worker({ online: false });
  await events.created(download);
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'native', 'cancel', 'erase']);
});
test('missing native installation resumes original browser download', async () => {
  const { events, calls } = worker({ online: false, native: false });
  await events.created(download);
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'native', 'resume']);
});
test('stored disabled setting, blobs, and extension-owned downloads are not intercepted', async () => {
  const disabled = worker({ enabled: false });
  await disabled.events.created(download);
  assert.equal(disabled.calls.length, 0);
  const normal = worker();
  await normal.events.created({ ...download, url: 'blob:https://example.com/test' });
  await normal.events.created({ ...download, byExtensionId: 'idg-test' });
  assert.equal(normal.calls.length, 0);
});
test('toggle persists across service worker restarts', async () => {
  const { message, calls } = worker();
  await message({ type: 'toggleIntercept', enabled: false });
  assert.equal(calls[0][0], 'settings');
  assert.equal(calls[0][1].interceptEnabled, false);
});
test('content scripts cannot read the queue or change settings', async () => {
  const { events, calls } = worker();
  const response = await new Promise(resolve => events.message({ type: 'toggleIntercept', enabled: false }, { tab: { id: 1 } }, resolve));
  assert.equal(response.ok, false);
  assert.equal(calls.length, 0);
});
test('extension identity matches the native host and requested permissions', () => {
  const manifest = JSON.parse(fs.readFileSync('extension/manifest.json'));
  const host = JSON.parse(fs.readFileSync('extension/com.rust_idm.host.json'));
  const id = crypto.createHash('sha256').update(Buffer.from(manifest.key, 'base64')).digest('hex').slice(0, 32).replace(/[0-9a-f]/g, digit => String.fromCharCode(97 + parseInt(digit, 16)));
  assert.equal(host.allowed_origins[0], `chrome-extension://${id}/`);
  for (const permission of ['downloads', 'management', 'nativeMessaging']) assert.ok(manifest.permissions.includes(permission));
  assert.ok(manifest.host_permissions.includes('<all_urls>'));
  assert.ok(fs.readFileSync('installer/setup.iss', 'utf8').includes(id));
});

test('YouTube handoff uses the video page and removes radio/playlist parameters', async () => {
  const { message, calls } = worker();
  const result = await message({ type: 'download', url: 'https://www.youtube.com/watch?v=5TBlPQrcweQ&list=RD5TBlPQrcweQ&start_radio=1' });
  assert.equal(result.ok, true);
  const request = calls.find(call => call[0] === 'post')[1];
  assert.equal(request.url, 'https://www.youtube.com/watch?v=5TBlPQrcweQ');
  assert.equal(request.show_dialog, true);
  assert.ok(request.media_cookies.startsWith('# Netscape HTTP Cookie File'));
});
test('YouTube handoff refuses old app capabilities with a useful upgrade instruction', async () => {
  const { message, calls } = worker({ youtube: false });
  const result = await message({ type: 'download', url: 'https://youtu.be/5TBlPQrcweQ' });
  assert.equal(result.ok, false);
  assert.match(result.error, /v1.2.0/);
  assert.equal(calls.some(call => call[0] === 'post'), false);
});
test('YouTube extraction does not accept spoofed domains', () => {
  const { context } = worker();
  assert.equal(vm.runInContext('youtubeVideoUrl("https://youtube.com.evil.test/watch?v=5TBlPQrcweQ")', context), null);
});
function overlayPage(reply = async () => ({ ok: true })) {
  const sent = [], events = {};
  function element(tag) {
    return { tagName: tag.toUpperCase(), nodeType: 1, style: {}, handlers: {}, attributes: {}, children: [],
      setAttribute(name, value) { this.attributes[name] = value; }, hasAttribute(name) { return name in this.attributes; },
      addEventListener(name, handler) { this.handlers[name] = handler; },
      appendChild(child) { child.parentElement = this; this.children.push(child); },
      remove() { const index = this.parentElement?.children.indexOf(this); if (index >= 0) this.parentElement.children.splice(index, 1); },
      querySelectorAll: () => [],
    };
  }
  const parent = element('div'), media = element('video');
  media.parentElement = parent; media.currentSrc = 'blob:https://www.youtube.com/player'; media.querySelector = () => null;
  const location = { hostname: 'www.youtube.com', pathname: '/watch', href: 'https://www.youtube.com/watch?v=5TBlPQrcweQ&list=radio' };
  const document = { readyState: 'complete', documentElement: {}, title: 'Sample video - YouTube',
    querySelectorAll: () => [media], addEventListener: (name, handler) => { events[name] = handler; }, createElement: element,
  };
  let mutation;
  const context = vm.createContext({ document, location, window: {}, URL,
    MutationObserver: class { constructor(callback) { mutation = callback; } observe() {} }, getComputedStyle: () => ({ position: 'relative' }),
    chrome: { runtime: { sendMessage: async message => { sent.push(message); return reply(message); } } },
  });
  vm.runInContext(fs.readFileSync('extension/content.js', 'utf8'), context);
  const event = overrides => ({ isTrusted: true, preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; }, ...overrides });
  return { sent, parent, media, location, event, get overlay() { return parent.children[0]; },
    scan: () => mutation([{ addedNodes: [{ nodeType: 1, tagName: 'DIV', hasAttribute: () => false, querySelectorAll: () => [media] }] }]),
    navigate: () => events['yt-navigate-finish'](),
  };
}

test('YouTube blob-player button follows the current SPA video URL', async () => {
  const page = overlayPage();
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.sent[0].url, page.location.href);
  assert.equal(page.sent[0].filename, 'Sample video.mp4');
  page.location.href = 'https://www.youtube.com/watch?v=BaW_jenozKc';
  page.navigate();
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.sent[1].url, page.location.href);
});

test('slim overlay can be closed without downloading or immediately reappearing', () => {
  const page = overlayPage(), overlay = page.overlay, close = overlay.children[1], event = page.event();
  assert.equal(overlay.style.height, '28px');
  assert.ok(close.attributes['aria-label'].includes('Hide'));
  close.handlers.click(event);
  assert.ok(event.prevented && event.stopped);
  assert.equal(page.sent.length, 0);
  assert.equal(page.parent.children.length, 0);
  page.scan(); page.navigate();
  assert.equal(page.parent.children.length, 0);
  page.location.href = 'https://www.youtube.com/watch?v=BaW_jenozKc';
  page.navigate();
  assert.equal(page.parent.children.length, 1);
});

test('closing during a handoff also suppresses later errors and notices', async () => {
  let finish;
  const page = overlayPage(() => new Promise(resolve => { finish = resolve; }));
  const pending = page.overlay.children[0].handlers.click(page.event());
  page.overlay.children[1].handlers.click(page.event());
  finish({ ok: false, error: 'App is offline' });
  await pending;
  assert.equal(page.parent.children.length, 0);
  assert.equal(page.sent.length, 1);
});

test('Escape and close remove error notices along with the media overlay', async () => {
  const page = overlayPage(async () => ({ ok: false, error: 'Open IDG and retry' }));
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.overlay.children[2].attributes.role, 'alert');
  page.overlay.handlers.keydown(page.event({ key: 'Escape' }));
  assert.equal(page.parent.children.length, 0);
  page.scan();
  assert.equal(page.parent.children.length, 0);
});

test('page scripts cannot activate or dismiss the media overlay', async () => {
  const page = overlayPage();
  await page.overlay.children[0].handlers.click(page.event({ isTrusted: false }));
  page.overlay.children[1].handlers.click(page.event({ isTrusted: false }));
  assert.equal(page.sent.length, 0);
  assert.equal(page.parent.children.length, 1);
});

test('extension reports its version and reloads only when its own updated files match', async () => {
  const updated = worker({ reload: true });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(updated.reports[0].version, '1.3.0');
  assert.equal(updated.reports[0].install_type, 'development');
  assert.ok(updated.reports.includes('reload'));
  const stale = worker({ reload: true, diskVersion: '1.3.0' });
  await new Promise(resolve => setImmediate(resolve));
  assert.ok(!stale.reports.includes('reload'));
});

test('extension reload waits until browser handoffs have finished', async () => {
  const active = worker({ reload: true });
  await new Promise(resolve => setImmediate(resolve));
  active.reports.length = 0;
  await vm.runInContext('activeHandoffs = 1; reportIntegration()', active.context);
  assert.ok(!active.reports.includes('reload'));
});

test('status-only popup requests connection status without requesting downloads or controls', async () => {
  const elements = { 'status-label': {}, 'connection-detail': {} }, panel = {}, messages = [];
  let refresh;
  const context = vm.createContext({ document: { getElementById: id => elements[id], querySelector: () => panel },
    chrome: { runtime: { sendMessage: async message => { messages.push(message.type); return { serverOnline: true, serverVersion: '1.4.0' }; } } },
    setInterval: callback => { refresh = callback; } });
  vm.runInContext(fs.readFileSync('extension/popup.js', 'utf8'), context);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(elements['status-label'].textContent, 'IDG is connected');
  await refresh();
  assert.deepEqual(messages, ['getStatus', 'getStatus']);
  const html = fs.readFileSync('extension/popup.html', 'utf8');
  assert.ok(!/<form|<button|<input|<footer|downloads-list|youtube-btn/.test(html));
});

test('popup migration enables capture when the old switch had been disabled', async () => {
  const { events, calls } = worker({ enabled: false, migrated: false });
  await events.created(download);
  assert.ok(calls.some(call => call[0] === 'post'));
  assert.ok(calls.some(call => call[0] === 'settings' && call[1].statusOnlyPopup === true));
});
