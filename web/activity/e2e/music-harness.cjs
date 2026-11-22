// Lecteur de musique dans un vrai navigateur, hors de Discord, avec la CSP de l'activité (lue dans
// src/activity/mod.rs) : un son joué depuis une URL `blob:` doit passer.
// Serveur : npx vite --port 5199 (web/activity).
// node web/activity/e2e/music-harness.cjs <dossier .ftl> <dossier des sons> <captures>
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const [ftlDir, audioDir, shots] = process.argv.slice(2);

const modRs = fs.readFileSync(path.join(__dirname, '../../../src/activity/mod.rs'), 'utf8');
// Le serveur de développement de Vite a besoin d'un script en ligne : seul ajout à la CSP réelle,
// dont `media-src` (lecture depuis `blob:`) est ce qu'on vérifie ici.
const csp = modRs.match(/const STATIC_CSP: &str = "([\s\S]*?)";/)[1].replace(/\\\n/g, '') + "; script-src 'self' 'unsafe-inline'";
const errors = [];

(async () => {
  // Politique de lecture d'un vrai navigateur : pas de son sans geste de l'utilisateur.
  const browser = await chromium.launch({ args: ['--autoplay-policy=user-gesture-required'] });
  const page = await browser.newPage({ viewport: { width: 900, height: 500 } });
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  // Deux endroits : la taverne (deux morceaux), puis la route (un morceau).
  let moves = 0;
  const places = [
    { spot: { kind: 'place', id: '1' }, tracks: [{ id: 'aaaaaaaaaaaaaaaaaaaaaaaa', title: 'Taverne du port', duration_ms: 2000 }, { id: 'bbbbbbbbbbbbbbbbbbbbbbbb', title: 'Chanson à boire', duration_ms: 2000 }] },
    { spot: { kind: 'road', id: '9' }, tracks: [{ id: 'cccccccccccccccccccccccc', title: 'Sur la route', duration_ms: 2000 }] },
  ];
  const files = { aaaaaaaaaaaaaaaaaaaaaaaa: ['tone.mp3', 'audio/mpeg'], bbbbbbbbbbbbbbbbbbbbbbbb: ['tone.ogg', 'audio/ogg'], cccccccccccccccccccccccc: ['tone.opus', 'audio/ogg'] };
  await page.route('**/music-harness.html', async (route) => {
    const res = await route.fetch();
    await route.fulfill({ response: res, headers: { ...res.headers(), 'content-security-policy': csp } });
  });
  await page.route('**/api/**', async (route) => {
    const p = new URL(route.request().url()).pathname;
    if (p.startsWith('/api/i18n/')) return route.fulfill({ status: 200, contentType: 'text/plain', body: fs.readFileSync(path.join(ftlDir, 'fr.ftl'), 'utf8') });
    if (p === '/api/music/here') return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(places[Math.min(moves, 1)]) });
    const m = p.match(/^\/api\/music\/tracks\/(\w+)$/);
    if (m && files[m[1]]) {
      const [file, type] = files[m[1]];
      return route.fulfill({ status: 200, headers: { 'content-type': type, 'content-security-policy': "sandbox; default-src 'none'" }, body: fs.readFileSync(path.join(audioDir, file)) });
    }
    return route.fulfill({ status: 404, contentType: 'application/json', body: '{"error":"activity__route_not_found"}' });
  });

  // Les éléments <audio> créés par le lecteur (hors du DOM) sont gardés pour les inspecter.
  await page.addInitScript(() => {
    const Native = window.Audio;
    window.__audios = [];
    window.Audio = function (...args) {
      const a = new Native(...args);
      window.__audios.push(a);
      return a;
    };
  });
  await page.goto('http://127.0.0.1:5199/music-harness.html');
  // Un vrai navigateur peut bloquer le son sans geste de l'utilisateur : alors le bouton
  // « Activer la musique » apparaît (cas couvert par les tests du lecteur). Chromium sans écran
  // laisse jouer le plus souvent.
  const enable = page.getByRole('button', { name: 'Activer la musique' });
  await Promise.race([enable.waitFor({ timeout: 15000 }), page.locator('.music-title').waitFor({ timeout: 15000 })]);
  if (await enable.isVisible()) {
    await page.screenshot({ path: path.join(shots, 'music-1-bloque.png') });
    await enable.click();
  }
  await page.locator('.music-title').waitFor();
  await page.waitForTimeout(800);
  const playing = await page.evaluate(() => window.__audios.map((a) => ({ paused: a.paused, time: Number(a.currentTime.toFixed(2)), volume: Number(a.volume.toFixed(2)), error: a.error && a.error.code, src: a.src.slice(0, 5) })));
  console.log('lecteur :', JSON.stringify(playing));
  const title1 = await page.locator('.music-title').textContent();
  console.log('à la taverne :', title1.trim());
  await page.screenshot({ path: path.join(shots, 'music-2-taverne.png') });

  // Déplacement : fondu puis musique de la route.
  moves = 1;
  await page.getByRole('button', { name: 'Déplacer' }).click();
  await page.waitForFunction(() => document.querySelector('.music-title')?.textContent?.includes('Sur la route'), null, { timeout: 10000 });
  console.log('sur la route :', (await page.locator('.music-title').textContent()).trim());
  await page.waitForTimeout(2500);
  console.log('lecteur après le fondu :', JSON.stringify(await page.evaluate(() => window.__audios.map((a) => ({ paused: a.paused, time: Number(a.currentTime.toFixed(2)), volume: Number(a.volume.toFixed(2)) })))));
  await page.getByRole('button', { name: 'Couper la musique' }).click();
  console.log('coupure gardée :', await page.evaluate(() => localStorage.getItem('verseengine.music')));
  await page.screenshot({ path: path.join(shots, 'music-3-route-coupee.png') });
  // Le canal de rechargement de Vite (développement seulement) est bloqué par la CSP : sans objet.
  console.log('erreurs :', errors.filter((e) => !/ws:\/\/|WebSocket/.test(e)));
  await browser.close();
})();
