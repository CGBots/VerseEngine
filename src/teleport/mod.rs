//! `/teleport` : commandes d'administration pour déplacer instantanément des joueurs.
//!
//! - `/teleport player <joueur> <destination>` : un joueur, où qu'il soit (lieu, route, voyage en
//!   cours, ou lieu supprimé). S'il voyage en groupe, il le quitte ; les autres continuent.
//! - `/teleport place <origine> <destination>` : tous les groupes arrêtés dans un lieu.
//!
//! La logique est dans [`service`], partagée avec l'activité (mode admin) : ici ne restent que
//! les commandes, l'autocomplétion et la mise en forme des réponses.

pub mod service;

use fluent::FluentArgs;
use mongodb::bson::oid::ObjectId;
use poise::serenity_prelude as serenity;
use serenity::all::User;

use crate::database::server::get_server_by_id;
use crate::discord::poise_structs::{Context, Error};
use crate::utility::reply::{reply, reply_with_args};
use service::{all_places, find_place, matching_places};

// `default_member_permissions` : Discord cache la commande aux non-admins ; `required_permissions`
// sur chaque sous-commande reste la vraie vérification.
#[poise::command(slash_command, subcommands("player", "place"), subcommand_required, default_member_permissions = "ADMINISTRATOR", rename = "teleport")]
pub async fn teleport(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Téléporte un joueur vers un lieu de l'univers.
#[poise::command(slash_command, required_permissions = "ADMINISTRATOR", guild_only, rename = "teleport_player")]
pub async fn player(
    ctx: Context<'_>,
    user: User,
    #[autocomplete = "autocomplete_place"] destination: String,
) -> Result<(), Error> {
    let Ok(_) = ctx.defer_ephemeral().await else { return Err("reply__reply_failed".into()) };
    match teleport_player(&ctx, user.id.get(), &destination).await {
        Ok(place) => {
            let mut args = FluentArgs::new();
            args.set("user", format!("<@{}>", user.id.get()));
            args.set("place", place);
            reply_with_args(ctx, Ok("teleport__player_success"), Some(args)).await?;
        }
        Err(e) => {
            reply(ctx, Err(e)).await?;
        }
    }
    Ok(())
}

/// Téléporte tous les joueurs arrêtés dans un lieu vers un autre lieu.
#[poise::command(slash_command, required_permissions = "ADMINISTRATOR", guild_only, rename = "teleport_place")]
pub async fn place(
    ctx: Context<'_>,
    #[autocomplete = "autocomplete_place"] origin: String,
    #[autocomplete = "autocomplete_place"] destination: String,
) -> Result<(), Error> {
    let Ok(_) = ctx.defer_ephemeral().await else { return Err("reply__reply_failed".into()) };
    match teleport_place(&ctx, &origin, &destination).await {
        Ok((count, from, to)) => {
            let mut args = FluentArgs::new();
            args.set("count", count);
            args.set("origin", from);
            args.set("place", to);
            reply_with_args(ctx, Ok("teleport__place_success"), Some(args)).await?;
        }
        Err(e) => {
            reply(ctx, Err(e)).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Autocomplétion et univers du serveur
// ---------------------------------------------------------------------------

/// Lieux de l'univers du serveur dont le nom contient la saisie (25 au plus, limite de Discord).
/// La valeur envoyée est l'identifiant de la catégorie du lieu : deux lieux homonymes restent
/// distincts.
async fn autocomplete_place(ctx: Context<'_>, partial: &str) -> Vec<serenity::AutocompleteChoice> {
    let Some(guild_id) = ctx.guild_id() else { return vec![] };
    let Ok(Some(server)) = get_server_by_id(guild_id.get()).await else { return vec![] };
    let Ok(places) = all_places(server.universe_id).await else { return vec![] };
    matching_places(&places, partial)
        .into_iter()
        .take(25)
        .map(|p| serenity::AutocompleteChoice::new(truncate(&p.name, 100), p.category_id.to_string()))
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

async fn universe_of(ctx: &Context<'_>) -> Result<ObjectId, Error> {
    let guild_id = ctx.guild_id().ok_or("teleport__server_not_found")?;
    match get_server_by_id(guild_id.get()).await {
        Ok(Some(server)) => Ok(server.universe_id),
        Ok(None) => Err("teleport__server_not_found".into()),
        Err(_) => Err("teleport__database_error".into()),
    }
}

async fn teleport_player(ctx: &Context<'_>, user_id: u64, destination: &str) -> Result<String, Error> {
    let universe_id = universe_of(ctx).await?;
    let dest = find_place(universe_id, destination).await?;
    service::teleport_player(&ctx.serenity_context().http, universe_id, user_id, &dest, true).await
}

async fn teleport_place(ctx: &Context<'_>, origin: &str, destination: &str) -> Result<(i64, String, String), Error> {
    let universe_id = universe_of(ctx).await?;
    let from = find_place(universe_id, origin).await?;
    let dest = find_place(universe_id, destination).await?;
    let count = service::teleport_place(&ctx.serenity_context().http, universe_id, &from, &dest).await?;
    Ok((count, from.name, dest.name))
}
