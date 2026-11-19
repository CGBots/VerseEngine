//! # Service de voyage partagé
//!
//! Toute la logique de `/travel_start`, `/travel_stop`, `/travel_join` et `/travel_leave`, sans
//! `poise::Context` : les commandes slash et l'API de l'activité appellent ces mêmes fonctions,
//! donc une action donne exactement le même résultat quelle que soit son origine.
//!
//! Reste propre aux commandes slash : la vérification du salon (`validate_channel`), qui n'a pas
//! de sens dans l'activité, et la mise en forme de la réponse à l'interaction.
//!
//! Les erreurs sont des clés Fluent (`travel__…`), affichées telles quelles par le bot et
//! renvoyées par l'API.

use std::collections::HashSet;
use std::sync::Arc;

use futures::TryStreamExt;
use serenity::all::{ChannelId, ChannelType, CreateMessage, GuildId, Http};

use crate::database::characters::get_character_by_user_id;
use crate::database::craft::PlayerCraft;
use crate::database::known_roads::{known_road_ids, remember_road};
use crate::database::places::{get_place_by_category_id, Place};
use crate::database::road::{get_road, get_road_by_channel_id, get_roads_of_place, Road};
use crate::database::server::{get_server_by_id, Server};
use crate::database::travel::{SpaceType, TravelGroup};
use crate::database::universe::get_universe_by_id;
use crate::discord::poise_structs::Error;
use crate::translation::get_by_locale;
use crate::travel::logic::{add_travel, calculate_current_distance, get_travel_threshold, stop_travel};

/// Joueur à l'origine de l'action.
#[derive(Debug, Clone, Copy)]
pub struct Actor<'a> {
    pub user_id: u64,
    /// Nom affiché si le joueur n'a pas de personnage (nom Discord).
    pub fallback_name: &'a str,
    /// Langue des messages publics produits par l'action.
    pub locale: &'a str,
}

/// Salon du message public annonçant l'action.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Announce {
    /// Salon de la commande slash (déjà validé par `validate_channel`).
    In(ChannelId),
    /// Salon de la position du groupe : la route, ou le premier salon textuel de la catégorie du
    /// lieu. Utilisé par l'activité, qui n'est pas dans un salon textuel.
    AtPosition,
}

/// Destination proposée depuis la position actuelle.
#[derive(Debug, Clone)]
pub struct Destination {
    pub place: Place,
    /// Distance restante en km.
    pub distance_km: f64,
}

/// Une route est visible par un joueur si elle n'est pas secrète, s'il l'a déjà empruntée
/// (`known_roads`, D10) ou s'il est dessus.
pub fn road_is_visible(road: &Road, known: &HashSet<u64>, current_road: Option<u64>) -> bool {
    !road.secret || known.contains(&road.channel_id) || current_road == Some(road.channel_id)
}

/// Salon d'annonce d'un lieu : le premier salon textuel (par position) de sa catégorie.
/// `channels` : `(id, catégorie parente, type, position)`.
pub fn pick_place_channel(channels: &[(u64, Option<u64>, ChannelType, u16)], category_id: u64) -> Option<u64> {
    channels
        .iter()
        .filter(|(_, parent, kind, _)| *parent == Some(category_id) && matches!(kind, ChannelType::Text | ChannelType::News))
        .min_by_key(|(id, _, _, position)| (*position, *id))
        .map(|(id, ..)| *id)
}

async fn server_and_group(guild_id: u64, user_id: u64) -> Result<(Server, TravelGroup), Error> {
    let server = get_server_by_id(guild_id).await?.ok_or("travel__server_not_found")?;
    let group = server.clone().get_player_move(user_id).await?.ok_or("travel__character_not_found")?;
    Ok((server, group))
}

async fn display_name(universe_id: mongodb::bson::oid::ObjectId, user_id: u64, fallback: &str) -> String {
    match get_character_by_user_id(universe_id, user_id).await {
        Ok(Some(c)) => c.name,
        _ => fallback.to_string(),
    }
}

