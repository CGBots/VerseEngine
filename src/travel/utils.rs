use crate::discord::poise_structs::{Context, Error};
use crate::database::server::get_server_by_id;
use crate::database::travel::SpaceType;
use crate::database::places::get_place_by_category_id;
use crate::database::road::get_road_by_channel_id;

/// Valide que l'utilisateur se trouve dans le bon salon Discord par rapport à sa position RP.
/// 
/// Si le personnage est dans un lieu (Place), il doit être dans un salon appartenant à la catégorie du lieu.
/// Si le personnage est sur une route (Road), il doit être dans le salon spécifique de la route.
/// 
/// # Arguments
/// * `ctx` - Le contexte de la commande Poise.
/// * `author_id` - L'identifiant Discord de l'auteur de la commande.
/// 
/// # Errors
/// Retourne une erreur si le serveur, le personnage ou le lieu/route n'est pas trouvé,
/// ou si le salon actuel ne correspond pas à la position du personnage.
pub async fn validate_channel(ctx: &Context<'_>, author_id: u64) -> Result<(), Error> {
    let server = get_server_by_id(ctx.guild_id().unwrap().get()).await?
        .ok_or("travel__server_not_found")?;

    let player_move = server.clone().get_player_move(author_id).await?
        .ok_or("travel__character_not_found")?;

    let current_channel = ctx.channel_id().to_channel(ctx).await?.guild()
        .ok_or("travel__invalid_channel")?;

    match player_move.actual_space_type {
        SpaceType::Place => {
            let parent_id = current_channel.parent_id.map(|id| id.get()).unwrap_or(0);
            if parent_id != player_move.actual_space_id {
                let place = get_place_by_category_id(server.universe_id, player_move.actual_space_id).await?
                    .ok_or("travel__place_not_found")?;
                
                return Err(format!("error:travel__wrong_channel:category={},channel={}", place.name, current_channel.name).into());
            }
        }
        SpaceType::Road => {
            if current_channel.id.get() != player_move.actual_space_id {
                let road = get_road_by_channel_id(server.universe_id, player_move.actual_space_id).await?
                    .ok_or("travel__road_not_found")?;
                
                return Err(format!("error:travel__wrong_channel:category=Route,channel={}", road.road_name).into());
            }
        }
    }
    Ok(())
}
