const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(require.resolve('playwright-core', { paths: [process.cwd(), path.resolve('target/browser-check')] }));
(async () => {
  const context = await chromium.launchPersistentContext(path.resolve(`target/browser-check/spotify-playback-${Date.now()}`), {
    executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true,
    args: [`--disable-extensions-except=${path.resolve('extension')}`, `--load-extension=${path.resolve('extension')}`, '--no-first-run'],
  });
  try {
    const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
    await worker.evaluate(async () => {
      await settingsReady;
      globalThis.requests=[];
      sendDownloadToIDG=async (...args)=>{requests.push(args);return {ok:true};};
    });
    const A='3PsejBjTRczNY88SlcWUyg', B='3uCdelPvIjIA0q0V6KRWFY', C='0000000000000000000003';
    const canonical=id=>`https://open.spotify.com/track/${id}`;
    const pageUrl=`https://open.spotify.com/collection/tracks?uri=spotify%3Atrack%3A${A}`;
    await context.route('https://open.spotify.com/**',route=>route.fulfill({contentType:'text/html',body:`<!doctype html><title>Fixed URL playback changes</title>
      <footer><div data-testid="now-playing-widget"><span data-testid="context-item-info-title"><a href="/track/${A}">Song A</a></span></div></footer>
      <aside data-testid="now-playing-view"><a href="/track/0000000000000000000009">Stale sidebar song</a></aside>`}));
    const page=await context.newPage();
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    await page.goto(pageUrl);
    const action=page.getByRole('button',{name:'Download this media with IDG'});
    const last=async()=> (await worker.evaluate(()=>requests)).at(-1)[0];
    await action.click();assert.equal(await last(),canonical(A));
    // The song changes; the collection URL and its old uri parameter stay fixed.
    await page.locator('footer a').evaluate((link,id)=>{link.href='/track/'+id;link.firstChild.data='Song B';},B);
    await page.getByText('Download with IDG',{exact:true}).waitFor();
    await action.click();assert.equal(await last(),canonical(B));assert.equal(page.url(),pageUrl);
    // A title update can arrive before its new link. Never hand off the old link.
    await page.locator('footer a').evaluate(link=>link.firstChild.data='Song A');
    await page.getByText('Open a Spotify track',{exact:true}).waitFor();
    const count=(await worker.evaluate(()=>requests)).length;
    await action.click();assert.equal((await worker.evaluate(()=>requests)).length,count);
    await page.locator('footer a').evaluate((link,id)=>link.href='/track/'+id,A);
    await page.getByText('Download with IDG',{exact:true}).waitFor();
    await action.click();assert.equal(await last(),canonical(A));
    // In the reverse order, the later title update must not invalidate a fresh link.
    await page.locator('footer a').evaluate((link,id)=>link.href='/track/'+id,B);
    await page.getByText('Download with IDG',{exact:true}).waitFor();
    await action.click();assert.equal(await last(),canonical(B));
    await page.locator('footer a').evaluate(link=>link.firstChild.data='Song B');
    await page.getByText('Opened in IDG',{exact:true}).waitFor();
    await action.click();assert.equal(await last(),canonical(B));
    // MediaSession can announce a track before a DOM mutation or URL navigation.
    await page.evaluate(()=>navigator.mediaSession.metadata=new MediaMetadata({title:'Song C',artist:'Artist'}));
    await page.getByText('Open a Spotify track',{exact:true}).waitFor();
    const before=(await worker.evaluate(()=>requests)).length;
    await action.click();assert.equal((await worker.evaluate(()=>requests)).length,before);
    await page.locator('footer a').evaluate((link,id)=>{link.removeAttribute('href');link.setAttribute('data-track-uri','spotify:track:'+id);link.firstChild.data='Song C';},C);
    await page.getByText('Download with IDG',{exact:true}).waitFor();
    await action.click();assert.equal(await last(),canonical(C));
    assert.equal(page.url(),pageUrl);
    // A track page can also remain open while Spotify plays a different song.
    await page.evaluate(url => history.replaceState({}, '', url), canonical(A));
    await action.click();assert.equal(await last(),canonical(C));
    assert.equal(page.url(),canonical(A));
    // A stale side panel cannot replace a temporarily detached primary player.
    await page.locator('footer').evaluate(footer=>footer.remove());
    await page.getByText('Open a Spotify track',{exact:true}).waitFor();
    const noFooter=(await worker.evaluate(()=>requests)).length;
    await action.click();assert.equal((await worker.evaluate(()=>requests)).length,noFooter);
    assert.deepEqual(errors,[]);
    console.log('PASS: unchanged Spotify collection URL, stale uri ignored after playback change, footer priority over stale sidebar, both link/title update orders, text-only and MediaSession changes, data-track-uri, missing-player safety.');
  } finally {await context.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
