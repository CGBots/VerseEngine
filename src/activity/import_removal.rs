//! Suppression des lieux existants lors d'un import (étape E6, décisions I15 et Q2 de
//! `docs/activity/IMPORT_SPEC.md`), et des lieux et routes disparus lors d'un ré-import (I10).
//!
//! - Les personnages présents sont déplacés vers le **lieu de départ** choisi dans la nouvelle
//!   carte ; les voyages en cours sont annulés.
//! - Pour chaque **objet posé**, l'admin choisit de le détruire ou de le rendre à son
//!   propriétaire avec son contenu. Un objet sans propriétaire ne peut qu'être détruit : l'admin
//!   en voit la liste et doit confirmer (Q2).
//! - Les salons, catégories et rôles des lieux supprimés, et les salons des routes supprimées,
//!   sont supprimés de Discord.
//!
//! Ordre (voir `import_exec`) : tout le nouveau est créé d'abord (et peut être défait) ; puis une
//! **seule transaction** écrit la carte, déplace les groupes, règle les objets et supprime les
//! documents : c'est le point de non-retour. Les suppressions sur Discord viennent après ; un
//! objet qui résiste est signalé, sans annuler le reste.

use std::collections::{HashMap, HashSet};

use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::ClientSession;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use serenity::all::{ChannelId, GuildId, Http, RoleId};

use crate::activity::map::import_plan::{ImportMode, ImportPlan};
use crate::database::db_client::get_db_client;
use crate::database::db_namespace::*;
use crate::database::inventory::{HolderType, Inventory};
use crate::database::places::Place;
use crate::database::road::Road;
use crate::database::tool::Tool;
use crate::database::travel::{SpaceType, TravelGroup};

// ---------------------------------------------------------------------------
// Ce que l'admin voit avant de valider
// ---------------------------------------------------------------------------

#[serde_as]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedItemInfo {
    #[serde_as(as = "DisplayFromStr")]
    pub tool_id: ObjectId,
    pub name: String,
    pub place_name: String,
    /// Nom du personnage propriétaire ; `None` : objet sans propriétaire (sera détruit, Q2).
    pub owner: Option<String>,
    /// Nombre d'objets dans son inventaire.
    pub content: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RemovalInfo {
    /// Joueurs à déplacer vers le lieu de départ.
    pub characters: usize,
    /// Dont en voyage (voyage annulé).
    pub travelling: usize,
    pub items: Vec<PlacedItemInfo>,
}

impl RemovalInfo {
    pub fn unowned(&self) -> usize {
        self.items.iter().filter(|i| i.owner.is_none()).count()
    }
}

// ---------------------------------------------------------------------------
// Choix de l'admin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemAction {
    Destroy,
    Return,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemChoice {
    #[serde_as(as = "DisplayFromStr")]
    pub tool_id: ObjectId,
    pub action: ItemAction,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RemovalChoices {
    /// Bourg de la nouvelle carte où vont les personnages.
    #[serde(default)]
    pub start_burg: Option<u64>,
    /// Par défaut : rendu au propriétaire s'il y en a un, sinon détruit.
    #[serde(default)]
    pub items: Vec<ItemChoice>,
    /// Q2 : l'admin a vu et accepte la destruction des objets sans propriétaire.
    #[serde(default)]
    pub confirm_destroy_unowned: bool,
}

/// Vérifie les choix ; retourne la clé d'erreur à montrer.
pub fn validate_choices(info: &RemovalInfo, choices: &RemovalChoices, plan: &ImportPlan) -> Result<(), &'static str> {
    if info.characters > 0 {
        let start = choices.start_burg.ok_or("activity__import_start_place_required")?;
        if !plan.places.iter().any(|p| p.burg_id == start) {
            return Err("activity__import_start_place_required");
        }
    }
    if info.unowned() > 0 && !choices.confirm_destroy_unowned {
        return Err("activity__import_confirm_unowned");
    }
    let unowned: HashSet<ObjectId> = info.items.iter().filter(|i| i.owner.is_none()).map(|i| i.tool_id).collect();
    if choices.items.iter().any(|c| c.action == ItemAction::Return && unowned.contains(&c.tool_id)) {
        return Err("activity__import_unowned_cannot_return");
    }
    Ok(())
}

