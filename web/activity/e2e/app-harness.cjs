// L'activité entière, hors de Discord, sur plusieurs écrans (ordinateur, Z Fold 6 plié et déplié,
// téléphones, tablette) : captures pour vérifier la mise en page. API simulée.
// Serveur : npx vite --port 5199 (web/activity).
// node web/activity/e2e/app-harness.cjs <dossier .ftl> <captures> [appareil…]
const { chromium, devices } = require('playwright');
const fs = require('fs');
const path = require('path');
const [ftlDir, shots, ...only] = process.argv.slice(2);

const SCREENS = {
  ordinateur: { viewport: { width: 1280, height: 800 } },
  'discord-pc': { viewport: { width: 1000, height: 640 } },
  'fold6-deplie': devices['Galaxy Z Fold 6'],
  'fold6-deplie-paysage': devices['Galaxy Z Fold 6 landscape'],
  'fold6-plie': devices['Galaxy Z Fold 6 Cover'],
  'fold6-plie-paysage': devices['Galaxy Z Fold 6 Cover landscape'],
  iphone: devices['iPhone 15 Pro Max'],
  'ipad-mini': devices['iPad Mini'],
};

const now = Math.floor(Date.now() / 1000);
const session = {
  access_token: 'a', session_token: 't', expires_in: 3600,
  user: { id: '42', username: 'testeur', global_name: 'Testeur', avatar: null },
  guild_id: '1', universe: { id: 'u', name: 'Les Terres de Moria' }, access: 'admin',
  can_import: true, is_creator: true, fog_enabled: false, admin_mode_available: true,
  universe_clock: { origin: now - 3600 * 5, phase_duration_secs: 3600 * 6 }, server_time: now,
};
const location = { kind: 'place', id: '26', name: 'Kerival', source_id: null, destination_id: null, destination_name: null };
const character = {
  id: 'c', name: 'Aldric de Kerival', description: 'Marchand itinérant, connu pour ses épices rares et sa langue bien pendue.',
  image_url: null, speed: 5, server_time: now, group_members: ['42'], travel: null, location,
  stats: [{ id: 's1', name: 'Force', value: 12 }, { id: 's2', name: 'Agilité', value: 9 }, { id: 's3', name: 'Charisme', value: 15 }, { id: 's4', name: 'Endurance', value: 11 }],
  statuses: [
    { stat: 'Force', value: 2, modifier_type: 'flat', level: 'player', end_timestamp: now + 5400, source: 'Potion' },
    { stat: 'Agilité', value: -1, modifier_type: 'flat', level: 'place', end_timestamp: null, source: 'Boue' },
  ],
};
const travel = {
  location, travel: null, group_members: ['42'], server_time: now,
  destinations: [{ id: '5', name: 'Monville', distance_km: 12 }, { id: '35', name: 'Lorroc', distance_km: 21 }, { id: '8', name: 'Valival', distance_km: 16 }],
};

(async () => {
  const map = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures', 'small-map.json'), 'utf8'));
  map.server_time = now;
  const browser = await chromium.launch({ args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] });
  const errors = [];
  for (const [name, device] of Object.entries(SCREENS)) {
    if (only.length && !only.includes(name)) continue;
    const ctx = await browser.newContext({ ...device });
    const page = await ctx.newPage();
    page.on('pageerror', (e) => errors.push(`${name}: ${e}`));
    await page.route('**/api/**', (route) => {
      const p = new URL(route.request().url()).pathname;
      const json = (b, s = 200) => route.fulfill({ status: s, contentType: 'application/json', body: JSON.stringify(b) });
      if (p.startsWith('/api/i18n/')) return route.fulfill({ status: 200, contentType: 'text/plain', body: fs.readFileSync(path.join(ftlDir, p.endsWith('fr') ? 'fr.ftl' : 'en-US.ftl'), 'utf8') });
      if (p === '/api/auth/token') return json(session);
      if (p === '/api/me/character') return json(character);
      if (p === '/api/me/travel') return json(travel);
      if (p === '/api/map') return json(map);
      if (p === '/api/music/here') return json({ spot: { kind: 'place', id: '26' }, tracks: [{ id: 'aaaaaaaaaaaaaaaaaaaaaaaa', title: 'Taverne du port', duration_ms: 2000, kind: 'file' }] });
      if (p.startsWith('/api/music/tracks/')) return route.fulfill({ status: 200, headers: { 'content-type': 'audio/mpeg' }, body: fs.readFileSync(path.join(__dirname, '../../../tests/fixtures/audio/tone.mp3')) });
      return json({ error: 'activity__route_not_found' }, 404);
    });
    await page.goto('http://127.0.0.1:5199/app-harness.html');
    await page.waitForTimeout(3500);
    await page.screenshot({ path: path.join(shots, `app-${name}.png`) });
    // Débordements horizontaux : rien ne doit dépasser de l'écran.
    const overflow = await page.evaluate(() => {
      const w = document.documentElement.clientWidth;
      return [...document.querySelectorAll('body *')]
        .filter((el) => { const r = el.getBoundingClientRect(); return r.width > 0 && !el.closest('.drawer:not(.open)') && (r.right > w + 1 || r.left < -1) && getComputedStyle(el).position !== 'fixed'; })
        .slice(0, 5).map((el) => `${el.tagName.toLowerCase()}.${String(el.className).split(' ')[0]}`);
    });
    console.log(`${name} (${device.viewport.width}×${device.viewport.height}) : débordements ${JSON.stringify(overflow)}`);
    // Fiche (colonne ou volet) et panneau de voyage, ouverts.
    const avatar = page.getByRole('button', { name: 'Personnage' });
    const wide = await page.locator('.layout.wide').count();
    if (!wide) {
      await avatar.click();
      await page.waitForTimeout(400);
      await page.screenshot({ path: path.join(shots, `app-${name}-fiche.png`) });
      await page.getByRole('button', { name: 'Fermer' }).first().click();
      await page.waitForTimeout(400);
      console.log(`  volet refermé : ${!(await page.locator('.drawer.open').count())}`);
      const toggle = page.locator('.panel-toggle');
      if ((await toggle.getAttribute('aria-expanded')) === 'false') await toggle.click();
      await page.waitForTimeout(300);
      await page.screenshot({ path: path.join(shots, `app-${name}-voyage.png`) });
    }
    await ctx.close();
  }
  console.log('erreurs :', errors);
  await browser.close();
})();
