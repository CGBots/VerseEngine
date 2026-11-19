//! Import d'une carte Azgaar Fantasy Map Generator (« Source A »).
//!
//! Format attendu : **Export → JSON → Full**. Le parseur est tolérant : les champs
//! inconnus sont ignorés, `pack.cells` est accepté sous forme de tableau d'objets
//! (`[{ "i":0, "p":[x,y], "h":34, … }]`) ou d'objet de tableaux (`{ "p":[…], "h":[…] }`).
//! Hauteurs : 0–100, la terre commence à 20.

use serde::Deserialize;
use serde_json::Value;

/// Hauteur minimale d'une cellule de terre dans Azgaar.
pub const LAND_HEIGHT: u8 = 20;
/// Taille max (colonnes ou lignes) de la heightmap rééchantillonnée.
pub const MAX_TERRAIN_SIDE: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub struct Burg {
    pub id: u64,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub cell: Option<usize>,
    pub capital: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutePoint {
    pub x: f64,
    pub y: f64,
    /// Cellule Azgaar du point : sert à détecter les bifurcations (points partagés).
    pub cell: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub id: Option<u64>,
    pub group: Option<String>,
    /// Longueur annoncée par Azgaar (dans `distance_unit`), si présente.
    pub length: Option<f64>,
    pub points: Vec<RoutePoint>,
}

#[derive(Debug, Clone)]
pub struct AzgaarMap {
    pub name: Option<String>,
    pub width: f64,
    pub height: f64,
    /// Unités de distance par pixel (`settings.distanceScale`), si présent.
    pub distance_scale: Option<f64>,
    pub distance_unit: Option<String>,
    pub burgs: Vec<Burg>,
    /// Centres des cellules et hauteurs associées.
    pub cells: Vec<(f64, f64, u8)>,
    pub routes: Vec<Route>,
}

#[derive(Debug, PartialEq)]
pub enum AzgaarError {
    InvalidJson(String),
    MissingPack,
    NoBurgs,
}

impl AzgaarError {
    /// Clé Fluent correspondante.
    pub fn key(&self) -> &'static str {
        match self {
            AzgaarError::InvalidJson(_) => "activity__azgaar_invalid_json",
            AzgaarError::MissingPack => "activity__azgaar_missing_pack",
            AzgaarError::NoBurgs => "activity__azgaar_no_burgs",
        }
    }
}

#[derive(Deserialize)]
struct RawRoot {
    info: Option<Value>,
    settings: Option<Value>,
    pack: Option<Value>,
}

pub(crate) fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Parse le JSON « Full » exporté par Azgaar.
/// Remplace les échappements `\uD800`–`\uDFFF` isolés (moitié de caractère sans l'autre) par
/// `\uFFFD`. Azgaar en produit en coupant des textes générés au milieu d'un caractère ; le JSON
/// reste sinon valide, mais `serde_json` les refuse. Les paires correctes et le reste sont gardés.
pub fn replace_lone_surrogates(json: &str) -> std::borrow::Cow<'_, str> {
    fn hex4(b: &[u8]) -> Option<u16> {
        u16::from_str_radix(std::str::from_utf8(b.get(..4)?).ok()?, 16).ok()
    }
    let b = json.as_bytes();
    let mut out: Option<String> = None;
    let mut last = 0;
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        if b[i + 1] != b'u' {
            i += 2; // autre échappement (\\, \", \n…) : on le saute en entier
            continue;
        }
        let Some(code) = hex4(&b[i + 2..]) else { i += 2; continue };
        let next_low = b.get(i + 6) == Some(&b'\\') && b.get(i + 7) == Some(&b'u')
            && hex4(&b[(i + 8).min(b.len())..]).is_some_and(|c| (0xDC00..=0xDFFF).contains(&c));
        let lone = match code {
            0xD800..=0xDBFF => !next_low,
            0xDC00..=0xDFFF => true, // une moitié basse valide a déjà été consommée avec sa paire
            _ => false,
        };
        if lone {
            let o = out.get_or_insert_with(|| String::with_capacity(json.len()));
            o.push_str(&json[last..i]);
            o.push_str("\\uFFFD");
            last = i + 6;
            i += 6;
        } else if (0xD800..=0xDBFF).contains(&code) {
            i += 12; // paire valide
        } else {
            i += 6;
        }
    }
    match out {
        Some(mut o) => {
            o.push_str(&json[last..]);
            std::borrow::Cow::Owned(o)
        }
        None => std::borrow::Cow::Borrowed(json),
    }
}

