pub mod create_character_sub_command;
pub mod inventory_subcommand;

use crate::characters::create_character_sub_command::create_character;
use crate::characters::inventory_subcommand::inventory_subcommand;
use crate::discord::poise_structs::{Context, Error};

#[poise::command(slash_command, subcommands("create_character", "inventory_subcommand"), subcommand_required, rename = "character")]
pub async fn character(_ctx: Context<'_>) -> Result<(), Error>{
    Ok(())
}
