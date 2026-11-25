//! Rendu façon Azgaar : ce qu'il faut pour redessiner la carte dans l'activité (biomes, mers,
//! lacs, rivières, États, icônes des bourgs), lu dans la sauvegarde `.map`.
//!
//! On ne peut ni exécuter Azgaar chez le joueur ni afficher son image sous le brouillard (elle
//! dévoilerait lieux et routes secrets) : le serveur extrait donc des **données** (une grille de
//! surface et des tracés), que le brouillard peut filtrer comme le relief, et le front les dessine.
//!
//! Lignes du `.map` utilisées (Azgaar 1.108 à 1.158, `save.ts`) :
//!
//! | Ligne | Contenu |
//! |------:|---------|
//! | 1 | réglages (JSON depuis 1.15x) : `graph.sampling`, `graph.jitter` |
//! | 3 | biomes : JSON `[{i, color}]`, ou ancien texte `couleurs|habitabilité|noms` |
//! | 6 | grille : `points`, `boundary`, `spacing`, `features` (type `lake`) |
//! | 7, 8, 9, 10, 11 | grille : hauteur, précipitations, entité, distance à la côte, température |
//! | 14 | États (`color`) |
//! | 15 | bourgs (`population`, `port`, `capital`) |
//! | 16, 25 | biome et État de chaque cellule du **pack** |
//! | 32 | rivières : cellules du pack traversées, débit |
//!
//! Les cellules du pack ne sont pas enregistrées : Azgaar les recalcule au chargement (`Pack.generate`,
//! ancien `reGraph`) à partir des voisins de Voronoï de la grille. On refait ce calcul, puis on le
//! **vérifie** (nombre de cellules, position des bourgs et des routes). S'il ne correspond pas (autre
//! version d'Azgaar), les biomes sont estimés depuis la grille, comme Azgaar les calcule, sans États
//! ni rivières (`MapStyle.exact = false`).

use serde_json::Value;

use crate::activity::map::azgaar::num;
use crate::activity::map::delaunay::{triangulate, voronoi_neighbors};
use crate::database::universe_maps::{BurgDetails, BurgStyle, MapStyle, PackCells, StyleRiver, PACK_LAKE, PACK_LAND, PACK_OCEAN, SURFACE_LAKE, SURFACE_OCEAN};

/// Plus grand côté de la grille de surface (cases). 480 × 270 ≈ 130 000 octets au plus (quelques
/// dizaines de Ko une fois encodés en suites).
pub const MAX_SURFACE_SIDE: usize = 480;
const SEA_LEVEL: u8 = 20;

/// Couleurs des biomes par défaut d'Azgaar (`getDefaultBiomes`).
pub const DEFAULT_BIOME_COLORS: [&str; 13] = [
    "#466eab", "#fbe79f", "#b5b887", "#d2d082", "#c8d68f", "#b6d95d", "#29bc56", "#7dcb35", "#409c43", "#4b6b32", "#96784b", "#d5e7eb", "#0b9131",
];

/// Matrice des biomes d'Azgaar (`biomes-generator.ts`) : humidité (5 bandes) × température (26).
const BIOMES_MATRIX: [[u8; 26]; 5] = [
    [1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 10],
    [3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 9, 9, 9, 9, 10, 10, 10],
    [5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 9, 9, 9, 9, 9, 10, 10, 10],
    [5, 6, 6, 6, 6, 6, 6, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 10, 10, 10],
    [7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 10, 10],
];

/// Biome d'une cellule, comme `Biomes.getId` d'Azgaar.
pub fn biome_id(moisture: f64, temperature: f64, height: u8, has_river: bool) -> u8 {
    if height < SEA_LEVEL {
        return 0;
    }
    if temperature < -5.0 {
        return 11;
    }
    if temperature >= 25.0 && !has_river && moisture < 8.0 {
        return 1;
    }
    let h = height as f64;
    let wetland = temperature > -2.0 && ((moisture > 40.0 && h < 25.0) || (moisture > 24.0 && h > 24.0 && h < 60.0));
    if wetland {
        return 12;
    }
    let m = ((moisture / 5.0).floor().max(0.0) as usize).min(4);
    let t = (20.0 - temperature).clamp(0.0, 25.0) as usize;
    BIOMES_MATRIX[m][t]
}

/// Couleur `#rrggbb` (ou `#rgb`) valide, sinon `None` : les couleurs viennent du fichier.
pub fn sanitize_color(c: &str) -> Option<String> {
    let c = c.trim();
    let hex = c.strip_prefix('#')?;
    let ok = (hex.len() == 6 || hex.len() == 3) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    if !ok {
        return None;
    }
    Some(if hex.len() == 3 { hex.chars().flat_map(|ch| [ch, ch]).collect::<String>() } else { hex.to_string() })
        .map(|h| format!("#{}", h.to_ascii_lowercase()))
}

