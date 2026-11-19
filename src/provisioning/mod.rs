//! Création d'objets Discord et de documents pour l'univers, avec journal et retour en arrière
//! complet (décision I13 de `docs/activity/IMPORT_SPEC.md`).
//!
//! Tout ce qui crée des lieux, des routes ou installe un serveur passe par un [`Provisioner`] :
//! les commandes slash (une création à la fois) comme l'import de carte (des centaines). Le
//! `Provisioner` n'a besoin que d'un client HTTP Discord, pas d'un `poise::Context`.
//!
//! - **Journal** : chaque objet créé (rôle, salon, document) est noté dans l'ordre. Pour une
//!   tâche longue, le journal est aussi écrit en base (`provisioning_jobs`) au fur et à mesure :
//!   si le bot s'arrête au milieu, [`rollback_interrupted_jobs`] défait tout au démarrage suivant.
//! - **Retour en arrière** : suppression dans l'ordre inverse (salons avant leur catégorie,
//!   documents avant les objets Discord). Un objet déjà supprimé compte comme un succès.
//! - **Progression** : `tokio::sync::watch`, lue par l'API de l'activité (barre de progression).
//! - **Espacement des rôles** (Q8) : Discord limite à 250 les créations de rôles par serveur sur
//!   environ 6 h ; les créations sont espacées d'au moins [`ROLE_CREATION_INTERVAL`].

pub mod place;
pub mod road;

use std::sync::Arc;
use std::time::Duration;

use futures::TryStreamExt;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, to_bson};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use serenity::all::{ChannelId, CreateChannel, EditRole, GuildChannel, GuildId, Http, Permissions, Role, RoleId};
use serenity::builder::Builder;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{PLACES_COLLECTION_NAME, PROVISIONING_JOBS_COLLECTION_NAME, ROADS_COLLECTION_NAME, VERSEENGINE_DB_NAME};

/// Intervalle minimal entre deux créations de rôles (Q8).
pub const ROLE_CREATION_INTERVAL: Duration = Duration::from_millis(1500);

/// Un objet créé, à supprimer en cas de retour en arrière.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Created {
    Role {
        #[serde_as(as = "DisplayFromStr")]
        guild_id: u64,
        #[serde_as(as = "DisplayFromStr")]
        role_id: u64,
    },
    Channel {
        #[serde_as(as = "DisplayFromStr")]
        channel_id: u64,
    },
    PlaceDoc { id: ObjectId },
    RoadDoc { id: ObjectId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Creating,
    /// Attente imposée par l'espacement des rôles ou par Discord.
    Waiting,
    RollingBack,
    Done,
    RolledBack,
    /// Certains objets n'ont pas pu être supprimés (listés dans le résultat du retour en arrière).
    RollbackIncomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub phase: Phase,
    pub done: u32,
    pub total: u32,
}

impl Default for Progress {
    fn default() -> Self {
        Self { phase: Phase::Creating, done: 0, total: 0 }
    }
}

/// Erreur d'une création : la clé Fluent à montrer, et ce qui n'a pas pu être défait.
#[derive(Debug)]
pub struct ProvisionError {
    pub key: &'static str,
    pub left_over: Vec<Created>,
}

/// Délai à attendre avant la prochaine création de rôle.
pub fn role_pacing_delay(last: Option<Instant>, now: Instant, interval: Duration) -> Duration {
    match last {
        Some(t) => interval.saturating_sub(now.saturating_duration_since(t)),
        None => Duration::ZERO,
    }
}

const DISCORD_UNKNOWN_CHANNEL: isize = 10003;
const DISCORD_UNKNOWN_ROLE: isize = 10011;

fn discord_code(error: &serenity::Error) -> Option<isize> {
    match error {
        serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(r)) => Some(r.error.code),
        _ => None,
    }
}

/// Crée des objets en les notant dans un journal. Voir le module.
pub struct Provisioner {
    http: Arc<Http>,
    created: Vec<Created>,
    progress: watch::Sender<Progress>,
    role_interval: Duration,
    last_role: Option<Instant>,
    job_id: Option<ObjectId>,
}

