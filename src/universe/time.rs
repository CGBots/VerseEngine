use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration};
use serenity::all::{ChannelId, CreateMessage, EditMessage, GetMessages, Http, MessageId, UserId};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use chrono::{Utc};
use std::sync::Arc;
use mongodb::bson::oid::ObjectId;
use crate::database::universe::Universe;
use crate::tr_locale;
use crate::travel::logic::HTTP_CLIENT;

pub static TIME_SLEEPER: Lazy<Arc<Mutex<Option<JoinHandle<()>>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
pub static PENDING_TIME_EVENTS: Lazy<Arc<Mutex<Vec<TimeEvent>>>> = Lazy::new(|| Arc::new(Mutex::new(vec![])));

/// Génération du planificateur de temps.
///
/// `setup_universal_time` est rappelé à chaque événement `ready` de Discord (donc aussi après une
/// reconnexion). Chaque appel incrémente cette valeur : un minuteur d'une génération précédente
/// s'arrête dès qu'il se réveille au lieu de continuer en parallèle du nouveau.
static TIME_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Durée d'une phase (un quart de journée) en secondes, pour un temps d'univers égal au temps réel.
const PHASE_SECS_AT_REAL_TIME: f64 = 21600.0;

/// Nombre maximal de messages relus pour retrouver l'annonce du bot (limite de l'API Discord).
const HISTORY_LOOKUP_LIMIT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimePhase {
    Midnight,
    Sunrise,
    Noon,
    Sunset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TimeEvent {
    pub universe_id: ObjectId,
    pub trigger_timestamp: u64,
    pub phase_index: u64,
    /// Origine du temps de l'univers (secondes UNIX), mémorisée pour pouvoir replanifier
    /// même si la base de données est momentanément injoignable.
    pub origin_secs: u64,
    /// Durée d'une phase de cet univers en secondes (voir [`phase_duration_secs`]).
    pub phase_duration_secs: u64,
}

impl TimeEvent {
    /// Crée l'événement de début de la phase `phase_index`.
    pub fn new(universe_id: ObjectId, origin_secs: u64, phase_duration_secs: u64, phase_index: u64) -> Self {
        Self {
            universe_id,
            trigger_timestamp: phase_start(origin_secs, phase_duration_secs, phase_index),
            phase_index,
            origin_secs,
            phase_duration_secs,
        }
    }
}

impl TimePhase {
    pub fn get_message_key(&self) -> &'static str {
        match self {
            TimePhase::Midnight => "time__midnight",
            TimePhase::Sunrise => "time__sunrise",
            TimePhase::Noon => "time__noon",
            TimePhase::Sunset => "time__sunset",
        }
    }

    pub fn from_index(index: u64) -> Self {
        match index % 4 {
            0 => TimePhase::Midnight,
            1 => TimePhase::Sunrise,
            2 => TimePhase::Noon,
            _ => TimePhase::Sunset,
        }
    }
}

/// Durée d'une phase en secondes pour un modificateur de temps (1.0 = temps réel, 2.0 = deux fois plus vite).
///
/// Retourne `None` si le modificateur n'est pas strictement positif (temps à l'arrêt).
/// La durée n'est jamais nulle : un modificateur énorme donnerait sinon une division par zéro.
pub fn phase_duration_secs(time_modifier: f64) -> Option<u64> {
    if !time_modifier.is_finite() || time_modifier <= 0.0 {
        return None;
    }
    Some(((PHASE_SECS_AT_REAL_TIME / time_modifier) as u64).max(1))
}

/// Index de la phase en cours à l'instant `now` (secondes UNIX).
///
/// `phase_duration_secs` doit être non nul (voir [`phase_duration_secs`]).
pub fn phase_index_at(now: u64, origin_secs: u64, phase_duration_secs: u64) -> u64 {
    now.saturating_sub(origin_secs) / phase_duration_secs.max(1)
}

/// Instant (secondes UNIX) où commence la phase `phase_index`.
pub fn phase_start(origin_secs: u64, phase_duration_secs: u64, phase_index: u64) -> u64 {
    origin_secs.saturating_add(phase_index.saturating_mul(phase_duration_secs))
}

/// Paramètres temporels d'un univers : `(origine en secondes, durée d'une phase en secondes)`.
/// `None` si le temps de l'univers est à l'arrêt.
fn universe_time_params(universe: &Universe) -> Option<(u64, u64)> {
    let modifier = universe.global_time_modifier as f64 / 100.0;
    let duration = phase_duration_secs(modifier)?;
    Some(((universe.time_origin_timestamp / 1000) as u64, duration))
}

