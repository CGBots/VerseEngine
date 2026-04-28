use crate::discord::poise_structs::{Context, Error};
use crate::database::server::get_server_by_id;
use crate::database::characters::get_character_by_user_id;
use crate::travel::logic::{calculate_current_distance, stop_travel, add_travel};
use crate::travel::utils::validate_channel;
use fluent::FluentArgs;

/// Permet à un joueur de quitter son groupe actuel.
/// 
/// L'utilisateur qui quitte le groupe crée un nouveau groupe individuel à sa position actuelle.
/// Si le groupe était en mouvement, le mouvement est mis à jour pour les deux groupes résultants.
/// 
/// # Errors
/// Retourne une erreur si :
/// - Le joueur est déjà seul dans son groupe.
/// - Le serveur ou le personnage n'est pas trouvé.
#[poise::command(slash_command, guild_only, rename = "travel_leave")]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    
    validate_channel(&ctx, user_id).await?;

    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let mut player_move = server.clone().get_player_move(user_id).await?
        .ok_or("travel__character_not_found")?;

    if player_move.members.len() <= 1 {
        return Err("travel__already_alone".into());
    }

    let leader_id_before = player_move.members[0];
    let char_quitting = get_character_by_user_id(server.universe_id, user_id).await?.map(|c| c.name).unwrap_or(ctx.author().name.clone());
    let char_leader = get_character_by_user_id(server.universe_id, leader_id_before).await?.map(|c| c.name).unwrap_or(format!("Leader ({})", leader_id_before));

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
    let db_client = crate::database::db_client::get_db_client().await;
    let mut session = db_client.start_session().await?;
    session.start_transaction().await?;

    player_move.upsert_with_session(&mut session).await?;
    new_group.clone().insert_with_session(&mut session).await?;

    session.commit_transaction().await?;

    // Relancer les processus si en mouvement
    if player_move.is_in_move {
        let leader_id = player_move.members[0];
        stop_travel(leader_id).await?;
        add_travel(ctx.serenity_context().http.clone(), server.server_id.into(), player_move).await?;
        add_travel(ctx.serenity_context().http.clone(), server.server_id.into(), new_group).await?;
    }

    let mut args = FluentArgs::new();
    args.set("user", char_quitting);
    args.set("leader", char_leader);

    let content = crate::translation::get(ctx, "travel__public_left", None, Some(&args));
    ctx.channel_id().say(&ctx.serenity_context().http, content).await?;
    
    Ok(())
}