impl Provisioner {
    /// Création courte (commande slash) : journal en mémoire seulement.
    pub fn new(http: Arc<Http>) -> Self {
        let (progress, _) = watch::channel(Progress::default());
        Self { http, created: vec![], progress, role_interval: ROLE_CREATION_INTERVAL, last_role: None, job_id: None }
    }

    /// Tâche longue (import) : le journal est aussi écrit en base, sous `kind` et `universe_id`.
    pub async fn with_job(http: Arc<Http>, kind: &str, universe_id: Option<ObjectId>, total: u32) -> mongodb::error::Result<Self> {
        let mut p = Self::new(http);
        let id = ObjectId::new();
        jobs().await
            .insert_one(Job { _id: id, kind: kind.into(), universe_id, status: JobStatus::Running, created: vec![], updated_at: now_secs() })
            .await?;
        p.job_id = Some(id);
        p.set_total(total);
        Ok(p)
    }

    pub fn http(&self) -> &Arc<Http> {
        &self.http
    }

    pub fn job_id(&self) -> Option<ObjectId> {
        self.job_id
    }

    pub fn created(&self) -> &[Created] {
        &self.created
    }

    pub fn subscribe(&self) -> watch::Receiver<Progress> {
        self.progress.subscribe()
    }

    #[cfg(test)]
    pub fn set_role_interval(&mut self, interval: Duration) {
        self.role_interval = interval;
    }

    pub fn set_total(&self, total: u32) {
        self.progress.send_modify(|p| p.total = total);
    }

    /// Une étape de la tâche est terminée (barre de progression).
    pub fn step_done(&self) {
        self.progress.send_modify(|p| p.done += 1);
    }

    async fn record(&mut self, created: Created) {
        if let Some(id) = self.job_id {
            if let Ok(entry) = to_bson(&created) {
                if let Err(e) = jobs().await
                    .update_one(doc! {"_id": id}, doc! {"$push": {"created": entry}, "$set": {"updated_at": now_secs() as i64}})
                    .await
                {
                    eprintln!("[provisioning] job {id}: cannot save the journal entry: {e:?}");
                }
            }
        }
        self.created.push(created);
    }

    /// Note un document inséré (lieu, route) pour pouvoir le supprimer au retour en arrière.
    pub async fn record_doc(&mut self, created: Created) {
        self.record(created).await;
    }

    /// Crée un rôle, en respectant l'espacement des créations de rôles.
    /// `permissions` absent : permissions par défaut de Discord (comme `/place_create_place`).
    pub async fn create_role(&mut self, guild_id: u64, name: &str, permissions: Option<Permissions>) -> serenity::Result<Role> {
        let wait = role_pacing_delay(self.last_role, Instant::now(), self.role_interval);
        if !wait.is_zero() {
            self.progress.send_modify(|p| p.phase = Phase::Waiting);
            tokio::time::sleep(wait).await;
            self.progress.send_modify(|p| p.phase = Phase::Creating);
        }
        let mut role = EditRole::new().name(name).audit_log_reason("VerseEngine");
        if let Some(permissions) = permissions {
            role = role.permissions(permissions);
        }
        let result = role
            .execute(self.http.as_ref(), (GuildId::new(guild_id), None))
            .await;
        self.last_role = Some(Instant::now());
        let role = result?;
        self.record(Created::Role { guild_id, role_id: role.id.get() }).await;
        Ok(role)
    }

    /// Crée un salon (catégorie comprise).
    pub async fn create_channel(&mut self, guild_id: u64, channel: CreateChannel<'_>) -> serenity::Result<GuildChannel> {
        let channel = channel.audit_log_reason("VerseEngine").execute(self.http.as_ref(), GuildId::new(guild_id)).await?;
        self.record(Created::Channel { channel_id: channel.id.get() }).await;
        Ok(channel)
    }