/// Retire de la file (triée) tous les événements échus à `now` et les retourne.
/// Les événements qui ne sont pas encore dus ne sont jamais touchés.
fn drain_due_events(events: &mut Vec<TimeEvent>, now: u64) -> Vec<TimeEvent> {
    let due_count = events.partition_point(|e| e.trigger_timestamp <= now);
    events.drain(..due_count).collect()
}

/// Insère un événement dans la file en gardant l'ordre chronologique.
fn insert_sorted(events: &mut Vec<TimeEvent>, event: TimeEvent) {
    let index = events.partition_point(|e| e.trigger_timestamp <= event.trigger_timestamp);
    events.insert(index, event);
}

/// Délai (secondes) avant le prochain événement de la file, 0 s'il est déjà en retard.
fn delay_until_next(events: &[TimeEvent], now: u64) -> Option<u64> {
    events.first().map(|e| e.trigger_timestamp.saturating_sub(now))
}

/// Choisit, parmi les messages d'un salon, le plus récent écrit par le bot.
///
/// `skip` permet d'écarter un message déjà essayé sans succès.
pub fn pick_bot_message_id(messages: &[(MessageId, UserId)], bot_id: UserId, skip: Option<MessageId>) -> Option<MessageId> {
    messages
        .iter()
        .filter(|(id, author)| *author == bot_id && Some(*id) != skip)
        .map(|(id, _)| *id)
        .max()
}

pub async fn setup_universal_time() {
    let universes = match Universe::get_all_universes().await {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Failed to get universes for universal time setup: {:?}", e);
            return;
        }
    };

    let now = Utc::now().timestamp() as u64;
    let mut events = Vec::new();

    for universe in universes {
        let Some((origin_secs, duration)) = universe_time_params(&universe) else { continue };

        // La prochaine phase est la phase actuelle + 1
        let next_phase_idx = phase_index_at(now, origin_secs, duration) + 1;
        events.push(TimeEvent::new(universe.universe_id, origin_secs, duration, next_phase_idx));
    }

    // Invalide et arrête tout minuteur précédent : ce setup est rappelé à chaque `ready`
    // (reconnexion comprise), et deux minuteurs se partageraient la même file d'événements.
    let generation = TIME_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let previous = TIME_SLEEPER.lock().await.take();
    if let Some(handle) = previous {
        handle.abort();
    }

    // Trier par timestamp croissant
    events.sort_by_key(|e| e.trigger_timestamp);
    let universes_count = events.len();
    let delay = delay_until_next(&events, now);

    *PENDING_TIME_EVENTS.lock().await = events;

    match delay {
        Some(delay) => {
            // Lancer le premier sleeper
            let handle = time_process(delay, generation);
            *TIME_SLEEPER.lock().await = Some(handle);
            println!("Universal Time system initialized: {} universes. Next event in {}s", universes_count, delay);
        }
        None => println!("Universal Time system initialized: 0 active universes."),
    }
}

fn time_process(delay: u64, generation: u64) -> JoinHandle<()> {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(delay)).await;

        // Un setup plus récent a pris la relève : ce minuteur n'a plus lieu d'être.
        if TIME_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }

        let now = Utc::now().timestamp() as u64;

        // On ne retire que les événements réellement échus. Aucun verrou n'est gardé pendant
        // les appels réseau / base de données qui suivent.
        let due = {
            let mut events = PENDING_TIME_EVENTS.lock().await;
            drain_due_events(&mut events, now)
        };

        let http_opt = {
            let lock = HTTP_CLIENT.lock().await;
            lock.clone()
        };

        for event in due {
            // Paramètres à jour (le modificateur de temps a pu changer). Si la base est injoignable,
            // on garde ceux mémorisés : une erreur passagère ne doit pas arrêter le cycle.
            let params = match crate::database::universe::get_universe_by_id(event.universe_id).await {
                Ok(Some(universe)) => universe_time_params(&universe), // `None` : temps à l'arrêt
                Ok(None) => None, // univers supprimé
                Err(e) => {
                    eprintln!("[time] cannot read universe {}: {:?}. Using the cached time settings.", event.universe_id, e);
                    Some((event.origin_secs, event.phase_duration_secs))
                }
            };
            let Some((origin_secs, duration)) = params else { continue };

            // L'heure réelle de l'univers fait foi : après une coupure ou un retard, on annonce la
            // phase actuelle au lieu de rejouer les phases manquées une par une.
            let phase_index = phase_index_at(now, origin_secs, duration);

            if let Some(http) = &http_opt {
                let phase = TimePhase::from_index(phase_index);
                tokio::spawn(process_universe_phase_change(event.universe_id, phase, http.clone()));
            }

            // La phase suivante commence toujours après `now` : pas de boucle de réveils immédiats.
            let next = TimeEvent::new(event.universe_id, origin_secs, duration, phase_index + 1);
            insert_sorted(&mut *PENDING_TIME_EVENTS.lock().await, next);
        }

        // Prépare le prochain sleep
        let next_delay = {
            let events = PENDING_TIME_EVENTS.lock().await;
            delay_until_next(&events, Utc::now().timestamp() as u64)
        };

        let mut sleeper = TIME_SLEEPER.lock().await;
        if TIME_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        *sleeper = next_delay.map(|delay| time_process(delay, generation));
    })
}