/// Salon textuel d'annonce d'un lieu (sur le serveur Discord du lieu).
async fn place_channel(http: &Http, place: &Place) -> Option<ChannelId> {
    let channels = http.get_channels(GuildId::new(place.server_id)).await.ok()?;
    let list: Vec<_> = channels.iter().map(|c| (c.id.get(), c.parent_id.map(|p| p.get()), c.kind, c.position)).collect();
    pick_place_channel(&list, place.category_id).map(ChannelId::new)
}

/// Envoie un message public. Un échec est journalisé : l'action de jeu a déjà eu lieu.
async fn announce(http: &Http, channel: Option<ChannelId>, content: String) {
    match channel {
        Some(channel) => {
            if let Err(e) = channel.send_message(http, CreateMessage::new().content(content)).await {
                eprintln!("[travel] cannot send the public message in {channel}: {e:?}");
            }
        }
        None => eprintln!("[travel] no channel for the public message: {content}"),
    }
}

/// Salon de la position actuelle d'un groupe.
async fn position_channel(http: &Http, universe_id: mongodb::bson::oid::ObjectId, group: &TravelGroup) -> Option<ChannelId> {
    match group.actual_space_type {
        SpaceType::Road => Some(ChannelId::new(group.actual_space_id)),
        SpaceType::Place => {
            let place = get_place_by_category_id(universe_id, group.actual_space_id).await.ok().flatten()?;
            place_channel(http, &place).await
        }
    }
}

async fn remember(universe_id: mongodb::bson::oid::ObjectId, members: &[u64], road_id: u64) {
    if let Err(e) = remember_road(universe_id, members, road_id).await {
        eprintln!("[travel] cannot remember road {road_id} for {members:?}: {e:?}");
    }
}

/// Destinations possibles depuis la position du joueur (menu de `/travel_start`, carte).
pub async fn destinations(guild_id: u64, user_id: u64) -> Result<Vec<Destination>, Error> {
    let (server, group) = server_and_group(guild_id, user_id).await?;
    let universe_id = server.universe_id;
    let mut out = Vec::new();
    match group.actual_space_type {
        SpaceType::Road => {
            let road = get_road_by_channel_id(universe_id, group.actual_space_id).await?.ok_or("travel__road_not_found")?;
            for place_id in [road.place_one_id, road.place_two_id] {
                if let Some(place) = get_place_by_category_id(universe_id, place_id).await? {
                    let distance_km = if Some(place_id) == group.destination_id {
                        road.distance as f64 - group.distance_traveled
                    } else {
                        group.distance_traveled
                    };
                    out.push(Destination { place, distance_km: distance_km.max(0.0) });
                }
            }
        }
        SpaceType::Place => {
            let known = known_road_ids(universe_id, user_id).await?;
            let roads: Vec<Road> = get_roads_of_place(universe_id, group.actual_space_id).await?.try_collect().await?;
            for road in roads.into_iter().filter(|r| road_is_visible(r, &known, None)) {
                let other = if road.place_one_id == group.actual_space_id { road.place_two_id } else { road.place_one_id };
                if let Some(place) = get_place_by_category_id(universe_id, other).await? {
                    out.push(Destination { place, distance_km: road.distance as f64 });
                }
            }
        }
    }
    Ok(out)
}

