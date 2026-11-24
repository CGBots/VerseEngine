//! Création d'un univers et rattachement d'un serveur, sans `poise::Context`. Partagés par
//! `/universe_create_universe`, `/universe_add_server` et l'activité (I2, I14).

use chrono::{TimeZone, Utc};
use mongodb::bson::oid::ObjectId;

use crate::database::server::{get_server_by_id, Server};
use crate::database::stats::StatValue::I64;
use crate::database::stats::{Stat, SPEED_STAT};
use crate::database::universe::{get_universe_by_id, Universe};
use crate::provisioning::server_setup::setup_server;
use crate::provisioning::{Created, ProvisionError, Provisioner};
use crate::universe::setup::setup_sub_command::SetupType;

/// Vitesse du temps par défaut : temps réel, 24 h par jour (I14).
pub const DEFAULT_TIME_MODIFIER: u32 = 100;
/// Bornes de la vitesse du temps acceptée à la création (en %).
pub const TIME_MODIFIER_RANGE: std::ops::RangeInclusive<u32> = 1..=10_000;

/// Paramètres de création d'un univers.
pub struct NewUniverse<'a> {
    pub name: &'a str,
    pub creator_id: u64,
    pub guild_id: u64,
    pub setup: SetupType,
    /// Vitesse du temps en %, `None` : temps réel.
    pub time_modifier: Option<u32>,
    /// Langue des noms de salons et de rôles.
    pub locale: &'a str,
    /// Brouillard sur la carte des joueurs (I17).
    pub fog: bool,
}

/// Valide le nom d'un univers : non vide, 100 caractères au plus.
pub fn valid_universe_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= 100).then(|| name.to_string())
}

/// Origine du temps de l'univers : minuit UTC du jour (midi de l'univers à 12 h UTC en temps réel).
pub fn time_origin_ms(now: chrono::DateTime<Utc>) -> u128 {
    Utc.from_utc_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap()).timestamp_millis() as u128
}

/// Crée l'univers, son premier serveur et la stat de vitesse (une transaction), puis installe le
/// serveur. En cas d'échec de l'installation, l'univers et ses documents sont aussi supprimés.
pub async fn create_universe(p: &mut Provisioner, new: NewUniverse<'_>) -> Result<(Universe, Server), ProvisionError> {
    let fail = |key: &'static str| ProvisionError { key, left_over: vec![] };
    let Some(name) = valid_universe_name(new.name) else { return Err(fail("create_universe__invalid_name")) };
    let time_modifier = new.time_modifier.unwrap_or(DEFAULT_TIME_MODIFIER);
    if !TIME_MODIFIER_RANGE.contains(&time_modifier) {
        return Err(fail("create_universe__invalid_time_modifier"));
    }
    match Universe::check_universe_limit(new.creator_id).await {
        Ok(true) => {}
        Ok(false) => return Err(fail("create_universe__universe_limit_reached")),
        Err(_) => return Err(fail("create_universe__check_universe_limit_failed")),
    }
    match get_server_by_id(new.guild_id).await {
        Ok(None) => {}
        Ok(Some(_)) => return Err(fail("create_universe__already_exist_for_this_server")),
        Err(_) => return Err(fail("create_universe__get_server_failed")),
    }

    let now = Utc::now();
    let universe = Universe {
        universe_id: ObjectId::new(),
        name,
        creator_id: new.creator_id,
        global_time_modifier: time_modifier,
        time_origin_timestamp: time_origin_ms(now),
        creation_timestamp: now.timestamp_millis() as u128, fog_enabled: new.fog
    };
    let mut server = Server::default().universe_id(universe.universe_id).server_id(new.guild_id).clone();
    server._id = ObjectId::new();
    let speed = Stat {
        _id: ObjectId::new(),
        universe_id: universe.universe_id,
        name: SPEED_STAT.to_string(),
        base_value: I64(3),
        formula: None,
        min: Some(I64(0)),
        max: Some(I64(999)),
        modifiers: vec![],
    };

    if let Err(key) = insert_universe_documents(&universe, &server, &speed).await {
        return Err(fail(key));
    }
    p.record_doc(Created::UniverseDocs { universe_id: universe.universe_id }).await;
    if let Err(e) = universe.setup_constraints().await {
        // Index : utiles mais non bloquants (comme la commande d'origine).
        eprintln!("[universe] constraints: {e:?}");
    }

    setup_server(p, &mut server, new.setup, new.locale).await?;
    Ok((universe, server))
}

async fn insert_universe_documents(universe: &Universe, server: &Server, speed: &Stat) -> Result<(), &'static str> {
    let client = crate::database::db_client::get_db_client().await;
    let mut session = client.start_session().await.map_err(|_| "create_universe__universe_insert_failed")?;
    session.start_transaction().await.map_err(|_| "create_universe__universe_insert_failed")?;
    let result: Result<(), &'static str> = async {
        universe.insert_universe_with_session(&mut session).await.map_err(|_| "create_universe__universe_insert_failed")?;
        server.insert_server_with_session(&mut session).await.map_err(|_| "create_universe__server_insert_failed")?;
        speed.insert_stat_with_session(&mut session).await.map_err(|_| "create_universe__speed_stat_insert_failed")?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => session.commit_transaction().await.map_err(|_| "create_universe__universe_insert_failed"),
        Err(key) => {
            let _ = session.abort_transaction().await;
            Err(key)
        }
    }
}