async fn process_universe_phase_change(universe_id: mongodb::bson::oid::ObjectId, phase: TimePhase, http: Arc<Http>) {
    let db_client = crate::database::db_client::get_db_client().await;
    let filter = mongodb::bson::doc! { "universe_id": universe_id };

    let mut cursor = match db_client
        .database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
        .collection::<crate::database::server::Server>(crate::database::db_namespace::SERVERS_COLLECTION_NAME)
        .find(filter)
        .await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[time] cannot list the servers of universe {}: {:?}", universe_id, e);
                return;
            }
        };

    let bot_user_id = match http.get_current_user().await {
        Ok(u) => u.id,
        Err(e) => {
            eprintln!("[time] cannot get the bot user: {:?}", e);
            return;
        }
    };

    use futures::TryStreamExt;
    loop {
        match cursor.try_next().await {
            Ok(Some(server)) => {
                if let Some(channel_id) = server.universal_time_channel_id {
                    if channel_id.id == 0 {
                        continue;
                    }
                    let locale = match http.get_guild(server.server_id.into()).await {
                        Ok(g) => g.preferred_locale,
                        Err(_) => "fr".to_string()
                    };
                    let msg = tr_locale!(locale.as_str(), phase.get_message_key());
                    let http_clone = http.clone();
                    let channel = ChannelId::new(channel_id.id);
                    let server_id = server.server_id;
                    let stored_message_id = server.universal_time_message_id;
                    tokio::spawn(async move {
                        announce_phase(&http_clone, bot_user_id, server_id, channel, stored_message_id, msg).await;
                    });
                }
            },
            Ok(None) => break,
            Err(e) => {
                eprintln!("[time] error while reading the servers of universe {}: {:?}", universe_id, e);
                break
            },
        }
    }
}

/// Met à jour l'annonce jour/nuit d'un salon.
///
/// 1. Modifie le message mémorisé pour ce serveur.
/// 2. À défaut (serveur antérieur à ce champ, message supprimé…), retrouve le dernier message du
///    bot dans les 100 derniers messages du salon et le modifie.
/// 3. Si aucune modification n'est possible, poste un nouveau message, le mémorise, puis supprime
///    les anciens messages qui n'ont pas pu être modifiés.
///
/// Chaque échec est journalisé avec l'erreur Discord, pour pouvoir en diagnostiquer la cause.
async fn announce_phase(
    http: &Http,
    bot_user_id: UserId,
    server_id: u64,
    channel: ChannelId,
    stored_message_id: Option<u64>,
    content: String,
) {
    let edit = || EditMessage::new().content(content.clone()).suppress_embeds(true);
    let mut stale: Vec<MessageId> = Vec::new();
    let mut tried: Option<MessageId> = None;

    // 1) Message mémorisé
    if let Some(id) = stored_message_id.filter(|id| *id != 0).map(MessageId::new) {
        tried = Some(id);
        match channel.edit_message(http, id, edit()).await {
            Ok(_) => return,
            Err(e) => {
                eprintln!("[time] server {server_id}: cannot edit the stored message {id} in channel {channel}: {e:?}");
                stale.push(id);
            }
        }
    }

    // 2) Dernier message du bot dans l'historique du salon
    match channel.messages(http, GetMessages::new().limit(HISTORY_LOOKUP_LIMIT)).await {
        Ok(messages) => {
            let authors: Vec<(MessageId, UserId)> = messages.iter().map(|m| (m.id, m.author.id)).collect();
            if let Some(id) = pick_bot_message_id(&authors, bot_user_id, tried) {
                match channel.edit_message(http, id, edit()).await {
                    Ok(_) => {
                        remember_message(server_id, stored_message_id, id).await;
                        delete_stale(http, channel, &stale).await;
                        return;
                    }
                    Err(e) => {
                        eprintln!("[time] server {server_id}: cannot edit message {id} in channel {channel}: {e:?}");
                        stale.push(id);
                    }
                }
            }
        }
        Err(e) => eprintln!("[time] server {server_id}: cannot read the history of channel {channel}: {e:?}"),
    }

    // 3) Nouveau message
    match channel.send_message(http, CreateMessage::new().content(content.clone())).await {
        Ok(message) => {
            remember_message(server_id, stored_message_id, message.id).await;
            delete_stale(http, channel, &stale).await;
        }
        Err(e) => eprintln!("[time] server {server_id}: cannot send the announcement in channel {channel}: {e:?}"),
    }
}

