const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const crypto = require('node:crypto');

function worker({ online = true, accepts = true, enabled = true, migrated = true, native = true, youtube = true, x = true, reload = false, diskVersion = "1.3.1", items = [], postDownloads = true } = {}) {
  const events = {}, calls = [], reports = [];
  const event = name => ({ addListener: listener => { events[name] = listener; } });
  const chrome = {
    runtime: {
      id: 'idg-test', onInstalled: event('installed'), onMessage: event('message'), onStartup: event('startup'),
      getManifest: () => ({ version: '1.3.0' }), getURL: path => `chrome-extension://idg-test/${path}`, reload: () => reports.push('reload'),
      sendNativeMessage(name, message, callback) {
        calls.push(['native', message]);
        if (!native) this.lastError = { message: 'Native host missing' };
        callback(native ? { ok: accepts, installed: true, serverOnline: online, capabilities: { youtube, x, post_downloads: postDownloads } } : undefined);
        delete this.lastError;
      },
    },
    management: { getSelf: async () => ({ installType: 'development' }) },
    alarms: { onAlarm: event('alarm'), create() {} },
    storage: { local: { get: async () => ({ interceptEnabled: enabled, statusOnlyPopup: migrated }), set: async value => calls.push(['settings', value]) } },
    action: { setBadgeText: async () => {}, setBadgeBackgroundColor: async () => {}, setTitle: async () => {} },
    cookies: { getAll: async () => [{ name: 'session', value: 'test-cookie' }] },
    webRequest: { onBeforeSendHeaders: event('requestHeaders'), onBeforeRequest: event('beforeRequest') },
    downloads: {
      onCreated: event('created'),
      onChanged: event('changed'),
      search: async ({ id }) => items.filter(item => item.id === id),
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
      return { ok: true, json: async () => ({ status: 'ok', version: '1.2.0', capabilities: { youtube, x, post_downloads: postDownloads } }) };
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
  const context = vm.createContext({ document, location, window: {}, URL, URLSearchParams,
    CustomEvent: class { constructor(type, options = {}) { Object.assign(this, { type, ...options }); } },
    MutationObserver: class { constructor(callback) { mutation = callback; } observe() {} }, getComputedStyle: () => ({ position: 'relative' }),
    chrome: { runtime: { sendMessage: async message => { sent.push(message); return reply(message); } } },
  });
  vm.runInContext(fs.readFileSync('extension/download-form.js', 'utf8'), context);
  vm.runInContext(fs.readFileSync('extension/content.js', 'utf8'), context);
  const event = overrides => ({ isTrusted: true, preventDefault() { this.prevented = true; }, stopPropagation() { this.stopped = true; }, ...overrides });
  return { sent, parent, media, location, event, events, document, context, get overlay() { return parent.children[0]; },
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

test('PHP endpoints use the resolved filename even with an unknown MIME and size', async () => {
  const { events, calls } = worker();
  await events.created({ ...download, url: 'https://example.com/waiting-resource.php?token=secret', mime: '', filename: 'C:\\Downloads\\archive.7z', fileSize: -1 });
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'post', 'cancel', 'erase']);
  assert.equal(calls[1][1].url, 'https://example.com/waiting-resource.php?token=secret');
});

test('late PHP metadata triggers exactly one handoff across concurrent browser events', async () => {
  const item = { ...download, url: 'https://example.com/waiting-resource.php', mime: '', filename: '', fileSize: -1, state: 'in_progress' };
  const { events, calls } = worker({ items: [item] });
  await events.created(item);
  assert.equal(calls.length, 0);
  item.filename = 'C:\\Downloads\\setup.exe';
  item.finalUrl = 'https://cdn.example.com/download?token=abc';
  await Promise.all([events.changed({ id: item.id, filename: { current: item.filename } }), events.changed({ id: item.id, mime: { current: 'application/octet-stream' } })]);
  await events.created(item);
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'post', 'cancel', 'erase']);
  assert.equal(calls[1][1].url, item.finalUrl);
});

test('late MIME discovery works and a rejected handoff is not repeatedly retried', async () => {
  const item = { ...download, url: 'https://example.com/resource.php', filename: '', mime: '', fileSize: -1 };
  const { events, calls } = worker({ accepts: false, items: [item] });
  await events.created(item);
  item.mime = 'application/zip';
  await events.changed({ id: item.id, mime: { current: item.mime } });
  await events.changed({ id: item.id, totalBytes: { current: 200000 } });
  assert.deepEqual(calls.map(call => call[0]), ['pause', 'post', 'resume']);
});

