//! Migration des routes à rôle vers les permissions individuelles (décisions Q1 et Q7).
//!
//! Avant Q1, chaque route avait un rôle Discord qui ouvrait son salon. Désormais, chaque voyageur
//! reçoit une permission individuelle sur le salon de la route qu'il emprunte. Pour chaque route
//! qui a encore un `role_id` :
//! 1. les joueurs actuellement sur cette route reçoivent leur permission individuelle ;
//! 2. le rôle est supprimé de Discord (un rôle déjà supprimé compte comme un succès) ;
//! 3. `role_id` est retiré du document de la route.
//!
//! Chaque étape est idempotente : si le bot s'arrête au milieu, la migration reprend au
//! démarrage suivant. Une route dont le rôle n'a pas pu être supprimé garde son `role_id` et sera
//! retentée.

use std::collections::HashMap;

use futures::TryStreamExt;
use mongodb::bson::doc;
use serenity::all::{GuildId, Http, RoleId};

use crate::database::db_client::get_db_client;
use crate::database::db_namespace::{ROADS_COLLECTION_NAME, TRAVELS_COLLECTION_NAME, VERSEENGINE_DB_NAME};
use crate::database::road::Road;
use crate::database::travel::{SpaceType, TravelGroup};

/// Code d'erreur JSON de Discord pour « Unknown Role ».
const DISCORD_UNKNOWN_ROLE: isize = 10011;

/// Bilan d'une migration.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub migrated: usize,
    pub failed: usize,
    pub accesses_granted: usize,
}

/// Joueurs actuellement sur chaque route (salon de la route → membres), d'après les groupes de
/// voyage. Un groupe arrêté sur une route y est toujours : il garde l'accès, comme avec le rôle.
pub fn travellers_by_road(groups: &[TravelGroup]) -> HashMap<u64, Vec<u64>> {
    let mut by_road: HashMap<u64, Vec<u64>> = HashMap::new();
    for g in groups.iter().filter(|g| g.actual_space_type == SpaceType::Road) {
        let road = g.road_id.unwrap_or(g.actual_space_id);
        let members = by_road.entry(road).or_default();
        for m in &g.members {
            if !members.contains(m) {
                members.push(*m);
            }
        }
    }
    by_road
}

fn is_unknown_role(error: &serenity::Error) -> bool {
    matches!(
        error,
        serenity::Error::Http(serenity::http::HttpError::UnsuccessfulRequest(response))
            if response.error.code == DISCORD_UNKNOWN_ROLE
    )
}

/// Migre toutes les routes qui ont encore un rôle. Voir le module.
pub async fn migrate_road_roles(http: &Http) -> mongodb::error::Result<MigrationReport> {
    migrate_road_roles_where(http, doc! {}).await
}

