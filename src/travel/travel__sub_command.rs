use poise::serenity_prelude::Context as SerenityContext;
use serenity::all::{CreateActionRow, CreateSelectMenuOption, ComponentInteraction};
use crate::database::places::{get_place_by_category_id,};
use crate::database::server::{get_server_by_id, Server};
use crate::database::travel::{TravelGroup, SpaceType};
use crate::database::craft::PlayerCraft;
use crate::database::universe::{get_universe_by_id};
use crate::discord::poise_structs::{Context, Error};
use crate::travel::logic::{add_travel, stop_travel, calculate_current_distance};
use crate::utility::reply::{reply, reply_with_args};
use futures::{TryStreamExt};
use poise::{CreateReply, serenity_prelude as serenity};
use crate::database::road::{get_road, get_road_by_channel_id, get_road_by_source, Road};

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

#[poise::command(slash_command, guild_only, subcommands("stop", "start", "join", "leave"), rename = "travel")]
pub async fn travel(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "travel_join")]
pub async fn join(
    ctx: Context<'_>,
    #[description = "Le joueur dont vous souhaitez rejoindre le groupe"]
    target: serenity::User,
) -> Result<(), Error> {
    let Ok(_) = ctx.defer_ephemeral().await else { return Err("reply__reply_failed".into()) };
    let author_id = ctx.author().id.get();
    let target_id = target.id.get();

    if author_id == target_id {
        return Err("travel__cannot_join_self".into());
    }

    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        _ => return Err("travel__server_not_found".into()),
    };

    let universe = match get_universe_by_id(server.universe_id).await {
        Ok(Some(u)) => u,
        _ => return Err("travel__universe_not_found".into()),
    };

    let my_move = match server.clone().get_player_move(author_id).await {
        Ok(Some(m)) => m,
        _ => return Err("travel__character_not_found".into()),
    };

    let mut target_move = match server.clone().get_player_move(target_id).await {
        Ok(Some(m)) => m,
        _ => return Err("travel__target_not_found".into()),
    };

    if my_move._id == target_move._id {
        return Err("travel__already_in_same_group".into());
    }

    // Condition de salon/route
    if my_move.actual_space_id != target_move.actual_space_id || my_move.actual_space_type != target_move.actual_space_type {
        return Err("travel__too_far_different_place".into());
    }

    // Condition de proximité si en mouvement
    if my_move.is_in_move || target_move.is_in_move {
        let dist1 = calculate_current_distance(&my_move);
        let dist2 = calculate_current_distance(&target_move);
        let diff = (dist1 - dist2).abs();
        let threshold = 50.0 + (5.0 * universe.global_time_modifier as f64);
        
        if diff > threshold / 1000.0 { // threshold est en mètres, distances en km
            return Err("travel__too_far_to_join".into());
        }

        // Logique de position : moyenne des distances
        let mean_dist = (dist1 + dist2) / 2.0;
        target_move.distance_traveled = mean_dist;
        
        // On met à jour les timestamps pour forcer le recalcul
        let now = chrono::Utc::now().timestamp() as u64;
        target_move.step_start_timestamp = Some(now);
        target_move.step_end_timestamp = Some(now);
        target_move.modified_speed = 0.0;
    }

    // Fusion des membres
    for member in my_move.members.clone() {
        if !target_move.members.contains(&member) {
            target_move.members.push(member);
        }
    }

    // Supprimer mon ancien groupe et sauvegarder le nouveau
    my_move.remove().await.map_err(|e| Error::from(format!("DB error: {:?}", e)))?;
    target_move.upsert().await.map_err(|e| Error::from(format!("DB error: {:?}", e)))?;

    // Si le groupe cible est en mouvement, on doit relancer le processus pour inclure les nouveaux membres (et leur vitesse)
    if target_move.is_in_move {
        // On arrête proprement pour nettoyer MOVES et SLEEPER
        let leader_id = target_move.members[0];
        stop_travel(leader_id).await?;
        add_travel(ctx.serenity_context().http.clone(), server.server_id.into(), target_move).await?;
    }

    let _ = reply_with_args(ctx, Ok("travel__joined_group"), None).await;
    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "travel_leave")]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let Ok(_) = ctx.defer_ephemeral().await else { return Err("reply__reply_failed".into()) };
    let user_id = ctx.author().id.get();

    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        _ => return Err("travel__server_not_found".into()),
    };

    let mut player_move = match server.clone().get_player_move(user_id).await {
        Ok(Some(m)) => m,
        _ => return Err("travel__character_not_found".into()),
    };

    if player_move.members.len() <= 1 {
        return Err("travel__cannot_leave_alone".into());
    }

    // Retirer l'utilisateur du groupe actuel
    player_move.members.retain(|&id| id != user_id);
    
    // Créer un nouveau groupe pour l'utilisateur qui quitte
    let mut new_group = player_move.clone();
    new_group._id = mongodb::bson::oid::ObjectId::new();
    new_group.members = vec![user_id];
    
    // Mettre à jour la distance actuelle pour les deux
    let current_dist = calculate_current_distance(&player_move);
    player_move.distance_traveled = current_dist;
    new_group.distance_traveled = current_dist;
    
    let now = chrono::Utc::now().timestamp() as u64;
    player_move.step_start_timestamp = Some(now);
    player_move.step_end_timestamp = Some(now);
    player_move.modified_speed = 0.0;
    
    new_group.step_start_timestamp = Some(now);
    new_group.step_end_timestamp = Some(now);
    new_group.modified_speed = 0.0;

    // Sauvegarder les deux
    player_move.upsert().await.map_err(|e| Error::from(format!("DB error: {:?}", e)))?;
    new_group.clone().insert().await.map_err(|e| Error::from(format!("DB error: {:?}", e)))?;

    // Relancer les processus si en mouvement
    if player_move.is_in_move {
        // Redémarrer le groupe original (le leader a pu changer ou la vitesse a pu changer)
        let leader_id = player_move.members[0];
        stop_travel(leader_id).await?;
        add_travel(ctx.serenity_context().http.clone(), server.server_id.into(), player_move).await?;
        
        // Démarrer le nouveau groupe (l'utilisateur qui a quitté)
        add_travel(ctx.serenity_context().http.clone(), server.server_id.into(), new_group).await?;
    }

    let _ = reply_with_args(ctx, Ok("travel__left_group"), None).await;
    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "travel_start")]
