//! # Service des objets partagé
//!
//! Création, modification et suppression d'un objet, sans `poise::Context` : `/item create`
//! (administrateur), `/item delete` et l'éditeur d'objets de l'activité (mode admin) appellent
//! ces mêmes fonctions.
//!
//! Les effets s'écrivent dans la syntaxe du modal de `/item create` (« Stat: valeur durée
//! niveau »), analysée par [`EffectParser`] ; l'éditeur relit ceux d'un objet existant avec
//! [`effects_text`].
//!
//! Wiki : un objet ne retient pas ses posts ; ils sont retrouvés par leur titre (le nom de
//! l'objet) dans le forum wiki de chaque serveur de l'univers, comme le faisait `/item delete`.

use std::collections::HashMap;

use fluent::FluentArgs;
use futures::{StreamExt, TryStreamExt};
use mongodb::bson::{doc, oid::ObjectId};
use poise::ChoiceParameter;
use serenity::all::{CacheHttp, ChannelId, Colour, CreateEmbed, CreateForumPost, CreateMessage, EditMessage, GuildChannel, GuildId, MessageId};

use crate::database::characters::Character;
use crate::database::inventory::{HolderType, Inventory};
use crate::database::items::{get_item_by_id, get_item_by_name, Item};
use crate::database::loot_tables::LootTable;
use crate::database::modifiers::{Modifier, ModifierLevel, ModifierType};
use crate::database::recipe::{Recipe, RECIPE_COLLECTION_NAME};
use crate::database::server::Server;
use crate::database::stats::{Stat, StatValue};
use crate::database::universe::{get_servers_from_universe, get_universe_by_id};
use crate::discord::channels::ITEM_TAG;
use crate::discord::poise_structs::Error;
use crate::item::ItemUsage;
use crate::translation::get_by_locale;
use crate::utility::effect_parser::EffectParser;
use crate::utility::loot_table_parser::VALID_NAME_RE;

/// Longueur maximale d'un nom d'objet (titre d'un post de forum : 100 caractères).
pub const MAX_ITEM_NAME: usize = 100;
/// Description : limite d'une description d'embed.
pub const MAX_ITEM_DESCRIPTION: usize = 4000;
/// Informations secrètes et effets : limite d'un champ d'embed.
pub const MAX_ITEM_FIELD: usize = 1024;
pub const MAX_IMAGE_URL: usize = 1000;
pub const MAX_INVENTORY_SIZE: u64 = 10_000;

/// Tout ce qui décrit un objet, sauf son nom.
#[derive(Debug, Clone)]
pub struct ItemFields {
    pub usage: ItemUsage,
    pub description: Option<String>,
    pub secret: Option<String>,
    pub image: Option<String>,
    pub inventory_size: u64,
    /// Effets, syntaxe du modal de `/item create` ; vide : aucun effet.
    pub effects_raw: String,
}

/// Que faire des posts wiki lors d'une modification (l'objet ne retient pas s'il est au wiki).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WikiAction {
    Keep,
    /// Met à jour les posts existants, en crée sur les serveurs qui n'en ont pas.
    Publish,
    Remove,
}

/// Nom d'objet acceptable : même règle que `/item create` (lettres, chiffres, `_`, `-`,
/// espaces), 100 caractères au plus.
pub fn valid_item_name(name: &str) -> bool {
    VALID_NAME_RE.is_match(name) && name.chars().count() <= MAX_ITEM_NAME
}

/// Champs bornés (longueurs des embeds Discord) ; l'image doit être une URL `https`.
pub fn validate_fields(f: &ItemFields) -> Result<(), &'static str> {
    let len = |s: &Option<String>| s.as_deref().map_or(0, |s| s.chars().count());
    if len(&f.description) > MAX_ITEM_DESCRIPTION || len(&f.secret) > MAX_ITEM_FIELD || f.effects_raw.chars().count() > MAX_ITEM_FIELD {
        return Err("activity__invalid_request");
    }
    if f.inventory_size > MAX_INVENTORY_SIZE {
        return Err("activity__invalid_request");
    }
    if let Some(url) = &f.image {
        if url.len() > MAX_IMAGE_URL || !url.starts_with("https://") || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err("activity__invalid_image_url");
        }
    }
    Ok(())
}

