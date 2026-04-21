use std::str::FromStr;
use mongodb::bson::oid::ObjectId;
use crate::discord::poise_structs::{Context, Error};
use crate::tr;
use crate::database::universe::{get_universe_by_id, get_universe_by_server_id, Universe};
use poise::CreateReply;
use poise::serenity_prelude::ComponentInteractionCollector;
use serenity::all::CreateSelectMenu;
use serenity::all::CreateSelectMenuKind;
use serenity::all::CreateSelectMenuOption;
use serenity::all::{ComponentInteractionDataKind, CreateActionRow};
use crate::database::server::Server;
use crate::universe::setup::setup_sub_command::{SetupType, _setup};
use crate::utility::reply::reply;

#[poise::command(slash_command, required_permissions = "ADMINISTRATOR", guild_only, rename = "universe_add_server")]
pub async fn add_server(
    ctx: Context<'_>,
    setup_type: SetupType
) -> Result<(), Error> {
    let Ok(_) = ctx.defer().await else { return Err("reply__reply_failed".into()) };
    let result = _add_server(&ctx, setup_type).await;
    let Ok(_) = reply(ctx, result).await else { return Err("reply__reply_failed".into()) };
    Ok(())
}

pub async fn _add_server(ctx: &Context<'_>, setup_type: SetupType) -> Result<&'static str, Error>{
    if check_server_in_universe(ctx.guild_id().unwrap().get()).await.is_ok() {
        return Ok("add_server_to_universe__already_bind");
    }

    let universes: Vec<Universe> = Universe::get_creator_universes(ctx.author().id.get()).await;

    if universes.is_empty() {
        return Err("add_server_to_universe__universes_unavailable".into());
    }

    let mut options = vec![];
    for universe in &universes {
        options.push(CreateSelectMenuOption::new(
            universe.name.clone(),
            universe.universe_id.to_string().clone(),
        ))
    }

    let action_row = CreateActionRow::SelectMenu(CreateSelectMenu::new(
        "selected_universe",
        CreateSelectMenuKind::String { options },
    ));

    let Ok(message) = ctx
        .send(
            CreateReply::default()
                .content(tr!(*ctx, "choose_universe"))
                .components(vec![action_row])
                .ephemeral(true),
        )
        .await else { return Err("reply__reply_failed".into()) };

    let serenity_context = ctx.serenity_context();

    while let Some(mci) = ComponentInteractionCollector::new(serenity_context)
        .timeout(std::time::Duration::from_secs(120))
        .filter(move |mci| mci.data.custom_id == "selected_universe")
        .await
    {
        if let ComponentInteractionDataKind::StringSelect { values } = &mci.data.kind {
            if let Some(selected) = values.get(0) {
                let _ = message.delete(*ctx).await;

                let Ok(universe_opt) = get_universe_by_id(ObjectId::from_str(selected.as_str())?).await else { return Err("create_character__database_error".into()) };
                let Some(universe) = universe_opt else {return Err("create_character__no_universe_found".into())};

                let Ok(res) = universe.clone().check_server_limit().await else { return Err("universe__check_server_limit_failed".into()) };

                if !res{
                    return Err("exceed_limit_number_of_servers_per_universe".into())
                }

                let Ok(_) = Server{
                    _id: Default::default(),
                    universe_id: universe.universe_id,
                    server_id: ctx.guild_id().unwrap().get(),
                    admin_role_id: Default::default(),
                    moderator_role_id: Default::default(),
                    spectator_role_id: Default::default(),
                    player_role_id: Default::default(),
                    everyone_role_id: Default::default(),
                    admin_category_id: Default::default(),
                    nrp_category_id: Default::default(),
                    rp_category_id: Default::default(),
                    road_category_id: Default::default(),
                    rp_wiki_channel_id: Default::default(),
                    log_channel_id: Default::default(),
                    moderation_channel_id: Default::default(),
                    commands_channel_id: Default::default(),
                    nrp_general_channel_id: Default::default(),
                    rp_character_channel_id: Default::default(),
                    universal_time_channel_id: Default::default(),
                    universal_invite_url: Default::default(),
                }.insert_server().await else { return Err("create_universe__server_insert_failed".into()) };
                let Ok(_) = _setup(&ctx, setup_type).await else { return Err("setup_server__failed".into()) };

                return Ok("add_server_to_universe__guild_linked");
            }
        }
    };

    Ok("")
}

/// Asynchronously checks if a specific guild (server) is associated with a universe.
///
/// This function attempts to retrieve a `Universe` object that corresponds to the provided
/// `guild_id`. If the server is associated with a universe, the function returns the universe;
/// otherwise, an error message is returned indicating that no universe is bound to the server.
///
/// # Arguments
///
/// * `guild_id` - The unique identifier (`u64`) of the guild (server) to check.
///
/// # Returns
///
/// * `Ok(Universe)` - If the guild is successfully found in the universe.
/// * `Err(String)` - If no universe is associated with the given `guild_id`, includes an
///   error message detailing the guild's ID.
///
/// # Errors
///
/// This function will return an error string if:
/// - The guild is not bound to any existing universe.
/// - Retrieving the universe encounters a failure.
///
/// # Examples
///
/// ```
/// let guild_id = 123456789;
/// match check_server_in_universe(guild_id).await {
///     Ok(universe) => println!("Found universe: {:?}", universe),
///     Err(error) => println!("Error: {}", error),
/// }
/// ```
///
/// Note: This function relies on the `Universe::get_universe_by_server_id` method to fetch
/// universe details asynchronously.
pub async fn check_server_in_universe(guild_id: u64) -> Result<Universe, String>{
    if let Ok(cursor) = get_universe_by_server_id(guild_id).await {
        if let Some(universe) = cursor{
            return Ok(universe);
        }
    }
    Err(format!("Guild {} not bind to any existing universe", guild_id))
}
