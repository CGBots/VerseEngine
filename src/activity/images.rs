//! Image d'un personnage (D5) : la fiche stocke une référence `{salon, message, pièce jointe}`,
//! jamais une URL, car les URL du CDN Discord sont signées et expirent. L'URL est relue auprès de
//! Discord par le bot, puis gardée en cache 30 min.
//!
//! Les URL `https://cdn.discordapp.com/attachments/…` sont une exception documentée à la CSP des
//! activités : le client peut les charger sans URL mapping.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serenity::all::{ChannelId, Http, MessageId};

use crate::database::characters::CharacterImage;

const CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_ENTRIES: usize = 10_000;

/// Seules les pièces jointes du CDN Discord sont acceptées (exception de la CSP des activités).
pub fn is_allowed_image_url(url: &str) -> bool {
    url.starts_with("https://cdn.discordapp.com/attachments/") || url.starts_with("https://media.discordapp.net/attachments/")
}

/// URL de la pièce jointe `attachment_id` parmi celles d'un message (`(id, url)`).
pub fn pick_attachment(attachments: &[(u64, String)], attachment_id: u64) -> Option<String> {
    attachments.iter().find(|(id, url)| *id == attachment_id && is_allowed_image_url(url)).map(|(_, url)| url.clone())
}

#[derive(Default)]
pub struct ImageCache {
    entries: Mutex<HashMap<u64, (Option<String>, Instant)>>,
}

impl ImageCache {
    fn get(&self, attachment_id: u64, now: Instant) -> Option<Option<String>> {
        let entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        entries.get(&attachment_id).filter(|(_, at)| now.duration_since(*at) < CACHE_TTL).map(|(url, _)| url.clone())
    }

    fn put(&self, attachment_id: u64, url: Option<String>, now: Instant) {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        if entries.len() >= MAX_ENTRIES {
            entries.retain(|_, (_, at)| now.duration_since(*at) < CACHE_TTL);
            if entries.len() >= MAX_ENTRIES {
                entries.clear();
            }
        }
        entries.insert(attachment_id, (url, now));
    }

    /// URL actuelle de l'image, ou `None` si le message ou la pièce jointe n'existe plus
    /// (suppression du message = retrait de l'image, D5).
    pub async fn resolve(&self, http: &Http, image: &CharacterImage) -> Option<String> {
        let now = Instant::now();
        if let Some(cached) = self.get(image.attachment_id, now) {
            return cached;
        }
        let url = match ChannelId::new(image.channel_id).message(http, MessageId::new(image.message_id)).await {
            Ok(message) => {
                let attachments: Vec<(u64, String)> = message.attachments.iter().map(|a| (a.id.get(), a.url.clone())).collect();
                pick_attachment(&attachments, image.attachment_id)
            }
            Err(e) => {
                tracing::debug!(error = %e, "activity: image du personnage illisible");
                None
            }
        };
        self.put(image.attachment_id, url.clone(), now);
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_discord_attachment_urls_are_used() {
        assert!(is_allowed_image_url("https://cdn.discordapp.com/attachments/1/2/a.png?ex=1"));
        assert!(is_allowed_image_url("https://media.discordapp.net/attachments/1/2/a.png"));
        assert!(!is_allowed_image_url("https://evil.example/attachments/a.png"));
        assert!(!is_allowed_image_url("http://cdn.discordapp.com/attachments/1/2/a.png"));
        assert!(!is_allowed_image_url("https://cdn.discordapp.com.evil.example/attachments/a.png"));
    }

    #[test]
    fn the_referenced_attachment_is_picked() {
        let list = vec![(1, "https://cdn.discordapp.com/attachments/9/1/a.png".to_string()), (2, "https://cdn.discordapp.com/attachments/9/2/b.png".to_string())];
        assert_eq!(pick_attachment(&list, 2).as_deref(), Some("https://cdn.discordapp.com/attachments/9/2/b.png"));
        assert_eq!(pick_attachment(&list, 3), None, "attachment removed from the message");
    }

    #[test]
    fn cached_urls_expire() {
        let cache = ImageCache::default();
        let t0 = Instant::now();
        cache.put(5, Some("u".into()), t0);
        assert_eq!(cache.get(5, t0 + Duration::from_secs(60)), Some(Some("u".into())));
        assert_eq!(cache.get(5, t0 + CACHE_TTL + Duration::from_secs(1)), None);
        cache.put(6, None, t0);
        assert_eq!(cache.get(6, t0), Some(None), "a missing image is cached too");
    }

    /// Contre Discord : une image postée par le bot se résout en URL du CDN ; supprimer le message
    /// retire l'image.
    #[test]
    #[ignore = "integration test: needs DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
    fn a_character_image_is_resolved_from_discord() {
        use crate::test_support::{block_on, discord_http, TempChannel};
        use serenity::all::{CreateAttachment, CreateMessage};
        block_on(async {
            let http = discord_http();
            let channel = TempChannel::create(&http, "verseengine-it-image").await;
            // PNG 1×1.
            let png: &[u8] = &[
                0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f,
                0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0d, 0x0a, 0x2d, 0xb4, 0, 0, 0, 0,
                0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
            ];
            let message = channel
                .id
                .send_message(&http, CreateMessage::new().add_file(CreateAttachment::bytes(png.to_vec(), "portrait.png")))
                .await
                .expect("post the image");
            let image = CharacterImage { channel_id: channel.id.get(), message_id: message.id.get(), attachment_id: message.attachments[0].id.get() };

            let url = ImageCache::default().resolve(&http, &image).await;
            message.delete(&http).await.unwrap();
            let after_delete = ImageCache::default().resolve(&http, &image).await;
            channel.delete(&http).await;

            let url = url.expect("resolved");
            assert!(url.starts_with("https://cdn.discordapp.com/attachments/"), "{url}");
            assert_eq!(after_delete, None);
        });
    }
}
