const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(require.resolve('playwright-core', { paths: [process.cwd(), path.resolve('target/browser-check')] }));
(async () => {
  const context = await chromium.launchPersistentContext(path.resolve(`target/browser-check/spotify-page-${Date.now()}`), {
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
    const a = '3PsejBjTRczNY88SlcWUyg', b = '3uCdelPvIjIA0q0V6KRWFY', playlist = '0000000000000000000001';
    await context.route('https://open.spotify.com/**', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html>
      <style>body{background:#121212;color:white;font:16px system-ui;margin:30px}main{margin-left:300px}[role=row]{height:80px;display:flex;align-items:center;justify-content:space-between}aside{position:absolute;width:240px;left:20px;top:30px}[role=listitem]{height:80px}footer{position:absolute;bottom:30px}button{width:34px;height:34px}</style>
      <aside id="Desktop_LeftSidebar_Id"><div role="listitem" id="liked"><span data-testid="listrow-title">Liked Songs</span></div><div role="listitem" id="playlist"><span data-testid="listrow-title">My playlist</span></div></aside>
      <main><h1>Liked Songs</h1><div role="row" data-testid="tracklist-row" id="song"><span data-testid="tracklist-row-title">First song</span><span>2:46</span><button aria-haspopup="menu">...</button></div>
      <div role="row" data-testid="tracklist-row" id="ambiguous"><span>Conflicting IDs</span></div>
      <div role="row" data-testid="tracklist-row" id="getter"><span>Getter must not run</span></div></main>
      <footer><div data-testid="now-playing-widget" id="player"><span data-testid="context-item-info-title">First song</span></div></footer>
      <script>
        window.getterReads=0;
        song.__reactProps$test={children:{props:{track:{uri:'spotify:track:${a}'}}}};
        player.__reactFiber$test={memoizedProps:{item:{type:'track',id:'${a}'}}};
        liked.__reactProps$test={entity:{uri:'spotify:collection:tracks'}};
        document.getElementById('playlist').__reactProps$test={playlist:{uri:'spotify:playlist:${playlist}'}};
        ambiguous.__reactProps$test={children:[{props:{uri:'spotify:track:${a}'}},{props:{uri:'spotify:track:${b}'}}]};
        getter.__reactProps$test={};
        Object.defineProperty(getter.__reactProps$test,'track',{get(){window.getterReads++;return {uri:'spotify:track:${a}'}}});
        Object.defineProperty(song.__reactProps$test,'session',{get(){window.getterReads++;throw Error('private field read')}});
      </script>` }));
    const page = await context.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`https://open.spotify.com/collection/tracks?uri=spotify%3Atrack%3A${a}`);
    const first = page.getByRole('button', { name: 'Download First song with IDG', exact: true });
    await first.waitFor();
    assert.equal(await page.locator('[data-idg-spotify-row]:not([data-idg-spotify-library])').count(), 1);
    await first.click();
    assert.equal((await worker.evaluate(() => requests)).at(-1)[0], `https://open.spotify.com/track/${a}`);
    await page.getByRole('button', { name: 'Download all Liked Songs with IDG' }).click();
    assert.equal((await worker.evaluate(() => requests)).at(-1)[0], 'https://open.spotify.com/collection/tracks');
    await page.getByRole('button', { name: 'Download all My playlist with IDG' }).click();
    assert.equal((await worker.evaluate(() => requests)).at(-1)[0], `https://open.spotify.com/playlist/${playlist}`);
    // React props change without any link or browser URL changing. Read again
    // synchronously on the genuine click, before a scan can refresh the cache.
    await page.evaluate(id => document.addEventListener('click', () => {
      document.getElementById('song').__reactProps$test={track:{uri:'spotify:track:'+id}};
    }, { once: true, capture: true }), b);
    await first.click();
    assert.equal((await worker.evaluate(() => requests)).at(-1)[0], `https://open.spotify.com/track/${b}`);
    await page.evaluate(() => document.getElementById('song').remove());
    await page.getByRole('button', { name: 'Download this media with IDG', exact: true }).waitFor();
    await page.evaluate(id => {
      document.getElementById('player').__reactFiber$test.memoizedProps={item:{uri:'spotify:track:'+id}};
      document.querySelector('[data-testid="context-item-info-title"]').textContent='Second song';
    }, b);
    await page.waitForFunction(() => document.getElementById('player').getAttribute('data-idg-spotify-uri')?.endsWith('3uCdelPvIjIA0q0V6KRWFY'));
    await page.getByRole('button', { name: 'Download this media with IDG', exact: true }).click();
    assert.equal((await worker.evaluate(() => requests)).at(-1)[0], `https://open.spotify.com/track/${b}`);
    assert.equal(await page.evaluate(() => window.getterReads), 0);
    assert.ok(page.url().includes(a), 'page URL remains on the original song');
    // Query the same diagnostic response used by the popup in the active tab.
    const status = await worker.evaluate(async () => {
      const [tab] = await chrome.tabs.query({ url: 'https://open.spotify.com/*' });
      return chrome.tabs.sendMessage(tab.id, { type: 'getContentStatus' }, { frameId: 0 });
    });
    assert.equal(status.build, 'spotify-reader-20261025');
    assert.equal(status.reader, true);
    assert.equal(status.libraryButtons, 2);
    assert.deepEqual(errors, []);
    console.log('PASS: linkless React-prop rows/player/library, exact same-click reused row, fixed page URL with changing playback, ambiguous IDs rejected, credential fields/getters untouched, and current-page diagnostics.');
  } finally { await context.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