/// Comme [`migrate_road_roles`], limité aux routes qui vérifient `filter` (utilisé par les tests).
pub async fn migrate_road_roles_where(http: &Http, filter: mongodb::bson::Document) -> mongodb::error::Result<MigrationReport> {
    let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
    let roads_coll = db.collection::<Road>(ROADS_COLLECTION_NAME);
    let travels_coll = db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME);

    let mut road_filter = doc! { "role_id": { "$exists": true, "$ne": null } };
    road_filter.extend(filter);
    let roads: Vec<Road> = roads_coll.find(road_filter).await?.try_collect().await?;
    let mut report = MigrationReport::default();
    if roads.is_empty() {
        return Ok(report);
    }

    let groups: Vec<TravelGroup> = travels_coll.find(doc! { "actual_space_type": "Road" }).await?.try_collect().await?;
    let travellers = travellers_by_road(&groups);

    for road in roads {
        let Some(role_id) = road.role_id else { continue };
        for &user_id in travellers.get(&road.channel_id).map(Vec::as_slice).unwrap_or(&[]) {
            crate::travel::logic::grant_road_access(http, road.channel_id, user_id).await;
            report.accesses_granted += 1;
        }
        let deleted = match http.delete_role(GuildId::new(road.server_id), RoleId::new(role_id), Some("VerseEngine: road roles replaced by individual permissions")).await {
            Ok(()) => true,
            Err(e) if is_unknown_role(&e) => true,
            Err(e) => {
                eprintln!("[road migration] road {}: cannot delete role {role_id}: {e:?}", road.channel_id);
                false
            }
        };
        if !deleted {
            report.failed += 1;
            continue;
        }
        roads_coll.update_one(doc! { "_id": road._id }, doc! { "$unset": { "role_id": "" } }).await?;
        travels_coll
            .update_many(doc! { "road_id": road.channel_id as i64 }, doc! { "$unset": { "road_role_id": "" } })
            .await?;
        report.migrated += 1;
    }
    println!("[road migration] {report:?}");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::oid::ObjectId;

    fn group(space: SpaceType, space_id: u64, road: Option<u64>, members: &[u64]) -> TravelGroup {
        TravelGroup {
            universe_id: ObjectId::new(),
            actual_space_type: space,
            actual_space_id: space_id,
            road_id: road,
            members: members.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn only_groups_on_a_road_keep_an_access() {
        let groups = [
            group(SpaceType::Road, 10, Some(10), &[1, 2]),
            group(SpaceType::Road, 10, Some(10), &[3]),
            group(SpaceType::Place, 99, None, &[4]),
            // arrêté sur la route 20 : il y est toujours
            group(SpaceType::Road, 20, Some(20), &[5]),
        ];
        let t = travellers_by_road(&groups);
        assert_eq!(t.len(), 2);
        assert_eq!(t[&10], vec![1, 2, 3]);
        assert_eq!(t[&20], vec![5]);
    }

    #[test]
    fn a_player_is_listed_once() {
        let groups = [group(SpaceType::Road, 10, Some(10), &[1, 1])];
        assert_eq!(travellers_by_road(&groups)[&10], vec![1]);
    }

    /// Route à rôle réelle sur le serveur de test : la migration pose la permission du voyageur
    /// (le propriétaire du serveur), supprime le rôle et retire `role_id`. Puis l'accès est
    /// retiré, deux fois (idempotence).
    #[test]
    #[ignore = "integration test: needs MongoDB, DISCORD_TOKEN and VERSEENGINE_TEST_GUILD_ID; run with --ignored"]
    fn a_road_role_is_migrated_to_an_individual_permission() {
        use crate::test_support::{block_on, discord_http, random_id, required_env_u64, TempChannel};
        use serenity::all::{EditRole, PermissionOverwriteType, UserId};
        block_on(async {
            let http = discord_http();
            let guild = GuildId::new(required_env_u64("VERSEENGINE_TEST_GUILD_ID"));
            let owner = http.get_guild(guild).await.unwrap().owner_id;
            let channel = TempChannel::create(&http, "test-road-migration").await;
            let role = guild.create_role(&http, EditRole::new().name("test-road-migration")).await.unwrap();

            let universe_id = ObjectId::new();
            let mut road = crate::travel::service::tests_support_road(universe_id, channel.id.get(), random_id(), random_id(), false);
            road.server_id = guild.get();
            road.role_id = Some(role.id.get());
            road.insert().await.unwrap();
            let mut traveller = group(SpaceType::Road, channel.id.get(), Some(channel.id.get()), &[owner.get()]);
            traveller._id = ObjectId::new();
            traveller.universe_id = universe_id;
            traveller.road_role_id = Some(role.id.get());
            traveller.clone().insert().await.unwrap();

            let report = migrate_road_roles_where(&http, doc! { "universe_id": universe_id }).await.unwrap();
            assert_eq!(report, MigrationReport { migrated: 1, failed: 0, accesses_granted: 1 });

            let has_access = |c: serenity::all::GuildChannel| {
                c.permission_overwrites.iter().any(|o| o.kind == PermissionOverwriteType::Member(UserId::new(owner.get())))
            };
            let c = http.get_channel(channel.id).await.unwrap().guild().unwrap();
            assert!(has_access(c), "the traveller can see the road");
            let roles = http.get_guild_roles(guild).await.unwrap();
            assert!(!roles.iter().any(|r| r.id == role.id), "the road role is deleted");
            let stored = crate::database::road::get_road_by_channel_id(universe_id, channel.id.get()).await.unwrap().unwrap();
            assert_eq!(stored.role_id, None);
            let g = crate::database::travel::get_group_of_user(universe_id, owner.get()).await.unwrap().unwrap();
            assert_eq!(g.road_role_id, None);

            // Une deuxième migration ne fait plus rien.
            let again = migrate_road_roles_where(&http, doc! { "universe_id": universe_id }).await.unwrap();
            assert_eq!(again, MigrationReport::default());

            crate::travel::logic::revoke_road_access(&http, channel.id.get(), owner.get()).await;
            crate::travel::logic::revoke_road_access(&http, channel.id.get(), owner.get()).await;
            let c = http.get_channel(channel.id).await.unwrap().guild().unwrap();
            assert!(!has_access(c), "the access is revoked");

            let db = get_db_client().await.database(VERSEENGINE_DB_NAME);
            db.collection::<Road>(ROADS_COLLECTION_NAME).delete_many(doc! { "universe_id": universe_id }).await.unwrap();
            db.collection::<TravelGroup>(TRAVELS_COLLECTION_NAME).delete_many(doc! { "universe_id": universe_id }).await.unwrap();
            channel.delete(&http).await;
        });
    }
}
