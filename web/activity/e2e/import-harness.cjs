// Parcours de l'assistant d'import dans un vrai navigateur, hors de Discord, avec une API simulée.
//
// 1. Réponses calculées par le code Rust sur une vraie carte :
//    AZGAAR_MAP_FILE=<carte.map> IMPORT_FIXTURE_DIR=<dossier> cargo test --locked a_real_map_preview_fixture -- --ignored
// 2. Serveur de développement : (cd web/activity && npx vite --port 5199 --strictPort)
// 3. node web/activity/e2e/import-harness.cjs <dossier> translations <dossier des captures> <carte.map>
//    (PHONE=1 : écran de téléphone, jusqu'à l'aperçu)
// Playwright doit être installé (il l'est dans l'environnement de développement cloud).
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const [fixtures, ftlDir, shots, mapFile] = process.argv.slice(2);
const upload = fs.readFileSync(path.join(fixtures, 'upload.json'), 'utf8');
const planBase = JSON.parse(fs.readFileSync(path.join(fixtures, 'plan.json'), 'utf8'));
const extra = Array.from({ length: 9 }, (_, i) => ({ guild_id: String(2000 + i), name: `Moria ${i + 2}`, linked: 'none', eligible: true, current: false }));
const servers = { servers: [{ guild_id: '1100433828931194931', name: 'RpBotTest2', linked: 'none', eligible: true, current: true }, ...extra], max_places_per_server: 60, invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot%20applications.commands&permissions=8' };
let jobPolls = 0;
const errors = [];

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage(process.env.PHONE ? { ...require('playwright').devices['iPhone 13'] } : { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  await page.route('**/api/**', async (route) => {
    const url = new URL(route.request().url());
    const p = url.pathname;
    const json = (body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: typeof body === 'string' ? body : JSON.stringify(body) });
    if (p.startsWith('/api/i18n/')) return route.fulfill({ status: 200, contentType: 'text/plain', body: fs.readFileSync(path.join(ftlDir, p.endsWith('fr') ? 'fr.ftl' : 'en-US.ftl'), 'utf8') });
    if (p === '/api/admin/import/map') return json(upload);
    if (p === '/api/admin/import/servers') return json(servers);
    if (p === '/api/admin/import/plan') {
      const req = JSON.parse(route.request().postData());
      const plan = JSON.parse(JSON.stringify(planBase));
      plan.plan.servers.forEach((s, i) => { if (i > 0) s.guild_id = req.servers[i - 1] ?? null; });
      plan.servers_to_create = plan.plan.servers.filter((s) => !s.guild_id).length;
      const secret = new Set(req.secret.map(([a, b]) => `${a}-${b}`));
      plan.plan.roads.forEach((r) => (r.secret = secret.has(`${r.burg_a}-${r.burg_b}`)));
      return json(plan);
    }
    if (p === '/api/admin/import/execute') return json({ job_id: 'a'.repeat(24), total: planBase.discord_objects });
    if (p.startsWith('/api/admin/import/jobs/')) {
      jobPolls += 1;
      const total = planBase.discord_objects;
      const done = Math.min(total, jobPolls * 600);
      return json({ progress: { phase: done >= total ? 'Done' : 'Creating', done, total }, outcome: done >= total ? { status: 'done', universe_id: 'b'.repeat(24) } : { status: 'running' } });
    }
    return json({ error: 'activity__route_not_found' }, 404);
  });

  await page.goto('http://127.0.0.1:5199/import-harness.html');
  await page.getByRole('button', { name: 'Importer une carte' }).click();
  await page.screenshot({ path: path.join(shots, '1-instructions.png') });
  await page.setInputFiles('input[type=file]', mapFile);
  await page.waitForSelector('.import-preview');
  await page.waitForFunction(() => document.querySelectorAll('.preview-road').length > 0, null, { timeout: 15000 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(shots, (process.env.PHONE ? 'tel-' : '') + '2-apercu-serveurs-manquants.png') });
  if (process.env.PHONE) { await page.screenshot({ path: path.join(shots, 'tel-2-page.png'), fullPage: true }); console.log('erreurs :', errors); await browser.close(); return; }
  const next = page.getByRole('button', { name: 'Suivant' });
  console.log('Suivant désactivé avec serveurs manquants :', await next.isDisabled());

  // Route secrète : clic sur la première route.
  await page.locator('.preview-road').first().click({ force: true });
  const selects = page.locator('.server-slot select');
  console.log('emplacements de serveur à choisir :', await selects.count());
  for (let i = 0; i < 8; i++) {
    await selects.nth(i).selectOption({ index: 1 });
    await page.waitForTimeout(50);
  }
  await page.waitForFunction(() => !document.querySelector('.import-warning'), null, { timeout: 10000 });
  await page.waitForTimeout(1600);
  await page.screenshot({ path: path.join(shots, '3-serveurs-choisis.png') });
  console.log('Suivant désactivé une fois tout choisi :', await next.isDisabled());
  await next.click();
  await page.getByLabel('Nom :').fill('Moria');
  const create = page.getByRole('button', { name: 'Créer' });
  await page.locator('.import-confirm > label input').fill('Mori');
  console.log('Créer désactivé avec un mauvais nom :', await create.isDisabled());
  await page.locator('.import-confirm > label input').fill('Moria');
  await page.screenshot({ path: path.join(shots, '4-confirmation.png') });
  console.log('Créer désactivé avec le bon nom :', await create.isDisabled());
  await create.click();
  await page.waitForTimeout(2300);
  await page.screenshot({ path: path.join(shots, '5-progression.png') });
  await page.getByRole('button', { name: "Ouvrir l'univers" }).waitFor({ timeout: 15000 });
  await page.screenshot({ path: path.join(shots, '6-termine.png') });
  console.log('erreurs :', errors);
  await browser.close();
})();