    /// La tâche a réussi : le journal n'est plus nécessaire.
    pub async fn finish(&mut self) {
        self.progress.send_modify(|p| {
            p.phase = Phase::Done;
            p.done = p.total.max(p.done);
        });
        if let Some(id) = self.job_id {
            let _ = jobs().await.update_one(doc! {"_id": id}, doc! {"$set": {"status": "Done", "updated_at": now_secs() as i64}}).await;
        }
        self.created.clear();
    }

    /// Supprime tout ce qui a été créé, dans l'ordre inverse. Retourne ce qui n'a pas pu l'être.
    pub async fn rollback(&mut self) -> Vec<Created> {
        let created = std::mem::take(&mut self.created);
        let left = rollback_entries(&self.http, &created, Some(&self.progress)).await;
        if let Some(id) = self.job_id {
            let status = if left.is_empty() { "RolledBack" } else { "RollbackIncomplete" };
            let _ = jobs().await.update_one(doc! {"_id": id}, doc! {"$set": {"status": status, "updated_at": now_secs() as i64}}).await;
        }
        left
    }

    /// Retour en arrière puis erreur `key` (raccourci pour les services).
    pub async fn fail(&mut self, key: &'static str) -> ProvisionError {
        let left_over = self.rollback().await;
        ProvisionError { key, left_over }
    }
}

/// Supprime `created` dans l'ordre inverse. Retourne ce qui n'a pas pu être supprimé.
async fn rollback_entries(http: &Http, created: &[Created], progress: Option<&watch::Sender<Progress>>) -> Vec<Created> {
    if let Some(p) = progress {
        p.send_modify(|p| {
            p.phase = Phase::RollingBack;
            p.done = 0;
            p.total = created.len() as u32;
        });
    }
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let mut left = vec![];
    for entry in created.iter().rev() {
        let ok = match entry {
            Created::PlaceDoc { id } => db.collection::<mongodb::bson::Document>(PLACES_COLLECTION_NAME).delete_one(doc! {"_id": id}).await.is_ok(),
            Created::RoadDoc { id } => db.collection::<mongodb::bson::Document>(ROADS_COLLECTION_NAME).delete_one(doc! {"_id": id}).await.is_ok(),
            Created::Channel { channel_id } => match ChannelId::new(*channel_id).delete(http).await {
                Ok(_) => true,
                Err(e) => discord_code(&e) == Some(DISCORD_UNKNOWN_CHANNEL),
            },
            Created::Role { guild_id, role_id } => match http.delete_role(GuildId::new(*guild_id), RoleId::new(*role_id), Some("VerseEngine: rollback")).await {
                Ok(()) => true,
                Err(e) => discord_code(&e) == Some(DISCORD_UNKNOWN_ROLE),
            },
        };
        if !ok {
            eprintln!("[provisioning] rollback: cannot delete {entry:?}");
            left.push(entry.clone());
        }
        if let Some(p) = progress {
            p.send_modify(|p| p.done += 1);
        }
    }
    if let Some(p) = progress {
        p.send_modify(|p| p.phase = if left.is_empty() { Phase::RolledBack } else { Phase::RollbackIncomplete });
    }
    left
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Running,
    Done,
    RolledBack,
    RollbackIncomplete,
}

/// Tâche longue enregistrée en base, avec son journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(rename = "_id")]
    pub _id: ObjectId,
    pub kind: String,
    pub universe_id: Option<ObjectId>,
    pub status: JobStatus,
    pub created: Vec<Created>,
    pub updated_at: u64,
}

async fn jobs() -> mongodb::Collection<Job> {
    get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<Job>(PROVISIONING_JOBS_COLLECTION_NAME)
}

fn now_secs() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

/// Défait les tâches interrompues par un arrêt du bot (statut `Running`). À appeler au démarrage,
/// avant d'accepter de nouvelles tâches.
pub async fn rollback_interrupted_jobs(http: &Http) -> mongodb::error::Result<usize> {
    rollback_interrupted_jobs_where(http, doc! {}).await
}

