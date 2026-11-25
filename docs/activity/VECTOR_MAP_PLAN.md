# Carte façon Azgaar en vecteurs (chantier « B »)

Objectif : remplacer le rendu pixel de la carte (grille 480 × N, floue de près) par des **contours
vectoriels** calculés comme Azgaar (`renderers/draw-biomes.ts` → `getIsolines` → tracés), filtrés
par joueur selon le brouillard. Rien d'Azgaar n'est exécuté chez le joueur ; le `.map` ne quitte
jamais le serveur.

## Décisions

- **Données gardées à l'import** (`MapStyle.pack`, optionnel ; anciens univers : rendu pixel inchangé) :
  points des cellules du pack (`Pack.points`), points de bordure de la grille, et par cellule :
  biome (u8), État (u16), eau (0 terre, 1 mer, 2 lac). Seulement si le pack est `exact`.
- **Géométrie recalculée côté serveur** (triangulation des points du pack + bordure → cellules de
  Voronoï : sommets = centres circonscrits, arêtes avec la cellule voisine), **mise en cache en
  mémoire** par univers (clé : identifiant de la carte).
- **Contours par requête** (`/api/map`), avec le masque du brouillard : une cellule cachée n'entre
  dans aucun contour (ni biome, ni État, ni côte). Couches : terre (côte), lacs, biomes, États
  (remplissage), frontières (lignes entre deux États visibles).
- Côte et lacs lissés (Chaikin fermé, 2 passes) ; biomes et États en polygones bruts, découpés par
  la terre côté client (`clipPath`), comme Azgaar.
- **Client** : le rendu pixel reste dessous (mer en bandes, brouillard) ; les couches vectorielles
  par-dessus. 3D inchangée pour l'instant.
- Sécurité : aucune donnée d'une cellule cachée n'est envoyée (test dédié) ; tailles bornées.

## Étapes

1. [x] `delaunay.rs` : `voronoi_cells` (sommets + polygone de chaque cellule avec voisin par arête). Tests.
2. [x] `map/isolines.rs` : contours fermés par valeur (arêtes entre valeurs différentes, chaînées),
       lignes de frontière, Chaikin fermé, chemins SVG compacts. Tests.
3. [x] Import : `MapStyle.pack` (`PackCells`, octets compacts) rempli dans `build_style` (pack exact).
       Zone du lasso appliquée au rendu à l'import (`import_exec::hide_outside_zone`, testé) : avant, le
       client ne la dessinait pas du tout.
4. [x] `/api/map` : `style.vector` (`map/vector.rs`, cache mémoire) ; brouillard dans `fog.rs`.
       Tests : `hidden_cells_send_nothing`, `vector_contours_only_cover_discovered_cells`.
5. [x] Client `MapView2D` (`map/azgaarVector.ts`) : couches vectorielles (couleurs Azgaar), repli pixel. Tests + captures.
6. [x] Vérifications sur 3 vraies cartes (`map_view::real_vector::vector_from_a_real_map`) : 4 000 à
       5 500 cellules, 25 à 65 ms par calcul (build de dev), contours ~300 Ko, réponse gzip ~140 Ko
       (compression de l'API ajoutée, jamais pour l'audio). Rendu vérifié en navigateur (Fludar).
       Le rendu pixel ne peint plus les terres quand les contours existent (`landAsSea`).
7. [x] « Mettre à jour le rendu seulement » (`activity/render_refresh.rs`, `POST /admin/import/render`) :
       renvoyer le même `.map` recalcule le rendu sans toucher à Discord (vérifie dimensions + bourgs).
       Le `.map` d'origine n'est pas conservé : sans le fichier, pas de rendu exact (Morgisnoy/TestBot3).
8. [ ] Pistes : ombrage du relief par-dessus les biomes ; vue 3D ; icônes de relief façon Azgaar.

## Sauvegardes

Instantanés locaux du code (sans toucher la branche ni l'index) : branche `wip/vector-map-snapshots`
(`scratchpad/snapshot.sh`). Rien n'est poussé.

## Journal

- 2026-11-24 : conception (ce document).
- 2026-11-24 : étape 3 faite ; étapes 1-2 confiées à un sous-agent (voronoi_cells, isolines.rs).
- 2026-11-24 : étapes 1, 2, 4, 5 faites (386 tests Rust, 175 front). Reste : étape 6 (vraie carte).
- 2026-11-24 : étape 6 faite (387 tests Rust, 176 front). Prêt à déployer.
- 2026-11-24 : mise à jour du rendu seule (option 1) ; aucun .map trouvé pour Morgisnoy (TestBot3).
- 2026-11-24 : Morgisnoy (TestBot3) laissé tel quel ; l'utilisateur refera une carte (vers le 2026-11-26). Pas de reconstruction approchée.
- 2026-11-24 : sauvegarde des .map (GridFS map_sources, 3 par univers, graine lue dans le fichier) + envoi en MP au créateur après import ou mise à jour du rendu (activity/map_source.rs).
