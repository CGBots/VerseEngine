//! Butin des fouilles terminées, gardé un court moment pour l'activité : à la fin d'une fouille,
//! le client relit l'état du farm (`farm_updated`) et affiche ce qui a été obtenu.
//! En mémoire seulement : après un redémarrage, le message privé du bot reste la trace.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use mongodb::bson::oid::ObjectId;

/// Durée pendant laquelle le résultat reste lisible (s).
pub const KEEP_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub at: u64,
    /// (nom de l'objet, quantité) réellement donnés.
    pub items: Vec<(String, u64)>,
}

type Store = Mutex<HashMap<(ObjectId, u64), Finished>>;

fn store() -> &'static Store {
    static STORE: OnceLock<Store> = OnceLock::new();
    STORE.get_or_init(Default::default)
}

pub fn record(universe: ObjectId, user: u64, mut items: Vec<(String, u64)>, at: u64) {
    items.sort();
    let mut map = store().lock().unwrap_or_else(|p| p.into_inner());
    map.retain(|_, f| at.saturating_sub(f.at) < KEEP_SECS);
    map.insert((universe, user), Finished { at, items });
}

pub fn recent(universe: ObjectId, user: u64, now: u64) -> Option<Finished> {
    let map = store().lock().unwrap_or_else(|p| p.into_inner());
    map.get(&(universe, user)).filter(|f| now.saturating_sub(f.at) < KEEP_SECS).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finished_farm_is_readable_for_a_short_while() {
        let u = ObjectId::new();
        record(u, 1, vec![("Pomme".into(), 2), ("Bois".into(), 1)], 1000);
        assert_eq!(recent(u, 1, 1010), Some(Finished { at: 1000, items: vec![("Bois".into(), 1), ("Pomme".into(), 2)] }));
        assert_eq!(recent(u, 2, 1010), None);
        assert_eq!(recent(u, 1, 1000 + KEEP_SECS), None);
    }
}
