//! # Service de téléportation partagé
//!
//! Toute la logique de `/teleport player` et `/teleport place`, sans `poise::Context` : la
//! commande slash et l'API de l'activité (mode admin) appellent ces mêmes fonctions, donc une
//! téléportation donne exactement le même résultat quelle que soit son origine.
//!
//! Même déplacement que la suppression de lieux de l'import ([`relocate`]) et mêmes accès qu'à
//! l'arrivée d'un voyage : rôle du lieu d'origine retiré, accès au salon de la route révoqué,
//! rôle du lieu de destination donné. Les écritures en base passent par une transaction.
//!
//! Les erreurs sont des clés Fluent (`teleport__…`), affichées telles quelles par le bot et
//! renvoyées par l'API.

use std::sync::Arc;

use mongodb::bson::oid::ObjectId;
use serenity::all::{ChannelId, CreateMessage, Http};

use crate::activity::import_removal::relocate;
use crate::database::characters::get_character_by_user_id;
use crate::database::places::{get_place_by_category_id, get_places_by_universe_id, Place};
use crate::database::travel::{get_group_of_user, get_groups_at_place, SpaceType, TravelGroup};
use crate::discord::poise_structs::Error;
use crate::place::create_place_sub_command::guild_locale;
use crate::travel::logic::{add_move, manage_roles, remove_move, revoke_road_access};

// ---------------------------------------------------------------------------
// Lieux
// ---------------------------------------------------------------------------

pub async fn all_places(universe_id: ObjectId) -> mongodb::error::Result<Vec<Place>> {
    use futures::TryStreamExt;
    get_places_by_universe_id(universe_id).await?.try_collect().await
}

/// Lieux dont le nom contient `partial` (sans tenir compte de la casse), triés par nom.
pub fn matching_places<'a>(places: &'a [Place], partial: &str) -> Vec<&'a Place> {
    let needle = partial.trim().to_lowercase();
    let mut found: Vec<&Place> = places.iter().filter(|p| p.name.to_lowercase().contains(&needle)).collect();
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found
}

/// Lieu désigné par une saisie : identifiant de catégorie (valeur de l'autocomplétion), mention
/// `<#id>`, ou nom exact (sans tenir compte de la casse) s'il n'y en a qu'un.
pub fn resolve_place<'a>(places: &'a [Place], input: &str) -> Result<&'a Place, &'static str> {
    let input = input.trim();
    let id = input.strip_prefix("<#").and_then(|s| s.strip_suffix('>')).unwrap_or(input);
    if let Ok(id) = id.parse::<u64>() {
        if let Some(p) = places.iter().find(|p| p.category_id == id) {
            return Ok(p);
        }
    }
    let lower = input.to_lowercase();
    let mut named = places.iter().filter(|p| p.name.to_lowercase() == lower);
    match (named.next(), named.next()) {
        (Some(p), None) => Ok(p),
        (Some(_), Some(_)) => Err("teleport__place_ambiguous"),
        (None, _) => Err("teleport__place_not_found"),
    }
}

/// Lieu de l'univers désigné par une saisie (voir [`resolve_place`]).
pub async fn find_place(universe_id: ObjectId, input: &str) -> Result<Place, Error> {
    let places = all_places(universe_id).await.map_err(|_| "teleport__database_error")?;
    Ok(resolve_place(&places, input)?.clone())
}

// ---------------------------------------------------------------------------
// Téléportation
// ---------------------------------------------------------------------------

/// Où se trouvait un joueur avant d'être téléporté : ce qu'il faut lui retirer sur Discord.
#[derive(Debug, Clone, PartialEq)]
pub enum PreviousAccess {
    /// Rôle d'un lieu (sur le serveur de ce lieu).
    PlaceRole { server_id: u64, role_id: u64 },
    /// Permission individuelle sur le salon d'une route.
    Road { channel_id: u64 },
    /// Rien à retirer (lieu supprimé entre-temps, ou aucun groupe).
    Nothing,
}

/// Accès à retirer pour un groupe à sa position actuelle.
async fn previous_access(group: &TravelGroup) -> Result<PreviousAccess, Error> {
    Ok(match group.actual_space_type {
        SpaceType::Place => match get_place_by_category_id(group.universe_id, group.actual_space_id).await.map_err(|_| "teleport__database_error")? {
            Some(p) => PreviousAccess::PlaceRole { server_id: p.server_id, role_id: p.role },
            None => PreviousAccess::Nothing,
        },
        SpaceType::Road => PreviousAccess::Road { channel_id: group.road_id.unwrap_or(group.actual_space_id) },
    })
}

