// Page admin « Musique » dans un vrai navigateur, hors de Discord, avec une API simulée qui garde
// son état. Serveur : npx vite --port 5199 (web/activity).
// node web/activity/e2e/music-admin-harness.cjs <dossier .ftl> <dossier des sons> <captures>
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const [ftlDir, audioDir, shots] = process.argv.slice(2);
const errors = [];

const places = [{ id: '1', name: 'Kérival' }, { id: '2', name: 'Lorroc' }, { id: '3', name: 'Monville' }];
const roads = [{ id: '9', name: '', from: 'Kérival', to: 'Lorroc' }];
const tracks = [];
const files = {};
const ytRequests = [];

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1000, height: 900 } });
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => m.type() === 'error' && !/WebSocket|ws:\/\//.test(m.text()) && errors.push(m.text()));
  const uploads = [];
  await page.route('**/api/**', async (route) => {
    const r = route.request();
    const url = new URL(r.url());
    const p = url.pathname;
    const json = (body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
    if (p.startsWith('/api/i18n/')) return route.fulfill({ status: 200, contentType: 'text/plain', body: fs.readFileSync(path.join(ftlDir, 'fr.ftl'), 'utf8') });
    if (p === '/api/admin/music') {
      return json({ tracks, used_bytes: tracks.reduce((n, t) => n + t.bytes, 0), max_bytes: 500 * 1024 * 1024, max_tracks: 300, max_file_bytes: 25 * 1024 * 1024, places, roads });
    }
    if (p === '/api/admin/music/upload') {
      const body = r.postDataBuffer();
      uploads.push({ title: url.searchParams.get('title'), rights: url.searchParams.get('rights'), bytes: body.length });
      const id = String(tracks.length + 1).padStart(24, '0');
      files[id] = body;
      const t = { id, title: url.searchParams.get('title'), duration_ms: 2000, bytes: body.length, origin: 'file', places: [], roads: [] };
      tracks.push(t);
      return json(t);
    }
    if (p === '/api/admin/music/youtube') {
      const body = JSON.parse(r.postData());
      ytRequests.push(body);
      const t = { id: '0000000000000000000000yt', title: body.title, duration_ms: 0, bytes: 0, origin: 'youtube', places: [], roads: [], video_id: 'dQw4w9WgXcQ' };
      tracks.push(t);
      return json(t);
    }
    const m = p.match(/^\/api\/admin\/music\/(\w+)$/);
    if (m && r.method() === 'PUT') {
      const t = tracks.find((x) => x.id === m[1]);
      Object.assign(t, JSON.parse(r.postData()));
      return json(t);
    }
    if (m && r.method() === 'DELETE') {
      tracks.splice(tracks.findIndex((x) => x.id === m[1]), 1);
      return route.fulfill({ status: 204 });
    }
    const f = p.match(/^\/api\/music\/tracks\/(\w+)$/);
    if (f && files[f[1]]) return route.fulfill({ status: 200, headers: { 'content-type': 'audio/mpeg' }, body: files[f[1]] });
    return json({ error: 'activity__route_not_found' }, 404);
  });

  await page.goto('http://127.0.0.1:5199/music-harness.html?view=admin');
  await page.getByText('Aucun morceau pour l\'instant.').waitFor();
  const add = page.getByRole('button', { name: 'Ajouter', exact: true });
  console.log('Ajouter désactivé sans fichier :', await add.isDisabled());
  await page.setInputFiles('input[type=file]', path.join(audioDir, 'tone.mp3'));
  console.log('titre proposé :', await page.getByLabel('Titre').first().inputValue());
  const upload = page.locator('.music-upload').first();
  await upload.getByLabel('Titre').fill('Taverne du port');
  console.log('Ajouter désactivé sans droits :', await add.isDisabled(), '/', await add.getAttribute('title'));
  await upload.getByLabel(/J'ai les droits/).check();
  await page.screenshot({ path: path.join(shots, 'music-admin-1-envoi.png') });
  await add.click();
  await page.locator('.music-track').waitFor();
  console.log('envoyé :', JSON.stringify(uploads));

  // Assignation : recherche sans accents, un lieu et une route.
  await page.getByRole('button', { name: 'Lieux et routes' }).click();
  await page.getByPlaceholder('Chercher un lieu ou une route').fill('kerival');
  const listed = await page.locator('.music-picker label').allTextContents();
  console.log('recherche « kerival » :', listed);
  await page.locator('.music-picker label', { hasText: /^Kérival$/ }).locator('input').check();
  await page.locator('.music-picker label', { hasText: '⇄' }).locator('input').check();
  await page.screenshot({ path: path.join(shots, 'music-admin-2-assignation.png') });
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await page.waitForTimeout(400);
  console.log('enregistré :', JSON.stringify(tracks.map((t) => ({ title: t.title, places: t.places, roads: t.roads }))));
  console.log('Enregistrer désactivé après :', await page.getByRole('button', { name: 'Enregistrer' }).isDisabled());

  // Écoute.
  await page.getByRole('button', { name: 'Écouter' }).click();
  await page.getByRole('button', { name: 'Arrêter' }).waitFor();
  console.log('écoute en cours');
  await page.getByRole('button', { name: 'Arrêter' }).click();

  // Suppression en deux temps.
  await page.getByRole('button', { name: 'Supprimer' }).click();
  await page.getByRole('button', { name: 'Confirmer la suppression' }).click();
  await page.getByText('Aucun morceau pour l\'instant.').waitFor();
  console.log('supprimé, morceaux restants :', tracks.length);
  // Vidéo YouTube : seul l'identifiant est envoyé (lecteur officiel visible, rien de téléchargé).
  await page.getByText('Ajouter une vidéo YouTube').waitFor();
  const addVideo = page.getByRole('button', { name: 'Ajouter la vidéo' });
  const yt = page.locator('.music-upload').nth(1);
  await yt.getByLabel('Lien de la vidéo YouTube').fill('https://youtu.be/dQw4w9WgXcQ');
  console.log('Ajouter la vidéo désactivé sans titre :', await addVideo.isDisabled());
  await yt.getByLabel('Titre').fill('Thème du port');
  await yt.getByLabel(/J'ai les droits/).check();
  await page.screenshot({ path: path.join(shots, 'music-admin-3-youtube.png') });
  await addVideo.click();
  await page.locator('.music-track').waitFor({ timeout: 10000 });
  console.log('demande YouTube :', JSON.stringify(ytRequests), '→ bibliothèque :', await page.locator('.music-track .muted').first().textContent(), '; bouton Écouter :', await page.locator('.music-track').getByRole('button', { name: 'Écouter' }).count());
  console.log('erreurs :', errors);
  await browser.close();
})();
