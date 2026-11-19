use crate::discord::poise_structs::{Context, Error};
use crate::travel::service::{self, Actor, Announce};
use crate::travel::utils::validate_channel;
use crate::utility::reply::reply_with_args_and_ephemeral;
use serenity::all as serenity;

/// Permet à un joueur de rejoindre le groupe d'un autre joueur.
///
/// Les deux joueurs doivent se trouver au même endroit (Lieu) ou à proximité sur une même Route.
/// Le seuil de proximité est calculé dynamiquement via `get_travel_threshold`.
/// La logique est dans [`service::join`], partagée avec l'activité.
///
/// # Errors
/// Retourne une erreur si :
/// - Le joueur tente de se rejoindre lui-même.
/// - Les joueurs ne sont pas dans le même lieu ou sur la même route.
/// - La distance entre les deux est supérieure au seuil autorisé.
#[poise::command(slash_command, guild_only, rename = "travel_join")]
pub async fn join(
    ctx: Context<'_>,
    #[description = "Le joueur dont vous souhaitez rejoindre le groupe"]
    target: serenity::User,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    if author_id == target.id.get() {
        return Err("travel__cannot_join_self".into());
    }
    validate_channel(&ctx, author_id).await?;
    let guild_id = ctx.guild_id().ok_or("travel__server_not_found")?.get();
    let actor = Actor { user_id: author_id, fallback_name: &ctx.author().name, locale: ctx.locale().unwrap_or("fr") };

    let key = service::join(&ctx.serenity_context().http, guild_id, actor, target.id.get(), &target.name, Announce::In(ctx.channel_id())).await?;
    let _ = reply_with_args_and_ephemeral(ctx, Ok(key), None, true).await;
    Ok(())
}