/// Sépare `user_id` de son groupe : (groupe du joueur, reste du groupe s'il y en a un).
/// Le joueur garde l'identifiant du groupe s'il était seul, sinon il en reçoit un nouveau.
pub fn split_member(group: &TravelGroup, user_id: u64) -> (TravelGroup, Option<TravelGroup>) {
    if group.members.len() <= 1 {
        return (group.clone(), None);
    }
    let mut alone = group.clone();
    alone._id = ObjectId::new();
    alone.members = vec![user_id];
    let mut rest = group.clone();
    rest.members.retain(|&m| m != user_id);
    (alone, Some(rest))
}

/// Ce qu'une téléportation d'un joueur doit écrire, calculé sans base ni Discord.
#[derive(Debug, Clone)]
pub struct PlayerMove {
    /// Groupe du joueur, posé à destination.
    pub moved: TravelGroup,
    /// Reste de son ancien groupe (il continue sans lui), s'il n'était pas seul.
    pub rest: Option<TravelGroup>,
    /// Le groupe d'origine voyageait : la file des voyages est à mettre à jour.
    pub was_moving: bool,
}

/// Plan de la téléportation de `user_id` vers `dest`. `group` : son groupe actuel (`None` :
/// personnage jamais placé, il en reçoit un, directement à destination).
pub fn plan_player_move(universe_id: ObjectId, user_id: u64, group: Option<&TravelGroup>, dest: &Place) -> Result<PlayerMove, &'static str> {
    match group {
        None => {
            let g = TravelGroup { _id: ObjectId::new(), universe_id, members: vec![user_id], server_id: dest.server_id, ..Default::default() };
            Ok(PlayerMove { moved: relocate(&g, dest), rest: None, was_moving: false })
        }
        Some(g) => {
            if g.actual_space_type == SpaceType::Place && g.actual_space_id == dest.category_id && !g.is_in_move {
                return Err("teleport__already_there");
            }
            let (alone, rest) = split_member(g, user_id);
            Ok(PlayerMove { moved: relocate(&alone, dest), rest, was_moving: g.is_in_move })
        }
    }
}

/// Écrit des groupes dans une transaction (tout ou rien).
async fn write_groups(groups: &[&TravelGroup]) -> Result<(), Error> {
    let client = crate::database::db_client::get_db_client().await;
    let mut session = client.start_session().await.map_err(|_| "teleport__database_error")?;
    session.start_transaction().await.map_err(|_| "teleport__database_error")?;
    let written: mongodb::error::Result<()> = async {
        for g in groups {
            g.upsert_with_session(&mut session).await?;
        }
        Ok(())
    }
    .await;
    match written {
        Ok(()) => session.commit_transaction().await.map_err(|_| "teleport__database_error")?,
        Err(e) => {
            let _ = session.abort_transaction().await;
            eprintln!("[teleport] database error: {e:?}");
            return Err("teleport__database_error".into());
        }
    }
    Ok(())
}

/// Téléporte un joueur vers `dest`, où qu'il soit (lieu, route, voyage en cours, ou lieu
/// supprimé). S'il voyage en groupe, il le quitte ; les autres continuent. Renvoie le nom du lieu.
///
/// `announce` : annoncer l'arrivée dans le salon générique du lieu (une téléportation groupée
/// l'annonce une seule fois pour tous, voir [`teleport_players`]).
pub async fn teleport_player(http: &Arc<Http>, universe_id: ObjectId, user_id: u64, dest: &Place, announce_arrival: bool) -> Result<String, Error> {
    let group = get_group_of_user(universe_id, user_id).await.map_err(|_| "teleport__database_error")?;
    let plan = plan_player_move(universe_id, user_id, group.as_ref(), dest)?;
    let previous = match &group {
        Some(g) => previous_access(g).await?,
        None => PreviousAccess::Nothing,
    };

    // Base : le joueur et, s'il a quitté un groupe, le reste du groupe, dans une transaction.
    let mut groups = vec![&plan.moved];
    groups.extend(plan.rest.as_ref());
    write_groups(&groups).await?;

    // File des voyages : le groupe quitte la file ; s'il reste des voyageurs, ils y reviennent.
    if plan.was_moving {
        remove_move(user_id).await;
        if let Some(rest) = plan.rest.filter(|r| r.is_in_move) {
            add_move(rest).await;
        }
    }

    apply_access(http, user_id, &previous, dest).await;
    if announce_arrival {
        announce(http, universe_id, &[user_id], dest).await;
    }
    Ok(dest.name.clone())
}

