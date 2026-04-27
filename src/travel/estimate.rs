use crate::discord::poise_structs::{Context, Error};
use crate::database::server::get_server_by_id;
use crate::database::universe::get_universe_by_id;
use crate::database::road::get_road_by_channel_id;
use crate::database::characters::get_character_by_user_id;
use crate::travel::logic::{calculate_current_distance, get_travel_threshold};
use crate::travel::utils::validate_channel;
use crate::utility::reply::reply_with_args_and_ephemeral;
use serenity::all as serenity;
use fluent::FluentArgs;

/// Estime la distance et le temps de trajet RP pour rejoindre un autre joueur.
/// 
/// L'estimation est possible si la cible est sur la même route/lieu et à moins de 2 fois le seuil de ralliement.
/// Le temps indiqué est le temps "In-Game" (RP).
/// 
/// # Arguments
/// * `ctx` - Le contexte de la commande Poise.
/// * `target` - L'utilisateur Discord cible.
/// 
/// # Errors
/// Retourne une erreur si :
/// - Le joueur tente de s'estimer lui-même.
/// - La cible est trop loin (au-delà de 2 * threshold).
/// - Les joueurs ne sont pas sur le même tronçon.
#[poise::command(slash_command, guild_only, rename = "travel_estimate")]
pub async fn estimate(
    ctx: Context<'_>,
    #[description = "Le joueur dont vous souhaitez estimer la distance"]
    target: serenity::User,
) -> Result<(), Error> {
    let author_id = ctx.author().id.get();
    let target_id = target.id.get();

    if author_id == target_id {
        return Err("travel__cannot_estimate_self".into());
    }

    validate_channel(&ctx, author_id).await?;

    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let universe = get_universe_by_id(server.universe_id).await?
        .ok_or("travel__universe_not_found")?;

    let my_move = server.clone().get_player_move(author_id).await?
        .ok_or("travel__character_not_found")?;

    let target_move = server.clone().get_player_move(target_id).await?
        .ok_or("travel__target_not_found")?;

    if my_move.actual_space_id != target_move.actual_space_id || my_move.actual_space_type != target_move.actual_space_type {
        return Err("travel__too_far_different_place".into());
    }

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
    let real_diff_m = (my_pos - target_pos).abs() * 1000.0;

    let threshold = get_travel_threshold(universe.global_time_modifier.into());
    
    let char_target = get_character_by_user_id(universe.universe_id, target_id).await?.map(|c| c.name).unwrap_or(target.name.clone());

    if real_diff_m > 2.0 * threshold {
        return Err(format!("error:travel__estimate_too_far:target={}", char_target).into());
    }

    // Calcul du temps RP : temps = distance_m / (vitesse_kmh / 3.6)
    let speed_kmh = my_move.modified_speed.max(1.0);
    let total_seconds = (real_diff_m / (speed_kmh / 3.6)) as u64;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    let time_str = format!("{} min {} s", minutes, seconds.max(if real_diff_m > 0.0 { 1 } else { 0 }));

    if real_diff_m <= threshold {
        let mut args = FluentArgs::new();
        args.set("target", char_target);
        args.set("time", time_str);
        reply_with_args_and_ephemeral(ctx, Ok("travel__estimate_can_join"), Some(args), true).await?;
    } else {
        let rounded_dist = ((real_diff_m / 10.0).round() * 10.0) as u64;
        let mut args = FluentArgs::new();
        args.set("target", char_target);
        args.set("distance", rounded_dist.to_string());
        args.set("time", time_str);
        reply_with_args_and_ephemeral(ctx, Ok("travel__estimate_result"), Some(args), true).await?;
    }

    Ok(())
}