/// Part vers `destination_id` (catégorie du lieu). Équivaut à `/travel_start destination`.
///
/// Retourne la clé du message de confirmation (`travel__started`, …).
pub async fn start(http: &Arc<Http>, guild_id: u64, actor: Actor<'_>, destination_id: u64, announce_in: Announce) -> Result<&'static str, Error> {
    let (server, mut group) = server_and_group(guild_id, actor.user_id).await?;

    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(server.universe_id, actor.user_id).await {
        return Err("travel__cannot_move_while_crafting".into());
    }
    let destination = get_place_by_category_id(server.universe_id, destination_id).await?.ok_or("travel__place_not_found")?;

    if group.is_in_move && group.actual_space_type == SpaceType::Road {
        if group.members.first().is_some_and(|leader| *leader != actor.user_id) {
            return Err("travel__only_leader_can_stop".into());
        }
        group = stop_travel(actor.user_id).await?;
    }

    match group.actual_space_type {
        SpaceType::Road => move_from_road(http, destination_id, &server, group).await,
        SpaceType::Place => {
            let source_place_id = group.actual_space_id;
            let members = group.members.clone();
            let (key, road) = move_from_place(http, destination_id, &server, group).await?;
            remember(server.universe_id, &members, road.channel_id).await;

            let name = display_name(server.universe_id, actor.user_id, actor.fallback_name).await;
            let mut args = fluent::FluentArgs::new();
            args.set("user", name);
            args.set("destination", destination.name.clone());
            let content = get_by_locale(actor.locale, "travel__moving_to_place", None, Some(&args));
            let channel = match announce_in {
                Announce::In(c) => Some(c),
                Announce::AtPosition => match get_place_by_category_id(server.universe_id, source_place_id).await? {
                    Some(source) => place_channel(http, &source).await,
                    None => None,
                },
            };
            announce(http, channel, content).await;
            Ok(key)
        }
    }
}

/// Groupe déjà sur une route : continuer vers la même destination, ou faire demi-tour.
async fn move_from_road(http: &Arc<Http>, destination_id: u64, server: &Server, mut group: TravelGroup) -> Result<&'static str, Error> {
    if Some(destination_id) == group.destination_id {
        add_travel(http.clone(), server.server_id, group).await?;
        return Ok("travel__already_moving_to_destination");
    }
    if Some(destination_id) != group.source_id {
        return Err("travel__invalid_road_destination".into());
    }

    std::mem::swap(&mut group.destination_id, &mut group.source_id);
    std::mem::swap(&mut group.destination_role_id, &mut group.source_role_id);
    std::mem::swap(&mut group.destination_server_id, &mut group.source_server_id);

    if let Some(road_id) = group.road_id {
        if let Ok(Some(road)) = get_road_by_channel_id(server.universe_id, road_id).await {
            group.distance_traveled = (road.distance as f64 - group.distance_traveled).max(0.0);
        }
    }
    let now = chrono::Utc::now().timestamp() as u64;
    group.step_start_timestamp = Some(now);
    group.step_end_timestamp = Some(now);
    group.modified_speed = 0.0;

    add_travel(http.clone(), server.server_id, group).await?;
    Ok("travel__started")
}

/// Groupe dans un lieu : il s'engage sur la route qui mène à `destination_id`.
async fn move_from_place(http: &Arc<Http>, destination_id: u64, server: &Server, mut group: TravelGroup) -> Result<(&'static str, Road), Error> {
    let road = get_road(server.universe_id, group.actual_space_id, destination_id).await?.ok_or("travel__no_road_found")?;
    let source_place_id = if road.place_one_id == destination_id { road.place_two_id } else { road.place_one_id };
    let source = get_place_by_category_id(server.universe_id, source_place_id).await?.ok_or("travel__source_place_not_found")?;
    let destination = get_place_by_category_id(server.universe_id, destination_id).await?.ok_or("travel__place_not_found")?;

    group.actual_space_id = road.channel_id;
    group.actual_space_type = SpaceType::Road;
    group.is_in_move = true;
    group.road_id = Some(road.channel_id);
    group.road_role_id = None;
    group.road_server_id = Some(road.server_id);
    group.source_id = Some(source_place_id);
    group.source_role_id = Some(source.role);
    group.source_server_id = Some(source.server_id);
    group.destination_id = Some(destination_id);
    group.destination_role_id = Some(destination.role);
    group.destination_server_id = Some(destination.server_id);
    group.distance_traveled = 0.0;

    let now = chrono::Utc::now().timestamp() as u64;
    group.step_start_timestamp = Some(now);
    group.step_end_timestamp = Some(now);
    group.modified_speed = 0.0;

    add_travel(http.clone(), server.server_id, group).await?;
    Ok(("travel__started", road))
}

