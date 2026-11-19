use crate::discord::poise_structs::{Context, Error};
use crate::travel::service;
use crate::travel::utils::validate_channel;
use crate::utility::reply::{reply, reply_with_args_and_ephemeral};

/// Arrête le mouvement actuel du groupe du joueur.
///
/// Seul le leader du groupe peut arrêter le voyage s'il est en cours sur une route.
/// La logique est dans [`service::stop`], partagée avec l'activité.
///
/// # Errors
/// Retourne une erreur si :
/// - Le joueur n'est pas dans le bon salon.
/// - Le joueur n'est pas le leader du groupe.
#[poise::command(slash_command, guild_only, rename = "travel_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    validate_channel(&ctx, author_id).await?;
    let guild_id = ctx.guild_id().ok_or("travel__server_not_found")?.get();

    match service::stop(guild_id, author_id).await {
        Ok(key) => {
            let _ = reply_with_args_and_ephemeral(ctx, Ok(key), None, true).await;
        }
        Err(e) if e.to_string() == "travel__not_in_move" => {
            let _ = reply(ctx, Ok("travel__not_in_move")).await;
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
