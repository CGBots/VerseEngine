//! Invitations à rejoindre un groupe de voyage, envoyées depuis l'activité (« Créer un groupe »).
//!
//! Accepter une invitation, c'est rejoindre le groupe de l'invitant avec le service de
//! `/travel_join` (`travel::service::join`), qui revérifie tout (même position, occupation…).
//! Les invitations vivent en mémoire : elles expirent et ne survivent pas à un redémarrage.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use mongodb::bson::oid::ObjectId;

use crate::database::travel::TravelGroup;

/// Durée de validité d'une invitation, en secondes.
pub const INVITE_TTL_SECS: u64 = 15 * 60;
/// Invitations en attente par joueur invité (les plus anciennes tombent).
const MAX_PER_INVITEE: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    pub from: u64,
    pub at: u64,
}

type Store = Mutex<HashMap<(ObjectId, u64), Vec<Invite>>>;

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

/// Enregistre l'invitation de `from` à `to` (une seule par invitant, rafraîchie).
pub fn invite(universe: ObjectId, from: u64, to: u64, now: u64) {
    let mut map = store().lock().unwrap_or_else(|p| p.into_inner());
    let list = map.entry((universe, to)).or_default();
    list.retain(|i| i.from != from && now.saturating_sub(i.at) < INVITE_TTL_SECS);
    list.push(Invite { from, at: now });
    if list.len() > MAX_PER_INVITEE {
        let extra = list.len() - MAX_PER_INVITEE;
        list.drain(..extra);
    }
}

/// Invitations encore valides reçues par `to`.
pub fn pending(universe: ObjectId, to: u64, now: u64) -> Vec<Invite> {
    let mut map = store().lock().unwrap_or_else(|p| p.into_inner());
    let Some(list) = map.get_mut(&(universe, to)) else { return vec![] };
    list.retain(|i| now.saturating_sub(i.at) < INVITE_TTL_SECS);
    list.clone()
}

/// Retire l'invitation de `from` à `to` ; vrai si elle existait (et était valide).
pub fn take(universe: ObjectId, from: u64, to: u64, now: u64) -> bool {
    let mut map = store().lock().unwrap_or_else(|p| p.into_inner());
    let Some(list) = map.get_mut(&(universe, to)) else { return false };
    let before = list.iter().any(|i| i.from == from && now.saturating_sub(i.at) < INVITE_TTL_SECS);
    list.retain(|i| i.from != from);
    before
}

/// Deux groupes au même endroit (même lieu, ou même route) : condition de `/travel_join`.
pub fn same_spot(a: &TravelGroup, b: &TravelGroup) -> bool {
    a.actual_space_id == b.actual_space_id && a.actual_space_type == b.actual_space_type
}

/// Groupes à la même position que `mine`, hors le sien.
pub fn nearby<'a>(mine: &TravelGroup, all: &'a [TravelGroup]) -> Vec<&'a TravelGroup> {
    all.iter().filter(|g| g._id != mine._id && same_spot(mine, g) && !g.members.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::travel::SpaceType;

    #[test]
    fn invitations_expire_are_refreshed_and_taken_once() {
        let u = ObjectId::new();
        invite(u, 1, 2, 100);
        invite(u, 1, 2, 200); // rafraîchie, pas dupliquée
        invite(u, 3, 2, 150);
        assert_eq!(pending(u, 2, 300).iter().map(|i| (i.from, i.at)).collect::<Vec<_>>(), vec![(1, 200), (3, 150)]);
        assert!(pending(u, 2, 150 + INVITE_TTL_SECS).iter().all(|i| i.from == 1), "expired invitations disappear");
        assert!(take(u, 1, 2, 300));
        assert!(!take(u, 1, 2, 300), "an invitation is used once");
        assert!(!take(u, 9, 2, 300));
    }

    #[test]
    fn nearby_groups_share_the_same_place_or_road() {
        let group = |space: u64, kind: SpaceType, members: Vec<u64>| TravelGroup { _id: ObjectId::new(), actual_space_id: space, actual_space_type: kind, members, ..Default::default() };
        let mine = group(10, SpaceType::Place, vec![1]);
        let all = vec![mine.clone(), group(10, SpaceType::Place, vec![2, 3]), group(10, SpaceType::Road, vec![4]), group(11, SpaceType::Place, vec![5])];
        let found: Vec<Vec<u64>> = nearby(&mine, &all).into_iter().map(|g| g.members.clone()).collect();
        assert_eq!(found, vec![vec![2, 3]]);
    }
}
