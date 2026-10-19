const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const crypto = require('node:crypto');

function worker({ online = true, accepts = true, enabled = true, native = true, youtube = true } = {}) {
  const events = {}, calls = [];
  const event = name => ({ addListener: listener => { events[name] = listener; } });
  const chrome = {
    runtime: {
      id: 'idg-test', onInstalled: event('installed'), onMessage: event('message'),
      sendNativeMessage(name, message, callback) {
        calls.push(['native', message]);
        if (!native) this.lastError = { message: 'Native host missing' };
        callback(native ? { ok: accepts, installed: true, serverOnline: online, capabilities: { youtube } } : undefined);
        delete this.lastError;
      },
    },
    storage: { local: { get: async () => ({ interceptEnabled: enabled }), set: async value => calls.push(['settings', value]) } },
    action: { setBadgeText: async () => {}, setBadgeBackgroundColor: async () => {}, setTitle: async () => {} },
    cookies: { getAll: async () => [{ name: 'session', value: 'test-cookie' }] },
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
    if (url.endsWith('/health')) {
      if (!online) throw new Error('Offline');
      return { ok: true, json: async () => ({ status: 'ok', version: '1.2.0', capabilities: { youtube } }) };
    }
    calls.push(['post', JSON.parse(options.body)]);
    return { ok: accepts, status: accepts ? 201 : 500 };
  } });
  vm.runInContext(fs.readFileSync('extension/background.js', 'utf8'), context);
  const message = value => new Promise(resolve => events.message(value, { id: 'idg-test' }, resolve));
  return { events, calls, message, context };
}
const download = { id: 17, url: 'https://example.com/file.zip', filename: 'C:\\Downloads\\file.zip', referrer: 'https://example.com/source', mime: 'application/zip', fileSize: 200000 };

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
test('YouTube blob-player button follows the current SPA video URL', async () => {
  const sent = [], elements = [];
  const parent = { style: {}, appendChild: element => elements.push(element) };
  const media = { parentElement: parent, currentSrc: 'blob:https://www.youtube.com/player', querySelector: () => null };
  const location = { hostname: 'www.youtube.com', pathname: '/watch', href: 'https://www.youtube.com/watch?v=5TBlPQrcweQ&list=radio' };
  const document = { readyState: 'complete', documentElement: {}, title: 'Sample video - YouTube',
    querySelectorAll: () => [media], addEventListener() {},
    createElement: tag => ({ tag, style: {}, handlers: {}, setAttribute() {}, addEventListener(name, handler) { this.handlers[name] = handler; }, remove() {} }),
  };
  const context = vm.createContext({ document, location, window: {}, URL, MutationObserver: class { observe() {} }, getComputedStyle: () => ({ position: 'relative' }),
    chrome: { runtime: { sendMessage: async message => { sent.push(message); return { ok: true }; } } },
  });
  vm.runInContext(fs.readFileSync('extension/content.js', 'utf8'), context);
  await elements[0].handlers.click({ isTrusted: true, preventDefault() {}, stopPropagation() {} });
  assert.equal(sent[0].url, location.href);
  assert.equal(sent[0].filename, 'Sample video.mp4');
  location.href = 'https://www.youtube.com/watch?v=BaW_jenozKc';
  await elements[0].handlers.click({ isTrusted: true, preventDefault() {}, stopPropagation() {} });
  assert.equal(sent[1].url, location.href);
});
