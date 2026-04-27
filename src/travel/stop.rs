use crate::discord::poise_structs::{Context, Error};
use crate::database::server::get_server_by_id;
use crate::database::travel::SpaceType;
use crate::travel::logic::stop_travel;
use crate::travel::utils::validate_channel;
use crate::utility::reply::reply;

#[poise::command(slash_command, guild_only, rename = "travel_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    
    validate_channel(&ctx, author_id).await?;

    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let player_move = server.clone().get_player_move(author_id).await?
        .ok_or("travel__character_not_found")?;

    if player_move.is_in_move && player_move.actual_space_type == SpaceType::Road {
        if !player_move.members.is_empty() && player_move.members[0] != author_id {
            return Err("travel__only_leader_can_stop".into());
        }

        match stop_travel(author_id).await {
            Ok(_) => {
                let _ = reply(ctx, Ok("travel__stopped")).await;
            },
            Err(_) => {
                let _ = reply(ctx, Ok("travel__not_in_move")).await;
            }
        }
    } else {
        let _ = reply(ctx, Ok("travel__not_in_move")).await;
    }

    Ok(())
}
