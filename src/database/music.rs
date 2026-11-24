//! Bibliothèque musicale d'un univers (`docs/activity/MUSIC_SPEC.md` § 2).
//!
//! Un morceau est soit un fichier audio contrôlé (`music::audio::check_audio`), stocké dans GridFS
//! (bucket `music_files`), soit une vidéo YouTube (identifiant seul, lue par le lecteur officiel de
//! YouTube dans l'activité : rien n'est téléchargé). Il est assigné à des lieux (`category_id`)
//! et/ou des routes (`channel_id`).

use futures::io::{AsyncReadExt, AsyncWriteExt};
use futures::TryStreamExt;
use mongodb::bson::{doc, oid::ObjectId, Bson, DateTime};
use mongodb::options::GridFsBucketOptions;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{MUSIC_TRACKS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::music::audio::AudioInfo;

/// Quota de fichiers audio par univers (500 Mio).
pub const MAX_UNIVERSE_BYTES: u64 = 500 * 1024 * 1024;
/// Nombre max de morceaux par univers.
pub const MAX_TRACKS_PER_UNIVERSE: u64 = 300;
/// Longueur max d'un titre (caractères).
pub const MAX_TITLE_CHARS: usize = 100;
const BUCKET: &str = "music_files";

/// D'où vient le morceau.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrackOrigin {
    File,
    Youtube { video_id: String },
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MusicTrack {
    #[serde(rename = "_id")]
    pub id: ObjectId,
    pub universe_id: ObjectId,
    pub title: String,
    pub origin: TrackOrigin,
    /// Fichier dans GridFS ; `None` pour une vidéo YouTube.
    #[serde(default)]
    pub file_id: Option<ObjectId>,
    /// Type servi, issu du contrôle du fichier (vide pour une vidéo YouTube).
    #[serde(default)]
    pub mime: String,
    pub duration_ms: u64,
    pub bytes: u64,
    /// Lieux (`category_id`).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub places: Vec<u64>,
    /// Routes (`channel_id`).
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub roads: Vec<u64>,
    #[serde_as(as = "DisplayFromStr")]
    pub created_by: u64,
    pub created_at: DateTime,
    /// L'admin a déclaré avoir les droits sur la musique.
    pub rights_confirmed: bool,
}

/// Titre nettoyé : caractères de contrôle retirés, espaces réduits, longueur bornée. `None` si vide.
pub fn clean_title(raw: &str) -> Option<String> {
    let t: String = raw.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(MAX_TITLE_CHARS).collect();
    (!t.is_empty()).then_some(t)
}

fn collection(db: &mongodb::Database) -> mongodb::Collection<MusicTrack> {
    db.collection::<MusicTrack>(MUSIC_TRACKS_COLLECTION_NAME)
}

fn bucket(db: &mongodb::Database) -> mongodb::gridfs::GridFsBucket {
    db.gridfs_bucket(GridFsBucketOptions::builder().bucket_name(BUCKET.to_string()).build())
}

async fn db() -> mongodb::Database {
    get_db_client().await.database(VERSEENGINE_DB_NAME)
}

/// Place occupée et nombre de morceaux d'un univers.
pub async fn usage(universe_id: ObjectId) -> mongodb::error::Result<(u64, u64)> {
    let mut cursor = collection(&db().await).find(doc! {"universe_id": universe_id}).await?;
    let (mut bytes, mut count) = (0u64, 0u64);
    while let Some(t) = cursor.try_next().await? {
        bytes += t.bytes;
        count += 1;
    }
    Ok((bytes, count))
}

/// Nouveau morceau (pas encore enregistré).
pub struct NewTrack<'a> {
    pub universe_id: ObjectId,
    pub title: String,
    pub origin: TrackOrigin,
    pub bytes: &'a [u8],
    pub info: AudioInfo,
    pub created_by: u64,
    pub rights_confirmed: bool,
}