pub async fn start(
    ctx: Context<'_>,
    destination: Option<String>,
) -> Result<(), Error> {
    let Ok(_) = ctx.defer_ephemeral().await else { return Err("reply__reply_failed".into()) };
    
    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        _ => return Err("travel__server_not_found".into()),
    };

    let _character = match server.clone().get_character_by_user_id(ctx.author().id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("travel__character_not_found".into()),
    };

    let mut player_move = match server.clone().get_player_move(ctx.author().id.get()).await {
        Ok(Some(m)) => m,
        _ => {return Err("travel__character_not_found".into())}
    };

    let author_id = ctx.author().id.get();
    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(server.universe_id, author_id).await {
        return Err("travel__cannot_move_while_crafting".into());
    }

    if player_move.is_in_move && player_move.actual_space_type == SpaceType::Road {
        // Seul le meneur peut modifier le trajet s'il est en mouvement
        if !player_move.members.is_empty() && player_move.members[0] != author_id {
            return Err("travel__only_leader_can_stop".into());
        }

        // Le joueur est sur une route, on l'arrête et on récupère la version mise à jour avec la distance calculée
        match stop_travel(author_id).await {
            Ok(m) => { player_move = m; }
            Err(e) => {
                log::error!("Failed to stop travel for user {}: {:?}", author_id, e);
                // On continue avec l'ancienne version si l'arrêt échoue, bien que ce soit anormal
            }
        }
    }

    match destination {
        None => {travel_without_destination(ctx).await?}
        Some(dest) => {
            let _error = match _travel_with_move(ctx, dest, player_move).await {
                Ok(_) => return Ok(()),
                Err(e) => reply(ctx, Err(e)).await,
            };}
    }


    Ok(())
}