/// Arrête le voyage du groupe (chef de groupe uniquement). Équivaut à `/travel_stop`.
///
/// # Errors
/// `travel__not_in_move` si le groupe n'est pas en route, `travel__only_leader_can_stop`.
pub async fn stop(guild_id: u64, user_id: u64) -> Result<&'static str, Error> {
    let (_, group) = server_and_group(guild_id, user_id).await?;
    if !(group.is_in_move && group.actual_space_type == SpaceType::Road) {
        return Err("travel__not_in_move".into());
    }
    if group.members.first().is_some_and(|leader| *leader != user_id) {
        return Err("travel__only_leader_can_stop".into());
    }
    stop_travel(user_id).await.map_err(|_| Error::from("travel__not_in_move"))?;
    Ok("travel__stopped")
}

/// Rejoint le groupe de `target_id`. Équivaut à `/travel_join`.
pub async fn join(http: &Arc<Http>, guild_id: u64, actor: Actor<'_>, target_id: u64, target_fallback_name: &str, announce_in: Announce) -> Result<&'static str, Error> {
    if actor.user_id == target_id {
        return Err("travel__cannot_join_self".into());
    }
    let (server, my_group) = server_and_group(guild_id, actor.user_id).await?;
    let universe = get_universe_by_id(server.universe_id).await?.ok_or("travel__universe_not_found")?;
    let mut target_group = server.clone().get_player_move(target_id).await?.ok_or("travel__target_not_found")?;

    if my_group._id == target_group._id {
        return Err("travel__already_in_same_group".into());
    }
    if my_group.actual_space_id != target_group.actual_space_id || my_group.actual_space_type != target_group.actual_space_type {
        return Err("travel__too_far_different_place".into());
    }

    let threshold = get_travel_threshold(universe.global_time_modifier.into());

    if my_group.actual_space_type == SpaceType::Road {
        let road = get_road_by_channel_id(server.universe_id, my_group.actual_space_id).await?.ok_or("travel__road_not_found")?;
        let absolute_position = |g: &TravelGroup| {
            if g.source_id == Some(road.place_one_id) { calculate_current_distance(g) } else { road.distance as f64 - calculate_current_distance(g) }
        };
        let my_position = absolute_position(&my_group);
        let target_position = absolute_position(&target_group);
        if (my_position - target_position).abs() * 1000.0 > threshold {
            return Err("travel__too_far_to_join".into());
        }
        if target_group.is_in_move {
            target_group.distance_traveled =
                if target_group.source_id == Some(road.place_one_id) { target_position } else { road.distance as f64 - target_position };
            let now = chrono::Utc::now().timestamp() as u64;
            target_group.step_start_timestamp = Some(now);
            target_group.step_end_timestamp = Some(now);
            target_group.modified_speed = 0.0;
        }
    }

    for member in &my_group.members {
        if !target_group.members.contains(member) {
            target_group.members.push(*member);
        }
    }

    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;
    let written: Result<(), mongodb::error::Error> = async {
        my_group.remove_with_session(&mut session).await?;
        target_group.upsert_with_session(&mut session).await?;
        Ok(())
    }
    .await;
    match written {
        Ok(()) => session.commit_transaction().await?,
        Err(e) => {
            let _ = session.abort_transaction().await;
            return Err(e.into());
        }
    }

    if target_group.is_in_move {
        add_travel(http.clone(), server.server_id, target_group.clone()).await?;
    }
    if let Some(road_id) = target_group.road_id.filter(|_| target_group.actual_space_type == SpaceType::Road) {
        remember(server.universe_id, &my_group.members, road_id).await;
    }

    let mut args = fluent::FluentArgs::new();
    args.set("user", display_name(universe.universe_id, actor.user_id, actor.fallback_name).await);
    args.set("target", display_name(universe.universe_id, target_id, target_fallback_name).await);
    let content = get_by_locale(actor.locale, "travel__public_joined", None, Some(&args));
    let channel = match announce_in {
        Announce::In(c) => Some(c),
        Announce::AtPosition => position_channel(http, server.universe_id, &target_group).await,
    };
    announce(http, channel, content).await;
    Ok("travel__joined_group")
}

/// Quitte son groupe : le joueur forme un groupe seul à sa position. Équivaut à `/travel_leave`.
pub async fn leave(http: &Arc<Http>, guild_id: u64, actor: Actor<'_>, announce_in: Announce) -> Result<&'static str, Error> {
    let (server, mut group) = server_and_group(guild_id, actor.user_id).await?;
    if group.members.len() <= 1 {
        return Err("travel__already_alone".into());
    }

    let leader_before = group.members[0];
    let quitting_name = display_name(server.universe_id, actor.user_id, actor.fallback_name).await;
    let leader_name = display_name(server.universe_id, leader_before, &format!("Leader ({leader_before})")).await;

    group.members.retain(|id| *id != actor.user_id);
    let mut new_group = group.clone();
    new_group._id = mongodb::bson::oid::ObjectId::new();
    new_group.members = vec![actor.user_id];

    let current = calculate_current_distance(&group);
    let now = chrono::Utc::now().timestamp() as u64;
    for g in [&mut group, &mut new_group] {
        g.distance_traveled = current;
        g.step_start_timestamp = Some(now);
        g.step_end_timestamp = Some(now);
        g.modified_speed = 0.0;
    }

    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;
    let written: Result<(), mongodb::error::Error> = async {
        group.upsert_with_session(&mut session).await?;
        new_group.insert_with_session(&mut session).await?;
        Ok(())
    }
    .await;
    match written {
        Ok(()) => session.commit_transaction().await?,
        Err(e) => {
            let _ = session.abort_transaction().await;
            return Err(e.into());
        }
    }

    if group.is_in_move {
        stop_travel(group.members[0]).await?;
        add_travel(http.clone(), server.server_id, group.clone()).await?;
        add_travel(http.clone(), server.server_id, new_group.clone()).await?;
    }

    let mut args = fluent::FluentArgs::new();
    args.set("user", quitting_name);
    args.set("leader", leader_name);
    let content = get_by_locale(actor.locale, "travel__public_left", None, Some(&args));
    let channel = match announce_in {
        Announce::In(c) => Some(c),
        Announce::AtPosition => position_channel(http, server.universe_id, &new_group).await,
    };
    announce(http, channel, content).await;
    Ok("travel__left_group")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn road(channel_id: u64, secret: bool) -> Road {
        super::tests_support_road(Default::default(), channel_id, 3, 4, secret)
    }

    #[test]
    fn public_roads_are_always_visible() {
        assert!(road_is_visible(&road(1, false), &HashSet::new(), None));
    }

    #[test]
    fn secret_roads_are_hidden_until_taken() {
        let secret = road(7, true);
        assert!(!road_is_visible(&secret, &HashSet::new(), None));
        assert!(!road_is_visible(&secret, &HashSet::from([8]), Some(9)));
        assert!(road_is_visible(&secret, &HashSet::from([7]), None), "already taken");
        assert!(road_is_visible(&secret, &HashSet::new(), Some(7)), "currently on it");
    }

    #[test]
    fn the_first_text_channel_of_the_category_is_picked() {
        let channels = [
            (10, Some(1), ChannelType::Text, 3),
            (11, Some(1), ChannelType::Voice, 0),
            (12, Some(1), ChannelType::Text, 1),
            (13, Some(2), ChannelType::Text, 0),
            (14, None, ChannelType::Text, 0),
        ];
        assert_eq!(pick_place_channel(&channels, 1), Some(12));
        assert_eq!(pick_place_channel(&channels, 2), Some(13));
    }

    #[test]
    fn no_text_channel_means_no_announcement_channel() {
        let channels = [(11, Some(1), ChannelType::Voice, 0), (12, Some(1), ChannelType::Category, 0)];
        assert_eq!(pick_place_channel(&channels, 1), None);
        assert_eq!(pick_place_channel(&[], 1), None);
    }

    #[test]
    fn equal_positions_are_ordered_by_id() {
        let channels = [(20, Some(1), ChannelType::Text, 0), (15, Some(1), ChannelType::News, 0)];
        assert_eq!(pick_place_channel(&channels, 1), Some(15));
    }
}

/// Tests d'intégration MongoDB du service (sans Discord).
#[cfg(test)]
mod integration {
    use super::*;
    use crate::database::db_client::get_db_client;
    use crate::database::db_namespace::*;
    use crate::test_support::{block_on, random_id};
    use mongodb::bson::{doc, oid::ObjectId};

    fn place(universe_id: ObjectId, category_id: u64, name: &str) -> Place {
        Place { _id: ObjectId::new(), universe_id, server_id: 1, category_id, role: random_id(), name: name.into(), modifiers: vec![] }
    }

    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn secret_roads_appear_in_destinations_once_taken() {
        block_on(async {
            let universe_id = ObjectId::new();
            let guild_id = random_id();
            let user_id = random_id();
            let (a, b, c) = (random_id(), random_id(), random_id());

            let mut server = Server::default();
            server.server_id(guild_id).universe_id(universe_id);
            server.insert_server().await.unwrap();
            for p in [place(universe_id, a, "A"), place(universe_id, b, "B"), place(universe_id, c, "C")] {
                p.insert().await.unwrap();
            }
            let mut public = super::tests_support_road(universe_id, random_id(), a, b, false);
            public.distance = 12;
            public.insert().await.unwrap();
            let secret = super::tests_support_road(universe_id, random_id(), c, a, true);
            secret.insert().await.unwrap();
            TravelGroup { _id: ObjectId::new(), universe_id, actual_space_id: a, members: vec![user_id], server_id: guild_id, ..Default::default() }
                .insert()
                .await
                .unwrap();

            let before = destinations(guild_id, user_id).await;
            remember_road(universe_id, &[user_id], secret.channel_id).await.unwrap();
            let after = destinations(guild_id, user_id).await;

            let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
            for name in [PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, KNOWN_ROADS_COLLECTION_NAME] {
                db.collection::<mongodb::bson::Document>(name).delete_many(doc! {"universe_id": universe_id}).await.unwrap();
            }
            db.collection::<mongodb::bson::Document>(SERVERS_COLLECTION_NAME).delete_one(doc! {"server_id": guild_id.to_string()}).await.unwrap();

            let names = |d: &[Destination]| { let mut n: Vec<_> = d.iter().map(|x| x.place.name.clone()).collect(); n.sort(); n };
            let before = before.unwrap();
            assert_eq!(names(&before), vec!["B"], "the secret road is hidden");
            assert_eq!(before[0].distance_km, 12.0);
            assert_eq!(names(&after.unwrap()), vec!["B", "C"], "a road already taken is shown");
        });
    }

    /// Voyage réel par le service partagé (commandes slash et activité) : au départ, le voyageur
    /// reçoit une permission individuelle sur le salon de la route ; à l'arrivée, elle est retirée.
    /// Aucun rôle de route n'est utilisé (Q1). Le serveur VerseEngine est fictif ; seul le salon de
    /// la route est réel, dans le serveur de test, et le voyageur est le propriétaire de ce serveur.
    #[test]
    #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
    fn a_trip_grants_then_revokes_the_road_access() {
        use crate::database::characters::Character;
        use crate::database::stats::{Stat, StatValue, SPEED_STAT};
        use crate::test_support::{discord_http, required_env_u64, TempChannel};
        use serenity::all::{GuildId, PermissionOverwriteType, UserId};
        block_on(async {
            let http = discord_http();
            *crate::travel::logic::HTTP_CLIENT.lock().await = Some(http.clone());
            let test_guild = GuildId::new(required_env_u64("VERSEENGINE_TEST_GUILD_ID"));
            let owner = http.get_guild(test_guild).await.unwrap().owner_id.get();
            let road_channel = TempChannel::create(&http, "test-road-access").await;

            let universe_id = ObjectId::new();
            let guild_id = random_id();
            let (a, b) = (random_id(), random_id());
            let mut server = Server::default();
            server.server_id(guild_id).universe_id(universe_id);
            server.insert_server().await.unwrap();
            for mut p in [place(universe_id, a, "A"), place(universe_id, b, "B")] {
                p.server_id = guild_id;
                p.insert().await.unwrap();
            }
            let mut road = super::tests_support_road(universe_id, road_channel.id.get(), a, b, false);
            road.server_id = guild_id;
            road.distance = 1;
            road.insert().await.unwrap();
            let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
            // 1 km à 600 km/h : 6 s de trajet.
            let speed = Stat { _id: ObjectId::new(), universe_id, name: SPEED_STAT.into(), base_value: StatValue::F64(600.0), formula: None, min: None, max: None, modifiers: vec![] };
            db.collection::<Stat>(STATS_COLLECTION_NAME).insert_one(&speed).await.unwrap();
            let character = Character { _id: ObjectId::new(), user_id: owner, universe_id, name: "Testeur".into(), stats: vec![], description: None, image: None };
            db.collection::<Character>(CHARACTERS_COLLECTION_NAME).insert_one(&character).await.unwrap();
            TravelGroup { _id: ObjectId::new(), universe_id, actual_space_id: a, members: vec![owner], server_id: guild_id, ..Default::default() }
                .insert()
                .await
                .unwrap();

            let has_access = || async {
                let c = http.get_channel(road_channel.id).await.unwrap().guild().unwrap();
                c.permission_overwrites.iter().any(|o| o.kind == PermissionOverwriteType::Member(UserId::new(owner)))
            };
            let actor = Actor { user_id: owner, fallback_name: "Testeur", locale: "fr" };
            let started = start(&http, guild_id, actor, b, Announce::In(road_channel.id)).await;
            let access_during_trip = has_access().await;
            let group_during_trip = crate::database::travel::get_group_of_user(universe_id, owner).await.unwrap().unwrap();

            let mut arrived = None;
            for _ in 0..40 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let g = crate::database::travel::get_group_of_user(universe_id, owner).await.unwrap().unwrap();
                if g.actual_space_type == SpaceType::Place && !has_access().await {
                    arrived = Some(g);
                    break;
                }
            }

            for name in [PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, KNOWN_ROADS_COLLECTION_NAME, STATS_COLLECTION_NAME, CHARACTERS_COLLECTION_NAME] {
                db.collection::<mongodb::bson::Document>(name).delete_many(doc! {"universe_id": universe_id}).await.unwrap();
            }
            db.collection::<mongodb::bson::Document>(SERVERS_COLLECTION_NAME).delete_one(doc! {"server_id": guild_id.to_string()}).await.unwrap();
            road_channel.delete(&http).await;

            assert_eq!(started.unwrap(), "travel__started");
            assert!(access_during_trip, "the traveller can see the road channel");
            assert_eq!(group_during_trip.road_role_id, None, "no road role any more");
            let arrived = arrived.expect("the group arrives and loses the road access");
            assert_eq!(arrived.actual_space_id, b);
        });
    }
}

#[cfg(test)]
pub(crate) fn tests_support_road(universe_id: mongodb::bson::oid::ObjectId, channel_id: u64, place_one_id: u64, place_two_id: u64, secret: bool) -> Road {
    Road {
        _id: mongodb::bson::oid::ObjectId::new(),
        universe_id,
        server_id: 1,
        server_two_id: None,
        road_name: "road".into(),
        role_id: None,
        channel_id,
        place_one_id,
        place_two_id,
        distance: 10,
        secret,
        modifiers: vec![],
    }
}
