// La carte sous brouillard (3D puis 2D) dans un vrai navigateur, hors de Discord.
// Entrée : <dossier>/fog-map.json, une réponse de /api/map avec son champ `fog` (par exemple
// enregistrée depuis l'activité sous un compte joueur). Serveur : npx vite --port 5199 (web/activity).
// node web/activity/e2e/fog-harness.cjs <dossier> <dossier des captures>
const { chromium } = require('playwright');
const fs = require('fs'); const path = require('path');
const [fixtures, shots] = process.argv.slice(2);
(async () => {
  const browser = await chromium.launch({ args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] });
  const errors = [];
  for (const view of ['3d', '2d']) {
    const page = await browser.newPage({ viewport: { width: 1100, height: 750 } });
    page.on('pageerror', (e) => errors.push(view + ': ' + String(e)));
    page.on('console', (m) => m.type() === 'error' && errors.push(view + ': ' + m.text()));
    await page.route('**/fixture/fog-map.json', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: fs.readFileSync(path.join(fixtures, 'fog-map.json'), 'utf8') }));
    await page.goto(`http://127.0.0.1:5199/fog-harness.html?view=${view}`);
    await page.waitForTimeout(view === '3d' ? 4000 : 1500);
    await page.screenshot({ path: path.join(shots, `fog-${view}-a.png`) });
    await page.waitForTimeout(3000);
    await page.screenshot({ path: path.join(shots, `fog-${view}-b.png`) });
    await page.close();
  }
  console.log('erreurs :', errors);
  await browser.close();
})();
