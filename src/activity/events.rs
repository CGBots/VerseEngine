//! Temps réel : change streams MongoDB → WebSocket.
//!
//! Un seul change stream par collection pour tout le processus. Chaque changement devient un
//! [`Event`] diffusé par un `tokio::sync::broadcast` ; chaque WebSocket ne garde que ce que son
//! joueur a le droit de voir ([`Event::is_visible_to`]).
//!
//! Comme les change streams voient **toutes** les écritures, une action faite par commande slash
//! est poussée exactement comme une action faite dans l'activité, sans toucher au code du bot.
//!
//! Les évènements ne transportent pas de données de jeu : ils disent seulement quoi relire
//! (`GET /api/me/travel`, `/api/map`…). Les réponses HTTP appliquent déjà le filtrage (routes
//! secrètes…), qu'il n'y a donc pas à dupliquer ici.

use std::time::Duration;

use futures::StreamExt;
use mongodb::bson::{doc, oid::ObjectId, Document};
use mongodb::change_stream::event::{ChangeStreamEvent, OperationType, ResumeToken};
use mongodb::options::FullDocumentType;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::activity::permissions::AccessLevel;
use crate::activity::session::Session;
use crate::database::db_namespace::*;

/// Ce qu'un client doit relire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Groupe de voyage modifié (départ, étape, arrivée, arrêt, groupe rejoint/quitté).
    TravelUpdated,
    /// Fiche de personnage modifiée (statuts, description, image…).
    CharacterUpdated,
    /// Lieux, routes ou carte de l'univers modifiés.
    MapUpdated,
    /// Inventaire du joueur modifié (et donc les recettes réalisables).
    InventoryUpdated,
    /// Craft du joueur lancé, terminé ou arrêté.
    CraftUpdated,
    /// Farm (fouille) du joueur lancé, terminé ou arrêté.
    FarmUpdated,
    /// Le bot a rejoint ou quitté un serveur : la liste des serveurs de l'import est à relire
    /// (administrateurs seulement).
    ServersUpdated,
    /// Bibliothèque musicale de l'univers modifiée : les playlists sont à relire.
    MusicUpdated,
    /// Des évènements ont pu être perdus : tout relire.
    Resync,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    /// `None` : concerne tous les univers (resynchronisation globale).
    pub universe_id: Option<ObjectId>,
    /// Joueurs concernés. Vide : tout l'univers.
    pub users: Vec<u64>,
}

impl Event {
    /// Un joueur ne reçoit que ce qui concerne son groupe ou sa fiche ; un modérateur ou un
    /// administrateur reçoit aussi les déplacements de tous les joueurs de l'univers.
    pub fn is_visible_to(&self, session: &Session) -> bool {
        if let Some(universe) = self.universe_id {
            if Some(universe) != session.universe_id {
                return false;
            }
        }
        match self.kind {
            EventKind::MapUpdated | EventKind::MusicUpdated | EventKind::Resync => true,
            EventKind::ServersUpdated => session.access == AccessLevel::Admin,
            EventKind::CharacterUpdated | EventKind::InventoryUpdated | EventKind::CraftUpdated | EventKind::FarmUpdated => {
                self.users.contains(&session.user_id)
            }
            EventKind::TravelUpdated => self.users.contains(&session.user_id) || session.access >= AccessLevel::Moderator,
        }
    }
}

/// Lit un identifiant Discord stocké en chaîne (ou en entier) dans un document.
fn snowflake(value: Option<&mongodb::bson::Bson>) -> Option<u64> {
    match value? {
        mongodb::bson::Bson::String(s) => s.parse().ok(),
        mongodb::bson::Bson::Int64(i) => u64::try_from(*i).ok(),
        mongodb::bson::Bson::Int32(i) => u64::try_from(*i).ok(),
        _ => None,
    }
}

