//! # Fiche d'un lieu dans le forum wiki
//!
//! « Publier » (mode admin de l'activité) crée, ou met à jour s'il existe, le post du lieu dans
//! le forum wiki (`Server.rp_wiki_channel_id`) de **chaque serveur de l'univers**. Les fils créés
//! sont mémorisés par serveur (`Place.wiki.posts`) : republier modifie le même post au lieu d'en
//! créer un autre. Un post supprimé à la main est recréé ; un forum absent est signalé.
//!
//! Le contenu ([`build_post`]) et l'enchaînement ([`publish`]) sont sans Discord : l'accès au
//! forum passe par [`WikiForum`] (le bot en production, un faux en test).

use futures::future::BoxFuture;
use serde::Serialize;
use serenity::all::{ChannelId, ChannelType, Colour, CreateEmbed, CreateForumPost, CreateMessage, EditMessage, EditThread, GuildId, Http, MessageId};

use crate::database::places::WikiPost;
use crate::database::universe_maps::{BurgDetails, BurgStyle};
use crate::discord::channels::PLACE_TAG;

/// Texte de la fiche : 4000 caractères au plus (la description d'un embed en accepte 4096 ; la
/// marge laisse la place aux informations du bourg dans la limite de 6000 de l'embed).
pub const MAX_DESCRIPTION_CHARS: usize = 4000;
/// Limites Discord.
pub const MAX_THREAD_TITLE: usize = 100;
pub const MAX_EMBED_TITLE: usize = 256;
pub const MAX_EMBED_DESCRIPTION: usize = 4096;
pub const MAX_FIELD_NAME: usize = 256;
pub const MAX_FIELD_VALUE: usize = 1024;
pub const MAX_EMBED_TOTAL: usize = 6000;

/// Contenu d'un post, indépendant de serenity (testable, mesurable).
#[derive(Debug, Clone, PartialEq)]
pub struct PostContent {
    /// Titre du fil (nom du lieu).
    pub thread_title: String,
    pub title: String,
    pub description: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostError {
    /// Rien à publier.
    Empty,
    /// Hors des limites de taille de Discord.
    TooLong,
}

/// Nombre de caractères tel que Discord le compte (approché par les points de code).
fn len(s: &str) -> usize { s.chars().count() }

fn truncate(s: &str, max: usize) -> String {
    if len(s) <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Nombre avec séparateur de milliers (espace fine insécable, lisible dans les deux langues).
pub fn format_population(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

/// Particularités d'un bourg (taille, capitale, port, citadelle…), en clés de traduction.
pub fn burg_traits(burg: &BurgStyle) -> Vec<&'static str> {
    let mut out = vec![match burg.size {
        0 => "place_wiki__size_village",
        1 => "place_wiki__size_town",
        _ => "place_wiki__size_city",
    }];
    if burg.capital {
        out.push("place_wiki__capital");
    }
    if burg.port {
        out.push("place_wiki__port");
    }
    for f in burg.details.iter().flat_map(|d| d.features.iter()) {
        out.push(match f.as_str() {
            "citadel" => "place_wiki__feature_citadel",
            "walls" => "place_wiki__feature_walls",
            "plaza" => "place_wiki__feature_plaza",
            "temple" => "place_wiki__feature_temple",
            "shanty" => "place_wiki__feature_shanty",
            _ => continue,
        });
    }
    out
}

/// Construit le post : le texte de la fiche, puis les informations principales du bourg Azgaar
/// (population, État, culture, particularités). `label` traduit une clé dans la langue du serveur.
pub fn build_post(place_name: &str, text: &str, burg: Option<&BurgStyle>, label: &dyn Fn(&str) -> String) -> Result<PostContent, PostError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(PostError::Empty);
    }
    if len(text) > MAX_DESCRIPTION_CHARS {
        return Err(PostError::TooLong);
    }
    let mut fields = Vec::new();
    if let Some(b) = burg {
        let d = b.details.clone().unwrap_or_else(BurgDetails::default);
        if let Some(p) = d.population {
            fields.push((label("place_wiki__population"), format_population(p)));
        }
        if let Some(s) = d.state {
            fields.push((label("place_wiki__state"), s));
        }
        if let Some(c) = d.culture {
            fields.push((label("place_wiki__culture"), c));
        }
        let traits: Vec<String> = burg_traits(b).into_iter().map(label).collect();
        fields.push((label("place_wiki__traits"), traits.join(", ")));
    }
    let fields: Vec<(String, String)> = fields.into_iter().map(|(n, v)| (truncate(&n, MAX_FIELD_NAME), truncate(&v, MAX_FIELD_VALUE))).collect();
    let post = PostContent {
        thread_title: truncate(place_name.trim(), MAX_THREAD_TITLE),
        title: truncate(place_name.trim(), MAX_EMBED_TITLE),
        description: text.to_string(),
        fields,
    };
    let total = len(&post.title) + len(&post.description) + post.fields.iter().map(|(n, v)| len(n) + len(v)).sum::<usize>();
    if len(&post.description) > MAX_EMBED_DESCRIPTION || total > MAX_EMBED_TOTAL || post.thread_title.is_empty() {
        return Err(PostError::TooLong);
    }
    Ok(post)
}