test('late events leave finished downloads, HTML waiting pages and removed items alone', async () => {
  for (const state of ['complete', 'interrupted', 'in_progress']) {
    const item = { ...download, state, url: 'https://example.com/your-file-is-loading-please-wait/', mime: 'text/html', filename: state === 'in_progress' ? 'wait.html' : 'file.zip' };
    const { events, calls } = worker({ items: [item] });
    await events.changed({ id: item.id, filename: { current: item.filename } });
    await events.changed({ id: 999, filename: { current: 'removed.zip' } });
    assert.equal(calls.length, 0);
  }
});

function downloadButton(page, attributes = {}) {
  page.document.baseURI = 'https://example.com/your-file-is-loading-please-wait/';
  const control = { getAttribute: name => attributes[name] ?? null, disabled: false };
  const event = page.event({ button: 0, target: { closest: () => control }, stopImmediatePropagation() { this.stopped = true; } });
  return { control, event, click: () => page.events.click(event) };
}

test('a dynamically inserted button_dm forwards the direct URL and filename', async () => {
  const page = overlayPage();
  const button = downloadButton(page, { 'data-href': '/files/resource.php?token=abc', download: 'setup.zip' });
  await button.click();
  assert.equal(page.sent[0].url, 'https://example.com/files/resource.php?token=abc');
  assert.equal(page.sent[0].filename, 'setup.zip');
  assert.ok(button.event.prevented && button.event.stopped);
});

test('button_dm supports simple location assignments and navigation calls', async () => {
  for (const onclick of ["location.href = '/file.zip';", "window.location = '/file.zip'; return false;", "window.location.assign('/file.zip');", "location.replace('/file.zip')"]) {
    const page = overlayPage();
    await downloadButton(page, { onclick }).click();
    assert.equal(page.sent[0].url, 'https://example.com/file.zip');
  }
});

test('button_dm keeps countdowns, token scripts, unsafe URLs and synthetic clicks in the page', async () => {
  for (const attributes of [{ onclick: 'generateToken(); startDownload();', 'data-url': '/file.zip' }, { href: '#' }, { href: 'javascript:startDownload()' }, { href: '/waiting-resource.php' }, { href: '/your-file-is-loading-please-wait/' }, {}]) {
    const page = overlayPage(), button = downloadButton(page, attributes);
    await button.click();
    assert.equal(page.sent.length, 0);
    assert.ok(!button.event.prevented);
  }
  for (const overrides of [{ isTrusted: false }, { ctrlKey: true }, { defaultPrevented: true }]) {
    const page = overlayPage(), button = downloadButton(page, { href: '/file.zip' });
    Object.assign(button.event, overrides);
    await button.click();
    assert.equal(page.sent.length, 0);
  }
  const page = overlayPage(), button = downloadButton(page, { href: '/file.zip' });
  button.control.disabled = true;
  await button.click();
  assert.equal(page.sent.length, 0);
});

test('button_dm falls back to the browser if the app rejects or the extension disconnects', async () => {
  for (const reply of [async () => ({ ok: false }), async () => { throw new Error('Disconnected'); }]) {
    const page = overlayPage(reply), navigated = [];
    page.location.assign = url => navigated.push(url);
    await downloadButton(page, { href: '/file.zip' }).click();
    assert.deepEqual(navigated, ['https://example.com/file.zip']);
  }
});

test('token POST handoff requires an updated desktop and preserves the encoded body', async () => {
  for (const online of [true, false]) {
    const { message, calls } = worker({ online });
    const result = await message({ type: 'download', url: 'https://server.example/download.php', post_data: 'token=secret%2Bvalue&filename=file.zip' });
    assert.equal(result.ok, true);
    const request = online ? calls.find(call => call[0] === 'post')[1] : calls.find(call => call[0] === 'native' && call[1].type === 'download')[1].download;
    assert.equal(request.post_data, 'token=secret%2Bvalue&filename=file.zip');
  }
  const old = worker({ postDownloads: false });
  assert.equal((await old.message({ type: 'download', url: 'https://example.com/file', post_data: 'token=secret' })).ok, false);
  assert.equal(old.calls.length, 0);
});

