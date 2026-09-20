use crate::discord::poise_structs::{Context, Error};
use crate::travel::join::join;
use crate::travel::leave::leave;
use crate::travel::start::start;
use crate::travel::stop::stop;
use crate::travel::estimate::estimate;

/// Commande principale pour la gestion des déplacements et des groupes de voyage.
#[poise::command(
    slash_command, 
    guild_only, 
    subcommands("stop", "start", "join", "leave", "estimate"), 
    rename = "travel"
)]
pub async fn travel(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}
