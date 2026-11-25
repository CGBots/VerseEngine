//! Sauvegardes `.map` d'Azgaar des univers (bucket GridFS `map_sources`), gardées à chaque import
//! ou mise à jour du rendu : c'est une donnée précieuse pour le créateur (refaire la carte,
//! la retoucher dans Azgaar, recalculer le rendu). Les 3 dernières par univers.
//!
//! Jamais envoyées aux joueurs : le fichier dévoile toute la carte (brouillard, routes secrètes).

use futures::io::{AsyncReadExt, AsyncWriteExt};
use futures::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId, Bson, Document};
use mongodb::options::GridFsBucketOptions;
use serde::Serialize;

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::VERSEENGINE_DB_NAME;

const BUCKET: &str = "map_sources";
/// Sauvegardes gardées par univers (les plus anciennes sont supprimées).
pub const KEEP_PER_UNIVERSE: usize = 3;

/// Ce qu'on sait d'une sauvegarde gardée.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MapSourceInfo {
    #[serde(skip)]
    pub file_id: ObjectId,
    /// Graine d'Azgaar (champ 4 de la première ligne du `.map`), si lisible.
    pub seed: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub bytes: u64,
    /// Date d'enregistrement (s).
    pub saved_at: u64,
}

/// Graine et version lues dans la première ligne d'un `.map` (`version|licence|date|graine|…`),
/// gzip compris. `None` si illisible.
pub fn read_header(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(text) = crate::activity::map::azgaar_file::decode(bytes) else { return (None, None) };
    let first = text.lines().next().unwrap_or("");
    let fields: Vec<&str> = first.split('|').collect();
    let clean = |s: Option<&&str>| s.map(|v| v.trim()).filter(|v| !v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')).map(str::to_string);
    (clean(fields.get(3)), clean(fields.first()))
}

fn bucket(db: &mongodb::Database) -> mongodb::gridfs::GridFsBucket {
    db.gridfs_bucket(GridFsBucketOptions::builder().bucket_name(BUCKET.to_string()).build())
}

fn info_of(d: &Document) -> Option<MapSourceInfo> {
    let meta = d.get_document("metadata").ok()?;
    let text = |k: &str| meta.get_str(k).ok().map(str::to_string);
    Some(MapSourceInfo {
        file_id: d.get_object_id("_id").ok()?,
        seed: text("seed"),
        name: text("name"),
        version: text("version"),
        bytes: match d.get("length") {
            Some(Bson::Int64(v)) => *v as u64,
            Some(Bson::Int32(v)) => *v as u64,
            _ => 0,
        },
        saved_at: meta.get_i64("saved_at").map(|v| v as u64).unwrap_or(0),
    })
}

/// Garde la sauvegarde `bytes` de l'univers ; supprime les plus anciennes au-delà de 3.
pub async fn save(universe_id: ObjectId, bytes: &[u8], name: Option<&str>, saved_by: u64, now: u64) -> mongodb::error::Result<MapSourceInfo> {
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let (seed, version) = read_header(bytes);
    let file_id = ObjectId::new();
    let mut metadata = doc! {"universe_id": universe_id, "saved_by": saved_by.to_string(), "saved_at": now as i64};
    if let Some(s) = &seed {
        metadata.insert("seed", s);
    }
    if let Some(v) = &version {
        metadata.insert("version", v);
    }
    if let Some(n) = name {
        metadata.insert("name", n.chars().take(100).collect::<String>());
    }
    let mut up = bucket(&db).open_upload_stream(format!("{}.map", universe_id.to_hex())).id(Bson::ObjectId(file_id)).metadata(metadata).await?;
    up.write_all(bytes).await.map_err(|e| mongodb::error::Error::custom(e.to_string()))?;
    up.close().await.map_err(|e| mongodb::error::Error::custom(e.to_string()))?;
    // Au-delà des 3 dernières : supprimées.
    let all = list(universe_id).await?;
    for old in all.iter().skip(KEEP_PER_UNIVERSE) {
        let _ = bucket(&db).delete(Bson::ObjectId(old.file_id)).await;
    }
    Ok(MapSourceInfo { file_id, seed, name: name.map(str::to_string), version, bytes: bytes.len() as u64, saved_at: now })
}

/// Sauvegardes de l'univers, la plus récente d'abord.
pub async fn list(universe_id: ObjectId) -> mongodb::error::Result<Vec<MapSourceInfo>> {
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let files = db.collection::<Document>(&format!("{BUCKET}.files"));
    let docs: Vec<Document> = files.find(doc! {"metadata.universe_id": universe_id}).sort(doc! {"uploadDate": -1, "_id": -1}).await?.try_collect().await?;
    Ok(docs.iter().filter_map(info_of).collect())
}

pub async fn read(file_id: ObjectId) -> mongodb::error::Result<Vec<u8>> {
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let mut stream = bucket(&db).open_download_stream(Bson::ObjectId(file_id)).await?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.map_err(|e| mongodb::error::Error::custom(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_and_version_are_read_from_the_first_line() {
        let text = "1.158.0|File can be loaded in azgaar.github.io/Fantasy-Map-Generator|2026-11-24|962345872|1609|550|1732\r\n{}";
        assert_eq!(read_header(text.as_bytes()), (Some("962345872".into()), Some("1.158.0".into())));
        assert_eq!(read_header(b"not a map"), (None, None));
        assert_eq!(read_header("x|y|z|<script>|".as_bytes()).0, None, "odd characters are refused");
    }

    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn only_the_three_latest_saves_are_kept() {
        crate::test_support::block_on(async {
            let u = ObjectId::new();
            for i in 0..5u64 {
                save(u, format!("1.158.0|lic|date|seed{i}|10|10|1\n").as_bytes(), Some("Carte"), 1, 100 + i).await.unwrap();
            }
            let all = list(u).await.unwrap();
            let seeds: Vec<_> = all.iter().map(|i| i.seed.clone().unwrap()).collect();
            let last = read(all[0].file_id).await.unwrap();
            for i in &all {
                let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
                let _ = bucket(&db).delete(Bson::ObjectId(i.file_id)).await;
            }
            assert_eq!(seeds, vec!["seed4", "seed3", "seed2"]);
            assert!(String::from_utf8_lossy(&last).contains("seed4"));
        });
    }
}
