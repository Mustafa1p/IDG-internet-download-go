const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(require.resolve('playwright-core', { paths: [process.cwd(), path.resolve('target/browser-check')] }));
(async () => {
  const extension = path.resolve('extension');
  const context = await chromium.launchPersistentContext(path.resolve(`target/browser-check/spotify-${Date.now()}`), {
    executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true,
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`, '--no-first-run'],
  });
  try {
    const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
    await worker.evaluate(async () => {
      await settingsReady;
      globalThis.fixtureRequests = [];
      sendDownloadToIDG = async (...args) => { fixtureRequests.push(args); return { ok: true }; };
    });
    await context.route('https://open.spotify.com/**', route => route.fulfill({ contentType: 'text/html',
      body: '<!doctype html><title>Spotify without audio or video</title><main><h1>Track fixture</h1></main>' }));
    const page = await context.newPage();
    const url = 'https://open.spotify.com/track/3uCdelPvIjIA0q0V6KRWFY';
    await page.goto(url);
    const action = page.getByRole('button', { name: 'Download this media with IDG' });
    await action.waitFor();
    assert.equal(await page.locator('audio,video').count(), 0);
    assert.equal(await action.count(), 1);
    await action.click();
    await page.getByText('Opened in IDG', { exact: true }).waitFor();
    assert.equal((await worker.evaluate(() => fixtureRequests))[0][0], url);
    const next = 'https://open.spotify.com/track/0000000000000000000001';
    await page.evaluate(url => history.pushState({}, '', url), next);
    await page.getByText('Download with IDG', { exact: true }).waitFor();
    await action.click();
    assert.equal((await worker.evaluate(() => fixtureRequests)).at(-1)[0], next);
    await page.getByRole('button', { name: 'Hide IDG download button for this video' }).click();
    assert.equal(await action.count(), 0);
    await page.evaluate(() => history.pushState({}, '', '/search'));
    await page.getByText('Open a Spotify track', { exact: true }).waitFor();
    const countBeforeGuidance = (await worker.evaluate(() => fixtureRequests)).length;
    await action.click();
    await page.getByRole('alert').waitFor();
    assert.equal((await worker.evaluate(() => fixtureRequests)).length, countBeforeGuidance);
    const collectionShare = 'https://open.spotify.com/collection/tracks?uid=ignored&uri=spotify%3Atrack%3A3PsejBjTRczNY88SlcWUyg';
    await page.evaluate(url => history.pushState({}, '', url), collectionShare);
    await page.getByText('Download with IDG', { exact: true }).waitFor();
    await action.click();
    assert.equal((await worker.evaluate(() => fixtureRequests)).at(-1)[0], 'https://open.spotify.com/track/3PsejBjTRczNY88SlcWUyg');
    await page.evaluate(url => {
      history.pushState({}, '', '/collection/tracks');
      document.body.insertAdjacentHTML('beforeend', `<main><a href="/track/0000000000000000000009">Unrelated listed song</a></main><div data-testid="now-playing-widget"><a href="${url}">Playing song</a></div>`);
    }, url);
    await page.getByText('Download with IDG', { exact: true }).waitFor();
    await action.click();
    assert.equal((await worker.evaluate(() => fixtureRequests)).at(-1)[0], url);
    await page.evaluate(() => document.querySelector('[data-testid="now-playing-widget"]').remove());
    await page.getByText('Open a Spotify track', { exact: true }).waitFor();
    await page.evaluate(url => history.pushState({}, '', url), url);
    await action.waitFor();
    await worker.evaluate(() => { sendDownloadToIDG = async () => ({ ok: false, error: 'Open IDG Settings > Music to connect Spotify.' }); });
    await action.click();
    await page.getByRole('alert').getByText('Open IDG Settings > Music to connect Spotify.').waitFor();
    await worker.evaluate(async () => {
      pausedSites = ['open.spotify.com']; await refreshSiteTabs();
    });
    await page.locator('[data-idg-music-action]').waitFor({ state: 'detached' });
    await worker.evaluate(async () => { pausedSites = []; await refreshSiteTabs(); });
    await action.waitFor();
    await page.screenshot({ path: 'target/spotify-download-action.png' });
    console.log('PASS: Spotify button without audio/video, SPA URL handoff, one button only, dismissal, visible setup error, Liked Songs uri query and playing-track resolution without guessing list tracks, and site pause/resume.');
  } finally { await context.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