/// Traduit un changement en évènement. `None` si le changement n'intéresse pas les clients.
///
/// Les suppressions sont ignorées : le document supprimé n'est plus lisible, et chaque
/// suppression de groupe (`/travel_join`) s'accompagne de la mise à jour du groupe d'arrivée,
/// qui contient les mêmes joueurs.
pub fn event_from_change(collection: &str, operation: &OperationType, full_document: Option<&Document>) -> Option<Event> {
    if !matches!(operation, OperationType::Insert | OperationType::Update | OperationType::Replace) {
        return None;
    }
    let doc = full_document?;
    let universe_id = doc.get_object_id("universe_id").ok()?;
    let (kind, users) = match collection {
        c if c == TRAVELS_COLLECTION_NAME => {
            let members = doc.get_array("members").ok()?.iter().filter_map(|m| snowflake(Some(m))).collect();
            (EventKind::TravelUpdated, members)
        }
        c if c == CHARACTERS_COLLECTION_NAME => (EventKind::CharacterUpdated, vec![snowflake(doc.get("user_id"))?]),
        c if c == PLACES_COLLECTION_NAME || c == ROADS_COLLECTION_NAME || c == UNIVERSE_MAPS_COLLECTION_NAME => (EventKind::MapUpdated, vec![]),
        _ => return None,
    };
    Some(Event { kind, universe_id: Some(universe_id), users })
}

/// Ligne d'inventaire d'un personnage : `(univers, personnage)`. Le document ne nomme pas le
/// joueur : il faut ensuite lire la fiche du personnage ([`inventory_event`]).
pub fn inventory_holder(operation: &OperationType, full_document: Option<&Document>) -> Option<(ObjectId, ObjectId)> {
    if !matches!(operation, OperationType::Insert | OperationType::Update | OperationType::Replace) {
        return None;
    }
    let doc = full_document?;
    let holder = doc.get_document("holder").ok()?;
    if holder.get_str("holder_type").ok()? != "Character" {
        return None;
    }
    Some((doc.get_object_id("universe_id").ok()?, holder.get_object_id("holder_id").ok()?))
}

/// Évènement d'un changement d'inventaire, adressé au joueur du personnage.
async fn inventory_event(operation: &OperationType, full_document: Option<&Document>) -> Option<Event> {
    let (universe_id, character_id) = inventory_holder(operation, full_document)?;
    let character = crate::database::characters::get_character_by_id(character_id).await.ok().flatten()?;
    Some(Event { kind: EventKind::InventoryUpdated, universe_id: Some(universe_id), users: vec![character.user_id] })
}

/// Collections suivies.
pub const WATCHED: [&str; 6] = [
    TRAVELS_COLLECTION_NAME,
    CHARACTERS_COLLECTION_NAME,
    PLACES_COLLECTION_NAME,
    ROADS_COLLECTION_NAME,
    UNIVERSE_MAPS_COLLECTION_NAME,
    INVENTORY_COLLECTION_NAME,
];

/// Diffuseur de l'API démarrée, pour les évènements qui viennent du bot (pas de la base).
static ACTIVE_HUB: std::sync::OnceLock<Hub> = std::sync::OnceLock::new();

/// À l'arrivée ou au départ du bot d'un serveur : les écrans d'import ouverts relisent la liste
/// des serveurs (sans interroger Discord en boucle). Sans activité démarrée, ne fait rien.
pub fn notify_servers_changed() {
    if let Some(hub) = ACTIVE_HUB.get() {
        hub.publish(Event { kind: EventKind::ServersUpdated, universe_id: None, users: vec![] });
    }
}

/// Prévient un joueur qu'une de ses occupations (craft, farm) a changé. Appelé par les files du
/// bot (`craft::logic`, `loot::logic`) : la suppression d'un craft ou d'un farm terminé n'est pas
/// visible dans un change stream (document supprimé). Sans activité démarrée, ne fait rien.
pub fn notify_player(kind: EventKind, universe_id: ObjectId, user_id: u64) {
    if let Some(hub) = ACTIVE_HUB.get() {
        hub.publish(Event { kind, universe_id: Some(universe_id), users: vec![user_id] });
    }
}

