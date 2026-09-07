// Uses the repo's existing Playwright installation; no application server or account needed.
const { createRequire } = require('node:module');
const { createServer } = require('node:http');
const { readFile } = require('node:fs/promises');
const path = require('node:path');
const requireFrontend = createRequire(path.resolve(__dirname, '../../../yap-frontend/package.json'));
const { chromium } = requireFrontend('@playwright/test');
const root = path.resolve(__dirname, '..');

(async () => {
  const server = createServer(async (req, res) => {
    try {
      const pathname = new URL(req.url, 'http://localhost').pathname;
      if (pathname === '/') {
        res.setHeader('Content-Type', 'text/html; charset=utf-8');
        res.end('<!doctype html><title>bridgerton browser tests</title>');
        return;
      }
      const file = path.resolve(root, `.${pathname}`);
      if (!file.startsWith(root + path.sep)) { res.writeHead(403).end(); return; }
      res.setHeader('Content-Type', file.endsWith('.wasm') ? 'application/wasm' : 'text/javascript; charset=utf-8');
      res.end(await readFile(file));
    } catch { res.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let browser;
  try {
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.addScriptTag({ url: '/tests/values.js' });
    const report = await page.evaluate(async () => {
      const { default: init, Counter } = await import('/generated/web/bridge_fixture.js');
      await init();
      const check = (condition, message) => { if (!condition) throw Error(message); };
      const counter = new Counter();
      await testValues(counter);
      let callbackValue;
      counter.observe(value => {
        callbackValue = value;
        check(counter.value() === value, 'reentrant value');
        counter.clear_observer();
      });
      check(counter.add(7) === 7 && callbackValue === 7, 'callback');
      check(counter.label() === 'Yap 語 — 7', 'UTF-8');
      const token = new AbortController();
      check(await counter.add_later(3, 5, token.signal) === 10, 'async');
      const cancelledToken = new AbortController();
      const pending = counter.add_later(100, 80, cancelledToken.signal);
      await new Promise(resolve => setTimeout(resolve, 5));
      cancelledToken.abort();
      try { await pending; throw Error('expected cancellation'); }
      catch (error) { check(String(error).includes('operation aborted'), 'cancellation error'); }
      check(counter.active_operations() === 0 && counter.value() === 10, 'cancel cleanup');
      await Promise.all(Array.from({ length: 24 }, (_, i) => counter.add_later(1, i % 5, token.signal)));
      check(counter.value() === 34 && counter.active_operations() === 0, 'interleaving');

      counter.free();
      return 'PASS: Chromium loads the real wasm-bindgen module; callback, UTF-8, async, cancellation, interleaving';
    });
    console.log(report);
  } finally {
    if (browser) await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