pub fn parse(json: &str) -> Result<AzgaarMap, AzgaarError> {
    let json = replace_lone_surrogates(json);
    let root: RawRoot = serde_json::from_str(&json).map_err(|e| AzgaarError::InvalidJson(e.to_string()))?;
    let pack = root.pack.ok_or(AzgaarError::MissingPack)?;
    let info = root.info.unwrap_or(Value::Null);
    let settings = root.settings.unwrap_or(Value::Null);

    let burgs: Vec<Burg> = pack
        .get("burgs")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_burg).collect())
        .unwrap_or_default();
    if burgs.is_empty() {
        return Err(AzgaarError::NoBurgs);
    }

    let cells = pack.get("cells").map(parse_cells).unwrap_or_default();
    let routes = pack
        .get("routes")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(parse_route).collect())
        .unwrap_or_default();

    let (mut width, mut height) = (num(info.get("width")).unwrap_or(0.0), num(info.get("height")).unwrap_or(0.0));
    if width <= 0.0 || height <= 0.0 {
        // Repli : étendue des burgs et cellules.
        width = burgs.iter().map(|b| b.x).chain(cells.iter().map(|c| c.0)).fold(1.0, f64::max);
        height = burgs.iter().map(|b| b.y).chain(cells.iter().map(|c| c.1)).fold(1.0, f64::max);
    }

    Ok(AzgaarMap {
        name: info.get("mapName").and_then(Value::as_str).map(str::to_owned),
        width,
        height,
        distance_scale: num(settings.get("distanceScale")).filter(|s| *s > 0.0),
        distance_unit: settings.get("distanceUnit").and_then(Value::as_str).map(str::to_owned),
        burgs,
        cells,
        routes,
    })
}

pub(crate) fn parse_burg(v: &Value) -> Option<Burg> {
    let obj = v.as_object()?;
    if obj.get("removed").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let id = obj.get("i").and_then(Value::as_u64)?;
    if id == 0 { return None; } // l'élément 0 est toujours vide dans Azgaar
    let name = obj.get("name").and_then(Value::as_str)?.trim().to_owned();
    if name.is_empty() { return None; }
    let x = num(obj.get("x"))?;
    let y = num(obj.get("y"))?;
    if !x.is_finite() || !y.is_finite() { return None; }
    Some(Burg {
        id,
        name,
        x,
        y,
        cell: obj.get("cell").and_then(Value::as_u64).map(|c| c as usize),
        capital: obj.get("capital").and_then(Value::as_u64).unwrap_or(0) == 1,
    })
}

fn parse_point(v: &Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((num(a.first())?, num(a.get(1))?))
}

fn clamp_height(v: Option<&Value>) -> u8 { num(v).unwrap_or(0.0).clamp(0.0, 100.0) as u8 }

fn parse_cells(v: &Value) -> Vec<(f64, f64, u8)> {
    match v {
        Value::Array(arr) => arr
            .iter()
            .filter_map(|c| {
                let (x, y) = parse_point(c.get("p")?)?;
                Some((x, y, clamp_height(c.get("h"))))
            })
            .collect(),
        Value::Object(obj) => {
            let (Some(p), Some(h)) = (obj.get("p").and_then(Value::as_array), obj.get("h")) else { return vec![] };
            let heights: Vec<u8> = match h {
                Value::Array(a) => a.iter().map(|x| clamp_height(Some(x))).collect(),
                // Typed arrays sérialisés en objet {"0":12,"1":34,…}
                Value::Object(o) => (0..o.len()).map(|i| clamp_height(o.get(&i.to_string()))).collect(),
                _ => vec![],
            };
            p.iter()
                .zip(heights)
                .filter_map(|(pt, h)| parse_point(pt).map(|(x, y)| (x, y, h)))
                .collect()
        }
        _ => vec![],
    }
}