/// Comme [`rollback_interrupted_jobs`], limité aux tâches qui vérifient `filter` (tests).
pub async fn rollback_interrupted_jobs_where(http: &Http, filter: mongodb::bson::Document) -> mongodb::error::Result<usize> {
    let mut f = doc! {"status": "Running"};
    f.extend(filter);
    let interrupted: Vec<Job> = jobs().await.find(f).await?.try_collect().await?;
    for job in &interrupted {
        let left = rollback_entries(http, &job.created, None).await;
        let status = if left.is_empty() { "RolledBack" } else { "RollbackIncomplete" };
        println!("[provisioning] interrupted job {} ({}) rolled back: {status}", job._id, job.kind);
        jobs().await.update_one(doc! {"_id": job._id}, doc! {"$set": {"status": status, "updated_at": now_secs() as i64}}).await?;
    }
    Ok(interrupted.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_role_is_not_delayed() {
        let now = Instant::now();
        assert_eq!(role_pacing_delay(None, now, Duration::from_secs(2)), Duration::ZERO);
    }

    #[test]
    fn roles_are_spaced() {
        let start = Instant::now();
        let later = start + Duration::from_millis(500);
        assert_eq!(role_pacing_delay(Some(start), later, Duration::from_secs(2)), Duration::from_millis(1500));
        let much_later = start + Duration::from_secs(3);
        assert_eq!(role_pacing_delay(Some(start), much_later, Duration::from_secs(2)), Duration::ZERO);
    }

    #[test]
    fn journal_entries_round_trip_through_bson() {
        let entries = vec![
            Created::Role { guild_id: u64::MAX, role_id: 1 },
            Created::Channel { channel_id: 2 },
            Created::PlaceDoc { id: ObjectId::new() },
        ];
        for e in entries {
            let b = to_bson(&e).unwrap();
            let back: Created = mongodb::bson::from_bson(b).unwrap();
            assert_eq!(back, e);
        }
    }

    /// Tests contre l'API REST Discord et MongoDB. Serveur : `VERSEENGINE_TEST_GUILD_ID`.
    mod integration {
        use super::super::place::provision_place;
        use super::super::road::provision_road;
        use super::*;
        use crate::test_support::{block_on, discord_http, required_env_u64};
        use serenity::all::PermissionOverwriteType;

        async fn exists_channel(http: &Http, id: u64) -> bool {
            http.get_channel(ChannelId::new(id)).await.is_ok()
        }

        async fn exists_role(http: &Http, guild: u64, id: u64) -> bool {
            http.get_guild_roles(GuildId::new(guild)).await.unwrap().iter().any(|r| r.id.get() == id)
        }

        async fn place_doc_exists(id: ObjectId) -> bool {
            get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<mongodb::bson::Document>(PLACES_COLLECTION_NAME)
                .find_one(doc! {"_id": id}).await.unwrap().is_some()
        }

        /// Un lieu complet est créé (rôle, catégorie visible du seul rôle, salon générique
        /// synchronisé), puis une route vers une catégorie invalide échoue : **tout** est défait,
        /// lieu compris (I13).
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn a_failure_rolls_back_everything_created_so_far() {
            block_on(async {
                // MongoDB d'abord : sans base, rien ne doit être créé sur Discord.
                let _ = get_db_client().await;
                let http = discord_http();
                let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
                let mut p = Provisioner::new(http.clone());
                p.set_role_interval(Duration::from_millis(200));
                let universe_id = ObjectId::new();

                let place = provision_place(&mut p, universe_id, guild, "test-provisioning", "fr").await.expect("the place is created");
                let central = place.central_channel_id.unwrap();
                let c = http.get_channel(ChannelId::new(central)).await.unwrap().guild().unwrap();
                assert_eq!(c.name, "place-centrale");
                assert_eq!(c.parent_id.map(|x| x.get()), Some(place.category_id));
                let cat = http.get_channel(ChannelId::new(place.category_id)).await.unwrap().guild().unwrap();
                let mut a: Vec<_> = c.permission_overwrites.iter().map(|o| (format!("{:?}", o.kind), o.allow.bits(), o.deny.bits())).collect();
                let mut b: Vec<_> = cat.permission_overwrites.iter().map(|o| (format!("{:?}", o.kind), o.allow.bits(), o.deny.bits())).collect();
                a.sort();
                b.sort();
                assert_eq!(a, b, "the central channel is synced with its category");
                assert!(cat.permission_overwrites.iter().any(|o| o.kind == PermissionOverwriteType::Role(RoleId::new(place.role))));
                assert!(place_doc_exists(place._id).await);
                assert_eq!(p.created().len(), 4, "role, category, channel, document");

                // Catégorie invalide : le salon de la route ne peut pas être créé.
                let err = provision_road(&mut p, guild, central, &place, &place, 3, false).await.expect_err("an invalid category");
                assert_eq!(err.key, "create_road__create_channel_failed");
                assert!(err.left_over.is_empty(), "everything is deleted: {:?}", err.left_over);
                assert!(!exists_channel(&http, central).await);
                assert!(!exists_channel(&http, place.category_id).await);
                assert!(!exists_role(&http, guild, place.role).await);
                assert!(!place_doc_exists(place._id).await);
            });
        }

        /// Une tâche longue interrompue (le bot s'arrête avant la fin) est défaite au démarrage
        /// suivant grâce à son journal en base.
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn an_interrupted_job_is_rolled_back_at_startup() {
            block_on(async {
                // MongoDB d'abord : sans base, rien ne doit être créé sur Discord.
                let _ = get_db_client().await;
                let http = discord_http();
                let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
                let universe_id = ObjectId::new();
                let mut p = Provisioner::with_job(http.clone(), "test", Some(universe_id), 1).await.unwrap();
                let job = p.job_id().unwrap();
                let place = provision_place(&mut p, universe_id, guild, "test-interrupted", "en-US").await.unwrap();
                assert_eq!(http.get_channel(ChannelId::new(place.central_channel_id.unwrap())).await.unwrap().guild().unwrap().name, "central-square");
                drop(p); // arrêt brutal : ni finish, ni rollback

                let stored = jobs().await.find_one(doc! {"_id": job}).await.unwrap().unwrap();
                assert_eq!(stored.status, JobStatus::Running);
                assert_eq!(stored.created.len(), 4);

                let n = rollback_interrupted_jobs_where(&http, doc! {"_id": job}).await.unwrap();
                assert_eq!(n, 1);
                assert!(!exists_channel(&http, place.category_id).await);
                assert!(!exists_role(&http, guild, place.role).await);
                assert!(!place_doc_exists(place._id).await);
                let stored = jobs().await.find_one(doc! {"_id": job}).await.unwrap().unwrap();
                assert_eq!(stored.status, JobStatus::RolledBack);
                jobs().await.delete_one(doc! {"_id": job}).await.unwrap();
            });
        }

        /// Progression : chaque étape est comptée, puis le retour en arrière a sa propre
        /// progression (I13).
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn progress_is_reported_while_creating_and_rolling_back() {
            block_on(async {
                // MongoDB d'abord : sans base, rien ne doit être créé sur Discord.
                let _ = get_db_client().await;
                let http = discord_http();
                let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
                let mut p = Provisioner::new(http.clone());
                let rx = p.subscribe();
                p.set_total(1);
                let place = provision_place(&mut p, ObjectId::new(), guild, "test-progress", "fr").await.unwrap();
                p.step_done();
                assert_eq!((rx.borrow().done, rx.borrow().total), (1, 1));
                let left = p.rollback().await;
                assert!(left.is_empty());
                let last = rx.borrow().clone();
                assert_eq!(last, Progress { phase: Phase::RolledBack, done: 4, total: 4 });
                assert!(!exists_channel(&http, place.category_id).await);
            });
        }
    }
}