fn embed(post: &PostContent) -> CreateEmbed {
    let mut e = CreateEmbed::new().title(post.title.clone()).description(post.description.clone()).colour(Colour::from_rgb(25, 125, 255));
    for (n, v) in &post.fields {
        e = e.field(n.clone(), v.clone(), true);
    }
    e
}

// ---------------------------------------------------------------------------
// Accès au forum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct GuildInfo {
    pub name: Option<String>,
    /// Langue du serveur (`preferred_locale`) : un post public est lu par tous (D12).
    pub locale: String,
}

/// Échec d'un appel Discord (le détail reste dans les logs).
#[derive(Debug, Clone, PartialEq)]
pub struct ForumError(pub String);

pub trait WikiForum: Send + Sync {
    fn guild<'a>(&'a self, guild_id: u64) -> BoxFuture<'a, GuildInfo>;
    /// `None` : le salon n'existe plus ou n'est pas un forum. Sinon, l'étiquette `place` s'il y en a une.
    fn forum<'a>(&'a self, forum_id: u64) -> BoxFuture<'a, Result<Option<Option<u64>>, ForumError>>;
    /// Crée le post ; retourne l'identifiant du fil.
    fn create_post<'a>(&'a self, forum_id: u64, tag: Option<u64>, post: &'a PostContent) -> BoxFuture<'a, Result<u64, ForumError>>;
    /// Met à jour le post ; `false` s'il a été supprimé.
    fn update_post<'a>(&'a self, thread_id: u64, post: &'a PostContent) -> BoxFuture<'a, Result<bool, ForumError>>;
}

/// Le forum via le client HTTP du bot.
pub struct LiveForum(pub std::sync::Arc<Http>);

fn not_found(e: &serenity::Error) -> bool {
    matches!(e, serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(r)) if r.status_code.as_u16() == 404)
}