pub(crate) fn parse_route(v: &Value) -> Option<Route> {
    let points: Vec<RoutePoint> = v
        .get("points")?
        .as_array()?
        .iter()
        .filter_map(|p| {
            let (x, y) = parse_point(p)?;
            let cell = p.as_array().and_then(|a| a.get(2)).and_then(Value::as_u64).map(|c| c as usize);
            Some(RoutePoint { x, y, cell })
        })
        .collect();
    if points.len() < 2 { return None; }
    Some(Route {
        id: v.get("i").and_then(Value::as_u64),
        group: v.get("group").and_then(Value::as_str).map(str::to_owned),
        length: num(v.get("length")).filter(|l| *l > 0.0),
        points,
    })
}

/// Hauteur (0–100) de la cellule d'un burg, si connue.
pub fn burg_height(map: &AzgaarMap, burg: &Burg) -> Option<u8> {
    burg.cell.and_then(|c| map.cells.get(c)).map(|c| c.2)
}

/// Heightmap rééchantillonnée : grille `cols × rows` (≤ `MAX_TERRAIN_SIDE`),
/// moyenne des cellules par case, cases vides comblées par dilatation.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    pub cols: usize,
    pub rows: usize,
    pub heights: Vec<u8>,
}

pub fn resample_terrain(map: &AzgaarMap, max_side: usize) -> Option<Terrain> {
    if map.cells.is_empty() || max_side == 0 { return None; }
    let aspect = map.width / map.height;
    let (cols, rows) = if aspect >= 1.0 {
        (max_side, ((max_side as f64 / aspect).round() as usize).max(1))
    } else {
        (((max_side as f64 * aspect).round() as usize).max(1), max_side)
    };
    let mut sum = vec![0u32; cols * rows];
    let mut count = vec![0u32; cols * rows];
    for &(x, y, h) in &map.cells {
        let cx = ((x / map.width) * cols as f64).floor().clamp(0.0, (cols - 1) as f64) as usize;
        let cy = ((y / map.height) * rows as f64).floor().clamp(0.0, (rows - 1) as f64) as usize;
        sum[cy * cols + cx] += h as u32;
        count[cy * cols + cx] += 1;
    }
    let mut heights: Vec<Option<u8>> = sum.iter().zip(&count).map(|(&s, &c)| (c > 0).then(|| (s / c) as u8)).collect();
    // Dilatation : chaque case vide prend la moyenne de ses voisines remplies.
    for _ in 0..(cols.max(rows)) {
        if heights.iter().all(Option::is_some) { break; }
        let snapshot = heights.clone();
        for y in 0..rows {
            for x in 0..cols {
                if snapshot[y * cols + x].is_some() { continue; }
                let mut acc = (0u32, 0u32);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= cols as i64 || ny >= rows as i64 { continue; }
                    if let Some(h) = snapshot[ny as usize * cols + nx as usize] { acc.0 += h as u32; acc.1 += 1; }
                }
                if acc.1 > 0 { heights[y * cols + x] = Some((acc.0 / acc.1) as u8); }
            }
        }
    }
    Some(Terrain { cols, rows, heights: heights.into_iter().map(|h| h.unwrap_or(0)).collect() })
}

