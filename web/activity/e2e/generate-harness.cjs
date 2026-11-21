// Parcours de l'éditeur « Générer une map » dans un vrai navigateur, avec une API simulée.
// Fixture : la même commande que pour import-harness.cjs écrit aussi generated.json.
// node web/activity/e2e/generate-harness.cjs <dossier des fixtures> translations <dossier des captures>
const { chromium } = require('playwright');
const fs = require('fs'); const path = require('path');
const [fixtures, ftlDir, shots] = process.argv.slice(2);
const gen = JSON.parse(fs.readFileSync(path.join(fixtures, 'generated.json'), 'utf8'));
let sent = null; const errors = [];
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  await page.route('**/api/**', (route) => {
    const p = new URL(route.request().url()).pathname;
    const json = (b) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(b) });
    if (p.startsWith('/api/i18n/')) return route.fulfill({ status: 200, contentType: 'text/plain', body: fs.readFileSync(path.join(ftlDir, 'fr.ftl'), 'utf8') });
    if (p === '/api/admin/import/generate') return json(gen);
    if (p === '/api/admin/import/generated') {
      sent = JSON.parse(route.request().postData());
      return json({ name: null, version: 'generated', width: 1000, height: 1000, distance_unit: 'km', route_count: sent.roads.length,
        burgs: sent.places.map((x) => ({ id: x.id, name: x.name, x: x.x, y: x.y, capital: x.capital })), terrain: null, suggestions: [], has_universe: false, generated: true });
    }
    if (p === '/api/admin/import/servers') return json({ servers: [{ guild_id: '1', name: 'RpBotTest2', linked: 'none', eligible: true, current: true }], max_places_per_server: 60, invite_url: null });
    if (p === '/api/admin/import/plan') return json({ removal: null, servers_to_create: 0, discord_objects: 100, universe_name: null, plan: { places: sent.places.map((x) => ({ burg_id: x.id, name: x.name, x: x.x, y: x.y, capital: x.capital, action: { kind: 'create' }, server: 0 })), roads: [], remove_places: [], remove_roads: [], servers: [{ guild_id: '1', budget: { places: 40, roads_hosted: 0, road_categories: 1, channels: 100, roles: 44 } }], cut_roads: 0, isolated: [] } });
    return route.fulfill({ status: 404, body: '{}' });
  });
  await page.goto('http://127.0.0.1:5199/import-harness.html');
  await page.getByRole('button', { name: 'Générer une map' }).click();
  await page.waitForSelector('.map-place');
  await page.screenshot({ path: path.join(shots, 'g1-genere.png') });
  // Renommer le premier lieu.
  await page.locator('.map-place circle').first().click({ force: true });
  await page.locator('fieldset:has(legend:text-is("Lieu")) input').fill('Ma capitale');
  // Ajouter un lieu.
  await page.getByLabel('Ajouter un lieu (clic sur la carte)').check();
  const box = await page.locator('svg.import-preview').boundingBox();
  await page.mouse.click(box.x + 20, box.y + 20);
  // Ajouter une route entre le nouveau lieu et le premier.
  await page.getByLabel('Ajouter une route (deux lieux)').check();
  await page.locator('.map-place circle').last().click({ force: true });
  await page.locator('.map-place circle').first().click({ force: true });
  // Supprimer une route.
  await page.getByLabel('Supprimer (lieu ou route)').check();
  const roadsBefore = await page.locator('polyline.preview-road').count();
  await page.locator('polyline.preview-road').nth(3).click({ force: true });
  const roadsAfter = await page.locator('polyline.preview-road').count();
  // Retoucher un tracé.
  await page.getByLabel('Retoucher un tracé (glisser la poignée)').check();
  const h = await page.locator('.road-handle').nth(2).boundingBox();
  await page.mouse.move(h.x + 7, h.y + 7); await page.mouse.down(); await page.mouse.move(h.x + 60, h.y + 40, { steps: 5 }); await page.mouse.up();
  await page.screenshot({ path: path.join(shots, 'g2-retouche.png') });
  await page.getByLabel('Vue 3D').check();
  await page.waitForTimeout(2500);
  await page.screenshot({ path: path.join(shots, 'g3-vue-3d.png') });
  await page.getByLabel('Vue 3D').uncheck();
  await page.getByRole('button', { name: 'Utiliser cette carte' }).click();
  await page.waitForSelector('.server-slot');
  await page.waitForTimeout(1600);
  await page.screenshot({ path: path.join(shots, 'g4-apercu.png') });
  console.log('routes avant/après suppression :', roadsBefore, roadsAfter);
  console.log('envoyé :', sent.places.length, 'lieux,', sent.roads.length, 'routes ; renommé :', sent.places[0].name, '; nouveau :', sent.places.at(-1).name);
  console.log('erreurs :', errors);
  await browser.close();
})();