/// Résultat de la téléportation d'un joueur parmi plusieurs.
#[derive(Debug)]
pub struct TeleportOutcome {
    pub user_id: u64,
    /// `Ok` : nom du lieu ; `Err` : clé Fluent de l'erreur (les autres joueurs sont quand même
    /// téléportés, comme si l'admin avait lancé la commande slash pour chacun).
    pub result: Result<String, String>,
}

/// Téléporte plusieurs joueurs vers `dest`, un par un (mêmes règles que `/teleport player`), puis
/// annonce en un seul message ceux qui sont arrivés.
pub async fn teleport_players(http: &Arc<Http>, universe_id: ObjectId, user_ids: &[u64], dest: &Place) -> Vec<TeleportOutcome> {
    let mut outcomes = Vec::with_capacity(user_ids.len());
    for &user_id in user_ids {
        let result = teleport_player(http, universe_id, user_id, dest, false).await.map_err(|e| e.to_string());
        outcomes.push(TeleportOutcome { user_id, result });
    }
    let arrived: Vec<u64> = outcomes.iter().filter(|o| o.result.is_ok()).map(|o| o.user_id).collect();
    announce(http, universe_id, &arrived, dest).await;
    outcomes
}

/// Téléporte tous les groupes arrêtés dans `from` vers `dest`. Renvoie le nombre de joueurs.
pub async fn teleport_place(http: &Arc<Http>, universe_id: ObjectId, from: &Place, dest: &Place) -> Result<i64, Error> {
    if from.category_id == dest.category_id {
        return Err("teleport__same_place".into());
    }
    let groups = get_groups_at_place(universe_id, from.category_id).await.map_err(|_| "teleport__database_error")?;
    if groups.is_empty() {
        return Err("teleport__nobody_here".into());
    }
    let moved: Vec<TravelGroup> = groups.iter().map(|g| relocate(g, dest)).collect();
    write_groups(&moved.iter().collect::<Vec<_>>()).await?;

    let previous = PreviousAccess::PlaceRole { server_id: from.server_id, role_id: from.role };
    let members: Vec<u64> = moved.iter().flat_map(|g| g.members.iter().copied()).collect();
    for &user_id in &members {
        apply_access(http, user_id, &previous, dest).await;
    }
    announce(http, universe_id, &members, dest).await;
    Ok(members.len() as i64)
}

/// Accès Discord : retire celui de l'ancienne position, donne le rôle du lieu de destination.
/// Les échecs sont journalisés par `manage_roles` ; la position en base fait foi.
async fn apply_access(http: &Arc<Http>, user_id: u64, previous: &PreviousAccess, dest: &Place) {
    match previous {
        PreviousAccess::PlaceRole { server_id, role_id } if *server_id == dest.server_id => {
            manage_roles(http.clone(), dest.server_id, user_id, Some(dest.role), Some(*role_id)).await;
            return;
        }
        PreviousAccess::PlaceRole { server_id, role_id } => {
            manage_roles(http.clone(), *server_id, user_id, None, Some(*role_id)).await;
        }
        PreviousAccess::Road { channel_id } => revoke_road_access(http, *channel_id, user_id).await,
        PreviousAccess::Nothing => {}
    }
    manage_roles(http.clone(), dest.server_id, user_id, Some(dest.role), None).await;
}

