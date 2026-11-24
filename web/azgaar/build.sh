#!/usr/bin/env bash
# VerseEngine : compile une version figée d'Azgaar's Fantasy Map Generator (MIT) pour l'activité.
# Sortie : web/activity/public/azgaar (servie sous /azgaar/, dans un bac à sable : voir
# src/activity/mod.rs). Usage : web/azgaar/build.sh [dossier de sortie]
set -euo pipefail

# Version figée : 1.158.0. Changer de version demande de relire prepare.mjs.
AZGAAR_REPO=https://github.com/Azgaar/Fantasy-Map-Generator
AZGAAR_COMMIT=74136292b8012abe0d57a11ac7780ddc43d9433d

here=$(cd "$(dirname "$0")" && pwd)
work=${AZGAAR_WORK:-$here/.work}
out=$(mkdir -p "${1:-$here/../activity/public/azgaar}" && cd "${1:-$here/../activity/public/azgaar}" && pwd)

if [ ! -d "$work/.git" ]; then
  git init -q "$work"
  git -C "$work" remote add origin "$AZGAAR_REPO"
fi
git -C "$work" fetch -q --depth 1 origin "$AZGAAR_COMMIT"
git -C "$work" checkout -q --force FETCH_HEAD
git -C "$work" clean -q -f -d -x -e node_modules

# Dépendances sans leurs scripts d'installation.
(cd "$work" && npm ci --ignore-scripts --no-audit --no-fund)
node "$here/prepare.mjs" "$work"
(cd "$work" && npx vite build --base /azgaar/ --outDir "$out" --emptyOutDir)
# Licence MIT d'Azgaar : elle accompagne toute copie servie.
cp "$work/LICENSE" "$out/LICENSE.txt"
echo "Azgaar compilé dans $out"