/// Action retenue pour un objet : celle de l'admin, sinon rendre s'il a un propriétaire.
pub fn item_action(tool: &Tool, choices: &RemovalChoices) -> ItemAction {
    if tool.owner_id.is_none() {
        return ItemAction::Destroy;
    }
    choices.items.iter().find(|c| Some(c.tool_id) == tool._id).map(|c| c.action).unwrap_or(ItemAction::Return)
}

// ---------------------------------------------------------------------------
// Logique pure
// ---------------------------------------------------------------------------

/// Groupes touchés : tous en mode remplacer ; sinon ceux qui sont dans un lieu supprimé, sur une
/// route supprimée, ou en route depuis ou vers un lieu supprimé.
pub fn affected_groups<'a>(groups: &'a [TravelGroup], mode: ImportMode, places: &HashSet<u64>, roads: &HashSet<u64>) -> Vec<&'a TravelGroup> {
    groups
        .iter()
        .filter(|g| {
            mode == ImportMode::Replace
                || match g.actual_space_type {
                    SpaceType::Place => places.contains(&g.actual_space_id),
                    SpaceType::Road => {
                        roads.contains(&g.actual_space_id)
                            || g.source_id.is_some_and(|p| places.contains(&p))
                            || g.destination_id.is_some_and(|p| places.contains(&p))
                    }
                }
        })
        .collect()
}

/// Le groupe arrive au lieu de départ ; son voyage éventuel est annulé.
pub fn relocate(group: &TravelGroup, start: &Place) -> TravelGroup {
    TravelGroup {
        _id: group._id,
        universe_id: group.universe_id,
        actual_space_id: start.category_id,
        actual_space_type: SpaceType::Place,
        is_in_move: false,
        is_end: false,
        step_start_timestamp: None,
        step_end_timestamp: None,
        road_role_id: None,
        road_id: None,
        road_server_id: None,
        destination_id: None,
        destination_role_id: None,
        destination_server_id: None,
        source_id: None,
        source_role_id: None,
        source_server_id: None,
        modified_speed: 0.0,
        distance_traveled: 0.0,
        members: group.members.clone(),
        server_id: start.server_id,
    }
}

// ---------------------------------------------------------------------------
// Lecture
// ---------------------------------------------------------------------------

/// Ce qui doit être supprimé, lu en base.
pub struct Removal {
    pub places: Vec<Place>,
    pub roads: Vec<Road>,
    pub groups: Vec<TravelGroup>,
    pub tools: Vec<Tool>,
    pub info: RemovalInfo,
}

pub async fn load_removal(universe_id: ObjectId, mode: ImportMode, plan: &ImportPlan) -> mongodb::error::Result<Removal> {
    let place_ids: HashSet<u64> = plan.remove_places.iter().copied().collect();
    let road_ids: HashSet<u64> = plan.remove_roads.iter().copied().collect();
    let all_places: Vec<Place> = crate::database::places::get_places_by_universe_id(universe_id).await?.try_collect().await?;
    let all_roads: Vec<Road> = crate::database::road::get_roads_by_universe(universe_id).await?.try_collect().await?;
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let groups: Vec<TravelGroup> = db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME).find(doc! {"universe_id": universe_id}).await?.try_collect().await?;
    let places: Vec<Place> = all_places.into_iter().filter(|p| place_ids.contains(&p.category_id)).collect();
    let roads: Vec<Road> = all_roads.into_iter().filter(|r| road_ids.contains(&r.channel_id)).collect();
    let affected: Vec<TravelGroup> = affected_groups(&groups, mode, &place_ids, &road_ids).into_iter().cloned().collect();

    let cats: Vec<i64> = place_ids.iter().map(|&c| c as i64).collect();
    let tools: Vec<Tool> = if cats.is_empty() {
        vec![]
    } else {
        db.collection::<Tool>(PLACED_ITEMS_COLLECTION_NAME).find(doc! {"universe_id": universe_id, "category_id": {"$in": cats}}).await?.try_collect().await?
    };
    let names: HashMap<u64, String> = places.iter().map(|p| (p.category_id, p.name.clone())).collect();
    let mut items = vec![];
    for t in &tools {
        let owner = match t.owner_id {
            Some(id) => db
                .collection::<crate::database::characters::Character>(CHARACTERS_COLLECTION_NAME)
                .find_one(doc! {"_id": id})
                .await?
                .map(|c| c.name),
            None => None,
        };
        let content = match t._id {
            Some(id) => Inventory::get_by_holder(universe_id, id, HolderType::Item).await?.iter().map(|i| i.quantity).sum(),
            None => 0,
        };
        items.push(PlacedItemInfo { tool_id: t._id.unwrap_or_default(), name: t.name.clone(), place_name: names.get(&t.category_id).cloned().unwrap_or_default(), owner, content });
    }
    let info = RemovalInfo {
        characters: affected.iter().map(|g| g.members.len()).sum(),
        travelling: affected.iter().filter(|g| g.actual_space_type == SpaceType::Road).map(|g| g.members.len()).sum(),
        items,
    };
    Ok(Removal { places, roads, groups: affected, tools, info })
}

