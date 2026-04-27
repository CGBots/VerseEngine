use crate::discord::poise_structs::{Context, Error};
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
use serenity::all::{CreateActionRow, CreateSelectMenuOption, ComponentInteraction};
use futures::TryStreamExt;

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
            match _travel_with_move(ctx, dest, player_move).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    let _ = reply(ctx, Err(e)).await;
                    Ok(())
                }
            }
        }
    }
}

pub async fn _travel_with_move(ctx: Context<'_>, destination_input: String, player_move: TravelGroup) -> Result<(), Error>{
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
            move_from_place(ctx.serenity_context(), ctx.channel_id().get(), destination_place.category_id, server, player_move.clone()).await?;
        }
    }

    Ok(())
}

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
                dests.push(p1);
            }
            if let Some(p2) = get_place_by_category_id(server.universe_id, road.place_two_id).await? {
                dests.push(p2);
            }
            dests
        }
        SpaceType::Place => {
            let mut cursor = crate::database::road::get_road_by_source(server.universe_id, player_move.actual_space_id).await?;
            let mut dests = Vec::new();
            while let Some(road) = cursor.try_next().await? {
                let dest_id = if road.place_one_id == player_move.actual_space_id { road.place_two_id } else { road.place_one_id };
                if let Some(p) = get_place_by_category_id(server.universe_id, dest_id).await? {
                    dests.push(p);
                }
            }
            dests
        }
    };

    if destinations.is_empty() {
        return Err("travel__no_destination_found".into());
    }

    let mut options = Vec::new();
    for dest in destinations {
        options.push(CreateSelectMenuOption::new(dest.name.clone(), dest.category_id.to_string()));
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

    match player_move.actual_space_type {
        SpaceType::Road => {
            move_from_road(&ctx, dest_id, server, player_move).await?;
        }
        SpaceType::Place => {
            move_from_place(&ctx, interaction.channel_id.get(), dest_id, server, player_move).await?;
        }
    }

    Ok("travel__started")
}

async fn move_from_place(_ctx: &serenity::Context, _source_id: u64, destination_id: u64, server: Server, mut player_move: TravelGroup) -> Result<&'static str, Error>{
    let road = get_road(server.universe_id, player_move.actual_space_id, destination_id).await?
        .ok_or("travel__no_road_found")?;

    player_move.actual_space_id = road.channel_id;
    player_move.actual_space_type = SpaceType::Road;
    player_move.is_in_move = true;
    player_move.road_id = Some(road.channel_id);
    player_move.road_role_id = Some(road.role_id);
    player_move.road_server_id = Some(road.server_id);
    player_move.source_id = Some(player_move.actual_space_id); 
    // Attendez, source_id dans TravelGroup semble être l'id de la Place de départ
    player_move.source_id = Some(if road.place_one_id == destination_id { road.place_two_id } else { road.place_one_id });
    
    // Correction: On vient de Place, donc source_id est l'ID de la Place actuelle (avant changement)
    // Mais on a déjà écrasé actual_space_id...
    // En fait move_from_place est appelé avec la Place actuelle.
    
    let source_place_id = if road.place_one_id == destination_id { road.place_two_id } else { road.place_one_id };
    player_move.source_id = Some(source_place_id);
    player_move.destination_id = Some(destination_id);
    player_move.distance_traveled = 0.0;
    
    let now = chrono::Utc::now().timestamp() as u64;
    player_move.step_start_timestamp = Some(now);
    player_move.step_end_timestamp = Some(now);
    player_move.modified_speed = 0.0;

    add_travel(_ctx.http.clone(), server.server_id, player_move).await?;
    Ok("travel__started")
}