/// Enregistre le fichier (déjà contrôlé) et le morceau. Quota vérifié ici ; l'appelant sérialise
/// les envois d'un même univers.
pub async fn insert_track(new: NewTrack<'_>) -> Result<MusicTrack, crate::discord::poise_structs::Error> {
    let (used, count) = usage(new.universe_id).await?;
    if count >= MAX_TRACKS_PER_UNIVERSE || used + new.bytes.len() as u64 > MAX_UNIVERSE_BYTES {
        return Err("activity__music_quota".into());
    }
    let db = db().await;
    let file_id = ObjectId::new();
    let mut upload = bucket(&db).open_upload_stream(file_id.to_hex()).id(Bson::ObjectId(file_id)).await?;
    upload.write_all(new.bytes).await?;
    upload.close().await?;
    let track = MusicTrack {
        id: ObjectId::new(),
        universe_id: new.universe_id,
        title: new.title,
        origin: new.origin,
        file_id: Some(file_id),
        mime: new.info.container.mime().to_string(),
        duration_ms: new.info.duration_ms,
        bytes: new.bytes.len() as u64,
        places: vec![],
        roads: vec![],
        created_by: new.created_by,
        created_at: DateTime::now(),
        rights_confirmed: new.rights_confirmed,
    };
    if let Err(e) = collection(&db).insert_one(&track).await {
        let _ = bucket(&db).delete(Bson::ObjectId(file_id)).await;
        return Err(e.into());
    }
    Ok(track)
}

/// Ajoute une vidéo YouTube (identifiant déjà vérifié) : rien n'est téléchargé ni stocké, hormis
/// l'identifiant. Compte dans le nombre de morceaux, pas dans la place occupée.
pub async fn insert_youtube_track(universe_id: ObjectId, title: String, video_id: String, created_by: u64, rights_confirmed: bool) -> Result<MusicTrack, crate::discord::poise_structs::Error> {
    let (_, count) = usage(universe_id).await?;
    if count >= MAX_TRACKS_PER_UNIVERSE {
        return Err("activity__music_quota".into());
    }
    let track = MusicTrack {
        id: ObjectId::new(),
        universe_id,
        title,
        origin: TrackOrigin::Youtube { video_id },
        file_id: None,
        mime: String::new(),
        duration_ms: 0,
        bytes: 0,
        places: vec![],
        roads: vec![],
        created_by,
        created_at: DateTime::now(),
        rights_confirmed,
    };
    collection(&db().await).insert_one(&track).await?;
    Ok(track)
}

/// Morceaux d'un univers, par titre.
pub async fn list_tracks(universe_id: ObjectId) -> mongodb::error::Result<Vec<MusicTrack>> {
    let mut tracks: Vec<MusicTrack> = collection(&db().await).find(doc! {"universe_id": universe_id}).await?.try_collect().await?;
    tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()).then(a.id.cmp(&b.id)));
    Ok(tracks)
}

pub async fn get_track(universe_id: ObjectId, id: ObjectId) -> mongodb::error::Result<Option<MusicTrack>> {
    collection(&db().await).find_one(doc! {"_id": id, "universe_id": universe_id}).await
}

/// Change le titre et les assignations. Lieux et routes déjà vérifiés par l'appelant.
pub async fn update_track(universe_id: ObjectId, id: ObjectId, title: &str, places: &[u64], roads: &[u64]) -> mongodb::error::Result<bool> {
    let places: Vec<String> = places.iter().map(u64::to_string).collect();
    let roads: Vec<String> = roads.iter().map(u64::to_string).collect();
    let r = collection(&db().await)
        .update_one(doc! {"_id": id, "universe_id": universe_id}, doc! {"$set": {"title": title, "places": places, "roads": roads}})
        .await?;
    Ok(r.matched_count == 1)
}

/// Supprime le morceau puis son fichier.
pub async fn delete_track(universe_id: ObjectId, id: ObjectId) -> mongodb::error::Result<bool> {
    let db = db().await;
    let Some(track) = collection(&db).find_one_and_delete(doc! {"_id": id, "universe_id": universe_id}).await? else {
        return Ok(false);
    };
    if let Some(file_id) = track.file_id {
        if let Err(e) = bucket(&db).delete(Bson::ObjectId(file_id)).await {
            tracing::warn!(error = %e, "music: fichier non supprimé");
        }
    }
    Ok(true)
}

/// Où se trouve le joueur, pour choisir la playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Place(u64),
    Road(u64),
}

/// Morceaux à jouer à cet endroit.
pub async fn playlist(universe_id: ObjectId, spot: Spot) -> mongodb::error::Result<Vec<MusicTrack>> {
    let filter = match spot {
        Spot::Place(p) => doc! {"universe_id": universe_id, "places": p.to_string()},
        Spot::Road(r) => doc! {"universe_id": universe_id, "roads": r.to_string()},
    };
    let mut tracks: Vec<MusicTrack> = collection(&db().await).find(filter).await?.try_collect().await?;
    tracks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tracks)
}