impl WikiForum for LiveForum {
    fn guild<'a>(&'a self, guild_id: u64) -> BoxFuture<'a, GuildInfo> {
        Box::pin(async move {
            match self.0.get_guild(GuildId::new(guild_id)).await {
                Ok(g) => GuildInfo { name: Some(g.name), locale: g.preferred_locale },
                Err(_) => GuildInfo { name: None, locale: "fr".into() },
            }
        })
    }

    fn forum<'a>(&'a self, forum_id: u64) -> BoxFuture<'a, Result<Option<Option<u64>>, ForumError>> {
        Box::pin(async move {
            match self.0.get_channel(ChannelId::new(forum_id)).await {
                Ok(c) => Ok(c.guild().filter(|g| g.kind == ChannelType::Forum).map(|g| g.available_tags.iter().find(|t| t.name == PLACE_TAG).map(|t| t.id.get()))),
                Err(e) if not_found(&e) => Ok(None),
                Err(e) => Err(ForumError(e.to_string())),
            }
        })
    }

    fn create_post<'a>(&'a self, forum_id: u64, tag: Option<u64>, post: &'a PostContent) -> BoxFuture<'a, Result<u64, ForumError>> {
        Box::pin(async move {
            let mut builder = CreateForumPost::new(post.thread_title.clone(), CreateMessage::new().embed(embed(post)));
            if let Some(tag) = tag {
                builder = builder.add_applied_tag(tag.into());
            }
            ChannelId::new(forum_id).create_forum_post(self.0.as_ref(), builder).await.map(|t| t.id.get()).map_err(|e| ForumError(e.to_string()))
        })
    }

    fn update_post<'a>(&'a self, thread_id: u64, post: &'a PostContent) -> BoxFuture<'a, Result<bool, ForumError>> {
        Box::pin(async move {
            let thread = ChannelId::new(thread_id);
            // Un post inactif est archivé : il faut le rouvrir pour en modifier le message.
            match thread.edit_thread(self.0.as_ref(), EditThread::new().archived(false).name(post.thread_title.clone())).await {
                Ok(_) => {}
                Err(e) if not_found(&e) => return Ok(false),
                Err(e) => return Err(ForumError(e.to_string())),
            }
            // Le premier message d'un post de forum a l'identifiant du fil.
            match thread.edit_message(self.0.as_ref(), MessageId::new(thread_id), EditMessage::new().embed(embed(post))).await {
                Ok(_) => Ok(true),
                Err(e) if not_found(&e) => Ok(false),
                Err(e) => Err(ForumError(e.to_string())),
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Publication
// ---------------------------------------------------------------------------

/// Un serveur de l'univers et son forum wiki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerTarget {
    pub server_id: u64,
    pub forum_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishStatus {
    Created,
    Updated,
    /// Le post avait été supprimé : un nouveau l'a remplacé.
    Recreated,
    /// Pas de forum wiki sur ce serveur (jamais installé, ou supprimé).
    NoForum,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServerReport {
    #[serde(serialize_with = "crate::place::wiki::as_string")]
    pub server_id: u64,
    pub server_name: Option<String>,
    pub status: PublishStatus,
    #[serde(serialize_with = "opt_string")]
    pub thread_id: Option<u64>,
}

pub fn as_string<S: serde::Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(&v.to_string()) }
fn opt_string<S: serde::Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => s.serialize_str(&v.to_string()),
        None => s.serialize_none(),
    }
}

/// Publie sur chaque serveur. `build(locale)` construit le post dans la langue du serveur.
/// Retourne les posts à mémoriser (un par serveur où il existe) et le compte rendu par serveur.
/// Un échec passager garde l'ancien post ; les serveurs sortis de l'univers sont oubliés.
pub async fn publish(
    forum: &dyn WikiForum,
    servers: &[ServerTarget],
    existing: &[WikiPost],
    build: &(dyn Fn(&str) -> Result<PostContent, PostError> + Sync),
) -> (Vec<WikiPost>, Vec<ServerReport>) {
    let mut posts = Vec::new();
    let mut reports = Vec::new();
    for target in servers {
        let info = forum.guild(target.server_id).await;
        let old = existing.iter().find(|p| p.server_id == target.server_id).map(|p| p.thread_id);
        let report = |status, thread_id| ServerReport { server_id: target.server_id, server_name: info.name.clone(), status, thread_id };
        let keep_old = |posts: &mut Vec<WikiPost>| {
            if let Some(t) = old {
                posts.push(WikiPost { server_id: target.server_id, thread_id: t });
            }
        };
        let Ok(post) = build(&info.locale) else {
            keep_old(&mut posts);
            reports.push(report(PublishStatus::Failed, old));
            continue;
        };
        let Some(forum_id) = target.forum_id else {
            reports.push(report(PublishStatus::NoForum, None));
            continue;
        };
        let tag = match forum.forum(forum_id).await {
            Ok(Some(tag)) => tag,
            Ok(None) => {
                reports.push(report(PublishStatus::NoForum, None));
                continue;
            }
            Err(e) => {
                tracing::warn!(server = target.server_id, error = %e.0, "wiki: forum illisible");
                keep_old(&mut posts);
                reports.push(report(PublishStatus::Failed, old));
                continue;
            }
        };
        let mut status = PublishStatus::Created;
        if let Some(thread) = old {
            match forum.update_post(thread, &post).await {
                Ok(true) => {
                    posts.push(WikiPost { server_id: target.server_id, thread_id: thread });
                    reports.push(report(PublishStatus::Updated, Some(thread)));
                    continue;
                }
                Ok(false) => status = PublishStatus::Recreated,
                Err(e) => {
                    tracing::warn!(server = target.server_id, error = %e.0, "wiki: mise à jour du post impossible");
                    keep_old(&mut posts);
                    reports.push(report(PublishStatus::Failed, old));
                    continue;
                }
            }
        }
        match forum.create_post(forum_id, tag, &post).await {
            Ok(thread) => {
                posts.push(WikiPost { server_id: target.server_id, thread_id: thread });
                reports.push(report(status, Some(thread)));
            }
            Err(e) => {
                tracing::warn!(server = target.server_id, error = %e.0, "wiki: création du post impossible");
                reports.push(report(PublishStatus::Failed, None));
            }
        }
    }
    (posts, reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    fn label(k: &str) -> String { k.to_string() }

    fn burg() -> BurgStyle {
        BurgStyle {
            burg_id: 1,
            size: 2,
            capital: true,
            port: false,
            details: Some(BurgDetails { population: Some(23_735), state: Some("Hublosia".into()), culture: Some("Luari".into()), features: vec!["walls".into()], ..Default::default() }),
        }
    }

    #[test]
    fn the_post_has_the_text_and_the_main_burg_information() {
        let p = build_post("  Arane ", " Une cité fluviale. ", Some(&burg()), &label).unwrap();
        assert_eq!((p.thread_title.as_str(), p.description.as_str()), ("Arane", "Une cité fluviale."));
        assert_eq!(p.fields[0], ("place_wiki__population".into(), "23\u{202f}735".into()));
        assert_eq!(p.fields[1].1, "Hublosia");
        assert_eq!(p.fields[3].1, "place_wiki__size_city, place_wiki__capital, place_wiki__feature_walls");
        assert!(build_post("Arane", "texte", None, &label).unwrap().fields.is_empty(), "no Azgaar burg: text only");
    }

    #[test]
    fn discord_size_limits_are_enforced() {
        assert_eq!(build_post("A", "   ", None, &label), Err(PostError::Empty));
        assert_eq!(build_post("A", &"é".repeat(MAX_DESCRIPTION_CHARS + 1), None, &label), Err(PostError::TooLong));
        assert!(build_post("A", &"é".repeat(MAX_DESCRIPTION_CHARS), Some(&burg()), &label).is_ok(), "counted in characters, not bytes");
        let long_name = "N".repeat(300);
        let p = build_post(&long_name, "t", None, &label).unwrap();
        assert_eq!((len(&p.thread_title), len(&p.title)), (MAX_THREAD_TITLE, MAX_EMBED_TITLE));
        // Champs très longs (État au nom démesuré) : tronqués, et le total reste sous 6000.
        let mut b = burg();
        b.details.as_mut().unwrap().state = Some("S".repeat(5000));
        let p = build_post("A", &"t".repeat(MAX_DESCRIPTION_CHARS), Some(&b), &label);
        assert!(p.is_ok_and(|p| p.fields.iter().all(|(_, v)| len(v) <= MAX_FIELD_VALUE)));
    }

    #[test]
    fn populations_are_grouped_by_thousands() {
        assert_eq!(format_population(7), "7");
        assert_eq!(format_population(1234567), "1\u{202f}234\u{202f}567");
    }

    /// Faux forum : forums existants (avec ou sans étiquette), posts existants.
    #[derive(Default)]
    struct Fake {
        forums: HashMap<u64, Option<u64>>,
        broken_forums: HashSet<u64>,
        threads: Mutex<HashMap<u64, PostContent>>,
        next: Mutex<u64>,
        created_in: Mutex<Vec<(u64, Option<u64>)>>,
    }

    impl WikiForum for Fake {
        fn guild<'a>(&'a self, g: u64) -> BoxFuture<'a, GuildInfo> {
            Box::pin(async move { GuildInfo { name: Some(format!("S{g}")), locale: if g == 2 { "en-US".into() } else { "fr".into() } } })
        }
        fn forum<'a>(&'a self, f: u64) -> BoxFuture<'a, Result<Option<Option<u64>>, ForumError>> {
            Box::pin(async move {
                if self.broken_forums.contains(&f) {
                    return Err(ForumError("500".into()));
                }
                Ok(self.forums.get(&f).copied())
            })
        }
        fn create_post<'a>(&'a self, f: u64, tag: Option<u64>, post: &'a PostContent) -> BoxFuture<'a, Result<u64, ForumError>> {
            Box::pin(async move {
                let mut n = self.next.lock().unwrap();
                *n += 1;
                let id = 1000 + *n;
                self.threads.lock().unwrap().insert(id, post.clone());
                self.created_in.lock().unwrap().push((f, tag));
                Ok(id)
            })
        }
        fn update_post<'a>(&'a self, t: u64, post: &'a PostContent) -> BoxFuture<'a, Result<bool, ForumError>> {
            Box::pin(async move {
                let mut threads = self.threads.lock().unwrap();
                Ok(threads.get_mut(&t).map(|p| *p = post.clone()).is_some())
            })
        }
    }

    fn build(locale: &str) -> Result<PostContent, PostError> { build_post("Arane", &format!("texte {locale}"), None, &label) }

    #[test]
    fn publishing_creates_then_updates_the_same_posts() {
        crate::test_support::block_on(async {
            let fake = Fake { forums: HashMap::from([(10, Some(7)), (20, None)]), ..Default::default() };
            let servers = [ServerTarget { server_id: 1, forum_id: Some(10) }, ServerTarget { server_id: 2, forum_id: Some(20) }];
            let (posts, reports) = publish(&fake, &servers, &[], &build).await;
            assert_eq!(reports.iter().map(|r| r.status).collect::<Vec<_>>(), vec![PublishStatus::Created; 2]);
            assert_eq!(*fake.created_in.lock().unwrap(), vec![(10, Some(7)), (20, None)], "the place tag when the forum has one");
            assert_eq!(fake.threads.lock().unwrap()[&posts[1].thread_id].description, "texte en-US", "in the language of each server");
            assert_eq!(reports[0].server_name.as_deref(), Some("S1"));

            let (again, reports) = publish(&fake, &servers, &posts, &build).await;
            assert_eq!(again, posts, "no duplicate");
            assert!(reports.iter().all(|r| r.status == PublishStatus::Updated));
            assert_eq!(fake.threads.lock().unwrap().len(), 2);
        });
    }

    #[test]
    fn a_post_deleted_by_hand_is_recreated_and_a_missing_forum_reported() {
        crate::test_support::block_on(async {
            let fake = Fake { forums: HashMap::from([(10, None)]), ..Default::default() };
            let servers = [ServerTarget { server_id: 1, forum_id: Some(10) }, ServerTarget { server_id: 2, forum_id: Some(99) }, ServerTarget { server_id: 3, forum_id: None }];
            let stale = [WikiPost { server_id: 1, thread_id: 555 }, WikiPost { server_id: 2, thread_id: 556 }, WikiPost { server_id: 9, thread_id: 557 }];
            let (posts, reports) = publish(&fake, &servers, &stale, &build).await;
            assert_eq!(reports.iter().map(|r| r.status).collect::<Vec<_>>(), vec![PublishStatus::Recreated, PublishStatus::NoForum, PublishStatus::NoForum]);
            assert_eq!(posts.len(), 1, "posts of missing forums and of servers out of the universe are forgotten");
            assert_ne!(posts[0].thread_id, 555);
        });
    }

    #[test]
    fn a_discord_failure_keeps_the_previous_post() {
        crate::test_support::block_on(async {
            let fake = Fake { broken_forums: HashSet::from([10]), ..Default::default() };
            let servers = [ServerTarget { server_id: 1, forum_id: Some(10) }];
            let old = [WikiPost { server_id: 1, thread_id: 555 }];
            let (posts, reports) = publish(&fake, &servers, &old, &build).await;
            assert_eq!(reports[0].status, PublishStatus::Failed);
            assert_eq!(posts, old.to_vec(), "retried next time instead of creating a duplicate");
        });
    }
}
