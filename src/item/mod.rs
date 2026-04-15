pub mod create_item_subcommand;
pub mod lookup_subcommand;
pub mod place_subcommand;
pub mod use_subcommand;

use crate::item::create_item_subcommand::create;
use crate::item::lookup_subcommand::lookup_subcommand;
use crate::item::place_subcommand::item_place;
use crate::item::use_subcommand::item_use;
use crate::discord::poise_structs::{Context, Error};
use poise::{ChoiceParameter};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(ChoiceParameter, Serialize, Deserialize, Debug, Clone)]
pub enum ItemUsage{
    Consumable, //Peut être consommée => Usage unique
    Usable, //Peut être utilisé => Usage multiple
    Placeable, //Peut être placé => Outils ou stockage utilisables liés à un lieu => Usage Unique
    Wearable, //Peut être équipé => Usage "multiples"
    None //Pas d'usage => Pour des items purement décoratif ou de lore
}

#[poise::command(slash_command, subcommands("create", "lookup_subcommand", "item_place", "item_use"))]
pub async fn item( _ctx: Context<'_>, ) -> Result<(), Error> { Ok(()) }
