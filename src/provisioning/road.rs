//! Création d'une route : salon (sans rôle, Q1) puis document `roads`. Partagé par
//! `/road_create_road` et par l'import de carte.

use mongodb::bson::oid::ObjectId;
use serenity::all::{ChannelType, CreateChannel};

use crate::database::places::Place;
use crate::database::road::Road;
use crate::provisioning::place::discord_name;
use crate::provisioning::{Created, ProvisionError, Provisioner};
use crate::roads::create_road_sub_command::road_channel_permissions;

/// Nom d'une route : `lieu1-lieu2`, 100 caractères au plus.
pub fn road_name(one: &Place, two: &Place) -> String {
    discord_name(&format!("{}-{}", one.name, two.name))
}

/// Serveur qui n'héberge pas le salon, si la route relie deux serveurs.
pub fn other_server(host_guild_id: u64, one: &Place, two: &Place) -> Option<String> {
    if one.server_id == two.server_id {
        None
    } else if one.server_id == host_guild_id {
        Some(two.server_id.to_string())
    } else {
        Some(one.server_id.to_string())
    }
}

/// Crée la route entre `one` et `two`, son salon étant dans la catégorie `category_id` du serveur
/// `host_guild_id`. `distance` est conservée telle quelle (I8).
pub async fn provision_road(p: &mut Provisioner, host_guild_id: u64, category_id: u64, one: &Place, two: &Place, distance: u64, secret: bool) -> Result<Road, ProvisionError> {
    let name = road_name(one, two);
    let channel = CreateChannel::new(name.clone())
        .kind(ChannelType::Text)
        .permissions(road_channel_permissions(host_guild_id))
        .category(category_id);
    let channel = match p.create_channel(host_guild_id, channel).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[provisioning] road {name}: channel: {e:?}");
            return Err(p.fail("create_road__create_channel_failed").await);
        }
    };
    let road = Road {
        _id: ObjectId::new(),
        universe_id: one.universe_id,
        server_id: host_guild_id,
        server_two_id: other_server(host_guild_id, one, two),
        road_name: name,
        role_id: None,
        channel_id: channel.id.get(),
        place_one_id: one.category_id,
        place_two_id: two.category_id,
        distance,
        secret,
        modifiers: vec![],
    };
    if let Err(e) = insert_in_transaction(&road).await {
        eprintln!("[provisioning] road {}: database: {e:?}", road.road_name);
        return Err(p.fail("create_road__insert_road_failed_rollback_success").await);
    }
    p.record_doc(Created::RoadDoc { id: road._id }).await;
    Ok(road)
}

async fn insert_in_transaction(road: &Road) -> mongodb::error::Result<()> {
    let mut session = crate::database::db_client::get_db_client().await.start_session().await?;
    session.start_transaction().await?;
    match road.insert_with_session(&mut session).await {
        Ok(_) => session.commit_transaction().await,
        Err(e) => {
            let _ = session.abort_transaction().await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(server_id: u64, name: &str) -> Place {
        Place { _id: ObjectId::new(), universe_id: ObjectId::new(), server_id, category_id: 1, role: 1, name: name.into(), modifiers: vec![], central_channel_id: None }
    }

    #[test]
    fn a_road_between_two_servers_names_the_other_one() {
        let (a, b) = (place(1, "A"), place(2, "B"));
        assert_eq!(other_server(1, &a, &b), Some("2".into()));
        assert_eq!(other_server(2, &a, &b), Some("1".into()));
        assert_eq!(other_server(1, &a, &place(1, "C")), None);
    }

    #[test]
    fn a_road_is_named_after_its_places() {
        assert_eq!(road_name(&place(1, "Bree"), &place(1, "Archet")), "Bree-Archet");
        assert_eq!(road_name(&place(1, &"x".repeat(80)), &place(1, &"y".repeat(80))).chars().count(), 100);
    }
}
