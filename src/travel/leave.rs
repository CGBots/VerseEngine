use crate::discord::poise_structs::{Context, Error};
use crate::travel::service::{self, Actor, Announce};
use crate::travel::utils::validate_channel;
use crate::utility::reply::reply_with_args_and_ephemeral;

/// Permet à un joueur de quitter son groupe actuel.
///
/// L'utilisateur qui quitte le groupe crée un nouveau groupe individuel à sa position actuelle.
/// Si le groupe était en mouvement, le mouvement est mis à jour pour les deux groupes résultants.
/// La logique est dans [`service::leave`], partagée avec l'activité.
///
/// # Errors
/// Retourne une erreur si :
/// - Le joueur est déjà seul dans son groupe.
/// - Le serveur ou le personnage n'est pas trouvé.
#[poise::command(slash_command, guild_only, rename = "travel_leave")]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    validate_channel(&ctx, user_id).await?;
    let guild_id = ctx.guild_id().ok_or("travel__server_not_found")?.get();
    let actor = Actor { user_id, fallback_name: &ctx.author().name, locale: ctx.locale().unwrap_or("fr") };

    let key = service::leave(&ctx.serenity_context().http, guild_id, actor, Announce::In(ctx.channel_id())).await?;
    let _ = reply_with_args_and_ephemeral(ctx, Ok(key), None, true).await;
    Ok(())
}