/// Rattache le serveur `guild_id` à l'univers `universe_id`, puis l'installe. En cas d'échec, le
/// rattachement est annulé.
pub async fn add_server(p: &mut Provisioner, universe_id: ObjectId, guild_id: u64, setup: SetupType, locale: &str) -> Result<Server, ProvisionError> {
    let fail = |key: &'static str| ProvisionError { key, left_over: vec![] };
    let universe = match get_universe_by_id(universe_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return Err(fail("create_character__no_universe_found")),
        Err(_) => return Err(fail("create_character__database_error")),
    };
    match get_server_by_id(guild_id).await {
        Ok(None) => {}
        Ok(Some(_)) => return Err(fail("create_universe__already_exist_for_this_server")),
        Err(_) => return Err(fail("create_universe__get_server_failed")),
    }
    match universe.check_server_limit().await {
        Ok(true) => {}
        Ok(false) => return Err(fail("exceed_limit_number_of_servers_per_universe")),
        Err(key) => return Err(fail(key)),
    }
    let mut server = Server::default().universe_id(universe_id).server_id(guild_id).clone();
    server._id = ObjectId::new();
    if server.insert_server().await.is_err() {
        return Err(fail("create_universe__server_insert_failed"));
    }
    p.record_doc(Created::ServerDoc { guild_id }).await;
    setup_server(p, &mut server, setup, locale).await?;
    Ok(server)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_universe_name_is_trimmed_and_bounded() {
        assert_eq!(valid_universe_name("  Moria "), Some("Moria".into()));
        assert_eq!(valid_universe_name("   "), None);
        assert_eq!(valid_universe_name(&"x".repeat(101)), None);
        assert!(valid_universe_name(&"é".repeat(100)).is_some());
    }

    #[test]
    fn the_time_origin_is_midnight_utc() {
        let t = Utc.with_ymd_and_hms(2026, 11, 19, 15, 42, 7).unwrap();
        assert_eq!(time_origin_ms(t), Utc.with_ymd_and_hms(2026, 11, 19, 0, 0, 0).unwrap().timestamp_millis() as u128);
    }

    #[test]
    fn real_time_is_the_default_speed() {
        assert_eq!(DEFAULT_TIME_MODIFIER, 100);
        assert!(TIME_MODIFIER_RANGE.contains(&DEFAULT_TIME_MODIFIER));
        assert!(!TIME_MODIFIER_RANGE.contains(&0));
    }

    /// Tests contre l'API REST Discord et MongoDB, sur `VERSEENGINE_TEST_GUILD_ID` (qui ne doit
    /// pas être relié à un univers dans la base de test).
    mod discord_integration {
        use super::*;
        use crate::database::db_client::get_db_client;
        use crate::database::db_namespace::{SERVERS_COLLECTION_NAME, STATS_COLLECTION_NAME, UNIVERSES_COLLECTION_NAME, VERSEENGINE_DB_NAME};
        use crate::test_support::{block_on, discord_http, random_id, required_env_u64};
        use mongodb::bson::doc;
        use serenity::all::{ChannelId, GuildId, Http};

        async fn count(coll: &str, filter: mongodb::bson::Document) -> u64 {
            get_db_client().await.database(VERSEENGINE_DB_NAME).collection::<mongodb::bson::Document>(coll).count_documents(filter).await.unwrap()
        }

        async fn channel_names(http: &Http, guild: u64) -> Vec<String> {
            http.get_channels(GuildId::new(guild)).await.unwrap().into_iter().map(|c| c.name).collect()
        }

        /// Univers créé depuis rien (I2, I14) avec installation complète et vitesse du temps
        /// choisie ; puis retour en arrière : plus rien sur Discord ni en base.
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn a_universe_is_created_with_a_full_setup_then_entirely_rolled_back() {
            block_on(async {
                let _ = get_db_client().await;
                let http = discord_http();
                let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
                let before = channel_names(&http, guild).await.len();
                let mut p = Provisioner::new(http.clone());
                let new = NewUniverse { name: "Test Moria", creator_id: random_id(), guild_id: guild, setup: SetupType::FullSetup, time_modifier: Some(200), locale: "fr", fog: false };
                let (universe, server) = create_universe(&mut p, new).await.expect("the universe is created");

                assert_eq!(universe.global_time_modifier, 200);
                assert_eq!(count(UNIVERSES_COLLECTION_NAME, doc! {"_id": universe.universe_id}).await, 1);
                assert_eq!(count(STATS_COLLECTION_NAME, doc! {"universe_id": universe.universe_id}).await, 1);
                let stored = get_server_by_id(guild).await.unwrap().expect("the server is linked");
                assert_eq!(stored.universe_id, universe.universe_id);
                assert!(stored.road_category_id.is_some() && stored.universal_time_channel_id.is_some() && stored.player_role_id.is_some());
                let names = channel_names(&http, guild).await;
                assert_eq!(names.len(), before + 11, "11 channels for a full setup");
                let road = http.get_channel(ChannelId::new(server.road_category_id.unwrap().id)).await.unwrap().guild().unwrap();
                assert_eq!(road.name, get_by_locale_fr("road_channel_name"));
                let roles = http.get_guild_roles(GuildId::new(guild)).await.unwrap();
                assert!(roles.iter().any(|r| r.id.get() == server.player_role_id.unwrap().id));
                let pos = |id: Option<crate::database::server::Id>| roles.iter().find(|r| r.id.get() == id.unwrap().id).unwrap().position;
                assert!(pos(server.admin_role_id) > pos(server.moderator_role_id), "admin above moderator");
                assert!(pos(server.moderator_role_id) > pos(server.spectator_role_id));
                assert!(pos(server.spectator_role_id) > pos(server.player_role_id));
                let created_roles = p.created().iter().filter(|c| matches!(c, Created::Role { .. })).count();
                assert_eq!(created_roles, 4);

                let left = p.rollback().await;
                assert!(left.is_empty(), "{left:?}");
                assert_eq!(channel_names(&http, guild).await.len(), before);
                let roles = http.get_guild_roles(GuildId::new(guild)).await.unwrap();
                assert!(!roles.iter().any(|r| r.id.get() == server.player_role_id.unwrap().id));
                assert_eq!(count(UNIVERSES_COLLECTION_NAME, doc! {"_id": universe.universe_id}).await, 0);
                assert_eq!(count(SERVERS_COLLECTION_NAME, doc! {"universe_id": universe.universe_id}).await, 0);
                assert_eq!(count(STATS_COLLECTION_NAME, doc! {"universe_id": universe.universe_id}).await, 0);
            });
        }

        /// Une installation partielle puis complète du même serveur : les rôles existants sont
        /// réutilisés (aucun rôle recréé, Q8), seuls les salons manquants sont créés.
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn a_second_setup_reuses_what_exists() {
            block_on(async {
                let _ = get_db_client().await;
                let http = discord_http();
                let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
                let mut first = Provisioner::new(http.clone());
                let new = NewUniverse { name: "Test reuse", creator_id: random_id(), guild_id: guild, setup: SetupType::PartialSetup, time_modifier: None, locale: "en-US", fog: false };
                let (universe, mut server) = create_universe(&mut first, new).await.expect("partial setup");
                assert_eq!(universe.global_time_modifier, DEFAULT_TIME_MODIFIER);
                let roles_before = server.player_role_id.unwrap().id;

                let mut second = Provisioner::new(http.clone());
                setup_server(&mut second, &mut server, SetupType::FullSetup, "en-US").await.expect("full setup");
                let new_roles = second.created().iter().filter(|c| matches!(c, Created::Role { .. })).count();
                let new_channels = second.created().iter().filter(|c| matches!(c, Created::Channel { .. })).count();
                assert_eq!(new_roles, 0, "no role is created again");
                assert_eq!(new_channels, 10, "only the complementary channels are created");
                assert_eq!(server.player_role_id.unwrap().id, roles_before);
                let road = http.get_channel(ChannelId::new(server.road_category_id.unwrap().id)).await.unwrap().guild().unwrap();
                assert_eq!(road.name, crate::translation::get_by_locale("en-US", "road_channel_name", None, None));

                assert!(second.rollback().await.is_empty());
                assert!(first.rollback().await.is_empty());
                assert_eq!(count(SERVERS_COLLECTION_NAME, doc! {"server_id": guild.to_string()}).await, 0);
            });
        }

        fn get_by_locale_fr(key: &str) -> String {
            crate::translation::get_by_locale("fr", key, None, None)
        }

        /// Validation : aucun appel à Discord pour un nom ou une vitesse invalide.
        #[test]
        #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
        fn invalid_parameters_create_nothing() {
            block_on(async {
                let _ = get_db_client().await;
                let mut p = Provisioner::new(discord_http());
                let guild = required_env_u64("VERSEENGINE_TEST_GUILD_ID");
                for (name, speed, key) in [(" ", None, "create_universe__invalid_name"), ("Ok", Some(0), "create_universe__invalid_time_modifier")] {
                    let new = NewUniverse { name, creator_id: random_id(), guild_id: guild, setup: SetupType::PartialSetup, time_modifier: speed, locale: "fr", fog: false };
                    let err = create_universe(&mut p, new).await.err().expect("refused");
                    assert_eq!(err.key, key);
                }
                assert!(p.created().is_empty());
            });
        }
    }
}