fn level_word(level: &ModifierLevel) -> &'static str {
    match level {
        ModifierLevel::Player => "player",
        ModifierLevel::Place => "place",
        ModifierLevel::Area => "area",
        ModifierLevel::Universe => "universe",
    }
}

fn number(v: &StatValue) -> String {
    match v {
        StatValue::I64(i) => i.to_string(),
        StatValue::F64(f) => f.to_string(),
        StatValue::String(s) => s.clone(),
        StatValue::Bool(b) => b.to_string(),
    }
}

/// Effets d'un objet réécrits dans la syntaxe de [`EffectParser`] : relus tels quels, ils
/// redonnent les mêmes modificateurs. La durée (`end_timestamp`, en secondes pour un objet) est
/// écrite en secondes.
pub fn effects_text(effects: &[Modifier], stats: &[Stat]) -> String {
    let names: HashMap<ObjectId, &str> = stats.iter().map(|s| (s._id, s.name.as_str())).collect();
    effects
        .iter()
        .map(|m| {
            let value = number(&m.value);
            let value = match m.modifier_type {
                ModifierType::Multiplier => format!("x{value}"),
                ModifierType::Flats => format!("{value} flat"),
                // Un nombre négatif se lit seul ; « +-2 » serait lu aussi, mais moins lisible.
                ModifierType::Base if value.starts_with('-') => value,
                ModifierType::Base => format!("+{value}"),
            };
            let duration = m.end_timestamp.map(|d| format!(" {d}s")).unwrap_or_default();
            let stat = names.get(&m.stat_id).copied().unwrap_or("?");
            format!("{stat}: {value}{duration} {}", level_word(&m.level))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Embed d'un objet (réponse de `/item create`, post wiki).
pub fn item_embed(name: &str, f: &ItemFields, locale: &str) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(name)
        .description(f.description.clone().unwrap_or_default())
        .field(get_by_locale(locale, "item_usage_title", None, None), get_by_locale(locale, f.usage.name(), None, None), true)
        .field(get_by_locale(locale, "item_inventory_size", None, None), f.inventory_size.to_string(), true)
        .colour(Colour::from_rgb(25, 125, 255))
        .thumbnail(f.image.clone().unwrap_or_default());
    if !f.effects_raw.is_empty() {
        embed = embed.field(get_by_locale(locale, "item_effects_title", None, None), f.effects_raw.clone(), false);
    }
    embed
}

async fn parse_effects(universe_id: ObjectId, raw: &str) -> Result<Vec<Modifier>, Error> {
    if raw.trim().is_empty() {
        return Ok(vec![]);
    }
    EffectParser::parse(raw, universe_id, universe_id).await.map_err(Error::from)
}

async fn universe_servers(universe_id: ObjectId) -> Result<Vec<Server>, Error> {
    Ok(get_servers_from_universe(&universe_id).await.map_err(|_| "item_db_error")?.try_collect().await.map_err(|_| "item_db_error")?)
}

/// Forum wiki d'un serveur, s'il en a un.
async fn wiki_forum(ctx: &impl CacheHttp, server: &Server) -> Option<GuildChannel> {
    let id = server.rp_wiki_channel_id.as_ref()?.id;
    ctx.http().get_channel(id.into()).await.ok()?.guild()
}

/// Ajoute au forum l'étiquette « Objet » si elle manque, et la renvoie.
async fn item_tag(ctx: &impl CacheHttp, channel: &mut GuildChannel) -> Option<serenity::all::ForumTagId> {
    let find = |c: &GuildChannel| c.available_tags.iter().find(|tag| tag.name.to_lowercase() == ITEM_TAG.to_lowercase() || tag.name == "Objet").map(|t| t.id);
    if let Some(t) = find(channel) {
        return Some(t);
    }
    let mut tags: Vec<serenity::all::CreateForumTag> =
        channel.available_tags.iter().map(|tag| serenity::all::CreateForumTag::new(tag.name.clone()).moderated(tag.moderated)).collect();
    tags.push(serenity::all::CreateForumTag::new("Objet"));
    if let Ok(updated) = channel.id.edit(ctx, serenity::all::EditChannel::new().available_tags(tags)).await {
        *channel = updated;
    }
    find(channel)
}

async fn create_post(ctx: &impl CacheHttp, channel: &mut GuildChannel, name: &str, embed: &CreateEmbed) -> Result<(), Error> {
    let mut post = CreateForumPost::new(name, CreateMessage::new().embed(embed.clone()));
    if let Some(tag_id) = item_tag(ctx, channel).await {
        post = post.add_applied_tag(tag_id);
    }
    channel.create_forum_post(ctx, post).await?;
    Ok(())
}

/// Posts du forum wiki `forum` (serveur `guild_id`) dont le titre est `name`, actifs ou archivés.
/// Seuls les fils de ce forum sont retenus : un fil homonyme d'un autre salon n'est pas un post.
async fn item_posts(ctx: &impl CacheHttp, guild_id: u64, forum: &GuildChannel, name: &str) -> Vec<GuildChannel> {
    let mut found = Vec::new();
    if let Ok(threads) = GuildId::new(guild_id).get_active_threads(ctx.http()).await {
        found.extend(threads.threads.into_iter().filter(|t| t.parent_id == Some(forum.id) && t.name() == name));
    }
    if let Ok(threads) = forum.id.get_archived_public_threads(ctx.http(), None, None).await {
        found.extend(threads.threads.into_iter().filter(|t| t.name() == name));
    }
    found
}

/// Crée un objet (administrateur). Renvoie l'objet et son embed (réponse de la commande slash).
pub async fn create_item(ctx: &impl CacheHttp, universe_id: ObjectId, name: &str, fields: ItemFields, into_wiki: bool, locale: &str) -> Result<(Item, CreateEmbed), Error> {
    if !valid_item_name(name) {
        return Err("create_item__invalid_name".into());
    }
    if get_item_by_name(universe_id, name).await?.is_some() {
        return Err("create_item__already_exists".into());
    }
    let effects = parse_effects(universe_id, &fields.effects_raw).await?;
    let item = Item {
        _id: ObjectId::new(),
        universe_id,
        item_name: name.to_string(),
        item_usage: fields.usage.clone(),
        effects,
        description: fields.description.clone(),
        image: fields.image.clone(),
        wiki_post_id: None,
        secret_informations: fields.secret.clone(),
        inventory_id: None,
        inventory_size: fields.inventory_size,
    };
    item.clone().save().await.map_err(|_| "create_item__db_error")?;

    let embed = item_embed(name, &fields, locale);
    if into_wiki {
        for server in universe_servers(universe_id).await? {
            let Some(mut channel) = wiki_forum(ctx, &server).await else { continue };
            create_post(ctx, &mut channel, name, &embed).await?;
        }
    }
    Ok((item, embed))
}

/// Modifie un objet existant (éditeur de l'activité). Le nom ne change pas : les tables de loot
/// et les posts wiki désignent l'objet par son nom.
pub async fn update_item(ctx: &impl CacheHttp, universe_id: ObjectId, item_id: ObjectId, fields: ItemFields, wiki: WikiAction, locale: &str) -> Result<Item, Error> {
    let mut item = get_item_by_id(item_id).await?.filter(|i| i.universe_id == universe_id).ok_or("item__not_found")?;
    item.effects = parse_effects(universe_id, &fields.effects_raw).await?;
    item.item_usage = fields.usage.clone();
    item.description = fields.description.clone();
    item.secret_informations = fields.secret.clone();
    item.image = fields.image.clone();
    item.inventory_size = fields.inventory_size;
    item.clone().replace().await?;

    if wiki != WikiAction::Keep {
        let embed = item_embed(&item.item_name, &fields, locale);
        for server in universe_servers(universe_id).await? {
            let Some(mut forum) = wiki_forum(ctx, &server).await else { continue };
            let posts = item_posts(ctx, server.server_id, &forum, &item.item_name).await;
            match wiki {
                WikiAction::Remove => {
                    for post in posts {
                        let _ = post.delete(ctx.http()).await;
                    }
                }
                _ if posts.is_empty() => create_post(ctx, &mut forum, &item.item_name, &embed).await?,
                _ => {
                    // Le premier message d'un post de forum a l'identifiant du fil.
                    for post in posts {
                        let _ = ChannelId::new(post.id.get()).edit_message(ctx, MessageId::new(post.id.get()), EditMessage::new().embed(embed.clone())).await;
                    }
                }
            }
        }
    }
    Ok(item)
}

/// Objet supprimé et recettes qui l'utilisaient (conservées, mais incomplètes).
#[derive(Debug)]
pub struct DeletedItem {
    pub name: String,
    pub affected_recipes: Vec<String>,
}

/// Supprime un objet : base (objet, inventaires, tables de loot) dans une transaction, puis
/// message privé aux personnages qui le possédaient et suppression des posts wiki.
/// `locale` : langue des messages privés (celle de l'administrateur, comme `/item delete`).
pub async fn delete_item(ctx: &impl CacheHttp, universe_id: ObjectId, name: &str, locale: &str) -> Result<DeletedItem, Error> {
    let universe = get_universe_by_id(universe_id).await?.ok_or("item_delete__universe_not_found")?;
    let universe_name = universe.name.clone();

    // 1. Récupérer l'item
    let item = get_item_by_name(universe_id, name).await?.ok_or("item_delete__not_found")?;
    let item_id = item._id;

    // 1.1 Identifier les recettes affectées (mais ne pas les supprimer)
    let db_client = crate::database::db_client::get_db_client().await;
    let recipes_col = db_client.database(crate::database::db_namespace::VERSEENGINE_DB_NAME).collection::<Recipe>(RECIPE_COLLECTION_NAME);
    let filter = doc! {
        "universe_id": universe_id,
        "$or": [
            {"ingredients": {"$elemMatch": {"1": item_id}}},
            {"result": {"$elemMatch": {"1": item_id}}},
            {"tools_needed": item_id}
        ]
    };
    let affected_recipes: Vec<String> = recipes_col.find(filter).await?.map_ok(|r| r.recipe_name).try_collect().await?;

    // 2. Identifier les propriétaires pour les notifier (seulement les personnages)
    let holders = Inventory::get_all_holders_by_item_id(universe_id, item_id).await?;
    let character_holders: Vec<ObjectId> =
        holders.into_iter().filter(|h| matches!(h.holder.holder_type, HolderType::Character)).map(|h| h.holder.holder_id).collect();
    let characters: Vec<Character> = if character_holders.is_empty() {
        vec![]
    } else {
        db_client
            .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
            .collection::<Character>(crate::database::db_namespace::CHARACTERS_COLLECTION_NAME)
            .find(doc! {"_id": {"$in": character_holders}})
            .await?
            .try_collect()
            .await?
    };

    let servers = universe_servers(universe_id).await.unwrap_or_default();

    // 3. Supprimer de l'inventaire et de la base de données (transaction)
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;
    let delete_item_res = Item::delete_with_session(universe_id, name, &mut session).await;
    let remove_inv_res = Inventory::remove_all_by_item_id_with_session(&mut session, universe_id, item_id).await;
    let remove_loot_res = LootTable::remove_item_from_all_tables_with_session(universe_id, name, &mut session).await;

    if delete_item_res.is_err() || remove_inv_res.is_err() || remove_loot_res.is_err() {
        session.abort_transaction().await?;
        // Échec critique : signalé dans le salon de log de chaque serveur de l'univers.
        let error_msg = format!("CRITICAL ERROR during item deletion: item='{}', universe='{}' (ID: {}). Database state may be inconsistent.", name, universe_name, universe_id);
        log::error!("{}", error_msg);
        let message = CreateMessage::new().content(format!("⚠️ **ERREUR CRITIQUE**\n{}", error_msg));
        for server in &servers {
            if let Some(log_channel_id) = &server.log_channel_id {
                let _ = ChannelId::new(log_channel_id.id).send_message(ctx, message.clone()).await;
            }
        }
        return Err("item_delete__critical_error".into());
    }
    session.commit_transaction().await?;

    // 4. Messages privés, avec une concurrence limitée (limites de débit de Discord).
    let http = ctx.http();
    futures::stream::iter(characters)
        .for_each_concurrent(10, |character| {
            let (item_name, universe_name) = (item.item_name.clone(), universe_name.clone());
            async move {
                let mut args = FluentArgs::new();
                args.set("item_name", item_name);
                args.set("character_name", character.name.clone());
                args.set("universe_name", universe_name);
                let embed = CreateEmbed::new()
                    .title(get_by_locale(locale, "item_delete__notification_title", None, None))
                    .description(get_by_locale(locale, "item_delete__notification", None, Some(&args)))
                    .colour(Colour::RED);
                if let Ok(dm_channel) = serenity::all::UserId::new(character.user_id).create_dm_channel(http).await {
                    let _ = dm_channel.send_message(http, CreateMessage::new().embed(embed)).await;
                }
            }
        })
        .await;

    // 5. Supprimer les posts wiki
    for server in &servers {
        let Some(forum) = wiki_forum(ctx, server).await else { continue };
        for post in item_posts(ctx, server.server_id, &forum, name).await {
            let _ = post.delete(ctx.http()).await;
        }
    }

    Ok(DeletedItem { name: item.item_name, affected_recipes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> ItemFields {
        ItemFields { usage: ItemUsage::None, description: None, secret: None, image: None, inventory_size: 0, effects_raw: String::new() }
    }

    #[test]
    fn item_names_follow_the_slash_command_rule() {
        assert!(valid_item_name("Epee de fer"));
        assert!(valid_item_name("potion_2-b"));
        assert!(!valid_item_name("Épée"), "même règle que /item create");
        assert!(!valid_item_name(""));
        assert!(!valid_item_name("a:b"));
        assert!(!valid_item_name(&"a".repeat(MAX_ITEM_NAME + 1)));
    }

    #[test]
    fn fields_are_bounded() {
        assert!(validate_fields(&fields()).is_ok());
        assert!(validate_fields(&ItemFields { description: Some("d".repeat(MAX_ITEM_DESCRIPTION + 1)), ..fields() }).is_err());
        assert!(validate_fields(&ItemFields { secret: Some("s".repeat(MAX_ITEM_FIELD + 1)), ..fields() }).is_err());
        assert!(validate_fields(&ItemFields { effects_raw: "e".repeat(MAX_ITEM_FIELD + 1), ..fields() }).is_err());
        assert!(validate_fields(&ItemFields { inventory_size: MAX_INVENTORY_SIZE + 1, ..fields() }).is_err());
    }

    #[test]
    fn the_image_must_be_an_https_url() {
        let with = |u: &str| validate_fields(&ItemFields { image: Some(u.into()), ..fields() });
        assert!(with("https://cdn.discordapp.com/a.png").is_ok());
        assert_eq!(with("http://x/a.png").unwrap_err(), "activity__invalid_image_url");
        assert_eq!(with("javascript:alert(1)").unwrap_err(), "activity__invalid_image_url");
        assert_eq!(with("https://x/a b.png").unwrap_err(), "activity__invalid_image_url");
    }

    #[test]
    fn effects_are_written_back_in_the_parser_syntax() {
        let stat = |name: &str| Stat {
            _id: ObjectId::new(),
            universe_id: ObjectId::new(),
            name: name.into(),
            base_value: StatValue::F64(1.0),
            formula: None,
            min: None,
            max: None,
            modifiers: vec![],
        };
        let (force, speed) = (stat("Force"), stat("Vitesse"));
        let m = |stat: &Stat, value: f64, t: ModifierType, end: Option<u64>, level: ModifierLevel| Modifier { stat_id: stat._id, value: StatValue::F64(value), modifier_type: t, end_timestamp: end, source: ObjectId::new(), level };
        let effects = [
            m(&force, 5.0, ModifierType::Base, Some(600), ModifierLevel::Player),
            m(&speed, 1.2, ModifierType::Multiplier, None, ModifierLevel::Place),
            m(&force, -2.0, ModifierType::Base, None, ModifierLevel::Universe),
            m(&force, 10.0, ModifierType::Flats, None, ModifierLevel::Area),
        ];
        assert_eq!(
            effects_text(&effects, &[force.clone(), speed]),
            "Force: +5 600s player\nVitesse: x1.2 place\nForce: -2 universe\nForce: 10 flat area"
        );
    }
}

/// Consomme un objet de l'inventaire du joueur et applique ses effets tout de suite.
///
/// Effets « joueur » sur le personnage, « lieu » sur sa position (lieu ou route où se trouve son
/// groupe), « zone » sur le salon central du lieu. Si le joueur voyage, la vitesse de son groupe
/// est recalculée aussitôt (`travel::logic::refresh_moving_group`). Craft et fouille ne dépendent
/// pas des statistiques : rien à recalculer. Renvoie le nom de l'objet.
pub async fn consume_item(universe_id: ObjectId, user_id: u64, item_id: ObjectId) -> Result<String, Error> {
    use crate::database::inventory::{HolderType, Inventory};
    use crate::database::modifiers::ModifierLevel;
    use crate::database::travel::SpaceType;
    use crate::item::ItemUsage;

    let mut character = crate::database::characters::get_character_by_user_id(universe_id, user_id).await?.ok_or("consume__character_not_found")?;
    let item = crate::database::items::get_item_by_id(item_id).await?.filter(|i| i.universe_id == universe_id).ok_or("consume__item_not_found")?;
    if !matches!(item.item_usage, ItemUsage::Consumable) {
        return Err("consume__not_consumable".into());
    }

    // Position du joueur : effets de lieu sur la route ou le lieu où se trouve son groupe.
    let group = crate::database::travel::get_group_of_user(universe_id, user_id).await?;
    let (mut road, mut place) = (None, None);
    if let Some(g) = &group {
        match g.actual_space_type {
            SpaceType::Road => road = crate::database::road::get_road_by_channel_id(universe_id, g.actual_space_id).await?,
            SpaceType::Place => place = crate::database::places::get_place_by_category_id(universe_id, g.actual_space_id).await?,
        }
    }
    let area_channel = place.as_ref().and_then(|p| p.central_channel_id);
    let mut area = match area_channel {
        Some(c) => crate::database::areas::get_area_by_channel_id(universe_id, c).await?,
        None => None,
    };

    let now = chrono::Utc::now().timestamp() as u64;
    let mut stats_changed = false;
    for modifier in &item.effects {
        let mut m = modifier.clone();
        m.source = item_id;
        // Durée de l'effet → fin absolue.
        if let Some(duration_secs) = m.end_timestamp {
            m.end_timestamp = Some(now + duration_secs);
        }
        match m.level {
            ModifierLevel::Player => {
                if let Some(stat) = character.stats.iter_mut().find(|s| s._id == m.stat_id) {
                    stat.modifiers.push(m);
                    stats_changed = true;
                }
            }
            ModifierLevel::Place => {
                if let Some(r) = road.as_mut() {
                    r.modifiers.push(m);
                    stats_changed = true;
                } else if let Some(p) = place.as_mut() {
                    p.modifiers.push(m);
                    stats_changed = true;
                }
            }
            ModifierLevel::Area => {
                if let Some(channel) = area_channel {
                    if area.is_none() {
                        area = Some(crate::database::areas::Area::new(universe_id, channel).await);
                    }
                    if let Some(a) = area.as_mut() {
                        a.modifiers.push(m);
                    }
                }
            }
            // L'univers n'a pas de modificateurs.
            ModifierLevel::Universe => {}
        }
    }

    let client = crate::database::db_client::get_db_client().await;
    let mut session = client.start_session().await?;
    session.start_transaction().await?;
    let written: Result<(), Error> = async {
        if !Inventory::remove_item_from_holder_with_session(&mut session, universe_id, character._id, HolderType::Character, item_id, 1).await? {
            return Err("consume__item_not_found".into());
        }
        character.update_with_session(&mut session).await?;
        if let Some(r) = road {
            r.update_with_optional_session(Some(&mut session)).await?;
        }
        if let Some(p) = place {
            p.update_with_optional_session(Some(&mut session)).await?;
        }
        if let Some(a) = area {
            a.update_with_session(&mut session).await?;
        }
        Ok(())
    }
    .await;
    match written {
        Ok(()) => session.commit_transaction().await?,
        Err(e) => {
            let _ = session.abort_transaction().await;
            return Err(e);
        }
    }

    // Vitesse du groupe en route : recalculée maintenant, pas au prochain changement prévu.
    if stats_changed {
        if let Err(e) = crate::travel::logic::refresh_moving_group(user_id).await {
            tracing::warn!(user = user_id, error = %e, "objet consommé : vitesse du groupe non recalculée");
        }
    }
    Ok(item.item_name)
}
