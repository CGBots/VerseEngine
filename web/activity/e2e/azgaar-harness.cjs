// Azgaar dans l'assistant d'import (E9), dans un vrai navigateur, hors de Discord.
// Prérequis : web/azgaar/build.sh (Azgaar compilé dans public/azgaar) ; serveur : npx vite --port 5199.
// Les réponses /azgaar/* reçoivent les en-têtes du serveur Rust (CSP `sandbox` lue dans
// src/activity/mod.rs), pour vérifier l'isolation telle qu'elle sera servie.
// node web/activity/e2e/azgaar-harness.cjs <dossier des fixtures> <dossier .ftl> <captures> <.map reçu>
// Fixtures : upload.json (réponse de /api/admin/import/map, voir import-harness.cjs).
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const [fixtures, ftlDir, shots, received] = process.argv.slice(2);

const modRs = fs.readFileSync(path.join(__dirname, '../../../src/activity/mod.rs'), 'utf8');
const csp = modRs.match(/const AZGAAR_CSP: &str = "([\s\S]*?)";/)[1].replace(/\\\n/g, '');
const upload = fs.readFileSync(path.join(fixtures, 'upload.json'), 'utf8');
const errors = [];
const outside = [];

(async () => {
  const browser = await chromium.launch({ args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  // Toute requête hors de notre serveur est notée (et bloquée par la CSP).
  page.on('request', (r) => { if (!r.url().startsWith('http://127.0.0.1:5199/') && !r.url().startsWith('data:') && !r.url().startsWith('blob:')) outside.push(r.url()); });
  await page.route('**/azgaar/**', async (route) => {
    const res = await route.fetch();
    await route.fulfill({ response: res, headers: { ...res.headers(), 'content-security-policy': csp, 'access-control-allow-origin': '*', 'x-content-type-options': 'nosniff' } });
  });
  let mapBytes = null;
  await page.route('**/api/**', async (route) => {
    const p = new URL(route.request().url()).pathname;
    if (p.startsWith('/api/i18n/')) return route.fulfill({ status: 200, contentType: 'text/plain', body: fs.readFileSync(path.join(ftlDir, p.endsWith('fr') ? 'fr.ftl' : 'en-US.ftl'), 'utf8') });
    if (p === '/api/admin/import/map') {
      mapBytes = route.request().postDataBuffer();
      return route.fulfill({ status: 200, contentType: 'application/json', body: upload });
    }
    if (p === '/api/admin/import/servers') return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ servers: [], max_places_per_server: 60, invite_url: null }) });
    return route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'activity__route_not_found' }) });
  });

  await page.goto('http://127.0.0.1:5199/import-harness.html');
  await page.getByRole('button', { name: 'Créer ou modifier dans Azgaar' }).click();
  const send = page.getByRole('button', { name: 'Envoyer cette carte à VerseEngine' });
  const t0 = Date.now();
  await send.waitFor({ timeout: 180000 });
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some((b) => b.textContent === 'Envoyer cette carte à VerseEngine' && !b.disabled), null, { timeout: 180000 });
  console.log(`Azgaar prêt en ${((Date.now() - t0) / 1000).toFixed(1)} s`);
  await page.waitForTimeout(1500);
  await page.screenshot({ path: path.join(shots, 'azgaar-1-studio.png') });

  // Isolation, vue de l'intérieur du cadre.
  const frame = page.frames().find((f) => f.url().includes('/azgaar/'));
  const inside = await frame.evaluate(async () => {
    const r = { origin: self.origin };
    try { r.parentDocument = !!window.parent.document; } catch { r.parentDocument = 'refusé'; }
    try { r.parentStorage = window.parent.localStorage.length; } catch { r.parentStorage = 'refusé'; }
    try { r.cookie = document.cookie; } catch { r.cookie = 'refusé'; }
    try { const res = await fetch('/api/session'); r.api = res.status; } catch { r.api = 'refusé'; }
    try { await fetch('https://example.com/'); r.external = 'joint'; } catch { r.external = 'refusé'; }
    try { r.popup = window.open('https://example.com/') ? 'ouverte' : 'refusée'; } catch { r.popup = 'refusée'; }
    r.memoryStorage = (() => { localStorage.setItem('k', 'v'); return localStorage.getItem('k'); })();
    return r;
  });
  console.log('dans le cadre :', JSON.stringify(inside));

  await send.click();
  await page.waitForSelector('.import-preview', { timeout: 60000 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(shots, 'azgaar-2-apercu.png') });
  if (mapBytes) fs.writeFileSync(received, mapBytes);
  console.log('carte reçue :', mapBytes ? `${mapBytes.length} octets, en-tête ${mapBytes.subarray(0, 60).toString().split('\n')[0]}` : 'aucune');

  // Retour : Azgaar est toujours chargé (la carte en cours n'est pas perdue).
  await page.getByRole('button', { name: 'Retour' }).last().click();
  console.log('Azgaar toujours affiché après retour :', await page.locator('.azgaar-frame').isVisible());
  console.log('requêtes hors de notre serveur :', outside);
  console.log('erreurs :', errors);
  await browser.close();
})();
