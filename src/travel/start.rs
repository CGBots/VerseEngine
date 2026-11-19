use crate::discord::poise_structs::{Context, Error};
use crate::travel::service::{self, Actor, Announce};
use crate::travel::utils::validate_channel;
use poise::CreateReply;
use crate::utility::reply::reply_with_args_and_ephemeral;
use serenity::all as serenity;
use serenity::all::{CreateActionRow, CreateSelectMenuOption, ComponentInteraction};

/// Tente d'extraire un identifiant numérique d'une chaîne (ex: mention de salon ou ID brut).
pub fn parse_channel_id(input: &str) -> Option<u64> {
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

/// Démarre un voyage vers une destination spécifiée ou propose une liste de destinations possibles.
///
/// La logique est dans [`service::start`], partagée avec l'activité.
///
/// # Arguments
/// * `destination` - Optionnellement, le nom ou la mention d'un lieu vers lequel voyager.
///
/// # Errors
/// Retourne une erreur si le joueur est en train de crafter ou si la destination est invalide.
#[poise::command(slash_command, guild_only, rename = "travel_start")]
pub async fn start(
    ctx: Context<'_>,
    destination: Option<String>,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    validate_channel(&ctx, author_id).await?;
    let guild_id = ctx.guild_id().ok_or("travel__server_not_found")?.get();

    match destination {
        None => travel_without_destination(ctx, guild_id).await,
        Some(dest) => {
            let result = match parse_channel_id(&dest) {
                Some(destination_id) => {
                    let actor = Actor { user_id: author_id, fallback_name: &ctx.author().name, locale: ctx.locale().unwrap_or("fr") };
                    service::start(&ctx.serenity_context().http, guild_id, actor, destination_id, Announce::In(ctx.channel_id())).await
                }
                None => Err(Error::from("travel__place_not_found")),
            };
            let ephemeral = true;
            let _ = reply_with_args_and_ephemeral(ctx, result, None, ephemeral).await;
            Ok(())
        }
    }
}

/// Affiche un menu de sélection des destinations possibles depuis la position actuelle.
async fn travel_without_destination(ctx: Context<'_>, guild_id: u64) -> Result<(), Error> {
    let destinations = service::destinations(guild_id, ctx.author().id.get()).await?;
    if destinations.is_empty() {
        return Err("travel__no_destination_found".into());
    }

    let options = destinations
        .into_iter()
        .map(|d| CreateSelectMenuOption::new(format!("{} - {:.2}km", d.place.name, d.distance_km), d.place.category_id.to_string()))
        .collect();

    let select_menu = serenity::CreateSelectMenu::new("travel_destination_select", serenity::CreateSelectMenuKind::String { options });
    ctx.send(CreateReply::default()
        .content("Choisissez votre destination :")
        .components(vec![CreateActionRow::SelectMenu(select_menu)])
        .ephemeral(true)
    ).await?;
    Ok(())
}

/// Gère l'interaction provenant du menu de sélection de destination (Select Menu).
pub async fn travel_from_handler(ctx: serenity::Context, interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let guild_id = interaction.guild_id.ok_or("travel__server_not_found")?.get();
    let dest_id = match &interaction.data.kind {
        serenity::ComponentInteractionDataKind::StringSelect { values } => values
            .first()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| Error::from("travel__invalid_destination"))?,
        _ => return Err("travel__invalid_interaction".into()),
    };

    let locale = interaction.locale.as_str();
    let actor = Actor { user_id: interaction.user.id.get(), fallback_name: &interaction.user.name, locale };
    let result = service::start(&ctx.http, guild_id, actor, dest_id, Announce::In(interaction.channel_id)).await;

    let content = match &result {
        Ok(key) => crate::tr_locale!(locale, key),
        Err(e) => crate::tr_locale!(locale, &e.to_string()),
    };
    interaction.create_response(&ctx, serenity::CreateInteractionResponse::Message(
        serenity::CreateInteractionResponseMessage::new().content(content).ephemeral(true)
    )).await?;
    result
}
