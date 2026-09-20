use crate::discord::poise_structs::{Context, Error};
use crate::database::characters::{get_character_by_user_id};
use crate::database::server::{get_server_by_id, Server};
use crate::database::travel::{TravelGroup, SpaceType};
use crate::database::craft::PlayerCraft;
use crate::database::places::{get_place_by_category_id};
use crate::database::road::{get_road_by_channel_id, get_road};
use crate::travel::logic::{add_travel, stop_travel};
use crate::travel::utils::validate_channel;
use poise::CreateReply;
use crate::utility::reply::reply;
use serenity::all as serenity;
use serenity::all::{CreateActionRow, CreateSelectMenuOption, ComponentInteraction, CreateMessage};
use futures::TryStreamExt;
use crate::{tr, tr_locale};

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

    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let mut player_move = server.clone().get_player_move(author_id).await?
        .ok_or("travel__character_not_found")?;

    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(server.universe_id, author_id).await {
        return Err("travel__cannot_move_while_crafting".into());
    }

    if player_move.is_in_move && player_move.actual_space_type == SpaceType::Road {
        if !player_move.members.is_empty() && player_move.members[0] != author_id {
            return Err("travel__only_leader_can_stop".into());
        }

        player_move = stop_travel(author_id).await?;
    }

    match destination {
        None => travel_without_destination(ctx).await,
        Some(dest) => {
            match travel_with_destination(ctx, dest, player_move).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    let _ = reply(ctx, Err(e)).await;
                    Ok(())
                }
            }
        }
    }
}

/// Initialise un voyage vers une destination précise fournie par l'utilisateur.
pub async fn travel_with_destination(ctx: Context<'_>, destination_input: String, player_move: TravelGroup) -> Result<(), Error>{
    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let destination_category_id = parse_channel_id(&destination_input).ok_or_else(|| Error::from("travel__place_not_found"))?;

    let destination_place = get_place_by_category_id(server.universe_id, destination_category_id).await?
        .ok_or("travel__place_not_found")?;

    match player_move.actual_space_type {
        SpaceType::Road => {
            move_from_road(ctx.serenity_context(), destination_place.category_id, server, player_move.clone()).await?;
        }
        SpaceType::Place => {
            move_from_place(ctx.serenity_context(), ctx.channel_id().get(), destination_place.category_id, server.clone(), player_move.clone()).await?;
            let user_display_name = match get_character_by_user_id(server.universe_id, ctx.author().id.get()).await {
                Ok(Some(c)) => c.name,
                _ => ctx.author().name.clone(),
            };
            ctx.http().send_message(ctx.channel_id(), vec![], &CreateMessage::new().content(tr!(ctx, "travel__moving_to_place", user: user_display_name, destination: destination_place.name))).await?;
        }
    }

    Ok(())
}

/// Gère le mouvement d'un groupe se trouvant déjà sur une route.
async fn move_from_road(_ctx: &serenity::Context, destination_id: u64, server: Server, mut player_move: TravelGroup) -> Result<&'static str, Error>{
    let dest_id = destination_id;

    if Some(dest_id) == player_move.destination_id {
        add_travel(_ctx.http.clone(), server.server_id, player_move.clone()).await?;
        return Ok("travel__already_moving_to_destination");
    } else if Some(dest_id) == player_move.source_id {
        let old_dest = player_move.destination_id;
        let old_dest_role = player_move.destination_role_id;
        let old_dest_server = player_move.destination_server_id;

        player_move.destination_id = player_move.source_id;
        player_move.destination_role_id = player_move.source_role_id;
        player_move.destination_server_id = player_move.source_server_id;

        player_move.source_id = old_dest;
        player_move.source_role_id = old_dest_role;
        player_move.source_server_id = old_dest_server;

        if let Ok(Some(road)) = get_road_by_channel_id(server.universe_id, player_move.road_id.unwrap()).await {
            player_move.distance_traveled = (road.distance as f64 - player_move.distance_traveled).max(0.0);
        }

        let now = chrono::Utc::now().timestamp() as u64;
        player_move.step_start_timestamp = Some(now);
        player_move.step_end_timestamp = Some(now);
        player_move.modified_speed = 0.0;

        add_travel(_ctx.http.clone(), server.server_id, player_move.clone()).await?;
    } else {
        return Err("travel__invalid_road_destination".into());
    }
    
    Ok("travel__started")
}