#[poise::command(slash_command, guild_only, rename = "travel_stop")]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let Ok(_) = ctx.defer_ephemeral().await else { return Err("reply__reply_failed".into()) };
    let user_id = ctx.author().id.get();
    
    // Vérification de leadership si le groupe est en mouvement
    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        _ => return Err("travel__server_not_found".into()),
    };

    if let Ok(Some(player_move)) = server.get_player_move(user_id).await {
        if player_move.is_in_move && !player_move.members.is_empty() && player_move.members[0] != user_id {
            return Err("travel__only_leader_can_stop".into());
        }
    }

    match stop_travel(user_id).await {
        Ok(_) => {
            let _ = reply_with_args(ctx, Ok("travel__stopped"), None).await;
            Ok(())
        },
        Err(_) => {
            let _ = reply_with_args(ctx, Ok("travel__not_in_move"), None).await;
            Ok(())
        }
    }
}

pub async fn _travel(ctx: Context<'_>, destination_input: String) -> Result<(), Error>{
    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err("travel__server_not_found".into()),
        Err(e) => {
            log::error!("Database error in _travel when fetching server: {:?}", e);
            return Err("travel__database_error".into());
        }
    };

    let player_move = match server.clone().get_player_move(ctx.author().id.get()).await {
        Ok(Some(m)) => m,
        _ => {return Err("travel__character_not_found".into())}
    };

    _travel_with_move(ctx, destination_input, player_move).await
}

pub async fn _travel_with_move(ctx: Context<'_>, destination_input: String, player_move: TravelGroup) -> Result<(), Error>{
    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err("travel__server_not_found".into()),
        Err(e) => {
            log::error!("Database error in _travel when fetching server: {:?}", e);
            return Err("travel__database_error".into());
        }
    };

    let destination_category_id = parse_channel_id(&destination_input).ok_or_else(|| Error::from("travel__place_not_found"))?;

    println!("travel__place_category_id: {:?}", destination_category_id);

    let destination_place = match get_place_by_category_id(server.universe_id, destination_category_id).await {
        Ok(Some(p)) => p,
        _ => return Err("travel__place_not_found".into()),
    };


    let _character = match server.clone().get_character_by_user_id(ctx.author().id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("travel__character_not_found".into()),
    };

    match player_move.actual_space_type {
        SpaceType::Road => {
            move_from_road(ctx.serenity_context(), destination_place.category_id, server, player_move.clone()).await?;
        }
        SpaceType::Place => {
            move_from_place(ctx.serenity_context(), ctx.channel_id().get(), destination_place.category_id, server, player_move.clone()).await?;
        }
    }

    Ok(())

    //Ok(("travel__started", destination_mention))
}

