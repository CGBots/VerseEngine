use std::future::Future;
use serenity::all::{ChannelId, ChannelType, CreateForumTag, EditChannel, GuildChannel, RoleId};
use crate::database::server::{Id, IdType, Server};
use crate::discord::channels::{create_channel, get_admin_category_permission_set, get_rp_character_permission_set, get_universal_time_permission_set, ITEM_TAG, PLACE_TAG, SPACE_TAG};
use crate::discord::poise_structs::{Context, Error};
use crate::tr;

/// Asynchronous function that sets up the necessary categories and channels for a server.
/// This function creates the required channel categories and text channels for the server setup
/// while resolving existing ones if already created. It follows a specific hierarchy and permissions setup.
///
/// # Parameters
/// - `ctx`: A reference to the async `Context` object for executing Discord operations such as API calls.
/// - `server`: A mutable reference to the server configuration that gets modified during the setup.
/// - `snapshot`: A snapshot of the original server configuration to serve as a fallback during the process.
///
/// # Returns
/// A `Result` containing:
/// - `Ok(&'a str)`: A string slice indicating successful setup.
/// - `Err(Error)`: An error if any portion of the setup fails.
///
/// # Errors
/// The function returns an error in the following cases:
/// - If permissions for categories cannot be obtained or set up.
/// - If any required category (admin, RP, NRP) or text channel
///   (e.g., log, commands, moderation, general) cannot be created.
/// - Discord API errors during channel/category creation or retrieval.
///
/// # Behavior
/// - The function attempts to create the following categories:
///   - Admin category
///   - Non-roleplay (NRP) category
///   - Roleplay (RP) category
/// - If a category already exists, it retrieves the data.
///   If the category ID is invalid, it creates the category anew.
/// - Within the Admin category, it attempts to create or retrieve channels:
///   - Log channel
///   - Commands channel
///   - Moderation channel
/// - An additional general channel for each of the NRP and RP categories is created.
/// - Tracks errors during the creation process and halts further processing in case of failure.
///
/// # Notes
/// - The function leverages pre-defined localized strings for category and channel names.
///
/// # Examples
/// ```rust
/// let mut server_config = configure_server();
/// let setup_result = complementary_setup(ctx, &mut server_config, snapshot).await;
///
/// match setup_result {
///     Ok(success_msg) => println!("Setup completed successfully: {}", success_msg),
///     Err(e) => eprintln!("Setup failed: {}", e),
/// }
/// ```
///
/// # Related Functions
/// - `create_channel`: Handles the actual API call for creating Discord channels.
/// - `get_admin_category_permission_set`: Configures the permission overrides for the Admin category.
///
/// # Context
/// Used in bots or services needing automated Discord server provisioning and setup. This function
/// follows best practices like resolving existing entities and minimal privilege access for channels.
///
/// # Dependencies
/// Relies on asynchronous Discord API interactions through the `Context` object, as well as utilities
/// for managing permissions, channel types, and localized translations.
pub async fn complementary_setup<'a>(ctx: &Context<'_>, server : &'a mut Server, snapshot: Server) -> Result<&'a str, Error> {
    // Tout ce qui est indispensable est vérifié AVANT de créer quoi que ce soit sur Discord :
    // un échec ici ne laisse donc aucun salon orphelin.
    let everyone_role = required_role(server.everyone_role_id)?;
    let spectator_role = required_role(server.spectator_role_id)?;
    let player_role = required_role(server.player_role_id)?;
    let moderator_role = required_role(server.moderator_role_id)?;
    let road_category_id = required_channel_id(server.road_category_id)?;
    let guild_id = ctx.guild_id().ok_or("setup__channel_setup_failed")?;

    let mut errors: Vec<&'static str> = vec![];

    let admin_category_permissions = get_admin_category_permission_set(everyone_role, spectator_role, player_role, moderator_role);

    let existing = server.admin_category_id;
    let admin_category_result = resolve_or_create(ctx, existing, "setup__admin_category_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "admin_category_name"), ChannelType::Category, 0, admin_category_permissions, None)
    }).await;
    if let (Ok((category, true)), Some(_)) = (&admin_category_result, existing) {
        server.admin_category_id((category.id.get(), IdType::Category));
    }

    let existing = server.nrp_category_id;
    let nrp_category_result = resolve_or_create(ctx, existing, "setup__nrp_category_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "nrp_category_name"), ChannelType::Category, 1, vec![], None)
    }).await;
    if let (Ok((category, true)), Some(_)) = (&nrp_category_result, existing) {
        server.nrp_category_id((category.id.get(), IdType::Category));
    }

    let existing = server.rp_category_id;
    let rp_category_result = resolve_or_create(ctx, existing, "setup__rp_category_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "rp_category_name"), ChannelType::Category, 1, vec![], None)
    }).await;
    if let (Ok((category, true)), Some(_)) = (&rp_category_result, existing) {
        server.rp_category_id((category.id.get(), IdType::Category));
    }

    let (Ok((admin_category, _)), Ok((nrp_category, _)), Ok((rp_category, _))) =
        (admin_category_result, nrp_category_result, rp_category_result)
    else {
        return Err("setup__roles_setup_failed".into())
    };

    // `errors` ne contient plus que ce qui concerne les salons créés ci-dessous.
    errors.clear();

    let admin_category_id = admin_category.id.get();
    let nrp_category_id = nrp_category.id.get();
    let rp_category_id = rp_category.id.get();

    let log_channel_result = resolve_or_create(ctx, server.log_channel_id, "setup__log_channel_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "log_channel_name"), ChannelType::Text, 0, vec![], Some(admin_category_id))
    }).await;

    let commands_channel_result = resolve_or_create(ctx, server.commands_channel_id, "setup__commands_channel_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "commands_channel_name"), ChannelType::Text, 0, vec![], Some(admin_category_id))
    }).await;

    let moderation_channel_result = resolve_or_create(ctx, server.moderation_channel_id, "setup__moderation_channel_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "moderation_channel_name"), ChannelType::Text, 0, vec![], Some(admin_category_id))
    }).await;

    let nrp_general_channel_result = resolve_or_create(ctx, server.nrp_general_channel_id, "setup__nrp_general_channel_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "nrp_general_channel_name"), ChannelType::Text, 0, vec![], Some(nrp_category_id))
    }).await;

    let character_channel_permissions = get_rp_character_permission_set(player_role);

    let rp_character_channel_result = resolve_or_create(ctx, server.rp_character_channel_id, "setup__rp_character_channel_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "rp_character_channel_name"), ChannelType::Text, 0, character_channel_permissions, Some(rp_category_id))
    }).await;

    let wiki_channel_result = resolve_or_create(ctx, server.rp_wiki_channel_id, "setup__wiki_channel_not_created", &mut errors, move || async move {
        let channel = create_channel(ctx, tr!(*ctx, "rp_wiki_channel_name"), ChannelType::Forum, 0, vec![], Some(rp_category_id)).await?;
        let _ = channel.clone().edit(ctx, EditChannel::new().available_tags(vec![
            CreateForumTag::new(PLACE_TAG),
            CreateForumTag::new(SPACE_TAG),
            CreateForumTag::new(ITEM_TAG),
        ])).await;
        Ok(channel)
    }).await;

    let universal_time_channel_permissions = get_universal_time_permission_set(everyone_role);

    let universal_time_channel_result = resolve_or_create(ctx, server.universal_time_channel_id, "setup__universal_time_channel_not_created", &mut errors, move || {
        create_channel(ctx, tr!(*ctx, "universal_time_channel_name"), ChannelType::Text, 0, universal_time_channel_permissions, Some(rp_category_id))
    }).await;

    if !errors.is_empty()  {
        server.rollback(ctx, snapshot).await;
        return Err("setup__channel_setup_failed".into())
    }

    let (
        Ok((log_channel, _)),
        Ok((commands_channel, _)),
        Ok((moderation_channel, _)),
        Ok((nrp_general_channel, _)),
        Ok((rp_character_channel, _)),
        Ok((wiki_channel, _)),
        Ok((universal_time_channel, _)),
    ) = (
        log_channel_result,
        commands_channel_result,
        moderation_channel_result,
        nrp_general_channel_result,
        rp_character_channel_result,
        wiki_channel_result,
        universal_time_channel_result,
    ) else {
        server.rollback(ctx, snapshot).await;
        return Err("setup__channel_setup_failed".into())
    };

    server.nrp_category_id(Id{ id: nrp_category.id.get(), id_type: IdType::Category });
    server.rp_category_id(Id{ id: rp_category.id.get(), id_type: IdType::Category });
    server.admin_category_id(Id{ id: admin_category.id.get(), id_type: IdType::Category });
    server.log_channel_id(Id{id: log_channel.id.get(), id_type: IdType::Channel });
    server.commands_channel_id(Id{id: commands_channel.id.get(), id_type: IdType::Channel });
    server.moderation_channel_id(Id{id: moderation_channel.id.get(), id_type: IdType::Channel });
    server.nrp_general_channel_id(Id{id: nrp_general_channel.id.get(), id_type: IdType::Channel });
    server.rp_character_channel_id(Id{id: rp_character_channel.id.get(), id_type: IdType::Channel });
    server.rp_wiki_channel_id(Id{id: wiki_channel.id.get(), id_type: IdType::Channel });
    server.universal_time_channel_id(Id{id: universal_time_channel.id.get(), id_type: IdType::Channel });

    // Réorganisation cosmétique des salons : un échec ici ne doit pas faire échouer tout le setup.
    match guild_id.channels(ctx).await {
        Ok(channels) => {
            let allowed = [
                admin_category.id.get(),
                nrp_category.id.get(),
                rp_category.id.get(),
                road_category_id.get(),
            ];
            let mut channel_order = vec![
                (admin_category.id, 0),
                (nrp_category.id, 1),
                (rp_category.id, 2),
                (road_category_id, 3),
            ];
            for (channel_id, _) in channels {
                if !allowed.contains(&channel_id.get()) {
                    channel_order.push((channel_id, 4))
                }
            }
            if let Err(e) = guild_id.reorder_channels(ctx, channel_order).await {
                tracing::warn!("complementary_setup: impossible de réorganiser les salons de la guilde {guild_id} : {e:?}");
            }
        }
        Err(e) => {
            tracing::warn!("complementary_setup: impossible de lister les salons de la guilde {guild_id} : {e:?}");
        }
    }

    match server.update().await {
        Ok(_) => {}
        Err(_) => {
            server.rollback(ctx, snapshot).await;
            return Err("setup__server_update_failed".into())}
    };

    Ok("setup__setup_success_message")
}

