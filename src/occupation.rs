//! # Occupations exclusives d'un joueur
//!
//! Voyage, craft et farm (fouille, `loot`) s'excluent : un joueur ne fait qu'une chose à la fois.
//! La règle est appliquée dans les services partagés (`travel::service`, `craft::service`,
//! `loot::service`), donc à l'identique pour les commandes slash et pour l'activité : l'interface
//! ne fait que la refléter.
//!
//! Les erreurs sont les clés Fluent déjà utilisées par les commandes (`recipe__…`,
//! `loot_table__…`, `travel__…`), pour que le joueur lise le même texte partout.

use mongodb::bson::oid::ObjectId;
use once_cell::sync::Lazy;

use crate::database::craft::PlayerCraft;
use crate::database::loot::PlayerLoot;
use crate::database::travel::get_group_of_user;
use crate::discord::poise_structs::Error;

/// Ce que le joueur veut commencer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occupation {
    Travel,
    Craft,
    Farm,
}

/// Ce que le joueur fait déjà.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Busy {
    /// Groupe en route (`is_in_move`). Un groupe arrêté au milieu d'une route ne voyage pas.
    pub travelling: bool,
    pub crafting: bool,
    pub farming: bool,
}

impl Busy {
    /// Peut-on commencer `wanted` ? Sinon, clé Fluent de l'erreur.
    ///
    /// Un voyage déjà en cours n'empêche pas de « repartir » : le service de voyage gère lui-même
    /// la poursuite ou le demi-tour sur la route.
    pub fn check(&self, wanted: Occupation) -> Result<(), &'static str> {
        let refusal = match wanted {
            Occupation::Craft => [
                (self.travelling, "recipe__cannot_craft_while_moving"),
                (self.crafting, "recipe__craft_already_in_progress"),
                (self.farming, "recipe__cannot_craft_while_farming"),
            ],
            Occupation::Farm => [
                (self.travelling, "loot_table__already_moving"),
                (self.crafting, "loot_table__already_crafting"),
                (self.farming, "loot_table__already_looting"),
            ],
            Occupation::Travel => [
                (false, ""),
                (self.crafting, "travel__cannot_move_while_crafting"),
                (self.farming, "travel__cannot_move_while_farming"),
            ],
        };
        match refusal.iter().find(|(busy, _)| *busy) {
            Some((_, key)) => Err(key),
            None => Ok(()),
        }
    }

    /// Occupé à autre chose que voyager (craft ou farm) : un tel joueur ne peut pas être emmené
    /// par son groupe.
    pub fn is_working(&self) -> bool {
        self.crafting || self.farming
    }
}

/// Lit l'état du joueur en base.
pub async fn busy(universe_id: ObjectId, user_id: u64) -> mongodb::error::Result<Busy> {
    let (group, craft, loot) = tokio::join!(
        get_group_of_user(universe_id, user_id),
        PlayerCraft::get_by_user_id(universe_id, user_id),
        PlayerLoot::get_by_user_id(universe_id, user_id),
    );
    Ok(Busy { travelling: group?.is_some_and(|g| g.is_in_move), crafting: craft?.is_some(), farming: loot?.is_some() })
}

/// Refuse `wanted` si le joueur est déjà occupé.
pub async fn ensure_free(universe_id: ObjectId, user_id: u64, wanted: Occupation) -> Result<(), Error> {
    busy(universe_id, user_id).await?.check(wanted).map_err(Error::from)
}

/// Refuse un départ si un membre du groupe crafte ou farme : le voyage l'emmènerait avec lui.
/// `actor` reçoit l'erreur précise ; pour un autre membre, `travel__group_member_busy`.
pub async fn ensure_group_can_travel(universe_id: ObjectId, actor: u64, members: &[u64]) -> Result<(), Error> {
    busy(universe_id, actor).await?.check(Occupation::Travel)?;
    for member in members.iter().filter(|m| **m != actor) {
        if busy(universe_id, *member).await?.is_working() {
            return Err("travel__group_member_busy".into());
        }
    }
    Ok(())
}