// ---------------------------------------------------------------------------
// Écriture (dans la transaction de l'import)
// ---------------------------------------------------------------------------

/// Déplace les groupes, règle les objets posés et supprime les documents des lieux et routes.
/// `channels_of_places` : salons de ces lieux (pour retirer leurs zones `areas`).
pub async fn apply_in_session(
    session: &mut ClientSession,
    universe_id: ObjectId,
    removal: &Removal,
    start: Option<&Place>,
    choices: &RemovalChoices,
    channels_of_places: &[u64],
) -> mongodb::error::Result<()> {
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    if let Some(start) = start {
        for g in &removal.groups {
            relocate(g, start).upsert_with_session(session).await?;
        }
    }
    for tool in &removal.tools {
        let Some(tool_id) = tool._id else { continue };
        let content = Inventory::get_by_holder_with_session(session, universe_id, tool_id, HolderType::Item).await?;
        if let (ItemAction::Return, Some(owner)) = (item_action(tool, choices), tool.owner_id) {
            Inventory::add_item_to_inventory_with_session(session, universe_id, owner, HolderType::Character, tool.original_item, 1).await?;
            for inv in content.iter().filter(|i| i.quantity > 0) {
                Inventory::add_item_to_inventory_with_session(session, universe_id, owner, HolderType::Character, inv.item_id, inv.quantity).await?;
            }
        }
        db.collection::<mongodb::bson::Document>(INVENTORY_COLLECTION_NAME)
            .delete_many(doc! {"universe_id": universe_id, "holder.holder_id": tool_id, "holder.holder_type": "Item"})
            .session(&mut *session)
            .await?;
        db.collection::<mongodb::bson::Document>(PLACED_ITEMS_COLLECTION_NAME).delete_one(doc! {"_id": tool_id}).session(&mut *session).await?;
    }
    let as_str = |v: &[u64]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let place_ids: Vec<u64> = removal.places.iter().map(|p| p.category_id).collect();
    let road_ids: Vec<u64> = removal.roads.iter().map(|r| r.channel_id).collect();
    if !place_ids.is_empty() {
        db.collection::<mongodb::bson::Document>(PLACES_COLLECTION_NAME)
            .delete_many(doc! {"universe_id": universe_id, "category_id": {"$in": as_str(&place_ids)}})
            .session(&mut *session)
            .await?;
    }
    if !road_ids.is_empty() {
        db.collection::<mongodb::bson::Document>(ROADS_COLLECTION_NAME)
            .delete_many(doc! {"universe_id": universe_id, "channel_id": {"$in": as_str(&road_ids)}})
            .session(&mut *session)
            .await?;
        db.collection::<mongodb::bson::Document>(KNOWN_ROADS_COLLECTION_NAME)
            .update_many(doc! {"universe_id": universe_id}, doc! {"$pull": {"road_ids": {"$in": as_str(&road_ids)}}})
            .session(&mut *session)
            .await?;
    }
    // Musique : les lieux et routes supprimés sortent des playlists (même transaction).
    if !place_ids.is_empty() || !road_ids.is_empty() {
        db.collection::<mongodb::bson::Document>(crate::database::db_namespace::MUSIC_TRACKS_COLLECTION_NAME)
            .update_many(doc! {"universe_id": universe_id}, doc! {"$pull": {"places": {"$in": as_str(&place_ids)}, "roads": {"$in": as_str(&road_ids)}}})
            .session(&mut *session)
            .await?;
    }
    if !channels_of_places.is_empty() {
        db.collection::<mongodb::bson::Document>(AREAS_COLLECTION_NAME)
            .delete_many(doc! {"universe_id": universe_id, "channel_id": {"$in": as_str(channels_of_places)}})
            .session(&mut *session)
            .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Discord (après la transaction)
// ---------------------------------------------------------------------------

/// Salons (hors catégories) des lieux supprimés, lus sur Discord, par serveur.
pub async fn channels_of_places(http: &Http, places: &[Place]) -> Vec<u64> {
    let cats: HashSet<u64> = places.iter().map(|p| p.category_id).collect();
    let guilds: HashSet<u64> = places.iter().map(|p| p.server_id).collect();
    let mut out = vec![];
    for g in guilds {
        if let Ok(chans) = http.get_channels(GuildId::new(g)).await {
            out.extend(chans.iter().filter(|c| c.parent_id.is_some_and(|p| cats.contains(&p.get()))).map(|c| c.id.get()));
        }
    }
    out
}

/// Ce qui n'a pas pu être supprimé de Discord (journalisé, montré à l'admin).
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct DiscordLeftovers {
    pub channels: usize,
    pub roles: usize,
}

/// Rôle du lieu de départ aux joueurs déplacés, puis suppression des salons, catégories et
/// rôles des lieux et routes supprimés. Un objet déjà absent compte comme supprimé.
pub async fn apply_on_discord(http: &std::sync::Arc<Http>, removal: &Removal, start: Option<&Place>, place_channels: &[u64]) -> DiscordLeftovers {
    if let Some(start) = start {
        for g in &removal.groups {
            for &user in &g.members {
                crate::travel::logic::manage_roles(http.clone(), start.server_id, user, Some(start.role), None).await;
            }
        }
    }
    let mut left = DiscordLeftovers::default();
    let gone = |e: &serenity::Error| matches!(e, serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(r)) if r.status_code.as_u16() == 404);
    for id in removal.roads.iter().map(|r| r.channel_id).chain(place_channels.iter().copied()).chain(removal.places.iter().map(|p| p.category_id)) {
        if let Err(e) = ChannelId::new(id).delete(http).await {
            if !gone(&e) {
                eprintln!("[import] cannot delete channel {id}: {e:?}");
                left.channels += 1;
            }
        }
    }
    for p in &removal.places {
        if let Err(e) = http.delete_role(GuildId::new(p.server_id), RoleId::new(p.role), Some("VerseEngine: map import")).await {
            if !gone(&e) {
                eprintln!("[import] cannot delete role {}: {e:?}", p.role);
                left.roles += 1;
            }
        }
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(space: SpaceType, id: u64, source: Option<u64>, dest: Option<u64>, members: &[u64]) -> TravelGroup {
        TravelGroup { _id: ObjectId::new(), actual_space_type: space, actual_space_id: id, source_id: source, destination_id: dest, members: members.to_vec(), ..Default::default() }
    }

    #[test]
    fn affected_groups_depend_on_the_mode() {
        let groups = [
            group(SpaceType::Place, 1, None, None, &[10]),          // lieu supprimé
            group(SpaceType::Place, 2, None, None, &[11]),          // lieu gardé
            group(SpaceType::Road, 50, Some(2), Some(3), &[12]),    // route supprimée
            group(SpaceType::Road, 60, Some(2), Some(1), &[13]),    // vers un lieu supprimé
            group(SpaceType::Road, 70, Some(2), Some(3), &[14]),    // route gardée
        ];
        let places: HashSet<u64> = [1].into_iter().collect();
        let roads: HashSet<u64> = [50].into_iter().collect();
        let ids = |v: Vec<&TravelGroup>| v.iter().flat_map(|g| g.members.clone()).collect::<Vec<_>>();
        assert_eq!(ids(affected_groups(&groups, ImportMode::Reimport, &places, &roads)), vec![10, 12, 13]);
        assert_eq!(ids(affected_groups(&groups, ImportMode::Replace, &places, &roads)), vec![10, 11, 12, 13, 14]);
    }

    #[test]
    fn a_relocated_group_is_at_the_start_place_and_no_longer_travels() {
        let mut g = group(SpaceType::Road, 50, Some(2), Some(3), &[12, 13]);
        g.is_in_move = true;
        g.road_id = Some(50);
        g.distance_traveled = 4.0;
        g.step_end_timestamp = Some(99);
        let start = Place { _id: ObjectId::new(), universe_id: ObjectId::new(), server_id: 7, category_id: 900, role: 901, name: "Départ".into(), modifiers: vec![], central_channel_id: None };
        let r = relocate(&g, &start);
        assert_eq!((r.actual_space_type, r.actual_space_id, r.server_id), (SpaceType::Place, 900, 7));
        assert!(!r.is_in_move && r.road_id.is_none() && r.step_end_timestamp.is_none() && r.destination_id.is_none());
        assert_eq!(r.distance_traveled, 0.0);
        assert_eq!((r._id, r.members.clone()), (g._id, vec![12, 13]), "same group, same members");
    }

    fn plan_with(burgs: &[u64]) -> ImportPlan {
        use crate::activity::map::import_plan::{PlaceAction, PlannedPlace};
        ImportPlan {
            places: burgs.iter().map(|&b| PlannedPlace { burg_id: b, name: format!("B{b}"), x: 0.0, y: 0.0, capital: false, action: PlaceAction::Create, server: 0 }).collect(),
            roads: vec![], remove_places: vec![1], remove_roads: vec![], servers: vec![], cut_roads: 0, isolated: vec![],
        }
    }

    fn item(owner: Option<&str>) -> PlacedItemInfo {
        PlacedItemInfo { tool_id: ObjectId::new(), name: "Coffre".into(), place_name: "Bree".into(), owner: owner.map(Into::into), content: 2 }
    }

    #[test]
    fn a_start_place_from_the_new_map_is_required_when_characters_move() {
        let info = RemovalInfo { characters: 2, travelling: 0, items: vec![] };
        let plan = plan_with(&[5, 6]);
        assert_eq!(validate_choices(&info, &RemovalChoices::default(), &plan), Err("activity__import_start_place_required"));
        let bad = RemovalChoices { start_burg: Some(99), ..Default::default() };
        assert_eq!(validate_choices(&info, &bad, &plan), Err("activity__import_start_place_required"));
        let ok = RemovalChoices { start_burg: Some(6), ..Default::default() };
        assert_eq!(validate_choices(&info, &ok, &plan), Ok(()));
        // Personne à déplacer : pas de lieu de départ nécessaire.
        assert_eq!(validate_choices(&RemovalInfo::default(), &RemovalChoices::default(), &plan), Ok(()));
    }

    #[test]
    fn unowned_items_must_be_confirmed_and_can_only_be_destroyed() {
        let unowned = item(None);
        let info = RemovalInfo { characters: 0, travelling: 0, items: vec![item(Some("Aria")), unowned.clone()] };
        let plan = plan_with(&[5]);
        assert_eq!(validate_choices(&info, &RemovalChoices::default(), &plan), Err("activity__import_confirm_unowned"));
        let confirmed = RemovalChoices { confirm_destroy_unowned: true, ..Default::default() };
        assert_eq!(validate_choices(&info, &confirmed, &plan), Ok(()));
        let give_back = RemovalChoices { confirm_destroy_unowned: true, items: vec![ItemChoice { tool_id: unowned.tool_id, action: ItemAction::Return }], ..Default::default() };
        assert_eq!(validate_choices(&info, &give_back, &plan), Err("activity__import_unowned_cannot_return"));
    }

    #[test]
    fn owned_items_are_returned_unless_the_admin_destroys_them() {
        let tool = |owner: Option<ObjectId>| Tool {
            _id: Some(ObjectId::new()), universe_id: ObjectId::new(), server_id: 1, owner_id: owner, category_id: 1, channel_id: 2, area_id: None,
            original_item: ObjectId::new(), name: "Coffre".into(), chained: None, inventory_id: None, inventory_size: 3,
        };
        let owned = tool(Some(ObjectId::new()));
        assert_eq!(item_action(&owned, &RemovalChoices::default()), ItemAction::Return);
        let destroy = RemovalChoices { items: vec![ItemChoice { tool_id: owned._id.unwrap(), action: ItemAction::Destroy }], ..Default::default() };
        assert_eq!(item_action(&owned, &destroy), ItemAction::Destroy);
        assert_eq!(item_action(&tool(None), &RemovalChoices::default()), ItemAction::Destroy);
    }
}
