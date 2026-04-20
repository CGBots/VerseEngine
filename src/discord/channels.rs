use serenity::all::{ChannelType, CreateChannel, GuildChannel, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId};
use poise::serenity_prelude::Builder;
use crate::discord::poise_structs::Context;

pub static SPACE_TAG: &str = "space";
pub static PLACE_TAG: &str = "place";
pub static ITEM_TAG: &str = "item";
pub static RECIPE_TAG: &str = "recipe";

/// Generates a set of permission overwrites for a "road" category to control access for different user roles.
///
/// # Arguments
/// - `everyone_role_id` - The `RoleId` representing the "everyone" role, which typically includes all users in the server.
/// - `player_role_id` - The `RoleId` representing the "player" role to restrict access for characters.
/// - `spectator_role_id` - The `RoleId` representing the "spectator" role to grant access for spectators.
/// - `moderator_role_id` - The `RoleId` representing the "moderator" role to grant access for moderators.
///
/// # Returns
/// A `Vec<PermissionOverwrite>` containing permission rules:
/// - Denies `VIEW_CHANNEL` for the `player_role_id`.
/// - Denies `VIEW_CHANNEL` for the `everyone_role_id`.
/// - Allows `VIEW_CHANNEL` for the `spectator_role_id`.
/// - Allows `VIEW_CHANNEL` for the `moderator_role_id`.
///
/// # Example
/// ```rust
/// let permissions = get_road_category_permission_set(
///     RoleId(1), 
///     RoleId(2), 
///     RoleId(3), 
///     RoleId(4)
/// );
///
/// // Permissions will now hold a set of permission overwrites as configured.
/// ```
pub fn get_road_category_permission_set(everyone_role_id: RoleId, player_role_id: RoleId, spectator_role_id: RoleId, moderator_role_id: RoleId) -> Vec<PermissionOverwrite> {
    vec![
        PermissionOverwrite {
            allow: Permissions::default(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(player_role_id)
        },
        PermissionOverwrite {
            allow: Permissions::default(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(everyone_role_id)
        },
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::default(),
            kind: PermissionOverwriteType::Role(spectator_role_id)
        },
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::default(),
            kind: PermissionOverwriteType::Role(moderator_role_id)
        }
    ]
}

