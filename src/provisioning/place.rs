//! Création d'un lieu : rôle, catégorie et salon générique (I5, Q5), puis document `places`.
//! Partagé par `/place_create_place` et par l'import de carte.

use mongodb::bson::oid::ObjectId;
use serenity::all::{ChannelType, CreateChannel, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId};

use crate::database::places::Place;
use crate::provisioning::{Created, ProvisionError, Provisioner};
use crate::translation::get_by_locale;

/// Permissions de la catégorie d'un lieu : visible par son rôle, cachée à @everyone.
pub fn place_category_permissions(guild_id: u64, place_role: RoleId) -> Vec<PermissionOverwrite> {
    vec![
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Role(place_role),
        },
        PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(RoleId::new(guild_id)),
        },
    ]
}

/// Nom d'un objet Discord : 100 caractères au plus (limite de Discord), sans couper un caractère.
pub fn discord_name(name: &str) -> String {
    name.trim().chars().take(100).collect()
}

/// Nom du salon générique d'un lieu dans la langue du serveur (`fr` → `place-centrale`, sinon
/// `central-square`).
pub fn central_channel_name(guild_locale: &str) -> String {
    get_by_locale(guild_locale, "place_central_channel_name", None, None)
}

/// Crée le lieu `name` sur le serveur `guild_id` de l'univers `universe_id`. En cas d'échec, tout
/// ce qui a été créé **par ce provisioner** est défait.
pub async fn provision_place(p: &mut Provisioner, universe_id: ObjectId, guild_id: u64, name: &str, guild_locale: &str) -> Result<Place, ProvisionError> {
    let name = discord_name(name);
    if name.is_empty() {
        return Err(p.fail("create_place__invalid_name").await);
    }
    let role = match p.create_role(guild_id, &name, None).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[provisioning] place {name}: role: {e:?}");
            return Err(p.fail("create_place__role_not_created").await);
        }
    };
    let category = CreateChannel::new(name.clone())
        .kind(ChannelType::Category)
        .permissions(place_category_permissions(guild_id, role.id));
    let category = match p.create_channel(guild_id, category).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[provisioning] place {name}: category: {e:?}");
            return Err(p.fail("create_place__category_not_created").await);
        }
    };
    // Sans permissions propres, le salon est synchronisé avec sa catégorie.
    let central = CreateChannel::new(central_channel_name(guild_locale)).kind(ChannelType::Text).category(category.id);
    let central = match p.create_channel(guild_id, central).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[provisioning] place {name}: central channel: {e:?}");
            return Err(p.fail("create_place__channel_not_created").await);
        }
    };

    let place = Place {
        _id: ObjectId::new(),
        universe_id,
        server_id: guild_id,
        category_id: category.id.get(),
        role: role.id.get(),
        name: category.name.clone(),
        modifiers: vec![],
        central_channel_id: Some(central.id.get()),
    };
    if let Err(e) = insert_in_transaction(&place).await {
        eprintln!("[provisioning] place {name}: database: {e:?}");
        return Err(p.fail("create_place__database_error").await);
    }
    p.record_doc(Created::PlaceDoc { id: place._id }).await;
    Ok(place)
}

async fn insert_in_transaction(place: &Place) -> mongodb::error::Result<()> {
    let mut session = crate::database::db_client::get_db_client().await.start_session().await?;
    session.start_transaction().await?;
    match place.insert_with_session(&mut session).await {
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

    #[test]
    fn the_central_channel_follows_the_server_language() {
        assert_eq!(central_channel_name("fr"), "place-centrale");
        assert_eq!(central_channel_name("en-US"), "central-square");
        assert_eq!(central_channel_name("de"), "central-square");
    }

    #[test]
    fn names_are_cut_at_100_characters_without_splitting_a_character() {
        let long = "é".repeat(150);
        let n = discord_name(&long);
        assert_eq!(n.chars().count(), 100);
        assert_eq!(discord_name("  Bree  "), "Bree");
    }

    #[test]
    fn a_place_category_is_only_visible_to_its_role() {
        let p = place_category_permissions(7, RoleId::new(9));
        assert_eq!(p[0].kind, PermissionOverwriteType::Role(RoleId::new(9)));
        assert!(p[0].allow.contains(Permissions::VIEW_CHANNEL));
        assert_eq!(p[1].kind, PermissionOverwriteType::Role(RoleId::new(7)));
        assert_eq!(p[1].deny, Permissions::VIEW_CHANNEL);
    }
}
