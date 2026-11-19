//! Fond de carte d'un univers : l'image exportée par Azgaar (*Export → PNG/JPEG*), affichée telle
//! quelle pour avoir exactement les visuels d'Azgaar. Les positions des lieux (`universe_maps`)
//! sont dans le même repère que l'image (pixels de la carte Azgaar).
//!
//! Stockée dans GridFS (une carte peut dépasser la limite de 16 Mo d'un document MongoDB).
//! Seuls PNG, JPEG et WebP sont acceptés, reconnus à leurs premiers octets : un SVG peut contenir
//! du script et n'est pas accepté.

use futures::io::{AsyncReadExt, AsyncWriteExt};
use mongodb::bson::{doc, oid::ObjectId, Bson};
use mongodb::options::GridFsBucketOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{UNIVERSE_MAPS_COLLECTION_NAME, VERSEENGINE_DB_NAME};

/// Taille maximale d'un fond de carte.
pub const MAX_BACKGROUND_BYTES: usize = 15 * 1024 * 1024;
/// Côté maximal (pixels) : au-delà, le navigateur d'un ordinateur modeste peine à l'afficher.
pub const MAX_BACKGROUND_SIDE: u32 = 16_384;
const BUCKET: &str = "map_backgrounds";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MapBackground {
    pub file_id: ObjectId,
    pub content_type: String,
    pub width: u32,
    pub height: u32,
    /// Empreinte SHA-256 (hex) : sert de version dans l'URL, donc de clé de cache.
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BackgroundError {
    TooLarge,
    UnsupportedFormat,
    InvalidImage,
    TooManyPixels,
}

impl BackgroundError {
    /// Clé Fluent renvoyée à l'administrateur.
    pub fn key(&self) -> &'static str {
        match self {
            BackgroundError::TooLarge => "activity__map_background_too_large",
            BackgroundError::UnsupportedFormat => "activity__map_background_unsupported",
            BackgroundError::InvalidImage | BackgroundError::TooManyPixels => "activity__map_background_invalid",
        }
    }
}

fn be16(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}
fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}
fn le16(b: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32)
}
fn le24(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, 0]))
}

/// Reconnaît le format et lit les dimensions : `(type MIME, largeur, hauteur)`.
pub fn inspect_image(bytes: &[u8]) -> Result<(&'static str, u32, u32), BackgroundError> {
    if bytes.len() > MAX_BACKGROUND_BYTES {
        return Err(BackgroundError::TooLarge);
    }
    let (mime, w, h) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        // Premier bloc : IHDR (largeur, hauteur en big-endian).
        if bytes.get(12..16) != Some(b"IHDR") {
            return Err(BackgroundError::InvalidImage);
        }
        ("image/png", be32(bytes, 16), be32(bytes, 20))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        let (w, h) = jpeg_size(bytes).ok_or(BackgroundError::InvalidImage)?;
        ("image/jpeg", Some(w), Some(h))
    } else if bytes.len() >= 30 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        match &bytes[12..16] {
            b"VP8X" => ("image/webp", le24(bytes, 24).map(|v| v + 1), le24(bytes, 27).map(|v| v + 1)),
            b"VP8 " => ("image/webp", le16(bytes, 26).map(|v| v & 0x3FFF), le16(bytes, 28).map(|v| v & 0x3FFF)),
            b"VP8L" => {
                let b = &bytes[21..25];
                let bits = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                ("image/webp", Some((bits & 0x3FFF) + 1), Some(((bits >> 14) & 0x3FFF) + 1))
            }
            _ => return Err(BackgroundError::InvalidImage),
        }
    } else {
        return Err(BackgroundError::UnsupportedFormat);
    };
    let (w, h) = (w.ok_or(BackgroundError::InvalidImage)?, h.ok_or(BackgroundError::InvalidImage)?);
    if w == 0 || h == 0 {
        return Err(BackgroundError::InvalidImage);
    }
    if w > MAX_BACKGROUND_SIDE || h > MAX_BACKGROUND_SIDE {
        return Err(BackgroundError::TooManyPixels);
    }
    Ok((mime, w, h))
}

/// Parcourt les segments JPEG jusqu'au premier SOFn (dimensions de l'image).
fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = be16(b, i + 2)? as usize;
        let is_sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            return Some((be16(b, i + 7)?, be16(b, i + 5)?));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn bucket(db: &mongodb::Database) -> mongodb::gridfs::GridFsBucket {
    db.gridfs_bucket(GridFsBucketOptions::builder().bucket_name(BUCKET.to_string()).build())
}