async fn move_from_road(_ctx: &SerenityContext, destination_id: u64, server: Server, mut player_move: TravelGroup) -> Result<&'static str, Error>{
    let dest_id = destination_id;
    
    // Si on est sur une route, on ne peut aller que vers les extrémités (source ou destination originelle)
    if Some(dest_id) == player_move.destination_id {
        // Déjà en train d'y aller ? On ne fait rien ou on confirme
        add_travel(_ctx.http.clone(), server.server_id.into(), player_move.clone()).await?;
        return Ok("travel__already_moving_to_destination");
    } else if Some(dest_id) == player_move.source_id {
        // Demi-tour
        let old_dest = player_move.destination_id;
        let old_dest_role = player_move.destination_role_id;
        let old_dest_server = player_move.destination_server_id;
        
        player_move.destination_id = player_move.source_id;
        player_move.destination_role_id = player_move.source_role_id;
        player_move.destination_server_id = player_move.source_server_id;
        
        player_move.source_id = old_dest;
        player_move.source_role_id = old_dest_role;
        player_move.source_server_id = old_dest_server;
        
        // On recalcule la distance parcourue (on repart dans l'autre sens)
        // Pour simplifier, on inverse juste la progression
        if let Ok(Some(road)) = get_road_by_channel_id(server.universe_id, player_move.road_id.unwrap()).await {
            player_move.distance_traveled = (road.distance as f64 - player_move.distance_traveled).max(0.0);
        }

        // On réinitialise les timestamps pour forcer next_step_logic à recalculer un vrai step
        let now = chrono::Utc::now().timestamp() as u64;
        player_move.step_start_timestamp = Some(now);
        player_move.step_end_timestamp = Some(now);
        player_move.modified_speed = 0.0;

        add_travel(_ctx.http.clone(), server.server_id.into(), player_move.clone()).await?;

    } else {
        println!("dest_id: {:?}", dest_id);
        println!("player_move.destination_id: {:?}", player_move.destination_id);
        println!("player_move.source_id: {:?}", player_move.source_id);
        return Err("travel__invalid_road_destination".into());
    }
    Ok("travel__started")
}


async fn travel_without_destination(ctx: Context<'_>) -> Result<(), Error>{
    let server = match get_server_by_id(ctx.guild_id().unwrap().get()).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err("travel__server_not_found".into()),
        Err(e) => {
            log::error!("Database error in travel_without_destination when fetching server: {:?}", e);
            return Err("travel_without_destination__database_error".into());
        }
    };

    let Some(player_move) = server.clone().get_player_move(ctx.clone().author().id.get()).await? else {return Err("travel__character_not_found".into())};

    let mut destinations = vec![];

    match player_move.actual_space_type {
        SpaceType::Road => {
            let original_source = player_move.source_id.unwrap();
            let original_destination = player_move.destination_id.unwrap();
            let Some(road) = get_road(server.universe_id, original_source, original_destination).await? else {return Err("move_from_place__road_not_found".into())};
            let Some(source_place) = get_place_by_category_id(road.universe_id, original_source).await? else{return Err("travel__source_place_not_found".into())};
            let Some(destination_place) = get_place_by_category_id(road.universe_id, original_destination).await? else{return Err("travel__place_not_found".into())};


            let distance_to_original_destination = road.distance as f64 - player_move.distance_traveled;
            let distance_to_original_source = player_move.distance_traveled;

            destinations.push(CreateSelectMenuOption::new( source_place.name + " • " + format!("{:.2}", distance_to_original_source).as_str() + "km", original_source.to_string()));
            destinations.push(CreateSelectMenuOption::new( destination_place.name + " • " + format!("{:.2}", distance_to_original_destination).as_str() + "km", original_destination.to_string()));
            //Les id sont bons ici

        }
        SpaceType::Place => {
            let available_roads: Vec<Road> = match get_road_by_source(
                server.universe_id,
                ctx.guild_channel().await.unwrap().parent_id.unwrap().get(),
            )
                .await
            {
                Ok(cursor) => {
                    let res = cursor.try_collect().await;
                    match res {
                        Ok(road) => {road}
                        Err(e) => {println!("{:?}", e); Vec::new()}
                    }

                },
                Err(_) => Vec::new(),
            };

            if available_roads.is_empty(){
                let _ = reply(ctx, Err("travel__no_road_available".into())).await;
                return Ok(());
            }

            for road in available_roads {
                let destination = if road.place_one_id == ctx.guild_channel().await.unwrap().parent_id.unwrap().get() {road.place_two_id}
                else {road.place_one_id};
                destinations.push(CreateSelectMenuOption::new(road.road_name + " • " + format!("{:.2}", road.distance).as_str() + "km", destination.to_string()));
            }
        }
    }

    let select_menu = serenity::all::CreateSelectMenu::new("select__menu__chose_destination",
        serenity::all::CreateSelectMenuKind::String {
            options: destinations,
        }
    );

    let components = vec![CreateActionRow::SelectMenu(select_menu)];

    let result = ctx.send(CreateReply::default().components(components).reply(true)).await;
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            log::error!("Reply failed in travel_without_destination: {:?}", e);
            Err("travel_without_destination__reply_failed".into())
        }
    }
}