/// Affiche un menu de sélection des destinations possibles depuis la position actuelle.
async fn travel_without_destination(ctx: Context<'_>) -> Result<(), Error>{
    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let player_move = server.clone().get_player_move(ctx.author().id.get()).await?
        .ok_or("travel__character_not_found")?;

    let destinations = match player_move.actual_space_type {
        SpaceType::Road => {
            let road = get_road_by_channel_id(server.universe_id, player_move.actual_space_id).await?
                .ok_or("travel__road_not_found")?;
            
            let mut dests = Vec::new();
            if let Some(p1) = get_place_by_category_id(server.universe_id, road.place_one_id).await? {
                let dist = if road.place_one_id == player_move.destination_id.unwrap_or(0) {
                    (road.distance as f64) - player_move.distance_traveled
                } else {
                    player_move.distance_traveled
                };
                dests.push((p1, dist));
            }
            if let Some(p2) = get_place_by_category_id(server.universe_id, road.place_two_id).await? {
                let dist = if road.place_two_id == player_move.destination_id.unwrap_or(0) {
                    (road.distance as f64) - player_move.distance_traveled
                } else {
                    player_move.distance_traveled
                };
                dests.push((p2, dist));
            }
            dests
        }
        SpaceType::Place => {
            let mut cursor = crate::database::road::get_road_by_source(server.universe_id, player_move.actual_space_id).await?;
            let mut dests = Vec::new();
            while let Some(road) = cursor.try_next().await? {
                let dest_id = if road.place_one_id == player_move.actual_space_id { road.place_two_id } else { road.place_one_id };
                if let Some(p) = get_place_by_category_id(server.universe_id, dest_id).await? {
                    dests.push((p, road.distance as f64));
                }
            }
            dests
        }
    };

    if destinations.is_empty() {
        return Err("travel__no_destination_found".into());
    }

    let mut options = Vec::new();
    for (dest, dist) in destinations {
        let label = format!("{} - {:.2}km", dest.name, dist);
        options.push(CreateSelectMenuOption::new(label, dest.category_id.to_string()));
    }

    let select_menu = serenity::CreateSelectMenu::new("travel_destination_select", serenity::CreateSelectMenuKind::String { options });
    let row = CreateActionRow::SelectMenu(select_menu);

    ctx.send(CreateReply::default()
        .content("Choisissez votre destination :")
        .components(vec![row])
        .ephemeral(true)
    ).await?;

    Ok(())
}

/// Gère l'interaction provenant du menu de sélection de destination (Select Menu).
pub async fn travel_from_handler(ctx: serenity::Context, interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let server = get_server_by_id(interaction.guild_id.unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let player_move = server.clone().get_player_move(interaction.user.id.get()).await?
        .ok_or("travel__character_not_found")?;

    let dest_id = if let serenity::ComponentInteractionDataKind::StringSelect { values } = &interaction.data.kind {
        values[0].parse::<u64>().map_err(|_| Error::from("travel__invalid_destination"))?
    } else {
        return Err("travel__invalid_interaction".into());
    };

    let locale = interaction.locale.as_str();
    let result = match player_move.actual_space_type {
        SpaceType::Road => {
            move_from_road(&ctx, dest_id, server, player_move).await
        }
        SpaceType::Place => {
            let destination_place = get_place_by_category_id(server.universe_id, dest_id).await?
                .ok_or("travel__place_not_found")?;
            let user_display_name = match get_character_by_user_id(server.universe_id, interaction.user.id.get()).await {
                Ok(Some(c)) => c.name,
                _ => interaction.user.name.clone(),
            };
            ctx.http.send_message(interaction.channel_id, vec![], &CreateMessage::new().content(tr_locale!(locale, "travel__moving_to_place", user: user_display_name, destination: destination_place.name))).await?;
            move_from_place(&ctx, interaction.channel_id.get(), dest_id, server, player_move).await
        }
    };

    match result {
        Ok(key) => {
            let content = crate::tr_locale!(locale, key);
            interaction.create_response(&ctx, serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true)
            )).await?;
            Ok(key)
        }
        Err(e) => {
            let content = crate::tr_locale!(locale, &e.to_string());
            interaction.create_response(&ctx, serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true)
            )).await?;
            Err(e)
        }
    }
}

/// Gère le mouvement d'un groupe quittant un lieu (Place) pour s'engager sur une route (Road).
async fn move_from_place(_ctx: &serenity::Context, _source_id: u64, destination_id: u64, server: Server, mut player_move: TravelGroup) -> Result<&'static str, Error>{
    let road = get_road(server.universe_id, player_move.actual_space_id, destination_id).await?
        .ok_or("travel__no_road_found")?;

    let source_place_id = if road.place_one_id == destination_id { road.place_two_id } else { road.place_one_id };
    
    // Récupération du lieu de départ pour obtenir son rôle
    let source_place = get_place_by_category_id(server.universe_id, source_place_id).await?
        .ok_or("travel__source_place_not_found")?;

    // Récupération du lieu de destination pour obtenir son rôle
    let destination_place = get_place_by_category_id(server.universe_id, destination_id).await?
        .ok_or("travel__place_not_found")?;

    player_move.actual_space_id = road.channel_id;
    player_move.actual_space_type = SpaceType::Road;
    player_move.is_in_move = true;
    player_move.road_id = Some(road.channel_id);
    player_move.road_role_id = Some(road.role_id);
    player_move.road_server_id = Some(road.server_id);
    player_move.source_id = Some(source_place_id);
    player_move.source_role_id = Some(source_place.role);
    player_move.source_server_id = Some(source_place.server_id);
    player_move.destination_id = Some(destination_id);
    player_move.destination_role_id = Some(destination_place.role);
    player_move.destination_server_id = Some(destination_place.server_id);
    player_move.distance_traveled = 0.0;
    
    let now = chrono::Utc::now().timestamp() as u64;
    player_move.step_start_timestamp = Some(now);
    player_move.step_end_timestamp = Some(now);
    player_move.modified_speed = 0.0;

    add_travel(_ctx.http.clone(), server.server_id, player_move).await?;
    Ok("travel__started")
}
