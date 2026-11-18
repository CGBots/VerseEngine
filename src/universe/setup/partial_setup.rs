use serenity::all::{ChannelType, Role, RoleId};
use crate::database::server::{Id, IdType, Server};
use crate::database::server::IdType::Category;
use crate::discord::channels::{create_channel, get_road_category_permission_set};
use crate::discord::poise_structs::{Context, Error};
use crate::discord::roles::{create_role, edit_role_positions, AdminRolePermissions, ModeratorRolePermissions, PlayerRolePermissions, SpectatorRolePermissions};
use crate::tr;

/// Performs a partial setup for a Discord server using the provided context and server configuration.
///
/// This asynchronous function sets up roles and channels for the server, ensuring proper configurations
/// for roles such as `Admin`, `Moderator`, `Spectator`, and `Player`. It also adjusts role positions
/// and creates a category channel for organizing related resources. If any setup step fails, the setup
/// is rolled back to maintain the integrity of the server state.
///
/// # Arguments
///
/// * `ctx` - The contextual object that provides access to the Discord API and tools for interacting
///   with the server.
/// * `server` - A mutable reference to the `Server` object representing the current state of the
///   server. It is updated during the setup process to reflect the new configuration.
/// * `snapshot` - A snapshot of the server's state before any changes have been made, used for
///   rollback if an error occurs.
///
/// # Returns
///
/// * `Ok(&'a str)` - A success message indicating that the setup completed successfully.
/// * `Err(Error)` - An error message describing the issue that occurred during setup.
///
/// # Errors
///
/// This function can return errors in a variety of scenarios:
/// * `"guild_only"` - The function was called in a context not associated with a server.
/// * `"partial_setup__get_guild_roles_error"` - Failed to retrieve existing guild roles from the API.
/// * `"setup__admin_role_not_created"` - Failed to create or retrieve the `Admin` role.
/// * `"setup__moderator_role_not_created"` - Failed to create or retrieve the `Moderator` role.
/// * `"setup__spectator_role_not_created"` - Failed to create or retrieve the `Spectator` role.
/// * `"setup__player_role_not_created"` - Failed to create or retrieve the `Player` role.
/// * `"setup__error_during_role_creation"` - One or more roles failed to be created or retrieved.
/// * `"setup__reorder_went_wrong"` - Failed to reorder the roles in the server.
/// * `"setup__road_category_not_created"` - Failed to create or retrieve the category channel.
/// * `"setup__server_update_failed"` - Failed to update the server configuration after setup.
///
/// # Rollback
///
/// If any error occurs during the setup process, the server's state is rolled back to its initial
/// configuration using the provided snapshot to prevent partial or inconsistent configurations.
///
/// # Example Usage
///
/// ```rust
/// // Assuming `ctx`, `server`, and `snapshot` are available.
/// match partial_setup(&ctx, &mut server, snapshot).await {
///     Ok(success_message) => println!("{}", success_message),
///     Err(e) => eprintln!("Setup failed: {}", e),
/// }
/// ```
///
/// # Notes
///
/// * This function is designed to operate within an asynchronous context.
/// * It relies on helper functions such as `create_role`, `edit_role_positions`, and
///   `create_channel` to manage server resources.
pub async fn partial_setup<'a>(ctx: &Context<'_>, server: &mut Server, snapshot: Server) -> Result<&'a str, Error> {
    //everyone role
    let Ok(guild_id) = ctx.guild_id().ok_or("guild_only") else { return Err("guild_only".into()) };
    let everyone_role = guild_id.everyone_role();
    
    let Ok(existing_roles) = ctx.http().get_guild_roles(guild_id).await else {return Err("partial_setup__get_guild_roles_error".into())};

    let mut roles_created: Vec<Role> = vec![];
    let mut errors: Vec<&str> = vec![];

    let admin_role = async {
        if let Some(role_id) = server.clone().admin_role_id {
            if let Ok(role) = guild_id.role(ctx, role_id.id.into()).await {
                return Ok(role);
            }
        }

        match create_role(ctx, tr!(*ctx, "admin_role_name"), *AdminRolePermissions).await {
            Ok(role) => {
                server.admin_role_id((role.id.get(), IdType::Role));
                roles_created.push(role.clone());
                Ok(role)
            }
            Err(e) => {
                errors.push("setup__admin_role_not_created");
                Err(e)
            }
        }
    }.await;


    let moderator_role = async {
        if let Some(role_id) = server.clone().moderator_role_id {
            if let Ok(role) = guild_id.role(ctx, role_id.id.into()).await {
                return Ok(role);
            }
        }

        match create_role(ctx, tr!(*ctx, "moderator_role_name"), *ModeratorRolePermissions).await {
            Ok(role) => {
                server.moderator_role_id((role.id.get(), IdType::Role));
                roles_created.push(role.clone());
                Ok(role)
            }
            Err(e) => {
                errors.push("setup__moderator_role_not_created");
                Err(e)
            }
        }
    }.await;


    let spectator_role = async {
        if let Some(role_id) = server.clone().spectator_role_id {
            if let Ok(role) = guild_id.role(ctx, role_id.id.into()).await {
                return Ok(role);
            }
        }

        match create_role(ctx, tr!(*ctx, "spectator_role_name"), *SpectatorRolePermissions).await {
            Ok(role) => {
                server.spectator_role_id((role.id.get(), IdType::Role));
                roles_created.push(role.clone());
                Ok(role)
            }
            Err(e) => {
                errors.push("setup__spectator_role_not_created");
                Err(e)
            }
        }
    }.await;


    let player_role = async {
        if let Some(role_id) = server.clone().player_role_id {
            if let Ok(role) = guild_id.role(ctx, role_id.id.into()).await {
                return Ok(role);
            }
        }

        match create_role(ctx, tr!(*ctx, "player_role_name"), *PlayerRolePermissions).await {
            Ok(role) => {
                server.player_role_id((role.id.get(), IdType::Role));
                roles_created.push(role.clone());
                Ok(role)
            }
            Err(e) => {
                errors.push("setup__player_role_not_created");
                Err(e)
            }
        }
    }.await;

    if !errors.is_empty() {
        server.rollback(ctx, snapshot).await;
        return Err("setup__error_during_role_creation".into())
    }

    let Ok(admin_role) = admin_role else { return Err("setup__admin_role_not_created".into()) };
    let Ok(moderator_role) = moderator_role else { return Err("setup__moderator_role_not_created".into()) };
    let Ok(spectator_role) = spectator_role else { return Err("setup__spectator_role_not_created".into()) };
    let Ok(player_role) = player_role else { return Err("setup__player_role_not_created".into()) };
    let everyone_role = everyone_role;

    let bot_id = ctx.cache().current_user().id;
    let bot_role = match guild_id.member(ctx.http(), bot_id).await {
        Ok(bot_member) => bot_member.roles.first().copied(),
        Err(_) => None,
    };
    let Some(bot_role) = bot_role else {
        // Le bot n'a pas de rôle propre (ou son membre est introuvable) : impossible de réordonner les rôles.
        server.rollback(ctx, snapshot).await;
        return Err("setup__reorder_went_wrong".into());
    };

    let mut roles_pos: Vec<(RoleId, Option<u64>)> = vec![(admin_role.id, Some(4)), (moderator_role.id, Some(3)), (spectator_role.id, Some(2)), (player_role.id, Some(1)), (bot_role, Some(5))];

    for role in &existing_roles {
        if role.id != everyone_role {
            roles_pos.push((role.id, Some((role.position + existing_roles.len() as u16).into())));
        }
    }
    
    let res = edit_role_positions(ctx, guild_id, roles_pos).await;

    match res {
        Ok(_) => {}
        Err(_) => {
            server.rollback(ctx, snapshot).await;
            return Err("setup__reorder_went_wrong".into())}
    }

    let permissions = get_road_category_permission_set(everyone_role, player_role.id, spectator_role.id, moderator_role.id);

    let result_road_category = match server.clone().road_category_id {
        None => { Err(create_channel(ctx, tr!(*ctx, "road_channel_name"), ChannelType::Category, 0, permissions, None).await) }
        Some(channel_id) => {
            match ctx.http().get_channel(channel_id.id.into()).await {
                Ok(channel) => { Ok(channel) }
                Err(_) => {
                    Err(create_channel(ctx, tr!(*ctx, "road_channel_name"), ChannelType::Category, 0, permissions, None).await)}
            }
        }
    };

    let road_category = match result_road_category {
        Ok(channel) => {
            match channel.guild() {
                Some(guild_channel) => guild_channel,
                None => {
                    server.rollback(ctx, snapshot).await;
                    return Err("setup__road_category_not_created".into());
                }
            }
        }
        Err(new_channel_result) => {
            match new_channel_result {
                Ok(channel) => {channel}
                Err(_) => {
                    server.rollback(ctx, snapshot).await;
                    return Err("setup__road_category_not_created".into()); }
            }
        }
    };
    
    server.admin_role_id(Id{ id: admin_role.id.get(), id_type: IdType::Role })
        .moderator_role_id(Id{ id: moderator_role.id.get(), id_type: IdType::Role })
        .spectator_role_id(Id{ id: spectator_role.id.get(), id_type: IdType::Role })
        .player_role_id(Id{ id: player_role.id.get(), id_type: IdType::Role })
        .everyone_role_id(Id{ id: everyone_role.get(), id_type: IdType::Role })
        .road_category_id(Id{ id: road_category.id.get(), id_type: Category });
    
    let update_result = server.update().await;

    if update_result.is_err() {
        server.rollback(ctx, snapshot).await;
        return Err("setup__server_update_failed".into());
    };

    // Create universal invite URL
    if let Ok(channels) = guild_id.channels(ctx).await {
        let mut channels: Vec<_> = channels.into_values().collect();
        channels.sort_by_key(|c| c.position);
        let target_channel = channels.iter().find(|c| c.is_text_based() && c.kind != serenity::all::ChannelType::Voice && c.kind != serenity::all::ChannelType::Stage);
        if let Some(target_channel) = target_channel {
            if let Ok(invite) = target_channel.id.create_invite(ctx, serenity::all::CreateInvite::new().max_age(0).max_uses(0).unique(true)).await {
                server.universal_invite_url = Some(invite.url());
                let _ = server.update().await;
            }
        }
    }

    Ok("setup__setup_success_message")
}
