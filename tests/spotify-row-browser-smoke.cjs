const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(require.resolve('playwright-core', { paths: [process.cwd(), path.resolve('target/browser-check')] }));
(async () => {
  const context = await chromium.launchPersistentContext(path.resolve(`target/browser-check/spotify-rows-${Date.now()}`), {
    executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true,
    viewport: { width: 1280, height: 900 },
    args: [`--disable-extensions-except=${path.resolve('extension')}`, `--load-extension=${path.resolve('extension')}`, '--no-first-run'],
  });
  try {
    const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
    await worker.evaluate(async () => {
      await settingsReady;
      globalThis.requests = [];
      sendDownloadToIDG = async (...args) => { requests.push(args); return { ok: true }; };
    });
    const songs = [
      ['I LOVE YOU FUNK - Slowed', '3PsejBjTRczNY88SlcWUyg', '2:46'],
      ['Boheme', '3uCdelPvIjIA0q0V6KRWFY', '2:54'],
      ['caramel', '0000000000000000000003', '2:43'],
      ['Habibi', '0000000000000000000004', '3:44'],
      ['Uber Driver', '0000000000000000000005', '3:54'],
      ['Solo', '0000000000000000000006', '2:46'],
      ['Arabic track', '0000000000000000000007', '4:10'],
    ];
    const rows = songs.map(([title, id, duration], index) => `<div role="row" data-testid="tracklist-row" data-row="${index}" ${index === 2 ? `data-uri="spotify:track:${id}"` : ''}>
      <span>${index + 1}</span><div class="song">${index === 2 ? `<span data-testid="tracklist-row-title">${title}</span>` : `<a href="${index === 3 ? '/collection/tracks?uri=spotify%3Atrack%3A' + id : '/track/' + id}">${title}</a>`}<small>Artist</small></div>
      <span>Album</span><span>1 day ago</span><span class="duration">${duration}</span><button aria-haspopup="menu" data-testid="more-button" aria-label="More options for ${title}">...</button></div>`).join('');
    await context.route('https://open.spotify.com/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><title>Liked Songs</title>
      <style>*{box-sizing:border-box}body{margin:0;background:#101010;color:white;font:16px system-ui}header{margin:20px 24px 0;padding:36px;background:linear-gradient(140deg,#6233b3,#342654);height:260px;display:flex;gap:28px;align-items:center}.art{width:180px;height:180px;display:grid;place-items:center;background:linear-gradient(135deg,#5314ef,#a9e8db);font-size:80px}h1{font-size:66px;margin:4px 0}.controls{padding:24px;font-size:28px;color:#1ed760}.list{margin:0 24px;height:480px;overflow:auto}.head,[role=row]{display:grid;grid-template-columns:32px minmax(200px,1fr) minmax(100px,.7fr) 150px 110px 34px;align-items:center;gap:8px;padding:0 8px}.head{height:36px;border-bottom:1px solid #333;color:#aaa}[role=row]{height:62px;border-radius:5px}[role=row]:hover{background:#252525}a{color:white;text-decoration:none}.song small{display:block;color:#aaa;margin-top:4px}.duration{justify-self:start;color:#bbb}[aria-haspopup=menu]{background:transparent;border:0;color:#aaa;width:32px;height:32px;font-size:20px}</style>
      <header><div class="art">&#9829;</div><div><span>Playlist</span><h1>Liked Songs</h1><strong>7 songs</strong></div></header><div class="controls">&#9654; &nbsp; &#8644;</div><div class="list"><div class="head"><span>#</span><span>Title</span><span>Album</span><span>Date added</span><span>Duration</span></div>${rows}</div>
      <script>window.rowClicks=0;document.querySelectorAll('[role=row]').forEach(row=>row.addEventListener('click',()=>window.rowClicks++));</script>` }));
    const page = await context.newPage();
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.goto('https://open.spotify.com/collection/tracks');
    await page.getByRole('button', { name: 'Download I LOVE YOU FUNK - Slowed with IDG', exact: true }).waitFor();
    assert.equal(await page.locator('[data-idg-spotify-row]').count(), 7);
    assert.equal(await page.locator('[data-idg-music-action]').count(), 0);
    await page.screenshot({ path: 'target/spotify-row-download-buttons.png' });
    await page.evaluate(() => document.body.insertAdjacentHTML('beforeend', '<footer><div data-testid="now-playing-widget"><a href="/track/3uCdelPvIjIA0q0V6KRWFY">Different playing song</a></div></footer>'));
    for (const [index, [title, id]] of songs.entries()) {
      const button = page.getByRole('button', { name: `Download ${title} with IDG`, exact: true });
      const box = await button.boundingBox();
      const row = page.locator(`[data-row="${index}"]`);
      const duration = await row.locator('.duration').boundingBox();
      const menu = await row.locator('[aria-haspopup=menu]').boundingBox();
      assert.ok(box.x >= duration.x + duration.width + 4, title + ': after duration');
      assert.ok(box.x + box.width <= menu.x - 4, title + ': before native menu');
      assert.ok(Math.abs(box.y + box.height / 2 - (duration.y + duration.height / 2)) < 3, title + ': vertically aligned');
      await button.click();
      assert.equal((await worker.evaluate(() => requests)).at(-1)[0], `https://open.spotify.com/track/${id}`);
    }
    assert.equal(await page.evaluate(() => window.rowClicks), 0);
    // Change the row in document capture, before the button's click handler
    // and before MutationObserver/animation-frame rescanning can update caches.
    await page.evaluate(() => document.addEventListener('click', () => {
      const link=document.querySelector('[data-row="0"] a');
      link.href='/track/0000000000000000000008';link.textContent='Reused row track';
    }, { once:true, capture:true }));
    await page.getByRole('button', { name: 'Download I LOVE YOU FUNK - Slowed with IDG', exact:true }).click();
    const reused = page.getByRole('button', { name: 'Download Reused row track with IDG', exact: true });
    await reused.waitFor();
    assert.equal((await worker.evaluate(() => requests)).at(-1)[0], 'https://open.spotify.com/track/0000000000000000000008');
    assert.equal(await page.locator('[data-idg-spotify-row]').count(), 7);
    await page.locator('.list').evaluate(list => list.style.height='150px');
    await page.locator('.list').evaluate(list => list.scrollTop=250);
    await reused.waitFor({ state: 'hidden' });
    await page.locator('.list').evaluate(list => list.scrollTop=0);
    await reused.waitFor({ state: 'visible' });
    await worker.evaluate(async () => { pausedSites=['open.spotify.com'];await refreshSiteTabs(); });
    await page.locator('[data-idg-spotify-row]').first().waitFor({ state: 'detached' });
    await worker.evaluate(async () => { pausedSites=[];await refreshSiteTabs(); });
    await reused.waitFor();
    assert.deepEqual(errors, []);
    console.log('PASS: seven Spotify row buttons after durations, native menus preserved, each exact row track handed off independently of playback and the collection URL, URI rows, same-click reused-row changes, scroll clipping, no floating duplicate, site pause/resume.');
  } finally { await context.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