fn csv<T: std::str::FromStr>(record: Option<&&str>) -> Vec<T> {
    record.map(|r| r.split(',').filter_map(|v| v.trim().parse::<T>().ok()).collect()).unwrap_or_default()
}

/// Comme `csv`, mais une valeur illisible compte (index préservés) : pour les tableaux indexés.
fn csv_f64(record: Option<&&str>) -> Vec<f64> {
    record.map(|r| r.split(',').map(|v| v.trim().parse::<f64>().unwrap_or(0.0)).collect()).unwrap_or_default()
}

fn json(record: Option<&&str>) -> Value {
    record
        .and_then(|r| serde_json::from_str(&crate::activity::map::azgaar::replace_lone_surrogates(r)).ok())
        .unwrap_or(Value::Null)
}

fn point(v: &Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((num(a.first())?, num(a.get(1))?))
}

/// Couleurs des biomes : JSON récent ou ancien texte ; à défaut, celles d'Azgaar.
pub fn biome_colors(record: Option<&&str>) -> Vec<String> {
    let mut colors: Vec<String> = DEFAULT_BIOME_COLORS.iter().map(|c| c.to_string()).collect();
    let Some(r) = record.map(|r| r.trim()) else { return colors };
    if r.starts_with('[') {
        if let Some(arr) = json(Some(&r)).as_array() {
            for (k, b) in arr.iter().enumerate() {
                let id = b.get("i").and_then(Value::as_u64).map(|i| i as usize).unwrap_or(k);
                if let Some(c) = b.get("color").and_then(Value::as_str).and_then(sanitize_color) {
                    if id < 200 {
                        if colors.len() <= id {
                            colors.resize(id + 1, DEFAULT_BIOME_COLORS[4].to_string());
                        }
                        colors[id] = c;
                    }
                }
            }
        }
    } else if let Some(first) = r.split('|').next() {
        for (id, c) in first.split(',').enumerate().take(200) {
            if let Some(c) = sanitize_color(c) {
                if colors.len() <= id {
                    colors.resize(id + 1, DEFAULT_BIOME_COLORS[4].to_string());
                }
                colors[id] = c;
            }
        }
    }
    colors
}

/// Index spatial : points rangés par case carrée, pour trouver le plus proche.
struct Buckets {
    size: f64,
    cols: usize,
    rows: usize,
    items: Vec<Vec<u32>>,
}

impl Buckets {
    fn new(points: &[(f64, f64)], size: f64, width: f64, height: f64) -> Self {
        let size = size.max(1e-6);
        let cols = ((width / size).ceil() as usize).clamp(1, 4096);
        let rows = ((height / size).ceil() as usize).clamp(1, 4096);
        let mut items = vec![vec![]; cols * rows];
        for (i, &(x, y)) in points.iter().enumerate() {
            let c = ((x / size).floor().max(0.0) as usize).min(cols - 1);
            let r = ((y / size).floor().max(0.0) as usize).min(rows - 1);
            items[r * cols + c].push(i as u32);
        }
        Self { size, cols, rows, items }
    }

    fn nearest(&self, points: &[(f64, f64)], x: f64, y: f64) -> Option<usize> {
        let c0 = ((x / self.size).floor().max(0.0) as isize).min(self.cols as isize - 1);
        let r0 = ((y / self.size).floor().max(0.0) as isize).min(self.rows as isize - 1);
        let mut best: Option<(usize, f64)> = None;
        for ring in 0..(self.cols.max(self.rows) as isize) {
            // Au-delà de ce cercle, aucune case ne peut contenir plus proche.
            if let Some((_, d)) = best {
                let reach = (ring as f64 - 1.0).max(0.0) * self.size;
                if reach * reach > d {
                    break;
                }
            }
            for r in (r0 - ring)..=(r0 + ring) {
                for c in (c0 - ring)..=(c0 + ring) {
                    if (r - r0).abs() != ring && (c - c0).abs() != ring {
                        continue;
                    }
                    if r < 0 || c < 0 || r >= self.rows as isize || c >= self.cols as isize {
                        continue;
                    }
                    for &i in &self.items[r as usize * self.cols + c as usize] {
                        let (px, py) = points[i as usize];
                        let d = (px - x).powi(2) + (py - y).powi(2);
                        if best.is_none_or(|(_, b)| d < b) {
                            best = Some((i as usize, d));
                        }
                    }
                }
            }
        }
        best.map(|(i, _)| i)
    }
}

/// Grille d'Azgaar lue dans la sauvegarde.
struct Grid {
    spacing: f64,
    points: Vec<(f64, f64)>,
    boundary: Vec<(f64, f64)>,
    h: Vec<u8>,
    t: Vec<i32>,
    f: Vec<usize>,
    prec: Vec<f64>,
    temp: Vec<f64>,
    lake_features: Vec<bool>,
}

