pub mod create_item_subcommand;
pub mod lookup_subcommand;

use crate::item::create_item_subcommand::create;
use crate::item::lookup_subcommand::lookup_subcommand;
use crate::discord::poise_structs::{Context, Error};
use poise::{ChoiceParameter};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(ChoiceParameter, Serialize, Deserialize, Debug, Clone)]
pub enum ItemUsage{
    Consumable, //Peut être consommée => Usage unique
    Usable, //Peut être utilisé => Usage multiple
    Placeable, //Peut être placé => Outils utilisables liés à un lieu => Usage Unique
    Wearable, //Peut être équipé => Usage "multiples"
    None //Pas d'usage => Pour des items purement décoratif ou de lore
}

#[poise::command(slash_command, subcommands("create", "lookup_subcommand"))]
pub async fn item( _ctx: Context<'_>, ) -> Result<(), Error> { Ok(()) }
