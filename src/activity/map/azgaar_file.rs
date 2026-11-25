//! Lecture d'une sauvegarde Azgaar (`.map`), le format que l'administrateur importe.
//!
//! Format (Azgaar 1.158, `services/io/save.ts` / `load.ts`) : un texte dont chaque « ligne »
//! (séparateur `\r\n`) est une donnée. Le fichier peut être compressé en gzip (versions
//! récentes) ou encodé en base64 + URI (anciennes versions). Lignes utilisées :
//!
//! | Ligne | Contenu |
//! |------:|---------|
//! | 0 | `version|licence|date|graine|largeur|hauteur|identifiant` |
//! | 1 | réglages (JSON) : `units.distance.scale`, `units.distance.unit`, `lore.name` |
//! | 5 | dessin de la carte (SVG) — Azgaar annonce sa suppression dans une version future |
//! | 6 | grille (JSON) : `points` des cellules |
//! | 7 | hauteur de chaque cellule de la grille (`h`, 0–100, séparées par des virgules) |
//! | 15 | bourgs (JSON, comme `pack.burgs` de l'export JSON) |
//! | 37 | routes (JSON, comme `pack.routes`) : `points: [[x, y, cellule], …]` |
//!
//! Le rendu façon Azgaar (biomes, rivières, États…) lit d'autres lignes : voir `azgaar_style`.

use std::io::Read;

use base64::Engine;
use serde_json::Value;

use super::azgaar::{num, parse_burg, parse_route, replace_lone_surrogates, AzgaarError, AzgaarMap};

/// Taille maximale du texte décompressé : une grande carte fait quelques dizaines de Mo ; au-delà,
/// c'est une « bombe » de compression.
pub const MAX_DECOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;

const SEP: &str = "\r\n";

/// Sauvegarde lue : les données de la carte et, si présent, son dessin SVG.
#[derive(Debug, Clone)]
pub struct MapFile {
    pub version: String,
    pub map: AzgaarMap,
    pub svg: Option<String>,
}

/// Texte de la sauvegarde : décompresse le gzip, décode l'ancien base64.
pub fn decode(bytes: &[u8]) -> Result<String, AzgaarError> {
    let text = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|e| AzgaarError::InvalidJson(format!("gzip: {e}")))?;
        if out.len() as u64 > MAX_DECOMPRESSED_BYTES {
            return Err(AzgaarError::InvalidJson("decompressed file too large".into()));
        }
        String::from_utf8(out).map_err(|_| AzgaarError::InvalidJson("not UTF-8".into()))?
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| AzgaarError::InvalidJson("not UTF-8".into()))?
    };
    // Format interne : la première ligne contient des « | ». Sinon : ancien base64 de l'URI.
    if text.chars().take(10).any(|c| c == '|') {
        return Ok(text);
    }
    let raw = base64::engine::general_purpose::STANDARD
        .decode(text.trim().as_bytes())
        .map_err(|_| AzgaarError::InvalidJson("not an Azgaar .map file".into()))?;
    let uri = String::from_utf8(raw).map_err(|_| AzgaarError::InvalidJson("not UTF-8".into()))?;
    urlencoding::decode(&uri).map(|c| c.into_owned()).map_err(|_| AzgaarError::InvalidJson("bad URI encoding".into()))
}

/// Comme Azgaar : des fins de ligne `\r\n` à l'intérieur du SVG casseraient le découpage.
fn protect_svg(text: &str) -> std::borrow::Cow<'_, str> {
    let Some(start) = text.find("<svg") else { return text.into() };
    let Some(rel_end) = text[start..].find("</svg>") else { return text.into() };
    let end = start + rel_end + "</svg>".len();
    if !text[start..end].contains(SEP) {
        return text.into();
    }
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&text[start..end].replace(SEP, "\n"));
    out.push_str(&text[end..]);
    out.into()
}

fn json(record: Option<&&str>) -> Value {
    record.and_then(|r| serde_json::from_str(&replace_lone_surrogates(r)).ok()).unwrap_or(Value::Null)
}