/// Diffuseur d'évènements, partagé par toutes les WebSockets.
#[derive(Clone)]
pub struct Hub {
    sender: broadcast::Sender<Event>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        Self { sender: broadcast::channel(1024).0 }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: Event) {
        // Aucune WebSocket ouverte : rien à faire.
        let _ = self.sender.send(event);
    }

    /// Lance un change stream par collection suivie, et reçoit les évènements du bot.
    pub fn start_watchers(&self) {
        let _ = ACTIVE_HUB.set(self.clone());
        for collection in WATCHED {
            let hub = self.clone();
            tokio::spawn(async move { hub.watch(collection).await });
        }
    }

    /// Suit une collection. En cas de coupure, reprend au dernier `resume_token` ; si la reprise
    /// échoue, repart de zéro et demande aux clients de tout relire.
    async fn watch(&self, collection: &'static str) {
        let mut resume: Option<ResumeToken> = None;
        let mut backoff = Duration::from_secs(1);
        loop {
            let coll = crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<Document>(collection);
            let pipeline = [doc! {"$match": {"operationType": {"$in": ["insert", "update", "replace"]}}}];
            let mut watch = coll.watch().pipeline(pipeline).full_document(FullDocumentType::UpdateLookup);
            if let Some(token) = resume.clone() {
                watch = watch.resume_after(token);
            }
            match watch.await {
                Ok(mut stream) => {
                    backoff = Duration::from_secs(1);
                    tracing::info!(collection, "activity: change stream ouvert");
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(change) => {
                                let change: ChangeStreamEvent<Document> = change;
                                resume = Some(change.id.clone());
                                let event = if collection == INVENTORY_COLLECTION_NAME {
                                    inventory_event(&change.operation_type, change.full_document.as_ref()).await
                                } else {
                                    event_from_change(collection, &change.operation_type, change.full_document.as_ref())
                                };
                                if let Some(event) = event {
                                    self.publish(event);
                                }
                            }
                            Err(e) => {
                                tracing::warn!(collection, error = %e, "activity: change stream interrompu");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(collection, error = %e, "activity: impossible d'ouvrir le change stream");
                    if resume.take().is_some() {
                        // Le jeton n'est plus valable (historique de l'oplog dépassé) : des
                        // changements ont pu être manqués.
                        self.publish(Event { kind: EventKind::Resync, universe_id: None, users: vec![] });
                    }
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }
}

#[cfg(test)]
mod servers_tests {
    use super::*;
    use crate::activity::session::test_session;

    #[test]
    fn only_admins_are_told_that_servers_changed() {
        let e = Event { kind: EventKind::ServersUpdated, universe_id: None, users: vec![] };
        let mut s = test_session(1);
        s.universe_id = None;
        assert!(!e.is_visible_to(&s));
        s.access = AccessLevel::Admin;
        assert!(e.is_visible_to(&s), "even without universe");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::session::test_session;

    fn session(user_id: u64, universe_id: ObjectId, access: AccessLevel) -> Session {
        Session { universe_id: Some(universe_id), access, ..test_session(user_id) }
    }

    #[test]
    fn a_travel_change_names_its_members() {
        let u = ObjectId::new();
        let doc = doc! {"universe_id": u, "members": ["11", "12"]};
        let e = event_from_change(TRAVELS_COLLECTION_NAME, &OperationType::Update, Some(&doc)).unwrap();
        assert_eq!(e, Event { kind: EventKind::TravelUpdated, universe_id: Some(u), users: vec![11, 12] });
    }

    #[test]
    fn a_character_change_names_its_owner() {
        let u = ObjectId::new();
        let doc = doc! {"universe_id": u, "user_id": "42", "name": "x"};
        let e = event_from_change(CHARACTERS_COLLECTION_NAME, &OperationType::Replace, Some(&doc)).unwrap();
        assert_eq!((e.kind, e.users), (EventKind::CharacterUpdated, vec![42]));
    }

    #[test]
    fn world_changes_concern_the_whole_universe() {
        let u = ObjectId::new();
        for c in [PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME, UNIVERSE_MAPS_COLLECTION_NAME] {
            let e = event_from_change(c, &OperationType::Insert, Some(&doc! {"universe_id": u})).unwrap();
            assert_eq!((e.kind, e.users.len()), (EventKind::MapUpdated, 0), "{c}");
        }
    }

    #[test]
    fn deletes_unknown_collections_and_malformed_documents_are_ignored() {
        let u = ObjectId::new();
        let doc = doc! {"universe_id": u, "members": ["1"]};
        assert!(event_from_change(TRAVELS_COLLECTION_NAME, &OperationType::Delete, Some(&doc)).is_none());
        assert!(event_from_change(TRAVELS_COLLECTION_NAME, &OperationType::Update, None).is_none());
        assert!(event_from_change("items", &OperationType::Update, Some(&doc)).is_none());
        assert!(event_from_change(TRAVELS_COLLECTION_NAME, &OperationType::Update, Some(&doc! {"members": ["1"]})).is_none());
    }

    #[test]
    fn an_inventory_line_names_its_character() {
        let (u, c) = (ObjectId::new(), ObjectId::new());
        let doc = doc! {"universe_id": u, "holder": {"holder_type": "Character", "holder_id": c}, "quantity": 3};
        assert_eq!(inventory_holder(&OperationType::Update, Some(&doc)), Some((u, c)));
        let in_a_chest = doc! {"universe_id": u, "holder": {"holder_type": "Item", "holder_id": c}};
        assert_eq!(inventory_holder(&OperationType::Update, Some(&in_a_chest)), None);
        assert_eq!(inventory_holder(&OperationType::Delete, Some(&doc)), None);
        // Le change stream générique l'ignore : il faut lire la fiche pour trouver le joueur.
        assert!(event_from_change(INVENTORY_COLLECTION_NAME, &OperationType::Update, Some(&doc)).is_none());
    }

    #[test]
    fn occupation_events_reach_their_player_only() {
        let u = ObjectId::new();
        let me = session(1, u, AccessLevel::Player);
        let moderator = session(2, u, AccessLevel::Moderator);
        for kind in [EventKind::InventoryUpdated, EventKind::CraftUpdated, EventKind::FarmUpdated] {
            let e = Event { kind, universe_id: Some(u), users: vec![1] };
            assert!(e.is_visible_to(&me), "{kind:?}");
            assert!(!e.is_visible_to(&moderator), "{kind:?}: not even a moderator");
        }
    }

    #[test]
    fn a_player_only_sees_their_own_group_and_character() {
        let u = ObjectId::new();
        let me = session(1, u, AccessLevel::Player);
        let travel = |users| Event { kind: EventKind::TravelUpdated, universe_id: Some(u), users };
        assert!(travel(vec![1, 2]).is_visible_to(&me));
        assert!(!travel(vec![2]).is_visible_to(&me));
        let character = |users| Event { kind: EventKind::CharacterUpdated, universe_id: Some(u), users };
        assert!(!character(vec![2]).is_visible_to(&me));
        assert!(character(vec![1]).is_visible_to(&me));
    }

    #[test]
    fn moderators_see_every_travel_of_their_universe_only() {
        let u = ObjectId::new();
        let moderator = session(1, u, AccessLevel::Moderator);
        assert!(Event { kind: EventKind::TravelUpdated, universe_id: Some(u), users: vec![5] }.is_visible_to(&moderator));
        assert!(!Event { kind: EventKind::TravelUpdated, universe_id: Some(ObjectId::new()), users: vec![5] }.is_visible_to(&moderator));
        assert!(!Event { kind: EventKind::CharacterUpdated, universe_id: Some(u), users: vec![5] }.is_visible_to(&moderator));
    }

    #[test]
    fn map_updates_and_global_resyncs_reach_everyone_concerned() {
        let u = ObjectId::new();
        let me = session(1, u, AccessLevel::Player);
        assert!(Event { kind: EventKind::MapUpdated, universe_id: Some(u), users: vec![] }.is_visible_to(&me));
        assert!(!Event { kind: EventKind::MapUpdated, universe_id: Some(ObjectId::new()), users: vec![] }.is_visible_to(&me));
        assert!(Event { kind: EventKind::Resync, universe_id: None, users: vec![] }.is_visible_to(&me));
    }

    /// Une écriture faite hors de l'activité (ici : directement en base, comme le ferait une
    /// commande slash) arrive sur le hub.
    #[test]
    #[ignore = "integration test: needs a MongoDB replica set (MONGODB_* env vars); run with --ignored"]
    fn a_database_write_reaches_the_hub() {
        use crate::database::travel::TravelGroup;
        use crate::test_support::{block_on, random_id};
        block_on(async {
            let hub = Hub::new();
            let mut rx = hub.subscribe();
            hub.start_watchers();
            tokio::time::sleep(Duration::from_secs(2)).await; // ouverture des streams

            let universe_id = ObjectId::new();
            let user = random_id();
            let group = TravelGroup { _id: ObjectId::new(), universe_id, members: vec![user], ..Default::default() };
            group.clone().insert().await.unwrap();

            let received = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let e = rx.recv().await.unwrap();
                    if e.universe_id == Some(universe_id) {
                        return e;
                    }
                }
            })
            .await;
            group.remove().await.unwrap();

            let e = received.expect("no event within 10 s");
            assert_eq!((e.kind, e.users), (EventKind::TravelUpdated, vec![user]));
        });
    }
}
