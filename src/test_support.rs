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
