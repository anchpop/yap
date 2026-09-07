// Exercise the changed Callback ABI in real Yap WASM, using an isolated browser profile.
// Build the web-target package in generated/web first (see README). No account or application server required.
const { createRequire } = require('node:module');
const { createServer } = require('node:http');
const { readFile } = require('node:fs/promises');
const path = require('node:path');
const root = path.resolve(__dirname, 'generated/web');
const requireFrontend = createRequire(path.resolve(__dirname, '../../yap-frontend/package.json'));
const { chromium } = requireFrontend('@playwright/test');

(async () => {
  const server = createServer(async (req, res) => {
    try {
      const pathname = new URL(req.url, 'http://localhost').pathname;
      if (pathname === '/') {
        res.setHeader('Content-Type', 'text/html; charset=utf-8');
        res.end('<!doctype html><title>Yap binding smoke test</title>');
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
  let timeout;
  try {
    browser = await chromium.launch({ headless: true });
    timeout = setTimeout(() => browser.close(), 30000);
    const page = await browser.newPage();
    page.on("pageerror", error => console.error("browser error:", error));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    console.log(await page.evaluate(async () => {
      const { default: init, Weapon, ListenerKey } = await import('/yap_frontend_rs.js');
      await init();
      const check = (condition, label) => { if (!condition) throw Error(label); };
      let syncs = 0;
      const weapon = await Weapon.create(undefined, (key, stream) => {
        check(key instanceof ListenerKey && stream === 'reviews', 'typed callback arguments');
        key.free();
        syncs++;
      });
      const device = weapon.device_id;
      let notifications = 0;
      const listener = weapon.subscribe_to_stream('reviews', () => {
        check(weapon.device_id === device, 'callback reentrancy');
        notifications++;
      });
      weapon.request_reviews();
      check(syncs > 0 && notifications > 0, 'callbacks invoked');
      await weapon.load_from_local_storage('reviews');
      weapon.unsubscribe(listener);
      const before = notifications;
      weapon.request_reviews();
      check(notifications === before, 'unsubscribed callback');
      try {
        await weapon.get_deck_state({ nativeLanguage: 'English', targetLanguage: 'French' }, 0);
        throw Error('expected missing-pack error');
      } catch (error) { check(String(error).includes('not loaded'), 'portable error'); }
      check(weapon.user_id == null, 'optional getter');
      const beforeEvents = weapon.num_events;
      for (const [invalidEvent, message] of [['{', 'EOF'], ['{}', 'missing field']]) {
        try {
          weapon.add_remote_event('invalid-fixture', 'reviews', invalidEvent);
          throw Error('expected JSON error');
        } catch (error) { check(String(error).includes(message), 'JSON error preserved'); }
      }
      check(weapon.num_events === beforeEvents, 'invalid JSON adds no events');
      const event = { type: 'Language', target_language: 'French', native_language: 'English',
        content: { type: 'SetDailyReviewTarget', daily_review_target: 'Intense' } };
      weapon.add_deck_event(event);
      weapon.add_deck_event_at(event, Date.now() + 1000);
      check(weapon.num_events === beforeEvents + 2, 'typed events and numeric getter');
      check(weapon.get_stream_num_events('reviews') >= 2, 'optional numeric return');
      check(weapon.get_sync_state('supabase').lastSyncError == null, 'generic sync state');
      check(weapon.get_timestamp_of_earliest_unsynced_event('supabase') != null, 'timestamp record');
      await weapon.sync('reviews', undefined, false, undefined, false);
      const reopened = await Weapon.create(undefined, () => {});
      check(reopened.device_id === device, 'persistent device ID');
      reopened.request_reviews();
      await reopened.load_from_local_storage('reviews');
      check(reopened.num_events === weapon.num_events, 'Web Locks, save and reopen');
      reopened.free();
      weapon.free();
      return 'PASS: real Yap WASM factory, OPFS, typed callbacks, reentrancy, unsubscribe, errors, typed events/getters, Web Locks, save/reopen, and stable device ID in Chromium';
    }));
  } finally {
    clearTimeout(timeout);
    if (browser) await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