/// Retourne le salon/la catégorie Discord enregistré(e) dans `existing`, à condition qu'il/elle
/// existe encore et soit bien un salon de serveur. Sinon `None` (jamais de panic).
async fn fetch_existing_guild_channel(ctx: &Context<'_>, existing: Option<Id>) -> Option<GuildChannel> {
    let id = existing?;
    if id.id == 0 {
        return None;
    }
    ctx.http().get_channel(id.id.into()).await.ok()?.guild()
}

/// Résout un salon déjà enregistré, ou le crée s'il n'a jamais existé ou a été supprimé.
///
/// Retourne le salon et un booléen valant `true` s'il vient d'être créé.
/// En cas d'échec de création, `error_key` est ajouté à `errors` et l'erreur est propagée.
async fn resolve_or_create<F, Fut>(
    ctx: &Context<'_>,
    existing: Option<Id>,
    error_key: &'static str,
    errors: &mut Vec<&'static str>,
    create: F,
) -> serenity::Result<(GuildChannel, bool)>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = serenity::Result<GuildChannel>>,
{
    if let Some(channel) = fetch_existing_guild_channel(ctx, existing).await {
        return Ok((channel, false));
    }
    match create().await {
        Ok(channel) => Ok((channel, true)),
        Err(e) => {
            errors.push(error_key);
            Err(e)
        }
    }
}

