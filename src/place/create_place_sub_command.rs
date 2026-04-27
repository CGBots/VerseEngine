use futures::TryStreamExt;
use serenity::all::{CreateChannel, CreateEmbed, CreateForumPost, CreateMessage, EditRole, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId};
use serenity::all::ChannelType::Category;
use crate::database::places::Place;
use crate::database::server::{get_server_by_id, Server};
use crate::discord::channels::PLACE_TAG;
use crate::discord::poise_structs::{Context, Error};
use crate::tr;
use crate::utility::reply::reply;

#[poise::command(slash_command, required_permissions= "ADMINISTRATOR", guild_only, rename = "place_create_place")]
pub async fn create_place(
    ctx: Context<'_>,
    name: String
) -> Result<(), Error>{
    let Ok(_) = ctx.defer().await else { return Err("reply__reply_failed".into()) };
    let result = _create_place(&ctx, name).await;
    let Ok(_) = reply(ctx, result).await else { return Err("reply__reply_failed".into()) };
    Ok(())
}

/// Asynchronously creates a new "place" within the given server context.
///
/// This function performs several steps to create a "place," which consists of:
/// - Creating a new server role.
/// - Creating a new channel category associated with the role.
/// - Ensuring proper permissions and relationships between the role and channel.
/// - Persisting the "place" data in the database.
///
/// If any step fails, the function attempts to roll back changes to leave the server in a consistent state.
///
/// # Arguments
/// - `ctx`: The context of the current operation, used to interact with the server and manage permissions.
/// - `name`: The desired name for the "place" (role and channel).
///
/// # Returns
/// - `Ok(&'static str)`: A success message indicating that the "place" was created successfully.
/// - `Err(Error)`: An error message/code describing why the operation failed.
///
/// # Errors
/// - `"create_place__server_not_found"`: The server was not found in the database.
/// - `"create_place__database_not_found"`: A database issue occurred while fetching the server.
/// - `"create_place__role_not_created"`: The role creation failed in the server.
/// - `"create_place__rollback_complete"`: Rollback successfully completed after a failure.
/// - `"create_role__rollback_failed"`: Rollback of either the role or channel failed.
///
/// # Rollback Behavior
/// - If an error occurs during the creation of the role or the channel:
///   - The function attempts to delete any created roles and channels.
///   - If rollback also fails, an appropriate error describing the failure is returned.
///
/// # Example
/// ```rust
/// let result = _create_place(&ctx, "My New Place".to_string()).await;
/// match result {
///     Ok(success_message) => println!("Success: {}", success_message),
///     Err(error_message) => eprintln!("Error: {}", error_message),
/// }
/// ```
pub async fn _create_place(ctx: &Context<'_>, name: String) -> Result<&'static str, Error>{
    let guild_id = ctx.guild_id().unwrap();
    let result = get_server_by_id(guild_id.get()).await;
    let server = match result {
        Ok(universe_result) => {
            match universe_result{
                None => {return Err("create_place__server_not_found".into())}
                Some(server) => {server}
            }
        }
        Err(_) => {return Err("create_place__database_not_found".into())}
    };

    let new_role = EditRole::new()
        .name(name.clone())
        .position(0)
        .audit_log_reason("Create new place");

    let mut role = match guild_id.create_role(ctx, new_role).await {
        Ok(role) => {role}
        Err(_) => {return Err("create_place__role_not_created".into())}
    };

    let permissions = vec![PermissionOverwrite {
        allow: Permissions::VIEW_CHANNEL
            | Permissions::SEND_MESSAGES
            | Permissions::READ_MESSAGE_HISTORY,
        deny: Permissions::empty(),
        kind: PermissionOverwriteType::Role(role.id),
    },
    PermissionOverwrite {
        allow: Permissions::empty(),
        deny: Permissions::VIEW_CHANNEL,
        kind: PermissionOverwriteType::Role(RoleId::new(guild_id.get())),
    }];

    let new_channel = CreateChannel::new(name.clone())
            .kind(Category)
            .permissions(permissions);

    let new_place = match guild_id.create_channel(ctx, new_channel).await {
        Ok(channel) => {channel}
        Err(_) => {
            match role.delete(ctx).await {
                Ok(_) => {return Err("create_place__rollback_complete".into())}
                Err(_) => {return Err("create_role__rollback_failed".into())}
            };
        }
    };

    let embed = CreateEmbed::new()
        .title(name.clone().to_string())
        .field(tr!(ctx.clone(), "create_place__channel_id"), "`".to_string() + new_place.clone().id.get().to_string().as_str() + "`", true);

    let Ok(servers_cursor) = server.get_other_servers().await else {return Err("create_place__servers_not_found".into())};
    let Ok(servers) = servers_cursor.try_collect::<Vec<Server>>().await else {return Err("create_place__server_collect_failed".into())};
    for server in servers {
        if let Some(wiki_channel_id) = server.rp_wiki_channel_id{
            let Ok(wiki_channel) = ctx.http().get_channel(wiki_channel_id.id.into()).await else {continue};
            let channel = wiki_channel.guild().unwrap();
            let place_tag = channel.available_tags.iter().find(|tag| tag.name == PLACE_TAG);
            let mut post = CreateForumPost::new(tr!(ctx.clone(), "create_place__new_place_title", place_name: name.clone()).to_string(), CreateMessage::new().embed(embed.clone()));
            if let Some(tag) = place_tag {
                post = post.add_applied_tag(tag.id);
            }
            let _ = channel.create_forum_post(ctx, post).await;
        }

    }

    let place = Place{
        _id: Default::default(),
        universe_id: server.universe_id,
        server_id: server.server_id,
        category_id: new_place.id.get(),
        role: role.id.get(),
        name: new_place.name.clone(),
        modifiers: vec![],
    };

    let mut session = crate::database::db_client::get_db_client().await.start_session().await?;
    session.start_transaction().await?;

    match place.insert_with_session(&mut session).await {
        Ok(_) => {
            session.commit_transaction().await?;
            Ok("create_place__success")
        }
        Err(_) => {
            session.abort_transaction().await?;
            match role.delete(ctx).await {
                Ok(_) => {}
                Err(_) => {return Err("create_role__rollback_failed".into())}
            };

            match new_place.delete(ctx).await {
                Ok(_) => {Err("create_place__rollback_complete".into())}
                Err(_) => {Err("create_role__rollback_failed".into())}
            }
        }
    }
}