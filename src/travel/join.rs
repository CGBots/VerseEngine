use crate::discord::poise_structs::{Context, Error};
use crate::database::server::get_server_by_id;
use crate::database::universe::get_universe_by_id;
use crate::database::road::get_road_by_channel_id;
use crate::database::characters::get_character_by_user_id;
use crate::travel::logic::{calculate_current_distance, get_travel_threshold, add_travel};
use crate::travel::utils::validate_channel;
use crate::utility::reply::reply_with_args_and_ephemeral;
use serenity::all as serenity;
use fluent::FluentArgs;

/// Permet à un joueur de rejoindre le groupe d'un autre joueur.
/// 
/// Les deux joueurs doivent se trouver au même endroit (Lieu) ou à proximité sur une même Route.
/// Le seuil de proximité est calculé dynamiquement via `get_travel_threshold`.
/// 
/// # Arguments
/// * `ctx` - Le contexte de la commande Poise.
/// * `target` - L'utilisateur Discord dont on souhaite rejoindre le groupe.
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
    let target_id = target.id.get();

    if author_id == target_id {
        return Err("travel__cannot_join_self".into());
    }

    validate_channel(&ctx, author_id).await?;

    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let universe = get_universe_by_id(server.universe_id).await?
        .ok_or("travel__universe_not_found")?;

    let my_move = server.clone().get_player_move(author_id).await?
        .ok_or("travel__character_not_found")?;

    let mut target_move = server.clone().get_player_move(target_id).await?
        .ok_or("travel__target_not_found")?;

    if my_move._id == target_move._id {
        return Err("travel__already_in_same_group".into());
    }

    if my_move.actual_space_id != target_move.actual_space_id || my_move.actual_space_type != target_move.actual_space_type {
        return Err("travel__too_far_different_place".into());
    }

    let threshold = get_travel_threshold(universe.global_time_modifier.into());

    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    if my_move.is_in_move || target_move.is_in_move {
        let road = get_road_by_channel_id(server.universe_id, my_move.actual_space_id).await?
            .ok_or("travel__road_not_found")?;

        let get_abs_pos = |m: &crate::database::travel::TravelGroup| {
            if m.source_id == Some(road.place_one_id) {
                calculate_current_distance(m)
            } else {
                (road.distance as f64) - calculate_current_distance(m)
            }
        };

        let my_pos = get_abs_pos(&my_move);
        let target_pos = get_abs_pos(&target_move);
        let diff_m = (my_pos - target_pos).abs() * 1000.0;

        if diff_m > threshold {
            return Err("travel__too_far_to_join".into());
        }

        if target_move.source_id == Some(road.place_one_id) {
            target_move.distance_traveled = target_pos;
        } else {
            target_move.distance_traveled = (road.distance as f64) - target_pos;
        }
        
        let now = chrono::Utc::now().timestamp() as u64;
        target_move.step_start_timestamp = Some(now);
        target_move.step_end_timestamp = Some(now);
        target_move.modified_speed = 0.0;
    }

    for member in my_move.members.clone() {
        if !target_move.members.contains(&member) {
            target_move.members.push(member);
        }
    }

    my_move.remove_with_session(&mut session).await?;
    target_move.upsert_with_session(&mut session).await?;

    session.commit_transaction().await?;

    if target_move.is_in_move {
        add_travel(ctx.serenity_context().http.clone(), server.server_id.into(), target_move.clone()).await?;
    }

    let char_author = get_character_by_user_id(universe.universe_id, author_id).await?.map(|c| c.name).unwrap_or(ctx.author().name.clone());
    let char_target = get_character_by_user_id(universe.universe_id, target_id).await?.map(|c| c.name).unwrap_or(target.name.clone());

    let mut args = FluentArgs::new();
    args.set("user", char_author);
    args.set("target", char_target);

    reply_with_args_and_ephemeral(ctx, Ok("travel__public_joined"), Some(args), false).await?;
    
    Ok(())
}