/// Convertit un rôle optionnel du serveur en `RoleId`, ou renvoie l'erreur de setup des rôles.
fn required_role(role: Option<Id>) -> Result<RoleId, Error> {
    match role {
        Some(Id { id, .. }) if id != 0 => Ok(RoleId::new(id)),
        _ => Err("setup__roles_setup_failed".into()),
    }
}

/// Convertit un identifiant de salon optionnel du serveur en `ChannelId`, ou renvoie l'erreur de setup des salons.
fn required_channel_id(channel: Option<Id>) -> Result<ChannelId, Error> {
    match channel {
        Some(Id { id, .. }) if id != 0 => Ok(ChannelId::new(id)),
        _ => Err("setup__channel_setup_failed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u64) -> Option<Id> {
        Some(Id { id: value, id_type: IdType::Role })
    }

    #[test]
    fn required_role_accepts_a_stored_role() {
        assert_eq!(required_role(id(42)).unwrap(), RoleId::new(42));
    }

    #[test]
    fn required_role_rejects_missing_or_zero() {
        assert!(required_role(None).is_err());
        assert!(required_role(id(0)).is_err());
    }

    #[test]
    fn required_channel_id_rejects_missing_or_zero() {
        assert_eq!(required_channel_id(id(7)).unwrap(), ChannelId::new(7));
        assert!(required_channel_id(None).is_err());
        assert!(required_channel_id(id(0)).is_err());
    }

    #[test]
    fn setup_errors_use_known_translation_keys() {
        let err = required_role(None).unwrap_err().to_string();
        assert_eq!(err, "setup__roles_setup_failed");
        let err = required_channel_id(None).unwrap_err().to_string();
        assert_eq!(err, "setup__channel_setup_failed");
    }
}