impl Grid {
    fn read(records: &[&str]) -> Option<Self> {
        let g = json(records.get(6));
        let pts = |k: &str| -> Vec<(f64, f64)> { g.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(point).collect()).unwrap_or_default() };
        let points = pts("points");
        let n = points.len();
        let h: Vec<u8> = csv_f64(records.get(7)).into_iter().map(|v| v.clamp(0.0, 100.0) as u8).collect();
        if n < 3 || h.len() != n {
            return None;
        }
        let fit = |mut v: Vec<f64>| {
            v.resize(n, 0.0);
            v
        };
        let lake_features = g
            .get("features")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|f| f.get("type").and_then(Value::as_str) == Some("lake")).collect())
            .unwrap_or_default();
        Some(Grid {
            spacing: num(g.get("spacing")).filter(|s| *s > 0.0)?,
            boundary: pts("boundary"),
            t: fit(csv_f64(records.get(10))).into_iter().map(|v| v as i32).collect(),
            f: fit(csv_f64(records.get(9))).into_iter().map(|v| v.max(0.0) as usize).collect(),
            prec: fit(csv_f64(records.get(8))),
            temp: fit(csv_f64(records.get(11))),
            points,
            h,
            lake_features,
        })
    }

    fn is_lake(&self, i: usize) -> bool {
        self.lake_features.get(self.f[i]).copied().unwrap_or(false)
    }

    /// Biome estimé depuis la grille (humidité = précipitations + 4, sans les rivières).
    fn estimated_biome(&self, i: usize) -> u8 {
        biome_id(4.0 + self.prec[i], self.temp[i], self.h[i], false)
    }
}

/// Cellules du pack recalculées : position et cellule de grille parente.
struct Pack {
    points: Vec<(f64, f64)>,
    grid_of: Vec<usize>,
    in_pack: Vec<bool>,
}

/// Refait `Pack.generate` d'Azgaar pour une distance de dédoublement donnée.
fn rebuild_pack(grid: &Grid, neighbors: &[Vec<usize>], border: &[bool], split: f64) -> Pack {
    let split2 = split * split;
    let rn1 = |v: f64| (v * 10.0).round() / 10.0;
    let mut pack = Pack { points: vec![], grid_of: vec![], in_pack: vec![false; grid.points.len()] };
    for i in 0..grid.points.len() {
        let (h, t) = (grid.h[i], grid.t[i]);
        if h < SEA_LEVEL && t != -1 && t != -2 {
            continue; // haute mer
        }
        if t == -2 && (i % 4 == 0 || grid.is_lake(i)) {
            continue; // lac ou mer non côtiers
        }
        let (x, y) = grid.points[i];
        pack.points.push((x, y));
        pack.grid_of.push(i);
        pack.in_pack[i] = true;
        // Côtes : un point de plus entre deux cellules côtières du même type assez éloignées.
        if (t == 1 || t == -1) && !border[i] {
            for &e in &neighbors[i] {
                if i > e || grid.t[e] != t {
                    continue;
                }
                let (ex, ey) = grid.points[e];
                if (y - ey).powi(2) + (x - ex).powi(2) < split2 {
                    continue;
                }
                pack.points.push((rn1((x + ex) / 2.0), rn1((y + ey) / 2.0)));
                pack.grid_of.push(i);
            }
        }
    }
    pack
}

/// Distance de voisinage du treillis d'Azgaar 1.15x (`Lattice.getSteps`).
fn neighbor_distance(sampling: &str, spacing: f64) -> f64 {
    let sqrt3 = 3f64.sqrt();
    match sampling {
        "hexPointy" => spacing * (2.0 / sqrt3).sqrt(),
        "hexFlat" => spacing * (2.0 / (3.0 * sqrt3)).sqrt(),
        "triangle" => spacing * (4.0 / sqrt3).sqrt() / sqrt3,
        _ => spacing,
    }
}

/// Part des bourgs et des points de route dont la cellule du pack tombe à leur place.
fn anchors_match(pack: &Pack, records: &[&str], spacing: f64) -> bool {
    let mut checked = 0usize;
    let mut ok = 0usize;
    let mut check = |cell: Option<u64>, x: Option<f64>, y: Option<f64>, tolerance: f64| {
        let (Some(cell), Some(x), Some(y)) = (cell, x, y) else { return };
        checked += 1;
        if let Some(&(px, py)) = pack.points.get(cell as usize) {
            if (px - x).hypot(py - y) <= tolerance {
                ok += 1;
            }
        }
    };
    if let Some(burgs) = json(records.get(15)).as_array() {
        for b in burgs.iter().filter(|b| b.get("removed").and_then(Value::as_bool) != Some(true)) {
            check(b.get("cell").and_then(Value::as_u64), num(b.get("x")), num(b.get("y")), spacing * 1.5);
        }
    }
    if let Some(routes) = json(records.get(37)).as_array() {
        for p in routes.iter().filter_map(|r| r.get("points")?.as_array()).flatten().filter_map(Value::as_array) {
            check(p.get(2).and_then(Value::as_u64), num(p.first()), num(p.get(1)), spacing * 1.5);
        }
    }
    checked == 0 || ok * 10 >= checked * 9
}

