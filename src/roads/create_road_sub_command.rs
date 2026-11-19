use serenity::all::{PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId};
use crate::provisioning::road::provision_road;
use crate::provisioning::Provisioner;
use tokio::join;
use crate::database::places::{check_existing_place};
use crate::database::road::count_non_secret_roads_for_place;
use crate::database::server::{get_server_by_id};
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::reply;

fn parse_channel_id(input: &str) -> Option<u64> {
    if let Ok(id) = input.parse::<u64>() {
        return Some(id);
    }
    if input.starts_with("<#") && input.ends_with('>') {
        if let Ok(id) = input[2..input.len() - 1].parse::<u64>() {
            return Some(id);
        }
    }
    None
}

#[poise::command(slash_command, required_permissions= "ADMINISTRATOR", guild_only, rename = "road_create_road")]
pub async fn create_road(
    ctx: Context<'_>,
    place_one: String,
    place_two: String,
    distance: u64,
    secret_channel: Option<bool>
) -> Result<(), Error> {
    let Ok(_) = ctx.defer().await else { return Err("reply__reply_failed".into()) };

    // Si place_one et place_two ne sont pas sur le même serveur, on vérifie qu'ils sont dans le même univers
    let result = _create_road(&ctx, place_one, place_two, distance, secret_channel).await;
    let Ok(_) = reply(ctx.clone(), result).await else { return Err("reply__reply_failed".into()) };
    Ok(())
}

///  Asynchronously creates a "road" between two places in a server's universe, represented by channels in a Discord guild.
///  The road creation sets up a channel and stores metadata in the database.
///
///  # Arguments
///
///  * `ctx` - The contextual information for the operation, which includes the guild and bot interaction data.
///  * `place_one` - The first `GuildChannel` that represents one end of the road.
///  * `place_two` - The second `GuildChannel` that represents the other end of the road.
///  * `distance` - The numeric distance between the two places.
///
///  # Returns
///
///  A `Result` containing a `&'static str` on success or an `Error` on failure with an appropriate error message.
///
///  # Errors
///
///  This function can return the following errors:
///
///  * `"create_road__server_not_found"`: The server information could not be found in the database.
///  * `"create_road__database_error"`: A database operation failed.
///  * `"create_place__place_one_not_found"`: The first place does not exist in the universe.
///  * `"create_place__place_two_not_found"`: The second place does not exist in the universe.
///  * `"create_road__create_channel_failed"`: Failed to create the road channel.
///  * `"create_road__insert_road_failed_rollback_success"`: Database insertion of the road failed, but created resources were successfully rolled back.
///  * `"create_road__insert_road_failed_rollback_channel_failed"`: Database insertion failed, and the created channel could not be rolled back.
///
///  # Process
///
///  1. Fetches the server from the database using the guild ID.
///  2. Verifies the existence of `place_one` and `place_two` in the server's universe.
///  3. Creates the road channel, hidden from @everyone (no road role any more, Q1: travellers get
///     an individual permission when they set off).
///  4. Inserts metadata related to the new road into the database.
///  5. Deletes the channel if the insertion fails.
///
///  # Example
///
///  ```rust
///  let result = _create_road(ctx, guild_channel_one, guild_channel_two, 100).await;
///  match result {
///      Ok(success_message) => println!("Road created successfully: {}", success_message),
///      Err(error_message) => eprintln!("Failed to create road: {}", error_message),
///  }
///  ```
pub async fn _create_road(ctx: &Context<'_>, place_one_str : String, place_two_str: String, distance: u64, secret_channel: Option<bool>) -> Result<&'static str, Error>{
    let guild_id = ctx.guild_id().unwrap();

    let place_one_id = parse_channel_id(&place_one_str).ok_or_else(|| Error::from("create_road__invalid_place_one"))?;
    let place_two_id = parse_channel_id(&place_two_str).ok_or_else(|| Error::from("create_road__invalid_place_two"))?;

    let server = get_server_by_id(guild_id.get()).await;
    let server = match server {
         Ok(server) => {
             match server {
                 None => {return Err("create_road__server_not_found".into())}
                 Some(serv) => {serv}
             }
         }
         Err(_) => {return Err("create_road__database_error".into())}
     };

    let universe_id = server.universe_id.clone();

    let check_place_one = check_existing_place(universe_id, place_one_id);
    let check_place_two = check_existing_place(universe_id, place_two_id);
    let (result_one, result_two) = join!(check_place_one, check_place_two);
    let place_one = match result_one {
        Ok(result) => {
            match result{
                None => {return Err("create_place__place_one_not_found".into())}
                Some(place) => {place}
            }
        }
        Err(_) => {return Err("create_road__database_error".into())}
    };

    let place_two = match result_two {
        Ok(result) => {
            match result{
                None => {return Err("create_place__place_two_not_found".into())}
                Some(place) => {place}
            }
        }
        Err(_) => {return Err("create_road__database_error".into())}
    };

    if place_one.universe_id != universe_id || place_two.universe_id != universe_id {
        return Err("create_road__universe_mismatch".into());
    }

    if let Ok(Some(_)) = server.clone().get_road(place_one.category_id, place_two.category_id).await {
        return Err("create_road__already_exists".into());
    }

    let secret_channel_value = secret_channel.unwrap_or(false);

    if !secret_channel_value {
        let count_one = count_non_secret_roads_for_place(universe_id, place_one.category_id).await.map_err(|_| Error::from("create_road__database_error"))?;
        let count_two = count_non_secret_roads_for_place(universe_id, place_two.category_id).await.map_err(|_| Error::from("create_road__database_error"))?;

        if count_one >= 25 || count_two >= 25 {
            return Err("create_road__limit_reached".into());
        }
    }

    let Some(road_category) = server.road_category_id else { return Err("create_road__server_not_found".into()) };

    // Même création que l'import de carte : salon sans rôle (Q1), puis document.
    let mut provisioner = Provisioner::new(ctx.serenity_context().http.clone());
    match provision_road(&mut provisioner, guild_id.get(), road_category.id, &place_one, &place_two, distance, secret_channel_value).await {
        Ok(_) => {
            provisioner.finish().await;
            Ok("create_road__success")
        }
        Err(e) if e.key == "create_road__create_channel_failed" => Err(e.key.into()),
        Err(e) if e.left_over.is_empty() => Err("create_road__insert_road_failed_rollback_success".into()),
        Err(_) => Err("create_road__insert_road_failed_rollback_channel_failed".into()),
    }
}

/// Permissions du salon d'une route : invisible pour @everyone. Les voyageurs reçoivent une
/// permission individuelle (Q1).
pub fn road_channel_permissions(guild_id: u64) -> Vec<PermissionOverwrite> {
    vec![PermissionOverwrite {
        allow: Permissions::empty(),
        deny: Permissions::VIEW_CHANNEL,
        kind: PermissionOverwriteType::Role(RoleId::new(guild_id)),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_road_channel_is_hidden_from_everyone_and_has_no_role() {
        let p = road_channel_permissions(42);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].kind, PermissionOverwriteType::Role(RoleId::new(42)));
        assert_eq!(p[0].deny, Permissions::VIEW_CHANNEL);
    }
}