/// Verrous par joueur, répartis sur un nombre fixe de mutex (mémoire bornée).
const STRIPES: usize = 64;
static LOCKS: Lazy<Vec<tokio::sync::Mutex<()>>> = Lazy::new(|| (0..STRIPES).map(|_| tokio::sync::Mutex::new(())).collect());

fn stripe(universe_id: ObjectId, user_id: u64) -> usize {
    let bytes = universe_id.bytes();
    let u = u64::from_le_bytes(bytes[4..12].try_into().unwrap_or_default());
    ((u ^ user_id.rotate_left(17)) % STRIPES as u64) as usize
}

/// Sérialise les débuts d'action d'un même joueur dans ce processus : sans lui, un craft lancé
/// par commande slash et un farm lancé depuis l'activité au même instant passeraient tous deux la
/// vérification. Tenu seulement le temps de vérifier puis d'écrire ; ne jamais le reprendre
/// pendant qu'on le tient (le mutex n'est pas réentrant).
pub async fn lock(universe_id: ObjectId, user_id: u64) -> tokio::sync::MutexGuard<'static, ()> {
    LOCKS[stripe(universe_id, user_id)].lock().await
}

#[cfg(test)]
mod tests {
    use super::*;

    const FREE: Busy = Busy { travelling: false, crafting: false, farming: false };

    #[test]
    fn a_free_player_may_start_anything() {
        for wanted in [Occupation::Travel, Occupation::Craft, Occupation::Farm] {
            assert_eq!(FREE.check(wanted), Ok(()), "{wanted:?}");
        }
    }

    #[test]
    fn travelling_blocks_craft_and_farm_but_not_a_new_departure() {
        let b = Busy { travelling: true, ..FREE };
        assert_eq!(b.check(Occupation::Craft), Err("recipe__cannot_craft_while_moving"));
        assert_eq!(b.check(Occupation::Farm), Err("loot_table__already_moving"));
        assert_eq!(b.check(Occupation::Travel), Ok(()), "continuing or turning back is the travel service's call");
    }

    #[test]
    fn crafting_blocks_everything() {
        let b = Busy { crafting: true, ..FREE };
        assert_eq!(b.check(Occupation::Craft), Err("recipe__craft_already_in_progress"));
        assert_eq!(b.check(Occupation::Farm), Err("loot_table__already_crafting"));
        assert_eq!(b.check(Occupation::Travel), Err("travel__cannot_move_while_crafting"));
    }

    #[test]
    fn farming_blocks_everything() {
        let b = Busy { farming: true, ..FREE };
        assert_eq!(b.check(Occupation::Craft), Err("recipe__cannot_craft_while_farming"));
        assert_eq!(b.check(Occupation::Farm), Err("loot_table__already_looting"));
        assert_eq!(b.check(Occupation::Travel), Err("travel__cannot_move_while_farming"));
    }

    #[test]
    fn travelling_is_reported_first_like_the_slash_commands() {
        let b = Busy { travelling: true, crafting: true, farming: true };
        assert_eq!(b.check(Occupation::Craft), Err("recipe__cannot_craft_while_moving"));
        assert_eq!(b.check(Occupation::Farm), Err("loot_table__already_moving"));
    }

    #[test]
    fn only_craft_and_farm_count_as_work() {
        assert!(!Busy { travelling: true, ..FREE }.is_working());
        assert!(Busy { crafting: true, ..FREE }.is_working());
        assert!(Busy { farming: true, ..FREE }.is_working());
    }

    #[test]
    fn the_same_player_always_takes_the_same_lock() {
        let u = ObjectId::new();
        assert_eq!(stripe(u, 42), stripe(u, 42));
        assert!(stripe(u, 42) < STRIPES);
    }

    #[tokio::test]
    async fn a_second_start_waits_for_the_first() {
        let u = ObjectId::new();
        let first = lock(u, 7).await;
        assert!(LOCKS[stripe(u, 7)].try_lock().is_err());
        drop(first);
        assert!(LOCKS[stripe(u, 7)].try_lock().is_ok());
    }
}