function postForm(page, { host = 'server53.safefilehosting.com', token = 'test+token', method = 'post' } = {}) {
  const listeners = new Map();
  let nativeSubmissions = 0;
  class Form {
    constructor() { this.tagName = 'FORM'; this.id = 'downloadForm'; this.method = method; this.action = `https://${host}/files/download.php`; }
    get elements() { return [{ name: 'token', value: token }, { name: 'url_str', value: token }, { name: 'filename', value: 'fixture.zip' }, { name: 'sdomain', value: 'igetintopc.com' }].map(input => ({ tagName: 'INPUT', type: 'hidden', ...input })); }
    addEventListener(name, fn) { listeners.set(name, fn); }
    removeEventListener(name) { listeners.delete(name); }
    dispatchEvent(event) {
      event.target = this;
      event.preventDefault = () => { event.defaultPrevented = true; };
      event.stopImmediatePropagation = () => {};
      if (listeners.has(event.type)) { const fn = listeners.get(event.type); listeners.delete(event.type); fn(event); }
      else page.events[event.type]?.(event);
      return !event.defaultPrevented;
    }
    submit() { nativeSubmissions++; }
  }
  page.context.HTMLFormElement = Form;
  vm.runInContext(fs.readFileSync('extension/form-hook.js', 'utf8'), page.context);
  return { form: new Form(), get nativeSubmissions() { return nativeSubmissions; } };
}

test('Adsume automatic form.submit preserves tokens and suppresses duplicate submissions', async () => {
  const page = overlayPage();
  page.location.hostname = 'adsume.com';
  const fixture = postForm(page);
  fixture.form.submit(); fixture.form.submit();
  await new Promise(resolve => setImmediate(resolve));
  fixture.form.submit();
  assert.equal(page.sent.length, 1);
  assert.equal(new URLSearchParams(page.sent[0].post_data).get('token'), 'test+token');
  assert.equal(page.sent[0].url, 'https://server53.safefilehosting.com/files/download.php');
  assert.equal(fixture.nativeSubmissions, 0);
});

test('POST hook restores the original browser form once if IDG fails', async () => {
  const page = overlayPage(async () => ({ ok: false }));
  page.location.hostname = 'adsume.com';
  const fixture = postForm(page);
  fixture.form.submit(); fixture.form.submit();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(page.sent.length, 1);
  assert.equal(fixture.nativeSubmissions, 1);
});

test('POST hook leaves missing tokens, credential URLs and GET forms untouched', async () => {
  for (const options of [{ host: 'user:password@example.com' }, { token: '' }, { method: 'get' }]) {
    const page = overlayPage(); page.location.hostname = 'adsume.com';
    const fixture = postForm(page, options);
    fixture.form.submit();
    assert.equal(page.sent.length, 0);
    assert.equal(fixture.nativeSubmissions, 1);
  }
});

test('native token POST fallback is never recaptured as a tokenless GET', async () => {
  const { events, calls } = worker();
  await events.created({ ...download, url: 'https://server53.safefilehosting.com/files/download.php', finalUrl: 'https://cdn.example.com/file.zip' });
  assert.equal(calls.length, 0);
});

test('other websites with explicit hidden download forms preserve all request parameters', async () => {
  const page = overlayPage(); page.location.hostname = 'downloads.example.org';
  const fixture = postForm(page, { host: 'files.example.org' });
  fixture.form.action = 'https://files.example.org/api/resource';
  Object.defineProperty(fixture.form, 'elements', { value: [
    { tagName: 'INPUT', type: 'hidden', name: 'file_name', value: 'fixture.7z' },
    { tagName: 'INPUT', type: 'hidden', name: 'ticket', value: 'opaque+ticket' },
    { tagName: 'INPUT', type: 'hidden', name: 'part', value: 'one' },
    { tagName: 'INPUT', type: 'hidden', name: 'part', value: 'two' },
  ] });
  fixture.form.submit();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(fixture.nativeSubmissions, 0);
  assert.equal(page.sent.length, 1);
  const fields = new URLSearchParams(page.sent[0].post_data);
  assert.equal(fields.get('ticket'), 'opaque+ticket');
  assert.deepEqual(fields.getAll('part'), ['one', 'two']);
});

test('uploads, login inputs, unsupported encoding and ordinary POST forms stay in the browser', () => {
  for (const change of [form => { form.id = 'login'; }, form => { form.enctype = 'multipart/form-data'; }, form => {
    Object.defineProperty(form, 'elements', { value: [...form.elements, { tagName: 'INPUT', type: 'password', name: 'password', value: 'private' }] });
  }, form => {
    Object.defineProperty(form, 'elements', { value: [...form.elements, { tagName: 'INPUT', type: 'file', name: 'upload', value: '' }] });
  }]) {
    const page = overlayPage(), fixture = postForm(page);
    change(fixture.form); fixture.form.submit();
    assert.equal(page.sent.length, 0);
    assert.equal(fixture.nativeSubmissions, 1);
  }
});