/// Enregistre `bytes` comme fond de carte de l'univers et l'attache à sa carte (`universe_maps`).
/// L'ancien fond est supprimé ensuite. La carte doit exister (positions importées).
pub async fn set_map_background(universe_id: ObjectId, bytes: &[u8]) -> Result<MapBackground, crate::discord::poise_structs::Error> {
    let (mime, width, height) = inspect_image(bytes).map_err(|e| e.key())?;
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let maps = db.collection::<mongodb::bson::Document>(UNIVERSE_MAPS_COLLECTION_NAME);
    let previous = maps
        .find_one(doc! {"universe_id": universe_id})
        .await?
        .ok_or("activity__map_not_found")?
        .get_document("background")
        .ok()
        .and_then(|b| b.get_object_id("file_id").ok());

    let file_id = ObjectId::new();
    let mut upload = bucket(&db).open_upload_stream(format!("{}.{}", universe_id.to_hex(), &mime[6..])).id(Bson::ObjectId(file_id)).await?;
    upload.write_all(bytes).await?;
    upload.close().await?;

    let background = MapBackground {
        file_id,
        content_type: mime.to_string(),
        width,
        height,
        sha256: Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect(),
        bytes: bytes.len() as u64,
    };
    let updated = maps
        .update_one(doc! {"universe_id": universe_id}, doc! {"$set": {"background": mongodb::bson::to_bson(&background)?}})
        .await?;
    if updated.matched_count == 0 {
        // La carte a disparu entre-temps : on ne laisse pas de fichier orphelin.
        let _ = bucket(&db).delete(Bson::ObjectId(file_id)).await;
        return Err("activity__map_not_found".into());
    }
    if let Some(old) = previous.filter(|old| *old != file_id) {
        if let Err(e) = bucket(&db).delete(Bson::ObjectId(old)).await {
            tracing::warn!(error = %e, "activity: ancien fond de carte non supprimé");
        }
    }
    Ok(background)
}

/// Contenu du fond de carte.
pub async fn read_map_background(file_id: ObjectId) -> mongodb::error::Result<Vec<u8>> {
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let mut stream = bucket(&db).open_download_stream(Bson::ObjectId(file_id)).await?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.map_err(|e| mongodb::error::Error::custom(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// PNG 3×2 minimal (en-tête + IHDR suffisent à l'inspection).
    pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    #[test]
    fn png_dimensions_are_read() {
        assert_eq!(inspect_image(&png(3000, 2000)), Ok(("image/png", 3000, 2000)));
    }

    #[test]
    fn jpeg_dimensions_are_read_after_other_segments() {
        // SOI, APP0 (16 octets), SOF0 : hauteur 600, largeur 800.
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        b.extend_from_slice(&[0; 14]);
        b.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x02, 0x58, 0x03, 0x20, 0x03]);
        b.extend_from_slice(&[0; 9]);
        assert_eq!(inspect_image(&b), Ok(("image/jpeg", 800, 600)));
    }

    #[test]
    fn webp_vp8x_dimensions_are_read() {
        let mut b = b"RIFF\x00\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x00\x00\x00\x00".to_vec();
        b.extend_from_slice(&[0xB7, 0x0B, 0x00]); // largeur − 1 = 2999
        b.extend_from_slice(&[0xCF, 0x07, 0x00]); // hauteur − 1 = 1999
        assert_eq!(inspect_image(&b), Ok(("image/webp", 3000, 2000)));
    }

    #[test]
    fn svg_and_other_formats_are_refused() {
        assert_eq!(inspect_image(b"<svg xmlns='http://www.w3.org/2000/svg'><script>alert(1)</script></svg>"), Err(BackgroundError::UnsupportedFormat));
        assert_eq!(inspect_image(b"GIF89a"), Err(BackgroundError::UnsupportedFormat));
        assert_eq!(inspect_image(b""), Err(BackgroundError::UnsupportedFormat));
    }

    #[test]
    fn truncated_or_absurd_images_are_refused() {
        assert_eq!(inspect_image(&png(3000, 2000)[..20]), Err(BackgroundError::InvalidImage));
        assert_eq!(inspect_image(&png(0, 10)), Err(BackgroundError::InvalidImage));
        assert_eq!(inspect_image(&png(40_000, 10)), Err(BackgroundError::TooManyPixels));
        assert_eq!(inspect_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]), Err(BackgroundError::InvalidImage));
        let big = vec![0u8; MAX_BACKGROUND_BYTES + 1];
        assert_eq!(inspect_image(&big), Err(BackgroundError::TooLarge));
    }
}
