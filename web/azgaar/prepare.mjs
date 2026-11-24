// VerseEngine : adapte une copie figée d'Azgaar avant sa compilation (voir build.sh).
// Chaque retouche vérifie que le texte visé existe : si une autre version d'Azgaar est utilisée,
// la compilation échoue au lieu de produire une page qui ne serait pas isolée.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = process.argv[2];
if (!root) throw new Error("usage: node prepare.mjs <dossier d'Azgaar>");

function patch(file, edits) {
  const path = join(root, file);
  let text = readFileSync(path, "utf8");
  for (const [pattern, replacement] of edits) {
    const next = text.replace(pattern, replacement);
    if (next === text) throw new Error(`${file} : motif introuvable ${pattern}`);
    text = next;
  }
  writeFileSync(path, text);
}

// 1. Page : ni Google Analytics, ni manifeste d'application ; le stockage en mémoire en premier.
patch("src/index.html", [
  [/<script async src="https:\/\/www\.googletagmanager\.com[^>]*><\/script>\s*/, ""],
  [/<script>\s*window\.dataLayer[\s\S]*?<\/script>\s*/, ""],
  [/<link rel="manifest"[^>]*>\s*/, ""],
  [/<meta charset="utf-8" \/>/, '<meta charset="utf-8" />\n    <script src="verseengine-shim.js"></script>'],
]);
copyFileSync(join(here, "shim.js"), join(root, "public", "verseengine-shim.js"));

// 2. Pas de service worker (impossible dans un bac à sable, et rien à mettre en cache chez Discord).
patch("src/services/platform.ts", [
  [/export function registerServiceWorker\(\): void \{\n/, "export function registerServiceWorker(): void {\n  return; // VerseEngine : désactivé\n"],
]);

// 3. Pont vers l'activité, chargé au démarrage.
copyFileSync(join(here, "bridge.ts"), join(root, "src", "verseengine-bridge.ts"));
patch("src/main.ts", [[/(import \{ log \} from "@\/utils\/logger";\n)/, '$1import "./verseengine-bridge";\n']]);

// 4. Polices servies par notre domaine : téléchargées une fois, ici, depuis fonts.gstatic.com.
const fontsFile = join(root, "src/services/fonts.ts");
const fontsText = readFileSync(fontsFile, "utf8");
const urls = [...new Set(fontsText.match(/https:\/\/fonts\.gstatic\.com\/s\/[A-Za-z0-9_./-]+\.woff2/g) ?? [])];
if (urls.length < 20) throw new Error(`fonts.ts : ${urls.length} polices trouvées, format inattendu`);
mkdirSync(join(root, "public", "fonts"), { recursive: true });
let localFonts = fontsText;
for (const url of urls) {
  const name = url.replace("https://fonts.gstatic.com/s/", "").replaceAll("/", "-");
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} : ${res.status}`);
  writeFileSync(join(root, "public", "fonts", name), Buffer.from(await res.arrayBuffer()));
  localFonts = localFonts.replaceAll(url, `./fonts/${name}`);
}
writeFileSync(fontsFile, localFonts);
console.log(`Azgaar préparé : ${urls.length} polices locales.`);