pub async fn travel_from_handler(ctx: SerenityContext, interaction: ComponentInteraction) -> Result<&'static str, Error>{
    let destination_input = match &interaction.data.kind {
        serenity::all::ComponentInteractionDataKind::StringSelect { values } => {
            values.get(0).ok_or("create_character__invalid_interaction")?
        }
        _ => return Err("create_character__invalid_interaction".into()),
    };
    let server = match get_server_by_id(interaction.guild_id.unwrap().get()).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err("travel__server_not_found".into()),
        Err(e) => {
            log::error!("Database error in _travel when fetching server: {:?}", e);
            return Err("travel__database_error".into());
        }
    };

    let destination_category_id = parse_channel_id(&destination_input).ok_or_else(|| Error::from("travel__place_not_found"))?;

    let _ = match get_place_by_category_id(server.universe_id, destination_category_id).await {
        Ok(Some(_)) => {},
        _ => return Err("travel__place_not_found".into()),
    };


    let _character = match server.clone().get_character_by_user_id(interaction.user.id.get()).await {
        Ok(Some(c)) => c,
        _ => return Err("travel__character_not_found".into()),
    };

    let player_move = match server.clone().get_player_move(interaction.user.id.get()).await {
        Ok(Some(m)) => m,
        _ => {return Err("travel__character_not_found".into())}
    };

    if let Ok(Some(_)) = PlayerCraft::get_by_user_id(server.universe_id, interaction.user.id.get()).await {
        return Err("travel__cannot_move_while_crafting".into());
    }

    match player_move.actual_space_type {
        SpaceType::Road => {
            move_from_road(&ctx, destination_category_id, server, player_move.clone()).await?;
        }
        SpaceType::Place => {
            move_from_place(&ctx, interaction.channel_id.get(), destination_input.parse().unwrap(), server, player_move.clone()).await?;
        }
    }

    Ok("")

    //Ok(("travel__started", destination_mention))
}


async fn move_from_place(ctx: &SerenityContext, source_id: u64, destination_id: u64, server: Server, mut player_move: TravelGroup) -> Result<&'static str, Error>{
    let source = ctx.http.get_channel(source_id.into()).await.unwrap();
    let source_id = source.clone().guild().unwrap().parent_id.unwrap().get();

    let dest_id = destination_id;

    let road = match server.clone().get_road(source_id, dest_id).await {
        Ok(Some(r)) => r,
        _ => return Err("move_from_place__road_not_found".into())
    };

    // Récupère les rôles des lieux source et destination
    let source_place = crate::database::places::get_place_by_category_id(server.universe_id, source_id).await
        .map_err(|_| Error::from("travel__database_error"))?
        .ok_or_else(|| Error::from("travel__source_place_not_found"))?;
    
    let dest_place = crate::database::places::get_place_by_category_id(server.universe_id, dest_id).await
        .map_err(|_| Error::from("travel__database_error"))?
        .ok_or_else(|| Error::from("travel__place_not_found"))?;

    player_move.actual_space_id = road.channel_id;
    player_move.actual_space_type = SpaceType::Road;
    player_move.road_id = Some(road.channel_id);
    player_move.road_role_id = Some(road.role_id);
    player_move.road_server_id = Some(road.server_id);
    player_move.source_id = Some(source_id);
    player_move.source_role_id = Some(source_place.role);
    player_move.source_server_id = Some(source_place.server_id);
    player_move.destination_id = Some(dest_id);
    player_move.destination_role_id = Some(dest_place.role);
    player_move.destination_server_id = Some(dest_place.server_id);
    player_move.distance_traveled = 0.0;
    
    add_travel(ctx.http.clone(), source.guild().unwrap().id.get(), player_move.clone()).await?;

    Ok("")
}