/// Taille d'un bourg d'après sa population (milliers d'habitants dans Azgaar).
pub fn burg_size(population: f64) -> u8 {
    if population >= 10.0 {
        2
    } else if population >= 2.0 {
        1
    } else {
        0
    }
}

/// Noms par identifiant d'une liste Azgaar (`[{i, name, fullName}]`, États ou cultures).
/// L'entrée 0 (« Neutrals », « Wildlands ») n'est pas un vrai État ni une culture : ignorée.
fn names_by_id(record: Option<&&str>, prefer_full: bool) -> std::collections::HashMap<u64, String> {
    let mut out = std::collections::HashMap::new();
    for e in json(record).as_array().into_iter().flatten() {
        if e.get("removed").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(i) = e.get("i").and_then(Value::as_u64).filter(|i| *i > 0) else { continue };
        let full = prefer_full.then(|| e.get("fullName").and_then(Value::as_str)).flatten();
        if let Some(name) = full.or_else(|| e.get("name").and_then(Value::as_str)).map(str::trim).filter(|n| !n.is_empty()) {
            out.insert(i, short_text(name, 100));
        }
    }
    out
}

/// Texte venu du fichier, borné (un `.map` peut contenir n'importe quoi).
fn short_text(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

/// Habitants réels : Azgaar enregistre la population en milliers, multipliée à l'affichage par
/// l'échelle des réglages (`units.population.scale`, ancien `populationRate`, 1000 par défaut).
fn population_scale(settings: &Value) -> f64 {
    let scale = num(settings.pointer("/units/population/scale")).or_else(|| num(settings.get("populationRate"))).filter(|v| *v > 0.0).unwrap_or(1000.0);
    let urbanization = num(settings.pointer("/units/population/urbanization/rate")).or_else(|| num(settings.get("urbanization"))).filter(|v| *v > 0.0).unwrap_or(1.0);
    scale * urbanization
}

fn burg_styles(records: &[&str]) -> Vec<BurgStyle> {
    let states = names_by_id(records.get(14), true);
    let cultures = names_by_id(records.get(13), false);
    let scale = population_scale(&json(records.get(1)));
    json(records.get(15))
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|b| b.get("removed").and_then(Value::as_bool) != Some(true))
                .filter_map(|b| {
                    let id = b.get("i").and_then(Value::as_u64).filter(|i| *i > 0)?;
                    let flag = |k: &str| b.get(k).and_then(Value::as_u64).unwrap_or(0) >= 1 || b.get(k).and_then(Value::as_bool) == Some(true);
                    let population = num(b.get("population"));
                    let text = |k: &str| b.get(k).and_then(Value::as_str).map(|v| short_text(v, 40)).filter(|v| !v.is_empty());
                    let details = BurgDetails {
                        name: b.get("name").and_then(Value::as_str).map(|n| short_text(n, 100)).filter(|n| !n.is_empty()),
                        population: population.filter(|p| p.is_finite() && *p > 0.0).map(|p| (p * scale).round() as u64),
                        state: b.get("state").and_then(Value::as_u64).and_then(|i| states.get(&i).cloned()),
                        culture: b.get("culture").and_then(Value::as_u64).and_then(|i| cultures.get(&i).cloned()),
                        kind: text("type"),
                        group: text("group"),
                        features: ["citadel", "walls", "plaza", "temple", "shanty"].into_iter().filter(|k| flag(k)).map(String::from).collect(),
                    };
                    Some(BurgStyle {
                        burg_id: id,
                        size: burg_size(population.unwrap_or(0.0)),
                        capital: flag("capital"),
                        port: flag("port"),
                        details: Some(details),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Largeur d'une rivière (unités de carte) à la source et à l'embouchure, d'après son débit.
pub fn river_width(discharge: f64) -> [f64; 2] {
    let mouth = (0.5 + discharge.max(1.0).log10() * 0.6).clamp(0.5, 3.5);
    [(mouth * 0.25).max(0.3), mouth]
}

/// Calcule le rendu d'une sauvegarde déjà découpée en lignes. `None` : pas de grille lisible.
pub fn build_style(records: &[&str], width: f64, height: f64) -> Option<MapStyle> {
    let grid = Grid::read(records)?;
    let n = grid.points.len();

    // Voisins de Voronoï de la grille (points + bordure), comme `calculateVoronoi`.
    let mut all = grid.points.clone();
    all.extend(&grid.boundary);
    let (neighbors, border) = voronoi_neighbors(&triangulate(&all), n);

    // Pack : on essaie les distances de dédoublement des différentes versions d'Azgaar, et on
    // garde celle qui redonne exactement le nombre de cellules enregistrées.
    let pack_biome: Vec<u8> = csv(records.get(16));
    let pack_state: Vec<u16> = csv(records.get(25));
    let settings = json(records.get(1));
    let mut splits = vec![grid.spacing];
    if let Some(jitter) = num(settings.pointer("/graph/jitter")) {
        let sampling = settings.pointer("/graph/sampling").and_then(Value::as_str).unwrap_or("square");
        splits.insert(0, neighbor_distance(sampling, grid.spacing) * (1.0 + (0.9 - jitter)));
    }
    let pack = splits
        .iter()
        .map(|&s| rebuild_pack(&grid, &neighbors, &border, s))
        .find(|p| !pack_biome.is_empty() && p.points.len() == pack_biome.len() && anchors_match(p, records, grid.spacing));
    let exact = pack.is_some();

    // Un tiers de cellule de la grille par case : des côtes fines, sans grossir la réponse.
    let cell = (grid.spacing / 3.0).max(width.max(height) / MAX_SURFACE_SIDE as f64);
    let cols = ((width / cell).ceil() as usize).clamp(1, MAX_SURFACE_SIDE);
    let rows = ((height / cell).ceil() as usize).clamp(1, MAX_SURFACE_SIDE);
    let grid_index = Buckets::new(&grid.points, grid.spacing, width, height);
    let pack_index = pack.as_ref().map(|p| Buckets::new(&p.points, grid.spacing, width, height));
    let states_used = exact && pack_state.iter().any(|&s| s > 0);
    let ocean = |g: usize| if grid.is_lake(g) { SURFACE_LAKE } else { SURFACE_OCEAN + (-grid.t[g]).clamp(1, 9) as u8 };
    let mut surface = vec![0u8; cols * rows];
    let mut states = if states_used { vec![0u8; cols * rows] } else { vec![] };
    for r in 0..rows {
        for c in 0..cols {
            let (x, y) = ((c as f64 + 0.5) * width / cols as f64, (r as f64 + 0.5) * height / rows as f64);
            let Some(g) = grid_index.nearest(&grid.points, x, y) else { continue };
            let k = r * cols + c;
            surface[k] = match (&pack, &pack_index) {
                (Some(pack), Some(index)) if pack.in_pack[g] => {
                    let p = index.nearest(&pack.points, x, y).unwrap_or(0);
                    let parent = pack.grid_of[p];
                    if grid.h[parent] < SEA_LEVEL {
                        ocean(parent)
                    } else {
                        if states_used {
                            states[k] = pack_state.get(p).copied().unwrap_or(0).min(255) as u8;
                        }
                        // Biome 0 (« marin ») sur une terre : estimé.
                        pack_biome.get(p).copied().filter(|b| *b > 0 && *b < SURFACE_OCEAN).unwrap_or_else(|| grid.estimated_biome(parent))
                    }
                }
                _ if grid.h[g] < SEA_LEVEL => ocean(g),
                _ => grid.estimated_biome(g),
            };
        }
    }

    let state_colors = if states_used {
        let mut colors = vec![String::new(); 256];
        if let Some(arr) = json(records.get(14)).as_array() {
            for s in arr.iter().filter(|s| s.get("removed").and_then(Value::as_bool) != Some(true)) {
                if let (Some(i), Some(c)) = (s.get("i").and_then(Value::as_u64), s.get("color").and_then(Value::as_str).and_then(sanitize_color)) {
                    if (1..256).contains(&i) {
                        colors[i as usize] = c;
                    }
                }
            }
        }
        while colors.last().is_some_and(String::is_empty) {
            colors.pop();
        }
        colors
    } else {
        vec![]
    };

    // Rivières : centres des cellules du pack traversées (seulement si le pack est exact).
    let rn1 = |v: f64| (v * 10.0).round() / 10.0;
    let rivers = match &pack {
        Some(pack) => json(records.get(32))
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| {
                        let points: Vec<[f64; 2]> = r
                            .get("cells")?
                            .as_array()?
                            .iter()
                            .filter_map(Value::as_i64)
                            .filter_map(|c| usize::try_from(c).ok())
                            .filter_map(|c| pack.points.get(c))
                            .map(|&(x, y)| [rn1(x), rn1(y)])
                            .collect();
                        (points.len() >= 2).then(|| StyleRiver { points, width: river_width(num(r.get("discharge")).unwrap_or(1.0)) })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => vec![],
    };

    // Cellules du pack pour les contours vectoriels (pack exact seulement) : positions, bordure de
    // la grille (ferme les cellules du bord), biome, État et nature de l'eau de chaque cellule.
    let pack_cells = pack.as_ref().map(|pack| {
        let water: Vec<u8> = pack
            .grid_of
            .iter()
            .map(|&g| if grid.h[g] >= SEA_LEVEL { PACK_LAND } else if grid.is_lake(g) { PACK_LAKE } else { PACK_OCEAN })
            .collect();
        let biome: Vec<u8> = (0..pack.points.len())
            .map(|p| {
                if water[p] != PACK_LAND {
                    return 0;
                }
                pack_biome.get(p).copied().filter(|b| *b > 0 && *b < SURFACE_OCEAN).unwrap_or_else(|| grid.estimated_biome(pack.grid_of[p]))
            })
            .collect();
        let state: Vec<u8> = (0..pack.points.len())
            .map(|p| if states_used && water[p] == PACK_LAND { pack_state.get(p).copied().unwrap_or(0).min(255) as u8 } else { 0 })
            .collect();
        let mut points = pack.points.clone();
        points.extend(&grid.boundary);
        PackCells { real: pack.points.len() as u32, points: PackCells::encode_points(&points), biome, state, water }
    });

    Some(MapStyle {
        cols: cols as u32,
        rows: rows as u32,
        surface,
        states,
        biome_colors: biome_colors(records.get(3)),
        state_colors,
        rivers,
        burgs: burg_styles(records),
        exact,
        pack: pack_cells,
    })
}

/// Grille de surface en texte compact : suites (valeur, longueur ≤ 255) en base64. Les biomes
/// forment de grandes plages : 125 000 cases tiennent en quelques dizaines de Ko.
pub fn encode_runs(values: &[u8]) -> String {
    use base64::Engine;
    let mut out = Vec::with_capacity(values.len() / 4);
    let mut i = 0;
    while i < values.len() {
        let v = values[i];
        let mut run = 1;
        while i + run < values.len() && values[i + run] == v && run < 255 {
            run += 1;
        }
        out.push(v);
        out.push(run as u8);
        i += run;
    }
    base64::engine::general_purpose::STANDARD.encode(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn biomes_follow_azgaar_rules() {
        assert_eq!(biome_id(50.0, 10.0, 10, false), 0, "water");
        assert_eq!(biome_id(10.0, -10.0, 40, false), 11, "glacier");
        assert_eq!(biome_id(5.0, 30.0, 40, false), 1, "hot desert");
        assert_eq!(biome_id(45.0, 10.0, 22, false), 12, "wetland near the coast");
        assert_eq!(biome_id(7.0, 12.0, 40, false), 4, "grassland");
        assert_eq!(biome_id(22.0, 12.0, 70, false), 8, "temperate rainforest");
    }

    #[test]
    fn colors_from_the_file_are_checked() {
        assert_eq!(sanitize_color(" #ABCdef "), Some("#abcdef".into()));
        assert_eq!(sanitize_color("#fa0"), Some("#ffaa00".into()));
        assert_eq!(sanitize_color("url(#hatch1)"), None);
        assert_eq!(sanitize_color("#12345g"), None);
        let old = biome_colors(Some(&"#000000,#111111,bad|0,4|Marine,Desert,X"));
        assert_eq!(&old[..3], &["#000000", "#111111", DEFAULT_BIOME_COLORS[2]]);
        let new = biome_colors(Some(&r##"[{"i":0,"color":"#010101"},{"i":14,"color":"#020202"}]"##));
        assert_eq!((new[0].as_str(), new[14].as_str(), new.len()), ("#010101", "#020202", 15));
        assert_eq!(biome_colors(None).len(), 13);
    }

    #[test]
    fn runs_are_encoded_in_pairs() {
        use base64::Engine;
        let mut v = vec![4u8; 300];
        v.extend([201, 201, 7]);
        let raw = base64::engine::general_purpose::STANDARD.decode(encode_runs(&v)).unwrap();
        assert_eq!(raw, vec![4, 255, 4, 45, 201, 2, 7, 1]);
        assert_eq!(encode_runs(&[]), "");
    }

    #[test]
    fn the_nearest_point_is_found_across_buckets() {
        let pts = vec![(1.0, 1.0), (9.0, 9.0), (5.2, 4.9), (30.0, 2.0)];
        let b = Buckets::new(&pts, 2.0, 40.0, 10.0);
        assert_eq!(b.nearest(&pts, 5.0, 5.0), Some(2));
        assert_eq!(b.nearest(&pts, 25.0, 9.0), Some(3), "far from any point");
        assert_eq!(b.nearest(&pts, 0.0, 0.0), Some(0));
    }

    /// Petite sauvegarde cohérente : grille carrée 12 × 8, une île au milieu, un lac, deux États.
    /// Le pack est calculé par notre propre reconstruction (ce test vérifie le reste du calcul ; la
    /// reconstruction elle-même est vérifiée sur de vraies cartes par `a_real_map_style_is_exact`).
    pub(crate) fn island_records() -> Vec<String> {
        let (cols, rows, spacing) = (12usize, 8usize, 10.0);
        let mut points = vec![];
        let mut h = vec![];
        let mut t = vec![];
        let mut f = vec![];
        for r in 0..rows {
            for c in 0..cols {
                let j = ((r * 7 + c * 3) % 5) as f64 * 0.3;
                points.push(format!("[{},{}]", c as f64 * spacing + 5.0 + j, r as f64 * spacing + 5.0 - j));
                let land = (3..9).contains(&c) && (2..6).contains(&r);
                let lake = c == 5 && r == 3;
                h.push(if land && !lake { 50 } else { 5 });
                let edge = land && !lake && (c == 3 || c == 8 || r == 2 || r == 5 || (c == 5 && (r == 2 || r == 4)) || (r == 3 && (c == 4 || c == 6)));
                t.push(if lake { -1 } else if land { if edge { 1 } else { 2 } } else if (2..10).contains(&c) && (1..7).contains(&r) { -1 } else { -3 });
                f.push(if lake { 3 } else if land { 2 } else { 1 });
            }
        }
        let mut boundary = vec![];
        for i in 0..=13 {
            let v = i as f64 * 10.0;
            boundary.push(format!("[{v},-10]"));
            boundary.push(format!("[{v},90]"));
        }
        for i in 0..=9 {
            let v = i as f64 * 10.0;
            boundary.push(format!("[-10,{v}]"));
            boundary.push(format!("[130,{v}]"));
        }
        let mut records = vec![String::new(); 56];
        records[0] = "1.158.0|x|2026-11-24|1|120|80|1".into();
        records[6] = format!(
            r#"{{"spacing":{spacing},"cellsX":{cols},"cellsY":{rows},"points":[{}],"boundary":[{}],"features":[0,{{"i":1,"type":"ocean"}},{{"i":2,"type":"island"}},{{"i":3,"type":"lake"}}]}}"#,
            points.join(","),
            boundary.join(",")
        );
        let join = |v: &[i32]| v.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
        records[7] = join(&h);
        records[8] = vec!["10"; cols * rows].join(",");
        records[9] = join(&f.iter().map(|&x| x as i32).collect::<Vec<_>>());
        records[10] = join(&t);
        records[11] = vec!["12"; cols * rows].join(",");
        records[3] = "#466eab,#fbe79f,#b5b887,#d2d082,#c8d68f,#b6d95d,#29bc56|0|Marine".into();
        records[14] = r##"[{"i":0,"name":"Neutrals"},{"i":1,"color":"#ff0000"},{"i":2,"color":"#00ff00"}]"##.into();
        records[15] = r#"[{},{"i":1,"name":"Port","x":35,"y":25,"cell":0,"population":12,"port":1,"capital":1},{"i":2,"name":"Hameau","x":75,"y":45,"population":0.4}]"#.into();

        // Pack recalculé par notre code : biomes 6 à l'ouest, 4 à l'est ; États 1 et 2.
        let recs: Vec<&str> = records.iter().map(String::as_str).collect();
        let grid = Grid::read(&recs).unwrap();
        let mut all = grid.points.clone();
        all.extend(&grid.boundary);
        let (nb, border) = voronoi_neighbors(&triangulate(&all), grid.points.len());
        let pack = rebuild_pack(&grid, &nb, &border, spacing);
        let west = |p: &(f64, f64)| p.0 < 60.0;
        records[16] = pack.points.iter().zip(&pack.grid_of).map(|(p, &g)| if grid.h[g] < 20 { "0" } else if west(p) { "6" } else { "4" }).collect::<Vec<_>>().join(",");
        records[25] = pack.points.iter().zip(&pack.grid_of).map(|(p, &g)| if grid.h[g] < 20 { "0" } else if west(p) { "1" } else { "2" }).collect::<Vec<_>>().join(",");
        // Rivière : deux cellules de terre puis la mer.
        let land: Vec<usize> = (0..pack.points.len()).filter(|&i| grid.h[pack.grid_of[i]] >= 20).take(2).collect();
        records[32] = format!(r#"[{{"i":1,"cells":[{},{},-1],"discharge":100}}]"#, land[0], land[1]);
        // Bourg 1 posé sur la cellule 0 du pack (ancre vérifiée).
        let (x0, y0) = pack.points[0];
        records[15] = records[15].replace(r#""x":35,"y":25"#, &format!(r#""x":{x0},"y":{y0}"#));
        records
    }

    #[test]
    fn an_island_map_gives_its_surface_states_and_rivers() {
        let records = island_records();
        let recs: Vec<&str> = records.iter().map(String::as_str).collect();
        let s = build_style(&recs, 120.0, 80.0).unwrap();
        assert!(s.exact, "the pack rebuilt matches the stored cells");
        assert_eq!((s.cols, s.rows), (36, 24), "a third of a grid cell per case");
        let case = 120.0 / 36.0;
        let at = |x: f64, y: f64| s.surface[(y / case) as usize * 36 + (x / case) as usize];
        assert_eq!(at(40.0, 40.0), 6, "western land");
        assert_eq!(at(80.0, 40.0), 4, "eastern land");
        assert!(s.surface.contains(&SURFACE_LAKE), "the lake cell");
        assert!(at(2.0, 2.0) > SURFACE_OCEAN && at(2.0, 2.0) < SURFACE_LAKE, "open sea, depth band");
        let k = |x: f64, y: f64| (y / case) as usize * 36 + (x / case) as usize;
        assert_eq!(s.states[k(40.0, 40.0)], 1);
        assert_eq!(s.states[k(80.0, 40.0)], 2);
        assert_eq!(&s.state_colors[1..], &["#ff0000", "#00ff00"]);
        assert_eq!(s.rivers.len(), 1);
        assert_eq!(s.rivers[0].points.len(), 2, "the -1 mouth is skipped");
        let icons: Vec<_> = s.burgs.iter().map(|b| (b.burg_id, b.size, b.capital, b.port)).collect();
        assert_eq!(icons, vec![(1, 2, true, true), (2, 0, false, false)]);
    }

    #[test]
    fn burg_details_are_kept_for_the_place_sheet() {
        let mut records = vec![String::new(); 16];
        records[1] = r#"{"units":{"population":{"scale":500,"urbanization":{"rate":2}}}}"#.into();
        records[13] = r#"[{"i":0,"name":"Wildlands"},{"i":4,"name":"Luari"}]"#.into();
        records[14] = r#"[{"i":0,"name":"Neutrals"},{"i":1,"name":"Hublosia","fullName":"Kingdom of Hublosia"}]"#.into();
        records[15] = r#"[{},{"i":1,"name":"Arane","state":1,"culture":4,"population":23.735,"type":"River","group":"capital","citadel":1,"walls":1,"plaza":0,"temple":1},{"i":2,"name":"Hameau","state":0,"culture":0,"population":0.4}]"#.into();
        let recs: Vec<&str> = records.iter().map(String::as_str).collect();
        let burgs = burg_styles(&recs);
        let d = burgs[0].details.clone().unwrap();
        assert_eq!(d.name.as_deref(), Some("Arane"));
        assert_eq!(d.population, Some(23_735), "thousands × scale × urbanization");
        assert_eq!(d.state.as_deref(), Some("Kingdom of Hublosia"));
        assert_eq!(d.culture.as_deref(), Some("Luari"));
        assert_eq!((d.kind.as_deref(), d.group.as_deref()), (Some("River"), Some("capital")));
        assert_eq!(d.features, vec!["citadel", "walls", "temple"]);
        let h = burgs[1].details.clone().unwrap();
        assert_eq!((h.state, h.culture, h.population), (None, None, Some(400)), "neutral state and wildlands are not names");
    }

    #[test]
    fn the_old_population_rate_and_its_default_are_understood() {
        assert_eq!(population_scale(&serde_json::json!({})), 1000.0);
        assert_eq!(population_scale(&serde_json::json!({"populationRate": 10})), 10.0);
    }

    #[test]
    fn a_pack_that_does_not_match_falls_back_to_estimated_biomes() {
        let mut records = island_records();
        records[16].push_str(",4"); // une cellule de trop : autre version d'Azgaar
        let recs: Vec<&str> = records.iter().map(String::as_str).collect();
        let s = build_style(&recs, 120.0, 80.0).unwrap();
        assert!(!s.exact);
        assert!(s.states.is_empty() && s.rivers.is_empty(), "no states nor rivers without the pack");
        // Précipitations 10, température 12 : prairie (4) partout sur l'île.
        assert_eq!(s.surface[12 * 36 + 12], biome_id(14.0, 12.0, 50, false));
    }

    #[test]
    fn a_save_without_grid_has_no_style() {
        assert!(build_style(&["1.158.0|x"], 100.0, 100.0).is_none());
    }

    /// Vraie sauvegarde (`AZGAAR_MAP_FILE`) : le pack recalculé correspond à celui d'Azgaar
    /// (nombre de cellules, bourgs et routes à leur place), et la surface reste compacte.
    #[test]
    #[ignore = "integration test: needs AZGAAR_MAP_FILE (a real Azgaar .map save)"]
    fn a_real_map_style_is_exact() {
        let path = std::env::var("AZGAAR_MAP_FILE").expect("set AZGAAR_MAP_FILE");
        let start = std::time::Instant::now();
        let f = crate::activity::map::azgaar_file::parse_map_file(&std::fs::read(&path).unwrap()).unwrap();
        let elapsed = start.elapsed();
        let s = f.map.style.expect("a style");
        let encoded = encode_runs(&s.surface).len() + encode_runs(&s.states).len();
        let land = s.surface.iter().filter(|&&v| v < SURFACE_OCEAN).count();
        eprintln!(
            "{} : exact {}, surface {}×{} ({} % de terre), {} rivières, {} États, {} bourgs, {} octets encodés, {:?}",
            f.version, s.exact, s.cols, s.rows, land * 100 / s.surface.len(), s.rivers.len(), s.state_colors.iter().filter(|c| !c.is_empty()).count(),
            s.burgs.len(), encoded, elapsed
        );
        assert!(s.exact, "the rebuilt pack matches the save");
        assert!(!s.rivers.is_empty());
        assert!(encoded < 200_000, "a reasonable response size");
    }

    #[test]
    fn river_width_grows_with_discharge() {
        let [s, m] = river_width(10.0);
        assert!(s < m);
        assert!(river_width(10_000.0)[1] > m);
        assert_eq!(river_width(1e12)[1], 3.5);
    }
}
