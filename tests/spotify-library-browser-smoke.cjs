const path=require('node:path');
const assert=require('node:assert/strict');
const {chromium}=require(require.resolve('playwright-core',{paths:[process.cwd(),path.resolve('target/browser-check')]}));
(async()=>{
 const context=await chromium.launchPersistentContext(path.resolve(`target/browser-check/spotify-library-${Date.now()}`),{
  executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true,viewport:{width:900,height:650},
  args:[`--disable-extensions-except=${path.resolve('extension')}`,`--load-extension=${path.resolve('extension')}`,'--no-first-run'],
 });
 try{
  const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker');
  await worker.evaluate(async()=>{await settingsReady;globalThis.requests=[];sendDownloadToIDG=async(...args)=>{requests.push(args);return {ok:true};};});
  const playlistA='0123456789012345678901',playlistB='0123456789012345678902';
  await context.route('https://open.spotify.com/**',route=>route.fulfill({contentType:'text/html',body:`<!doctype html><title>Spotify Your Library</title>
   <style>*{box-sizing:border-box}body{margin:0;background:#000;color:#fff;font:18px system-ui}#Desktop_LeftSidebar_Id{width:520px;padding:14px;background:#121212;height:610px}h2{font-size:22px;color:#b3b3b3;margin:12px 10px 24px}.library{height:470px;overflow:auto}[role=listitem]{height:82px;position:relative;border-radius:7px}[role=listitem]:first-child{background:#282828}a{display:flex;gap:15px;align-items:center;padding:8px 10px;height:100%;color:#fff;text-decoration:none}.cover{width:60px;height:60px;display:grid;place-items:center;background:linear-gradient(140deg,#5c19ed,#b1e6e0);font-size:32px}.artist .cover{border-radius:50%;background:#26353a}.title{font-size:23px}.meta{display:block;color:#b3b3b3;font-size:16px;margin-top:3px}.speaker{position:absolute;right:14px;top:29px;color:#1ed760;width:22px}.extras{height:300px}</style>
   <nav id="Desktop_LeftSidebar_Id" aria-label="Your Library"><h2>Your Library</h2><div class="library" role="list">
    <div role="listitem" data-testid="rootlist-item" data-item="liked"><a href="/collection/tracks?uri=spotify%3Atrack%3A3PsejBjTRczNY88SlcWUyg"><span class="cover">&#9829;</span><span><span class="title" data-testid="listrow-title">Liked Songs</span><small class="meta">Playlist &#8226; 55 songs</small></span></a><span class="speaker">&#9835;</span></div>
    <div role="listitem" class="artist"><a href="/artist/0000000000000000000001"><span class="cover">&#9834;</span><span><span class="title">Young Madz</span><small class="meta">Artist</small></span></a></div>
    <div role="listitem" data-item="playlistA"><a href="/playlist/${playlistA}"><span class="cover">&#9834;</span><span><span class="title" data-testid="listrow-title">This Is Uzishetan</span><small class="meta">Playlist &#8226; Spotify</small></span></a></div>
    <div role="listitem" data-item="playlistB"><a href="/playlist/${playlistB}?si=ignored"><span class="cover">&#9834;</span><span><span class="title" data-testid="listrow-title">2000s Mix</span><small class="meta">Playlist &#8226; Made for you</small></span></a></div>
    <div role="listitem" class="artist"><a href="/artist/0000000000000000000002"><span class="cover">&#9834;</span><span><span class="title">Glizz</span><small class="meta">Artist</small></span></a></div>
    <div class="extras"></div></div></nav><main><h1>Playlist content</h1><a href="/playlist/0000000000000000000009">Unrelated main-page playlist</a></main>
   <script>window.libraryClicks=0;document.querySelectorAll('[role=listitem]').forEach(row=>row.addEventListener('click',()=>window.libraryClicks++));</script>`}));
  const page=await context.newPage();const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto('https://open.spotify.com/collection/tracks');
  const liked=page.getByRole('button',{name:'Download all Liked Songs with IDG',exact:true});await liked.waitFor();
  assert.equal(await page.locator('[data-idg-spotify-library]').count(),3);
  const box=await liked.boundingBox(),speaker=await page.locator('.speaker').boundingBox();
  assert.ok(box.x+box.width<speaker.x-6);assert.ok(Math.abs(box.y+box.height/2-(speaker.y+speaker.height/2))<8);
  await page.screenshot({path:'target/spotify-library-download-all.png',clip:await page.locator('#Desktop_LeftSidebar_Id').boundingBox()});
  for(const [title,url]of [['Liked Songs','https://open.spotify.com/collection/tracks'],['This Is Uzishetan',`https://open.spotify.com/playlist/${playlistA}`],['2000s Mix',`https://open.spotify.com/playlist/${playlistB}`]]){
   await page.getByRole('button',{name:`Download all ${title} with IDG`,exact:true}).click();
   assert.equal((await worker.evaluate(()=>requests)).at(-1)[0],url);
  }
  assert.equal(await page.evaluate(()=>window.libraryClicks),0);
  // A recycled sidebar item must stop targeting its previous playlist.
  await page.locator('[data-item=playlistB] a').evaluate(link=>{link.href='/artist/0000000000000000000003';link.querySelector('.title').textContent='Artist instead';});
  await page.getByRole('button',{name:'Download all 2000s Mix with IDG',exact:true}).waitFor({state:'detached'});
  assert.equal(await page.locator('[data-idg-spotify-library]').count(),2);
  await page.locator('.library').evaluate(list=>list.scrollTop=300);await liked.waitFor({state:'hidden'});
  await page.locator('.library').evaluate(list=>list.scrollTop=0);await liked.waitFor();
  await worker.evaluate(async()=>{pausedSites=['open.spotify.com'];await refreshSiteTabs();});
  await page.locator('[data-idg-spotify-library]').first().waitFor({state:'detached'});
  await worker.evaluate(async()=>{pausedSites=[];await refreshSiteTabs();});await liked.waitFor();
  assert.deepEqual(errors,[]);
  console.log('PASS: Your Library download-all buttons for playlists and Liked Songs, full-collection links without playback uri, artists/main-page links excluded, speaker preserved, virtualized-row cleanup, scroll clipping, pause/resume.');
 }finally{await context.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
