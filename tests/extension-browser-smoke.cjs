// npm install --prefix target/browser-check playwright-core --no-audit --no-fund
const path = require('node:path');
const { chromium } = require(require.resolve('playwright-core', { paths: [process.cwd(), path.resolve('target/browser-check')] }));
const assert = require('node:assert/strict');
(async () => {
  const extension = path.resolve('extension');
  const context = await chromium.launchPersistentContext(path.resolve(`target/browser-check/profile-${Date.now()}`), {
    executablePath: process.env.IDG_TEST_BROWSER || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true,
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`, '--no-first-run'],
  });
  try {
    const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker', { timeout: 15000 });
    await worker.evaluate(() => {
      globalThis.fixtureRequests = [];
      sendDownloadToIDG = async (...args) => { globalThis.fixtureRequests.push(args); return { ok: true }; };
    });
    let browserPosts = 0;
    await context.route('https://server53.safefilehosting.com/**', route => { browserPosts++; return route.fulfill({ status: 200, body: 'Unexpected native submission' }); });
    await context.route('https://adsume.com/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><title>POST handoff fixture</title><form id="downloadForm" method="POST" action="https://server53.safefilehosting.com/files/download.php"><input type="hidden" name="token" value="fixture+token"><input type="hidden" name="filename" value="fixture.zip"></form><script>document.getElementById('downloadForm').submit();document.getElementById('downloadForm').submit();</script>` }));
    const page = await context.newPage();
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.goto('https://adsume.com/your-file-is-loading-please-wait/');
    await page.waitForTimeout(500);
    const requests = await worker.evaluate(() => globalThis.fixtureRequests);
    assert.equal(requests.length, 1, JSON.stringify({requests,browserPosts,errors}));
    assert.equal(requests[0][4], 'token=fixture%2Btoken&filename=fixture.zip');
    assert.equal(browserPosts, 0);
    assert.deepEqual(errors, []);
    await context.route('https://downloads.example.org/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><form id="download_form" method="POST" action="https://files.example.org/api/resource"><input type="hidden" name="file_name" value="fixture.7z"><input type="hidden" name="ticket" value="opaque+ticket"><input type="hidden" name="part" value="one"><input type="hidden" name="part" value="two"></form><script>document.getElementById('download_form').requestSubmit();</script>` }));
    await worker.evaluate(() => { globalThis.fixtureRequests = []; });
    const genericPage = await context.newPage();
    await genericPage.goto('https://downloads.example.org/start');
    await genericPage.waitForTimeout(300);
    const genericRequests = await worker.evaluate(() => globalThis.fixtureRequests);
    assert.equal(genericRequests.length, 1);
    assert.equal(genericRequests[0][0], 'https://files.example.org/api/resource');
    assert.deepEqual(new URLSearchParams(genericRequests[0][4]).getAll('part'), ['one', 'two']);
    await genericPage.close();
    await context.route('https://x.com/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><article><a href="/Talal1/status/2113571139595510128"><time>Today</time></a><div><video style="width:320px;height:180px"></video></div></article><script>document.querySelector('video').src=URL.createObjectURL(new Blob(['fixture'],{type:'video/mp4'}));</script>` }));
    await worker.evaluate(() => { globalThis.fixtureRequests = []; });
    const xPage = await context.newPage();
    await xPage.goto('https://x.com/home');
    await xPage.getByRole('button', { name: 'Download this media with IDG' }).click();
    await xPage.getByText('Opened in IDG', { exact: true }).waitFor();
    const xRequests = await worker.evaluate(() => globalThis.fixtureRequests);
    assert.equal(xRequests.length, 1);
    assert.equal(xRequests[0][0], 'https://x.com/Talal1/status/2113571139595510128');
    await xPage.close();
    for (const disabled of [false, true]) {
      await worker.evaluate(disabled => {
        globalThis.fixtureRequests = [];
        interceptEnabled = !disabled;
        sendDownloadToIDG = async (...args) => { globalThis.fixtureRequests.push(args); return { ok: false }; };
      }, disabled);
      browserPosts = 0;
      const fallbackPage = await context.newPage();
      await fallbackPage.goto('https://adsume.com/your-file-is-loading-please-wait/');
      await fallbackPage.waitForURL('https://server53.safefilehosting.com/files/download.php');
      await fallbackPage.waitForTimeout(200);
      assert.equal(browserPosts, 1);
      const fallbackRequests = await worker.evaluate(() => globalThis.fixtureRequests);
      assert.equal(fallbackRequests.length, disabled ? 0 : 1);
      await fallbackPage.close();
    }
    console.log('Real Edge MV3 tests passed: X timeline blob-player post handoff, MAIN/isolated-world token handoff, cross-site requestSubmit and repeated fields, duplicate suppression, rejected handoff fallback, and disabled-interception fallback.');
  } finally { await context.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