/// L'univers est supprimé : tous ses morceaux et leurs fichiers.
pub async fn delete_universe_music(universe_id: ObjectId) -> mongodb::error::Result<()> {
    let db = db().await;
    let tracks: Vec<MusicTrack> = collection(&db).find(doc! {"universe_id": universe_id}).await?.try_collect().await?;
    for file_id in tracks.iter().filter_map(|t| t.file_id) {
        let _ = bucket(&db).delete(Bson::ObjectId(file_id)).await;
    }
    collection(&db).delete_many(doc! {"universe_id": universe_id}).await?;
    Ok(())
}

/// Contenu du fichier d'un morceau.
pub async fn read_track_file(file_id: ObjectId) -> mongodb::error::Result<Vec<u8>> {
    let mut stream = bucket(&db().await).open_download_stream(Bson::ObjectId(file_id)).await?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.map_err(|e| mongodb::error::Error::custom(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_cleaned_and_bounded() {
        assert_eq!(clean_title("  Taverne \n du\t port  ").as_deref(), Some("Taverne du port"));
        assert_eq!(clean_title("\u{0}\u{7}  "), None);
        assert_eq!(clean_title(&"é".repeat(300)).unwrap().chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn the_origin_is_tagged() {
        assert_eq!(mongodb::bson::to_bson(&TrackOrigin::File).unwrap(), Bson::Document(doc! {"kind": "file"}));
        assert_eq!(
            mongodb::bson::to_bson(&TrackOrigin::Youtube { video_id: "dQw4w9WgXcQ".into() }).unwrap(),
            Bson::Document(doc! {"kind": "youtube", "video_id": "dQw4w9WgXcQ"})
        );
    }

    /// Bout en bout contre MongoDB : envoi, quota, assignation, playlist, suppression.
    #[test]
    #[ignore = "integration test: needs MongoDB"]
    fn a_track_lives_and_dies_in_mongodb() {
        crate::test_support::block_on(async {
            let _ = get_db_client().await;
            let universe_id = ObjectId::new();
            let bytes = std::fs::read(format!("{}/tests/fixtures/audio/tone.ogg", env!("CARGO_MANIFEST_DIR"))).unwrap();
            let info = crate::music::audio::check_audio(&bytes).unwrap();
            let new = |title: &str| NewTrack { universe_id, title: title.into(), origin: TrackOrigin::File, bytes: &bytes, info: info.clone(), created_by: 42, rights_confirmed: true };
            let a = insert_track(new("Taverne")).await.unwrap();
            let b = insert_track(new("Forêt")).await.unwrap();
            assert_eq!(usage(universe_id).await.unwrap(), (2 * bytes.len() as u64, 2));
            assert_eq!(read_track_file(a.file_id.unwrap()).await.unwrap(), bytes);

            assert!(update_track(universe_id, a.id, "Taverne", &[10, 11], &[]).await.unwrap());
            assert!(update_track(universe_id, b.id, "Forêt", &[11], &[500]).await.unwrap());
            assert!(!update_track(ObjectId::new(), a.id, "x", &[], &[]).await.unwrap(), "another universe cannot touch it");
            let ids = |v: Vec<MusicTrack>| v.into_iter().map(|t| t.title).collect::<Vec<_>>();
            assert_eq!(ids(playlist(universe_id, Spot::Place(11)).await.unwrap()), vec!["Taverne", "Forêt"]);
            assert_eq!(ids(playlist(universe_id, Spot::Place(10)).await.unwrap()), vec!["Taverne"]);
            assert_eq!(ids(playlist(universe_id, Spot::Road(500)).await.unwrap()), vec!["Forêt"]);
            assert_eq!(ids(list_tracks(universe_id).await.unwrap()), vec!["Forêt", "Taverne"]);

            assert!(delete_track(universe_id, a.id).await.unwrap());
            assert!(read_track_file(a.file_id.unwrap()).await.is_err(), "the file is deleted too");
            assert!(delete_track(universe_id, b.id).await.unwrap());
            assert_eq!(usage(universe_id).await.unwrap(), (0, 0));

            // Vidéo YouTube : comptée, sans fichier ni place occupée.
            let v = insert_youtube_track(universe_id, "Thème".into(), "dQw4w9WgXcQ".into(), 42, true).await.unwrap();
            assert_eq!(usage(universe_id).await.unwrap(), (0, 1));
            assert_eq!(get_track(universe_id, v.id).await.unwrap().unwrap().origin, TrackOrigin::Youtube { video_id: "dQw4w9WgXcQ".into() });
            assert!(delete_track(universe_id, v.id).await.unwrap());
        });
    }
}
