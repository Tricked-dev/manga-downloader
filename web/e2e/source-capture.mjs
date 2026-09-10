// Runs the production browser capture against delayed and paginated local pages.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const rawkuma = await readFile(new URL('../../crates/sources/src/rawkuma/mod.rs', import.meta.url), 'utf8');
const selector = rawkuma.match(/self\.document\(url\.into\(\), "([^"]+)"\)/)[1];
const source = await readFile(new URL('../../crates/sources/src/comix/browser_capture.rs', import.meta.url), 'utf8');
const captureScript = source.match(/pub const CHAPTER_LIST_SCRIPT: &str = r#"([\s\S]*?)"#;/)?.[1]
  ?? await readFile(new URL('../../crates/sources/src/comix/chapter-capture.js', import.meta.url), 'utf8');
const browser = await chromium.launch({executablePath:process.env.CHROMIUM_EXECUTABLE_PATH, headless:true});
let failed = false;
async function test(name, run) {
  const context = await browser.newContext();
  try { await run(await context.newPage()); console.log('PASS', name); }
  catch(error) { failed = true; console.error('FAIL', name, error.message); }
  finally { await context.close(); }
}
try {
  for (const empty of [false, true]) await test(`Rawkuma waits for ${empty ? 'empty state' : 'search results'}`, async page => {
    await page.route('https://capture.test/**', route => route.fulfill({contentType:'text/html', body:`<div id="search-results"></div>`}));
    await page.goto('https://capture.test/library/');
    assert.equal(await page.evaluate(selector => !!document.querySelector(selector), selector), false, 'loading container was accepted');
    await page.evaluate(empty => setTimeout(() => {
      document.querySelector('#search-results').innerHTML = empty ? '<div><p>No results found for your search.</p></div>' : '<div><a href="/manga/example/">Example</a></div>';
    }, 30), empty);
    await page.waitForFunction(selector => document.readyState !== 'loading' && !!document.querySelector(selector), selector, {timeout:1000});
    assert.match(await page.content(), empty ? /No results found/ : /Example/);
  });
  for (const transport of ['fetch', 'xhr', 'decoded']) await test(`Comix preserves signed ${transport} requests and captures every page`, async page => {
    const requested = [];
    await page.addInitScript(captureScript);
    await page.route('https://capture.test/**', async route => {
      const url = new URL(route.request().url());
      if (url.pathname.endsWith('/chapters')) {
        const number = Number(url.searchParams.get('page')); requested.push(number);
        if (url.searchParams.get('limit') !== '20' || url.searchParams.get('_token') !== `signed-${number}`) {
          await route.fulfill({status:403, json:{message:'Invalid token.'}}); return;
        }
        const value = {result:{items:[{id:number,mangaId:42,number}],meta:{page:number,lastPage:2,hasNext:number<2,total:2}}};
        await route.fulfill({json:transport === 'decoded' ? {encoded:JSON.stringify(value)} : value}); return;
      }
      await route.fulfill({contentType:'text/html', body:`<section class="mpage__chapters"><div id="chapter-list"></div><nav class="npager"><button class="npager__num" aria-current="page">1</button><button class="npager__num">2</button></nav></section><script>
      async function load(number) {
        const url = '/api/v1/manga/test/chapters?page='+number+'&limit=20&_token=signed-'+number;
        const text = ${transport === 'xhr' ? "await new Promise(resolve => { const xhr = new XMLHttpRequest(); xhr.open('GET', url); xhr.onload=()=>resolve(xhr.responseText); xhr.send(); })" : "await (await fetch(url)).text()"};
        let value = JSON.parse(text);
        ${transport === 'decoded' ? 'value=JSON.parse(value.encoded);' : ''}
        if (!value.result) return;
        document.querySelector('#chapter-list').textContent = 'Chapter '+number;
        for(const button of document.querySelectorAll('.npager__num')) button.toggleAttribute('aria-current', Number(button.textContent) === number);
      }
      for(const button of document.querySelectorAll('.npager__num')) button.onclick=()=>load(Number(button.textContent));
      load(1);
      </script>`});
    });
    await page.goto('https://capture.test/title/test');
    await page.waitForFunction(() => window.__mangaServerCaptureDone, undefined, {timeout:2500});
    const payloads = await page.evaluate(() => window.__mangaServerPayloads.map(JSON.parse));
    assert.deepEqual(payloads.map(value => value.result.meta.page), [1,2]);
    assert.deepEqual(requested, [1,2]);
  });
} finally { await browser.close(); }
if (failed) process.exitCode = 1;