/// Lit une sauvegarde `.map`.
pub fn parse_map_file(bytes: &[u8]) -> Result<MapFile, AzgaarError> {
    let text = decode(bytes)?;
    let text = protect_svg(&text);
    let records: Vec<&str> = text.split(SEP).collect();
    let params: Vec<&str> = records.first().map(|r| r.split('|').collect()).unwrap_or_default();
    let version = params.first().copied().unwrap_or("").to_string();
    if version.is_empty() || records.len() < 16 {
        return Err(AzgaarError::InvalidJson("not an Azgaar .map file".into()));
    }
    let size = |i: usize| params.get(i).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| *v > 0.0);

    let settings = json(records.get(1));
    let burgs: Vec<_> = json(records.get(15)).as_array().map(|a| a.iter().filter_map(parse_burg).collect()).unwrap_or_default();
    if burgs.is_empty() {
        return Err(AzgaarError::NoBurgs);
    }
    let routes = json(records.get(37)).as_array().map(|a| a.iter().filter_map(parse_route).collect()).unwrap_or_default();

    // Hauteurs : cellules de la grille (points de la ligne 6, hauteurs de la ligne 7).
    let grid = json(records.get(6));
    let heights: Vec<u8> = records.get(7).map(|r| r.split(',').map(|h| h.trim().parse::<f64>().unwrap_or(0.0).clamp(0.0, 100.0) as u8).collect()).unwrap_or_default();
    let cells: Vec<(f64, f64, u8)> = grid
        .get("points")
        .and_then(Value::as_array)
        .map(|pts| {
            pts.iter()
                .zip(heights.iter())
                .filter_map(|(p, h)| {
                    let a = p.as_array()?;
                    Some((num(a.first())?, num(a.get(1))?, *h))
                })
                .collect()
        })
        .unwrap_or_default();

    let units = settings.pointer("/units/distance");
    let width = size(4).unwrap_or_else(|| burgs.iter().map(|b| b.x).fold(1.0, f64::max));
    let height = size(5).unwrap_or_else(|| burgs.iter().map(|b| b.y).fold(1.0, f64::max));
    let svg = records.get(5).filter(|r| r.trim_start().starts_with("<svg")).map(|r| r.to_string());
    let style = super::azgaar_style::build_style(&records, width, height);

    Ok(MapFile {
        version,
        svg,
        map: AzgaarMap {
            name: settings.pointer("/lore/name").and_then(Value::as_str).map(str::to_owned),
            width,
            height,
            distance_scale: num(units.and_then(|u| u.get("scale"))).filter(|s| *s > 0.0),
            distance_unit: units.and_then(|u| u.get("unit")).and_then(Value::as_str).map(str::to_owned),
            burgs,
            cells,
            routes,
            style,
        },
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// Sauvegarde minimale au format d'Azgaar 1.158 (lignes inutilisées vides).
    pub(crate) fn sample_map_text() -> String {
        let mut records = vec![String::new(); 56];
        records[0] = "1.158.0|File can be loaded in azgaar.github.io/Fantasy-Map-Generator|2026-11-19|424242|1600|1000|1795000000000".into();
        records[1] = r#"{"units":{"distance":{"scale":3,"unit":"mi"}},"lore":{"name":"Intreaulet"}}"#.into();
        // SVG contenant une fin de ligne CRLF (Azgaar la protège au chargement).
        records[5] = "<svg id=\"map\" width=\"1600\" height=\"1000\"><text>a\r\nb</text></svg>".into();
        records[6] = r#"{"spacing":10,"cellsX":2,"cellsY":1,"points":[[100,100],[900,500]]}"#.into();
        records[7] = "5,60".into();
        records[15] = r#"[{},{"i":1,"name":"Harwick","x":120.5,"y":80,"cell":1,"capital":1},{"i":2,"name":"Westham","x":130,"y":300,"cell":3},{"i":3,"name":"Ruine","x":1,"y":1,"removed":true}]"#.into();
        records[37] = r#"[{"i":0,"group":"roads","points":[[120.5,80,1],[140,150,2],[130,300,3]]}]"#.into();
        records.join("\r\n")
    }

    #[test]
    fn a_plain_save_is_read() {
        let f = parse_map_file(sample_map_text().as_bytes()).unwrap();
        assert_eq!(f.version, "1.158.0");
        assert_eq!((f.map.width, f.map.height), (1600.0, 1000.0));
        assert_eq!(f.map.name.as_deref(), Some("Intreaulet"));
        assert_eq!((f.map.distance_scale, f.map.distance_unit.as_deref()), (Some(3.0), Some("mi")));
        assert_eq!(f.map.burgs.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), vec!["Harwick", "Westham"], "removed burgs are skipped");
        assert_eq!(f.map.routes.len(), 1);
        assert_eq!(f.map.routes[0].points.len(), 3);
        assert_eq!(f.map.cells, vec![(100.0, 100.0, 5), (900.0, 500.0, 60)]);
        let svg = f.svg.unwrap();
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>") && svg.contains("a\nb"), "CRLF inside the svg does not split records");
    }

    #[test]
    fn a_gzip_save_is_read() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(sample_map_text().as_bytes()).unwrap();
        let f = parse_map_file(&gz.finish().unwrap()).unwrap();
        assert_eq!(f.map.burgs.len(), 2);
    }

    #[test]
    fn an_old_base64_save_is_read() {
        let encoded = urlencoding::encode(&sample_map_text()).into_owned();
        let b64 = base64::engine::general_purpose::STANDARD.encode(encoded.as_bytes());
        assert_eq!(parse_map_file(b64.as_bytes()).unwrap().map.routes.len(), 1);
    }

    #[test]
    fn the_route_network_of_a_save_gives_road_candidates() {
        let f = parse_map_file(sample_map_text().as_bytes()).unwrap();
        let c = crate::activity::map::network::road_candidates(&f.map, &[1, 2], &Default::default(), false, 0.0);
        assert_eq!(c.len(), 1);
        // 3 mi par pixel, deux tronçons : (120,5 ; 80) → (140 ; 150) → (130 ; 300).
        let px = (19.5f64.powi(2) + 70f64.powi(2)).sqrt() + (10f64.powi(2) + 150f64.powi(2)).sqrt();
        assert!((c[0].distance_km - px * 3.0 * 1.609_344).abs() < 1e-6, "{}", c[0].distance_km);
    }

    #[test]
    fn other_files_are_refused() {
        assert!(parse_map_file(b"{\"info\":{}}").is_err());
        assert!(parse_map_file(b"\x89PNG\r\n\x1a\n").is_err());
        assert!(parse_map_file(b"").is_err());
        let mut no_burgs = sample_map_text();
        no_burgs = no_burgs.replace("Harwick", "").replace("Westham", "");
        assert!(matches!(parse_map_file(no_burgs.as_bytes()), Err(AzgaarError::NoBurgs)));
    }

    #[test]
    fn a_compression_bomb_is_refused() {
        // 300 Mo de zéros compressés en quelques centaines de Ko.
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        let chunk = vec![b'0'; 1 << 20];
        for _ in 0..300 {
            gz.write_all(&chunk).unwrap();
        }
        let bomb = gz.finish().unwrap();
        assert!(bomb.len() < 2 << 20);
        assert!(parse_map_file(&bomb).is_err());
    }

    /// Vraie sauvegarde `.map` (fichier fourni par `AZGAAR_MAP_FILE`). Avec `AZGAAR_FULL_JSON`
    /// (export JSON « Full » de la même carte), vérifie que les deux donnent les mêmes données.
    #[test]
    #[ignore = "integration test: needs AZGAAR_MAP_FILE (a real Azgaar .map save)"]
    fn a_real_map_save_is_read() {
        let path = std::env::var("AZGAAR_MAP_FILE").expect("set AZGAAR_MAP_FILE");
        let f = parse_map_file(&std::fs::read(&path).unwrap()).expect("the save is read");
        assert!(f.map.width > 0.0 && f.map.height > 0.0);
        assert!(f.map.burgs.len() > 5 && !f.map.routes.is_empty() && !f.map.cells.is_empty());
        assert!(f.svg.as_deref().is_some_and(|s| s.starts_with("<svg") && s.ends_with("</svg>")), "svg record");
        let ids: Vec<u64> = f.map.burgs.iter().take(12).map(|b| b.id).collect();
        let roads = crate::activity::map::network::road_candidates(&f.map, &ids, &Default::default(), false, 6.0);
        assert!(roads.iter().all(|r| r.path.len() >= 2), "every road has a path");

        if let Ok(json) = std::env::var("AZGAAR_FULL_JSON") {
            let j = crate::activity::map::azgaar::parse(&std::fs::read_to_string(json).unwrap()).unwrap();
            assert_eq!((f.map.width, f.map.height), (j.width, j.height));
            assert_eq!(f.map.burgs, j.burgs, "same burgs");
            assert_eq!(f.map.routes.len(), j.routes.len(), "same routes");
            assert_eq!((f.map.distance_scale, f.map.distance_unit.clone()), (j.distance_scale, j.distance_unit.clone()));
            let jr = crate::activity::map::network::road_candidates(&j, &ids, &Default::default(), false, 6.0);
            assert_eq!(roads.len(), jr.len(), "same road candidates");
        }
        eprintln!(
            "{} {}: {}×{}, {} bourgs, {} routes, {} cellules, svg {} Ko, {} routes candidates pour 12 bourgs",
            f.map.name.as_deref().unwrap_or("?"), f.version, f.map.width, f.map.height, f.map.burgs.len(), f.map.routes.len(),
            f.map.cells.len(), f.svg.as_ref().map_or(0, |s| s.len() / 1024), roads.len()
        );
        // Densité du réseau complet : sert à dimensionner la répartition sur plusieurs serveurs.
        let all: Vec<u64> = f.map.burgs.iter().map(|b| b.id).collect();
        for sea in [false, true] {
            let full = crate::activity::map::network::road_candidates(&f.map, &all, &Default::default(), sea, 6.0);
            let linked: std::collections::HashSet<u64> = full.iter().flat_map(|r| [r.burg_a, r.burg_b]).collect();
            eprintln!(
                "réseau complet (maritime : {sea}) : {} routes, {} bourgs desservis, {:.2} routes par bourg desservi",
                full.len(), linked.len(), 2.0 * full.len() as f64 / linked.len().max(1) as f64
            );
        }
    }
}