/// Generates a set of permission overwrites for a specific administrative category in a system.
///
/// This function creates and returns a `Vec` of `PermissionOverwrite` objects
/// that define the permissions for various roles. The permissions configured are primarily related
/// to the ability to view the channel associated with the category.
///
/// # Parameters
/// - `everyone_role_id`: The `RoleId` representing the default "everyone" role.
/// - `spectator_role_id`: The `RoleId` representing the spectator role.
/// - `player_role_id`: The `RoleId` representing the player role.
/// - `moderator_role_id`: The `RoleId` representing the moderator role.
///
/// # Returns
/// A `Vec<PermissionOverwrite>` where each entry configures the `VIEW_CHANNEL` permission as follows:
/// - Deny `VIEW_CHANNEL` permission for `everyone_role_id`, `spectator_role_id`, and `player_role_id`.
/// - Allow `VIEW_CHANNEL` permission for `moderator_role_id`.
///
/// # Example
/// ```rust
/// let permission_overwrites = get_admin_category_permission_set(
///     everyone_role_id,
///     spectator_role_id,
///     player_role_id,
///     moderator_role_id
/// );
/// // `permission_overwrites` will now contain permission configurations for the roles.
/// ```
///
/// # Note
/// This function assumes that the input role IDs are valid and exist in the application's context.
///
/// # Important
/// - Permissions for the "everyone," "spectator," and "player" roles are set to deny `VIEW_CHANNEL`.
/// - Permissions for the "moderator" role are set to allow `VIEW_CHANNEL`.
pub fn get_admin_category_permission_set(everyone_role_id: RoleId, spectator_role_id: RoleId, player_role_id: RoleId, moderator_role_id: RoleId) -> Vec<PermissionOverwrite>{
    vec ! [
        PermissionOverwrite{
            allow: Permissions::default(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(everyone_role_id)
        },
        PermissionOverwrite{
            allow: Permissions::default(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(spectator_role_id)
        },
        PermissionOverwrite{
            allow: Permissions::default(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(player_role_id)
        },
        PermissionOverwrite{
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::default(),
            kind: PermissionOverwriteType::Role(moderator_role_id)
        }
    ]
}

/// Generates a vector containing a single permission overwrite for a role in a role-playing 
/// context, restricting the ability to view a channel.
///
/// # Arguments
///
/// * `player_role_id` - The `RoleId` of the player role that the permission overwrite applies to.
///
/// # Returns
///
/// Returns a `Vec<PermissionOverwrite>` containing one element where:
/// - `allow` is set to the default permissions (none explicitly allowed),
/// - `deny` restricts the `VIEW_CHANNEL` permission,
/// - `kind` is set to the specified role identified by `player_role_id`.
///
/// # Example
///
/// ```rust
/// let role_id = RoleId(12345);
/// let permission_set = get_rp_character_permission_set(role_id);
/// assert_eq!(permission_set.len(), 1);
/// assert!(permission_set[0].deny.contains(Permissions::VIEW_CHANNEL));
/// assert!(permission_set[0].allow.is_empty());
/// ```
///
/// This function is commonly used in role-playing systems to dynamically manage channel access
/// for specific roles.
pub fn get_rp_character_permission_set(player_role_id: RoleId) -> Vec<PermissionOverwrite> {
    vec![
        PermissionOverwrite {
            allow: Permissions::default(),
            deny: Permissions::VIEW_CHANNEL,
            kind: PermissionOverwriteType::Role(player_role_id)
        }
    ]
}

pub fn get_universal_time_permission_set(everyone_role_id: RoleId) -> Vec<PermissionOverwrite> {
    vec![
        PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY,
            deny: Permissions::SEND_MESSAGES,
            kind: PermissionOverwriteType::Role(everyone_role_id),
        },
    ]
}

/// Asynchronously creates a new channel in a Discord guild.
///
/// # Parameters
///
/// - `ctx`: A reference to the context of the current command or operation, holding
///          necessary data like the HTTP client and guild information.
/// - `channel_name`: The name of the channel to be created.
/// - `channel_type`: The type of the channel to be created (e.g., text, voice, or category).
/// - `position`: The position of the channel in the guild's channel listing.
/// - `permissions`: A vector of `PermissionOverwrite` objects specifying permission 
///                  overrides for roles or users within the new channel.
/// - `category`: An optional category ID to organize the channel into a specific category.
///
/// # Returns
///
/// This function returns a `Result` wrapping a `GuildChannel` object on success, or a
/// `serenity::Error` if the operation fails.
///
/// # Behavior
///
/// - A new channel is created with the specified `channel_name`, `channel_type`, `position`,
///   and `permissions`.
/// - If the `channel_type` is not a category and a valid `category` ID is provided, the
///   channel will be placed within the specified category.
/// - Executes the operation using the guild's ID and the provided context.
///
/// # Errors
///
/// - Returns an error if the HTTP request to create the channel fails.
/// - Returns an error if the guild ID is not present in the context.
///
/// # Examples
///
/// ```rust
/// use serenity::model::id::ChannelId;
/// use serenity::model::channel::{ChannelType, PermissionOverwrite, PermissionOverwriteType};
/// use serenity::model::permissions::Permissions;
/// use serenity::prelude::*;
///
/// let permissions = vec![
///     PermissionOverwrite {
///         allow: Permissions::SEND_MESSAGES,
///         deny: Permissions::empty(),
///         kind: PermissionOverwriteType::Role(RoleId(123456789012345678)),
///     },
/// ];
/// let channel = create_channel(&ctx, "general".to_string(), ChannelType::Text, 0, permissions, None).await?;
/// println!("Created channel ID: {:?}", channel.id);
/// ```
pub async fn create_channel(ctx: &Context<'_>, channel_name: String, channel_type: ChannelType, position: u16, permissions: Vec<PermissionOverwrite>, category: Option<u64>) -> serenity::Result<GuildChannel> {
    let mut channel = CreateChannel::new(channel_name)
        .kind(channel_type)
        .position(position)
        .permissions(permissions);

    if channel_type != ChannelType::Category {
        if let Some(cat) = category {
            channel = channel.category(cat);
        }
    }
    
    channel.execute(ctx.http(), ctx.guild_id().unwrap()).await
}
