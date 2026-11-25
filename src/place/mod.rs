use crate::place::create_place_sub_command::create_place;
use crate::discord::poise_structs::{Context, Error};

pub mod create_place_sub_command;
pub mod wiki;

#[poise::command(slash_command, subcommands("create_place"), subcommand_required, rename = "place")]
pub async fn place(_ctx: Context<'_>) -> Result<(), Error>{
    Ok(())
}