/// Annonce l'arrivée dans le salon générique du lieu, s'il en a un.
async fn announce(http: &Http, universe_id: ObjectId, members: &[u64], dest: &Place) {
    let Some(channel) = dest.central_channel_id else { return };
    if members.is_empty() {
        return;
    }
    let locale = guild_locale(http, dest.server_id).await;
    let mut names = Vec::with_capacity(members.len());
    for &user_id in members {
        names.push(match get_character_by_user_id(universe_id, user_id).await {
            Ok(Some(c)) => c.name,
            _ => format!("<@{user_id}>"),
        });
    }
    let msg = crate::tr_locale!(locale.as_str(), "teleport__arrived", users: names.join(", "), count: names.len() as i64);
    if let Err(e) = ChannelId::new(channel).send_message(http, CreateMessage::new().content(msg)).await {
        eprintln!("[teleport] cannot announce in {channel}: {e:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(id: u64, name: &str) -> Place {
        Place { _id: ObjectId::new(), universe_id: ObjectId::new(), server_id: 1, category_id: id, role: id + 1, name: name.into(), modifiers: vec![], central_channel_id: None, wiki: Default::default() }
    }

    #[test]
    fn a_place_is_found_by_id_mention_or_unique_name() {
        let places = vec![place(10, "Port Royal"), place(20, "Forêt"), place(30, "forêt")];
        assert_eq!(resolve_place(&places, "10").unwrap().category_id, 10);
        assert_eq!(resolve_place(&places, "<#20>").unwrap().category_id, 20);
        assert_eq!(resolve_place(&places, "port royal").unwrap().category_id, 10);
        assert_eq!(resolve_place(&places, "Forêt").unwrap_err(), "teleport__place_ambiguous");
        assert_eq!(resolve_place(&places, "Atlantide").unwrap_err(), "teleport__place_not_found");
        assert_eq!(resolve_place(&places, "999").unwrap_err(), "teleport__place_not_found");
    }

    #[test]
    fn autocomplete_matches_any_part_of_the_name_sorted() {
        let places = vec![place(1, "Zeta"), place(2, "Port Alpha"), place(3, "alpine")];
        let names: Vec<&str> = matching_places(&places, "ALP").iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["alpine", "Port Alpha"]);
        assert_eq!(matching_places(&places, "").len(), 3);
    }

    #[test]
    fn a_member_leaves_a_group_and_the_others_keep_it() {
        let g = TravelGroup { _id: ObjectId::new(), members: vec![1, 2, 3], is_in_move: true, ..Default::default() };
        let (alone, rest) = split_member(&g, 2);
        assert_eq!(alone.members, vec![2]);
        assert_ne!(alone._id, g._id);
        let rest = rest.unwrap();
        assert_eq!(rest._id, g._id);
        assert_eq!(rest.members, vec![1, 3]);
        assert!(rest.is_in_move);
    }

    #[test]
    fn a_lone_player_keeps_his_group() {
        let g = TravelGroup { _id: ObjectId::new(), members: vec![7], ..Default::default() };
        let (alone, rest) = split_member(&g, 7);
        assert_eq!(alone._id, g._id);
        assert!(rest.is_none());
    }

    #[test]
    fn a_player_without_group_gets_one_at_the_destination() {
        let universe = ObjectId::new();
        let dest = place(40, "Port");
        let plan = plan_player_move(universe, 9, None, &dest).unwrap();
        assert_eq!(plan.moved.members, vec![9]);
        assert_eq!((plan.moved.actual_space_type.clone(), plan.moved.actual_space_id), (SpaceType::Place, 40));
        assert_eq!(plan.moved.universe_id, universe);
        assert!(plan.rest.is_none() && !plan.was_moving);
    }

    #[test]
    fn a_travelling_player_leaves_his_group_which_keeps_moving() {
        let dest = place(40, "Port");
        let g = TravelGroup { _id: ObjectId::new(), members: vec![1, 2], is_in_move: true, actual_space_type: SpaceType::Road, actual_space_id: 77, road_id: Some(77), ..Default::default() };
        let plan = plan_player_move(ObjectId::new(), 2, Some(&g), &dest).unwrap();
        assert!(plan.was_moving);
        assert_eq!(plan.moved.members, vec![2]);
        assert!(!plan.moved.is_in_move && plan.moved.road_id.is_none());
        assert_eq!(plan.moved.actual_space_id, 40);
        let rest = plan.rest.unwrap();
        assert_eq!((rest.members.clone(), rest.is_in_move, rest.actual_space_id), (vec![1], true, 77));
    }

    #[test]
    fn a_player_already_stopped_at_the_destination_is_refused() {
        let dest = place(40, "Port");
        let here = TravelGroup { _id: ObjectId::new(), members: vec![1], actual_space_type: SpaceType::Place, actual_space_id: 40, ..Default::default() };
        assert_eq!(plan_player_move(ObjectId::new(), 1, Some(&here), &dest).unwrap_err(), "teleport__already_there");
        // En route vers ce lieu, mais pas encore arrivé : téléporté.
        let leaving = TravelGroup { is_in_move: true, ..here };
        assert!(plan_player_move(ObjectId::new(), 1, Some(&leaving), &dest).is_ok());
    }
}
