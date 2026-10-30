const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(require.resolve('playwright-core', { paths: [process.cwd(), path.resolve('target/browser-check')] }));
(async () => {
  const context = await chromium.launchPersistentContext(path.resolve(`target/browser-check/media-${Date.now()}`), {
    executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true,
    viewport: { width: 1000, height: 700 },
    args: [`--disable-extensions-except=${path.resolve('extension')}`, `--load-extension=${path.resolve('extension')}`, '--no-first-run'],
  });
  try {
    const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
    await worker.evaluate(async () => {
      await settingsReady;
      globalThis.requests = [];
      globalThis.liveSendDownloadToIDG = sendDownloadToIDG;
      sendDownloadToIDG = async (...args) => { requests.push(args); return { ok: true }; };
    });
    const cases = [
      ['YouTube', 'https://www.youtube.com/watch?v=5TBlPQrcweQ', 'https://www.youtube.com/watch?v=BaW_jenozKc'],
      ['X', 'https://x.com/Talal1/status/2113571139595510128/video/1', 'https://x.com/other/status/123/video/1'],
      ['Instagram', 'https://www.instagram.com/reel/Chunk8-jurw/', 'https://www.instagram.com/reel/OtherPost/'],
      ['TikTok', 'https://www.tiktok.com/@patroxofficial/video/6742501081818877190', 'https://www.tiktok.com/@other/video/123'],
    ];
    for (const [name, url, next] of cases) {
      await context.route(new URL(url).origin + '/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html>
        <style>body{margin:0;padding:60px 30px;height:1800px;background:#162033;color:white;font:16px system-ui}
        .player{position:relative;width:360px;height:160px;overflow:hidden;transform:translateZ(0);pointer-events:none}
        video{width:360px;height:180px;background:#334155}
        button{pointer-events:none!important;opacity:0!important;display:none!important}
        [data-idg-overlay]{display:none!important;pointer-events:none!important;z-index:-1!important}
        .shield{position:absolute;inset:0;z-index:2147483647;pointer-events:auto}</style>
        <h1>${name} player fixture</h1><div class="player"><video></video><div class="shield"></div></div>` }));
      const page = await context.newPage();
      const errors = []; page.on('pageerror', error => errors.push(error.message));
      await page.goto(url);
      const action = page.getByRole('button', { name: 'Download this media with IDG' });
      await action.waitFor();
      const hitbox = await action.boundingBox();
      assert.ok(hitbox.width > 140, 'The full green download area must be clickable');
      await action.click({ position: { x: hitbox.width - 8, y: 12 }, timeout: 5000 });
      await page.getByText('Opened in IDG', { exact: true }).waitFor();
      assert.equal((await worker.evaluate(() => requests)).at(-1)[0], url);
      assert.equal(await action.count(), 1);
      // Scrolling hides controls for an offscreen player and restores them on return.
      await page.evaluate(() => window.scrollTo(0, 900));
      await action.waitFor({ state: 'hidden' });
      await page.evaluate(() => window.scrollTo(0, 0));
      await action.waitFor({ state: 'visible' });
      // The site can remove controls or move its player without leaving a dead button.
      await page.evaluate(() => document.querySelector('[data-idg-overlay]').remove());
      await action.waitFor();
      await page.getByRole('button', { name: 'Hide IDG download button for this video' }).click();
      await action.waitFor({ state: 'detached' });
      await page.evaluate(url => history.pushState({}, '', url), next);
      await action.waitFor();
      await action.click();
      await page.getByText('Opened in IDG', { exact: true }).waitFor();
      assert.equal((await worker.evaluate(() => requests)).at(-1)[0], next);
      await worker.evaluate(() => { sendDownloadToIDG = async () => ({ ok: false, error: 'IDG is offline. Open the desktop app.' }); });
      await action.click();
      await page.getByRole('alert').getByText('IDG is offline. Open the desktop app.').waitFor();
      await page.screenshot({ path: `target/media-button-${name.toLowerCase()}.png` });
      assert.deepEqual(errors, []);
      await worker.evaluate(() => { sendDownloadToIDG = async (...args) => { requests.push(args); return { ok: true }; }; });
      await page.close();
      console.log(`PASS ${name}: natural click above clipped/player-blocked content and hostile button CSS; scroll visibility, removed-control recovery, SPA navigation, dismissal and visible desktop errors.`);
    }
    if (process.env.IDG_TEST_LIVE_DESKTOP === '1') {
      const health = await (await fetch('http://127.0.0.1:17843/health')).json();
      assert.equal(health.capabilities.music, true);
      const before = await (await fetch('http://127.0.0.1:17843/downloads')).json();
      const oldIds = new Set(before.map(item => item.id));
      await worker.evaluate(() => { sendDownloadToIDG = liveSendDownloadToIDG; });
      const url = 'https://open.spotify.com/collection/tracks?uid=ignored&uri=spotify%3Atrack%3A3PsejBjTRczNY88SlcWUyg';
      const expectedUrl = 'https://open.spotify.com/track/3PsejBjTRczNY88SlcWUyg';
      await context.route('https://open.spotify.com/**', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><h1>Real IDG desktop handoff test</h1>' }));
      const page = await context.newPage();
      await page.goto(url);
      await page.getByRole('button', { name: 'Download this media with IDG' }).click();
      await page.getByText('Opened in IDG', { exact: true }).waitFor();
      const after = await (await fetch('http://127.0.0.1:17843/downloads')).json();
      const created = after.filter(item => !oldIds.has(item.id) && item.url === expectedUrl);
      assert.equal(created.length, 1);
      console.log(`PASS real desktop: a natural Spotify page-button click reached IDG ${health.version}; exactly one request is ${created[0].status}. No download was auto-started.`);
      await page.close();
    }
  } finally { await context.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