test('unrecognized native POST downloads are not converted to GET on any website', async () => {
  const { events, calls } = worker();
  const url = 'https://small-site.example/download.php';
  events.beforeRequest({ method: 'POST', tabId: 1, url });
  await events.created({ ...download, url, finalUrl: 'https://cdn.example/file.zip' });
  assert.equal(calls.length, 0);
  await events.created({ ...download, id: 18, url: 'https://other.example/file.zip' });
  assert.equal(calls.filter(call => call[0] === 'post').length, 1);
});

test('X post handoff canonicalizes aliases and forwards media cookies online and after shutdown', async () => {
  for (const online of [true, false]) {
    const { message, calls } = worker({ online });
    const response = await message({ type: 'download', url: 'https://twitter.com/Talal1/status/2113571139595510128/video/1?s=20' });
    assert.equal(response.ok, true);
    const request = online ? calls.find(c => c[0] === 'post')[1] : calls.find(c => c[0] === 'native' && c[1].type === 'download')[1].download;
    assert.equal(request.url, 'https://x.com/Talal1/status/2113571139595510128/video/1');
    assert.ok(request.media_cookies.startsWith('# Netscape HTTP Cookie File'));
  }
  const old = worker({ x: false });
  const response = await old.message({ type: 'download', url: 'https://x.com/Talal1/status/2113571139595510128/video/1' });
  assert.equal(response.ok, false);
  assert.match(response.error, /v1.4.7/);
  assert.equal(old.calls.some(c => c[0] === 'post'), false);
});

test('X URL detection rejects spoofed hosts, malformed IDs, and non-web schemes', () => {
  const { context } = worker();
  for (const value of ['https://x.com.evil.test/u/status/123', 'https://x.com/home', 'https://x.com/u/status/no', 'https://x.com/u/status/123/video/0', 'ftp://x.com/u/status/123']) {
    context.value = value;
    assert.equal(vm.runInContext('xVideoUrl(value)', context), null);
  }
});

test('X blob player sends the post URL and follows reused SPA players', async () => {
  const page = await overlayPage();
  Object.assign(page.location, { hostname: 'x.com', pathname: '/Talal1/status/2113571139595510128/video/1', href: 'https://x.com/Talal1/status/2113571139595510128/video/1' });
  page.media.currentSrc = 'blob:https://x.com/player';
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.sent[0].url, page.location.href);
  page.location.href = 'https://x.com/other/status/123/video/2';
  page.scan();
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.sent[1].url, page.location.href);
});

test('X timeline resolves the media article instead of downloading the timeline or another post', async () => {
  const page = await overlayPage();
  Object.assign(page.location, { hostname: 'x.com', pathname: '/home', href: 'https://x.com/home' });
  page.media.currentSrc = 'blob:https://x.com/player';
  page.media.closest = () => ({ querySelectorAll: () => [
    { href: 'https://x.com/person', pathname: '/person', querySelector: () => null },
    { href: 'https://x.com/Talal1/status/2113571139595510128', pathname: '/Talal1/status/2113571139595510128', querySelector: () => ({}) },
  ] });
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.sent[0].url, 'https://x.com/Talal1/status/2113571139595510128');
  page.media.closest = () => ({ querySelectorAll: () => [] });
  await page.overlay.children[0].handlers.click(page.event());
  assert.equal(page.sent.length, 1);
  assert.equal(page.overlay.children[0].textContent, 'No direct link');
});

test('context menu handles X video pages while ordinary links keep their own destination', async () => {
  const { events, calls } = worker();
  const tab = { url: 'https://x.com/Talal1/status/2113571139595510128/video/1' };
  await events.context({ menuItemId: 'idg-download-video', srcUrl: 'blob:https://x.com/player' }, tab);
  assert.equal(calls.find(c => c[0] === 'post')[1].url, tab.url);
  await events.context({ menuItemId: 'idg-download-link', linkUrl: 'https://example.org/file.zip' }, tab);
  assert.equal(calls.filter(c => c[0] === 'post')[1][1].url, 'https://example.org/file.zip');
});