/// Mémorise l'identifiant du message d'annonce s'il a changé.
async fn remember_message(server_id: u64, previous: Option<u64>, message_id: MessageId) {
    if previous == Some(message_id.get()) {
        return;
    }
    if let Err(e) = crate::database::server::set_universal_time_message_id(server_id, Some(message_id.get())).await {
        eprintln!("[time] server {server_id}: cannot save the announcement message id: {e:?}");
    }
}

/// Supprime (au mieux) les anciens messages d'annonce devenus inutiles.
async fn delete_stale(http: &Http, channel: ChannelId, stale: &[MessageId]) {
    for id in stale {
        if let Err(e) = channel.delete_message(http, *id).await {
            // Souvent un 404 : le message n'existe déjà plus. Rien d'autre à faire.
            eprintln!("[time] cannot delete the old announcement {id} in channel {channel}: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(trigger: u64) -> TimeEvent {
        TimeEvent { universe_id: ObjectId::new(), trigger_timestamp: trigger, phase_index: 0, origin_secs: 0, phase_duration_secs: 100 }
    }

    fn triggers(events: &[TimeEvent]) -> Vec<u64> {
        events.iter().map(|e| e.trigger_timestamp).collect()
    }

    // ---- phases ----

    #[test]
    fn phases_cycle_through_the_day() {
        assert_eq!(TimePhase::from_index(0), TimePhase::Midnight);
        assert_eq!(TimePhase::from_index(1), TimePhase::Sunrise);
        assert_eq!(TimePhase::from_index(2), TimePhase::Noon);
        assert_eq!(TimePhase::from_index(3), TimePhase::Sunset);
        assert_eq!(TimePhase::from_index(4), TimePhase::Midnight);
        assert_eq!(TimePhase::from_index(7), TimePhase::Sunset);
    }

    #[test]
    fn phase_duration_follows_the_time_modifier() {
        assert_eq!(phase_duration_secs(1.0), Some(21_600)); // 24 h par jour
        assert_eq!(phase_duration_secs(2.0), Some(10_800));
        assert_eq!(phase_duration_secs(0.5), Some(43_200));
    }

    #[test]
    fn phase_duration_is_none_when_time_is_stopped_or_invalid() {
        assert_eq!(phase_duration_secs(0.0), None);
        assert_eq!(phase_duration_secs(-1.0), None);
        assert_eq!(phase_duration_secs(f64::NAN), None);
        assert_eq!(phase_duration_secs(f64::INFINITY), None);
    }

    #[test]
    fn phase_duration_is_never_zero() {
        // Un modificateur démesuré ne doit pas produire une durée de 0 s (division par zéro ensuite).
        assert_eq!(phase_duration_secs(1.0e9), Some(1));
    }

    #[test]
    fn phase_index_counts_elapsed_phases() {
        assert_eq!(phase_index_at(1_000, 1_000, 100), 0);
        assert_eq!(phase_index_at(1_099, 1_000, 100), 0);
        assert_eq!(phase_index_at(1_100, 1_000, 100), 1);
        assert_eq!(phase_index_at(1_950, 1_000, 100), 9);
    }

    #[test]
    fn phase_index_before_the_origin_is_zero() {
        assert_eq!(phase_index_at(500, 1_000, 100), 0);
    }

    #[test]
    fn phase_start_is_the_inverse_of_phase_index() {
        for index in [0u64, 1, 7, 1_000] {
            let start = phase_start(1_000, 100, index);
            assert_eq!(phase_index_at(start, 1_000, 100), index);
            assert_eq!(phase_index_at(start + 99, 1_000, 100), index);
        }
    }

    #[test]
    fn phase_start_does_not_overflow() {
        assert_eq!(phase_start(u64::MAX - 1, u64::MAX, u64::MAX), u64::MAX);
    }

    #[test]
    fn time_event_is_scheduled_at_the_start_of_its_phase() {
        let e = TimeEvent::new(ObjectId::new(), 1_000, 100, 5);
        assert_eq!(e.trigger_timestamp, 1_500);
        assert_eq!(e.phase_index, 5);
        assert_eq!((e.origin_secs, e.phase_duration_secs), (1_000, 100));
    }

    // ---- file d'événements ----

    #[test]
    fn only_due_events_are_drained() {
        let mut events = vec![event(100), event(200), event(300)];
        let due = drain_due_events(&mut events, 200);
        assert_eq!(triggers(&due), vec![100, 200]); // un événement pile à `now` est échu
        assert_eq!(triggers(&events), vec![300]);
    }

    #[test]
    fn nothing_is_drained_before_the_first_event() {
        let mut events = vec![event(100), event(200)];
        assert!(drain_due_events(&mut events, 99).is_empty());
        assert_eq!(triggers(&events), vec![100, 200]);
    }

    #[test]
    fn events_are_inserted_in_chronological_order() {
        let mut events = vec![event(100), event(300)];
        insert_sorted(&mut events, event(200));
        insert_sorted(&mut events, event(50));
        insert_sorted(&mut events, event(400));
        assert_eq!(triggers(&events), vec![50, 100, 200, 300, 400]);
    }

    #[test]
    fn delay_until_next_event() {
        assert_eq!(delay_until_next(&[], 1_000), None);
        assert_eq!(delay_until_next(&[event(1_500)], 1_000), Some(500));
        assert_eq!(delay_until_next(&[event(900)], 1_000), Some(0)); // en retard : tout de suite
    }

    // ---- scénarios du planificateur ----

    #[test]
    fn after_downtime_the_real_current_phase_is_announced_once() {
        // Phases de 100 s à partir de 1 000 s. L'événement attendu était la phase 5, mais le bot
        // n'a pas tourné pendant 4 phases : il se réveille à 1 910 s (phase 9).
        let (origin, duration) = (1_000u64, 100u64);
        let scheduled = TimeEvent::new(ObjectId::new(), origin, duration, 5);
        let now = 1_910;

        let announced = phase_index_at(now, origin, duration);
        assert_eq!(announced, 9);
        assert_eq!(TimePhase::from_index(announced), TimePhase::Sunrise);

        // Le suivant est dans le futur : pas de rafale d'annonces pour rattraper les phases manquées.
        let next = TimeEvent::new(scheduled.universe_id, origin, duration, announced + 1);
        assert_eq!(next.trigger_timestamp, 2_000);
        assert!(next.trigger_timestamp > now);
    }

    #[test]
    fn the_next_event_is_always_in_the_future() {
        let (origin, duration) = (1_000u64, 100u64);
        for now in [1_000u64, 1_001, 1_099, 1_100, 1_150, 5_000, 123_456] {
            let announced = phase_index_at(now, origin, duration);
            let next = TimeEvent::new(ObjectId::new(), origin, duration, announced + 1);
            assert!(next.trigger_timestamp > now, "now = {now}");
            assert!(next.trigger_timestamp - now <= duration, "now = {now}");
        }
    }

    // ---- choix du message du bot ----

    fn msg(id: u64, author: u64) -> (MessageId, UserId) {
        (MessageId::new(id), UserId::new(author))
    }

    #[test]
    fn the_most_recent_bot_message_is_picked() {
        let bot = UserId::new(7);
        let messages = [msg(30, 1), msg(20, 7), msg(10, 7), msg(40, 2)];
        assert_eq!(pick_bot_message_id(&messages, bot, None), Some(MessageId::new(20)));
    }

    #[test]
    fn the_message_order_does_not_matter() {
        let bot = UserId::new(7);
        let messages = [msg(10, 7), msg(20, 7), msg(15, 7)];
        assert_eq!(pick_bot_message_id(&messages, bot, None), Some(MessageId::new(20)));
    }

    #[test]
    fn a_message_already_tried_is_skipped() {
        let bot = UserId::new(7);
        let messages = [msg(20, 7), msg(10, 7)];
        assert_eq!(pick_bot_message_id(&messages, bot, Some(MessageId::new(20))), Some(MessageId::new(10)));
        assert_eq!(pick_bot_message_id(&[msg(20, 7)], bot, Some(MessageId::new(20))), None);
    }

    #[test]
    fn no_bot_message_means_nothing_to_edit() {
        let bot = UserId::new(7);
        assert_eq!(pick_bot_message_id(&[msg(10, 1), msg(20, 2)], bot, None), None);
        assert_eq!(pick_bot_message_id(&[], bot, None), None);
    }
}
