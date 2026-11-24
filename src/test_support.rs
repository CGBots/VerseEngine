//! Outils communs aux tests d'intégration (MongoDB, API REST Discord).
//!
//! Le client MongoDB est global (`DB_CLIENT`) : ses tâches de fond vivent dans le runtime tokio
//! qui l'a créé. Avec `#[tokio::test]`, chaque test a son propre runtime, détruit à la fin du
//! test : le test suivant réutilise alors un client dont les tâches sont mortes et échoue en
//! « server selection timeout ». Tous les tests d'intégration passent donc par [`block_on`], qui
//! utilise un seul runtime pour tout le binaire de test.

use std::future::Future;
use std::sync::Arc;

use once_cell::sync::Lazy;
use serenity::all::{ChannelId, GuildId, Http};
use tokio::runtime::Runtime;

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("cannot build the test runtime")
});

/// Exécute `future` sur le runtime partagé des tests d'intégration.
pub fn block_on<F: Future>(future: F) -> F::Output {
    RUNTIME.block_on(future)
}

/// Identifiant aléatoire non nul, pour que les tests n'entrent jamais en collision avec des
/// données existantes (ni entre eux quand ils tournent en parallèle).
pub fn random_id() -> u64 {
    rand::random::<u64>() >> 2 | 1
}

/// Lit une variable d'environnement obligatoire pour un test d'intégration.
pub fn required_env_u64(name: &str) -> u64 {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("integration test: set {name}"))
        .parse()
        .unwrap_or_else(|_| panic!("integration test: {name} must be a Discord id"))
}

/// Client HTTP Discord pour les tests REST.
///
/// Le jeton vient de `DISCORD_TOKEN`. Dans l'environnement cloud, un proxy ajoute lui-même le
/// vrai jeton à chaque requête vers discord.com : on peut alors mettre n'importe quelle valeur
/// (par exemple `DISCORD_TOKEN=dummy`), le vrai jeton n'apparaît jamais dans le processus.
pub fn discord_http() -> Arc<Http> {
    let token = std::env::var("DISCORD_TOKEN")
        .expect("integration test: set DISCORD_TOKEN (any value behind the credential proxy)");
    let mut client = reqwest_serenity::Client::builder();
    // serenity n'accepte que les racines webpki : derrière un proxy HTTPS qui ré-émet les
    // certificats, on ajoute son CA (fichier PEM désigné par `SSL_CERT_FILE`).
    if let Ok(path) = std::env::var("SSL_CERT_FILE") {
        let pem = std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read SSL_CERT_FILE {path}: {e}"));
        for cert in reqwest_serenity::Certificate::from_pem_bundle(&pem).expect("invalid PEM bundle") {
            client = client.add_root_certificate(cert);
        }
    }
    let client = client.build().expect("cannot build the HTTP client");
    Arc::new(serenity::http::HttpBuilder::new(&token).client(client).build())
}

/// Salon textuel temporaire créé dans le serveur de test, supprimé par [`TempChannel::delete`].
pub struct TempChannel {
    pub id: ChannelId,
}

impl TempChannel {
    /// Crée un salon dans le serveur `VERSEENGINE_TEST_GUILD_ID`.
    pub async fn create(http: &Http, name: &str) -> Self {
        use serenity::all::{ChannelType, CreateChannel};
        let guild = GuildId::new(required_env_u64("VERSEENGINE_TEST_GUILD_ID"));
        let channel = guild
            .create_channel(http, CreateChannel::new(name).kind(ChannelType::Text).topic("VerseEngine integration test (temporary)"))
            .await
            .expect("cannot create the test channel (Manage Channels permission?)");
        Self { id: channel.id }
    }

    pub async fn delete(self, http: &Http) {
        if let Err(e) = self.id.delete(http).await {
            eprintln!("cannot delete the test channel {}: {e:?}", self.id);
        }
    }
}

/// Supprime d'un univers de test tout ce qu'il a sur Discord (lieux, routes, installation des
/// serveurs) et en base.
pub async fn delete_universe_everywhere(http: &Http, universe_id: mongodb::bson::oid::ObjectId) {
    use crate::database::db_namespace::*;
    use futures::TryStreamExt;
    use mongodb::bson::doc;
    use serenity::all::RoleId;
    let places: Vec<crate::database::places::Place> = crate::database::places::get_places_by_universe_id(universe_id).await.unwrap().try_collect().await.unwrap();
    let roads: Vec<crate::database::road::Road> = crate::database::road::get_roads_by_universe(universe_id).await.unwrap().try_collect().await.unwrap();
    let servers: Vec<crate::database::server::Server> = crate::database::universe::get_servers_from_universe(&universe_id).await.unwrap().try_collect().await.unwrap();
    for r in &roads {
        let _ = ChannelId::new(r.channel_id).delete(http).await;
    }
    for p in &places {
        if let Some(c) = p.central_channel_id {
            let _ = ChannelId::new(c).delete(http).await;
        }
        let _ = ChannelId::new(p.category_id).delete(http).await;
        let _ = http.delete_role(GuildId::new(p.server_id), RoleId::new(p.role), None).await;
    }
    for s in &servers {
        // Salons avant catégories : l'ordre de `all_ids` met les catégories en premier, on inverse.
        let ids: Vec<_> = s.all_ids().into_iter().cloned().collect();
        for id in ids.iter().rev() {
            match id.id_type {
                crate::database::server::IdType::Role if id.id != s.server_id => {
                    let _ = http.delete_role(GuildId::new(s.server_id), RoleId::new(id.id), None).await;
                }
                crate::database::server::IdType::Role => {}
                _ => {
                    let _ = ChannelId::new(id.id).delete(http).await;
                }
            }
        }
    }
    let db = crate::database::db_client::get_db_client().await.database(VERSEENGINE_DB_NAME);
    for name in [PLACES_COLLECTION_NAME, ROADS_COLLECTION_NAME, SERVERS_COLLECTION_NAME, STATS_COLLECTION_NAME, UNIVERSE_MAPS_COLLECTION_NAME, PROVISIONING_JOBS_COLLECTION_NAME] {
        db.collection::<mongodb::bson::Document>(name).delete_many(doc! {"universe_id": universe_id}).await.unwrap();
    }
    db.collection::<mongodb::bson::Document>(UNIVERSES_COLLECTION_NAME).delete_one(doc! {"_id": universe_id}).await.unwrap();
}