/// Normalise un nom pour la comparaison : minuscules, sans accents, alphanumérique.
pub fn normalize_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for ch in s.chars().flat_map(char::to_lowercase) {
        let base = match ch {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
            'ñ' | 'ń' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
            'ý' | 'ÿ' => 'y',
            'ß' => 's',
            'œ' => { out.push('o'); 'e' }
            'æ' => { out.push('a'); 'e' }
            c => c,
        };
        if base.is_alphanumeric() {
            out.push(base);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim_end().to_owned()
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchStatus {
    /// Un seul burg porte exactement ce nom.
    Exact(u64),
    /// Plusieurs candidats : l'admin doit choisir.
    Ambiguous(Vec<u64>),
    /// Correspondance partielle (préfixe) : à confirmer.
    Partial(Vec<u64>),
    None,
}

/// Propose un rattachement burg pour chaque lieu `(place_id, nom)`.
pub fn match_places(places: &[(u64, String)], burgs: &[Burg]) -> Vec<(u64, MatchStatus)> {
    let normalized: Vec<(u64, String)> = burgs.iter().map(|b| (b.id, normalize_name(&b.name))).collect();
    places
        .iter()
        .map(|(pid, name)| {
            let n = normalize_name(name);
            if n.is_empty() { return (*pid, MatchStatus::None); }
            let exact: Vec<u64> = normalized.iter().filter(|(_, b)| *b == n).map(|(id, _)| *id).collect();
            let status = match exact.len() {
                1 => MatchStatus::Exact(exact[0]),
                0 => {
                    let partial: Vec<u64> = normalized
                        .iter()
                        .filter(|(_, b)| !b.is_empty() && (b.starts_with(&n) || n.starts_with(b.as_str())))
                        .map(|(id, _)| *id)
                        .collect();
                    if partial.is_empty() { MatchStatus::None } else { MatchStatus::Partial(partial) }
                }
                _ => MatchStatus::Ambiguous(exact),
            };
            (*pid, status)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "info": {"version": "1.105", "mapName": "Testland", "width": 1000, "height": 500},
      "settings": {"distanceUnit": "km", "distanceScale": "3"},
      "pack": {
        "cells": [
          {"i":0,"p":[10,10],"h":5},
          {"i":1,"p":[500,250],"h":60},
          {"i":2,"p":[900,400],"h":35}
        ],
        "burgs": [
          {},
          {"i":1,"name":"Élanor","x":500.5,"y":250.1,"cell":1,"capital":1},
          {"i":2,"name":"Port-Royal","x":900,"y":400,"cell":2,"capital":0},
          {"i":3,"name":"Ruine","x":1,"y":1,"cell":0,"removed":true},
          {"i":4,"name":"Port Royal","x":905,"y":401,"cell":2}
        ],
        "routes": [
          {"i":0,"group":"roads","points":[[500,250,1],[900,400,2]]},
          {"i":1,"group":"trails","points":[[1,1,0]]}
        ],
        "unknownField": 42
      }
    }"#;

    #[test]
    fn parses_full_export() {
        let m = parse(SAMPLE).unwrap();
        assert_eq!(m.name.as_deref(), Some("Testland"));
        assert_eq!((m.width, m.height), (1000.0, 500.0));
        assert_eq!(m.distance_scale, Some(3.0));
        assert_eq!(m.burgs.len(), 3, "burg vide et burg retiré ignorés");
        assert!(m.burgs[0].capital);
        assert_eq!(m.cells.len(), 3);
        assert_eq!(m.routes.len(), 1, "route à un seul point ignorée");
        assert_eq!(burg_height(&m, &m.burgs[0]), Some(60));
    }

    #[test]
    fn parses_cells_as_struct_of_arrays() {
        let json = r#"{"pack":{"burgs":[{},{"i":1,"name":"A","x":1,"y":2}],
            "cells":{"p":[[0,0],[10,10]],"h":{"0":10,"1":70}}}}"#;
        let m = parse(json).unwrap();
        assert_eq!(m.cells, vec![(0.0, 0.0, 10), (10.0, 10.0, 70)]);
        assert!(m.width >= 10.0);
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(matches!(parse("not json"), Err(AzgaarError::InvalidJson(_))));
        assert_eq!(parse(r#"{"info":{}}"#).unwrap_err(), AzgaarError::MissingPack);
        assert_eq!(parse(r#"{"pack":{"burgs":[{}]}}"#).unwrap_err(), AzgaarError::NoBurgs);
    }

    #[test]
    fn normalizes_names() {
        assert_eq!(normalize_name("  Élanor-la  Grande! "), "elanor la grande");
        assert_eq!(normalize_name("Cœur"), "coeur");
        assert_eq!(normalize_name("Port-Royal"), normalize_name("port royal"));
    }

    #[test]
    fn matches_places_to_burgs() {
        let m = parse(SAMPLE).unwrap();
        let places = vec![
            (100, "elanor".to_string()),
            (101, "Port Royal".to_string()),
            (102, "Élan".to_string()),
            (103, "Atlantide".to_string()),
        ];
        let r = match_places(&places, &m.burgs);
        assert_eq!(r[0], (100, MatchStatus::Exact(1)));
        assert_eq!(r[1], (101, MatchStatus::Ambiguous(vec![2, 4])));
        assert_eq!(r[2], (102, MatchStatus::Partial(vec![1])));
        assert_eq!(r[3], (103, MatchStatus::None));
    }

    #[test]
    fn resamples_terrain_and_fills_gaps() {
        let m = parse(SAMPLE).unwrap();
        let t = resample_terrain(&m, 8).unwrap();
        assert_eq!((t.cols, t.rows), (8, 4));
        assert_eq!(t.heights.len(), 32);
        assert_eq!(t.heights[0], 5);
        assert_eq!(t.heights[2 * 8 + 4], 60);
        assert!(t.heights.iter().all(|&h| h <= 100));
    }

    #[test]
    fn lone_surrogates_from_azgaar_are_replaced() {
        // Cas réels d'un export : moitiés hautes répétées, moitié basse seule, paire correcte.
        let raw = r#"{"note":"a\ud802\ud802b\udc2dc\ud83d\ude00d\\ud802e\n"}"#;
        let fixed = replace_lone_surrogates(raw);
        assert_eq!(fixed, r#"{"note":"a\uFFFD\uFFFDb\uFFFDc\ud83d\ude00d\\ud802e\n"}"#);
        let v: serde_json::Value = serde_json::from_str(&fixed).unwrap();
        assert_eq!(v["note"], "a\u{FFFD}\u{FFFD}b\u{FFFD}c\u{1F600}d\\ud802e\n");
        assert!(serde_json::from_str::<serde_json::Value>(raw).is_err(), "serde_json refuses the raw text");
    }

    #[test]
    fn valid_json_is_left_untouched() {
        let raw = r#"{"a":"\u00e9\ud83d\ude00\\u0041"}"#;
        assert!(matches!(replace_lone_surrogates(raw), std::borrow::Cow::Borrowed(_)));
    }

    /// Vrai export « Full » d'Azgaar (fichier fourni par `AZGAAR_FULL_JSON`) : il se lit, ses bourgs
    /// et ses routes sont trouvés, et les routes candidates relient des bourgs avec une distance.
    /// Avec `AZGAAR_SUMMARY_OUT`, écrit un résumé (bourgs retenus, routes) en JSON.
    #[test]
    #[ignore = "integration test: needs AZGAAR_FULL_JSON (a real Azgaar Full JSON export)"]
    fn a_real_full_export_is_parsed() {
        let path = std::env::var("AZGAAR_FULL_JSON").expect("set AZGAAR_FULL_JSON");
        let map = parse(&std::fs::read_to_string(&path).unwrap()).expect("the export parses");
        assert!(map.width > 0.0 && map.height > 0.0);
        assert!(map.burgs.len() > 5, "burgs: {}", map.burgs.len());
        assert!(!map.routes.is_empty(), "routes");
        assert!(!map.cells.is_empty(), "cells");
        assert!(resample_terrain(&map, MAX_TERRAIN_SIDE).is_some());

        // Capitales d'abord, puis les autres : une douzaine de lieux.
        let mut burgs = map.burgs.clone();
        burgs.sort_by_key(|b| (!b.capital, b.id));
        burgs.truncate(12);
        let ids: Vec<u64> = burgs.iter().map(|b| b.id).collect();
        let roads = crate::activity::map::network::road_candidates(&map, &ids, &crate::activity::map::network::Zone::default(), false, 6.0);
        assert!(!roads.is_empty(), "no road between the chosen burgs");
        assert!(roads.iter().all(|r| r.distance_km > 0.0 && ids.contains(&r.burg_a) && ids.contains(&r.burg_b)));

        if let Ok(out) = std::env::var("AZGAAR_SUMMARY_OUT") {
            let summary = serde_json::json!({
                "name": map.name, "width": map.width, "height": map.height,
                "burgs": burgs.iter().map(|b| serde_json::json!({"id": b.id, "name": b.name, "x": b.x, "y": b.y, "capital": b.capital})).collect::<Vec<_>>(),
                "roads": roads.iter().map(|r| serde_json::json!({"a": r.burg_a, "b": r.burg_b, "km": r.distance_km, "sea": r.uses_sea, "path": r.path})).collect::<Vec<_>>(),
            });
            std::fs::write(out, serde_json::to_string_pretty(&summary).unwrap()).unwrap();
        }
    }
}
