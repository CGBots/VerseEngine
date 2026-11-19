use crate::roads::create_road_sub_command::create_road;
use crate::discord::poise_structs::{Context, Error};

pub mod create_road_sub_command;
pub mod migration;

#[poise::command(slash_command, subcommands("create_road"), subcommand_required, rename = "road")]
pub async fn road(_ctx: Context<'_>) -> Result<(), Error>{
    Ok(())
}
