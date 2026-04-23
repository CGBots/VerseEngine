pub mod create_item_subcommand;
pub mod lookup_subcommand;
pub mod place_subcommand;
pub mod use_subcommand;
pub mod consume_subcommand;
pub mod delete_subcommand;
pub mod populate_test_subcommand;

use crate::item::create_item_subcommand::create;
use crate::item::lookup_subcommand::lookup_subcommand;
use crate::item::place_subcommand::item_place;
use crate::item::use_subcommand::item_use;
use crate::item::consume_subcommand::consume;
use crate::item::delete_subcommand::delete;
use crate::item::populate_test_subcommand::populate_test;
use crate::discord::poise_structs::{Context, Error, Data};
use crate::translation::get;
use poise::{ChoiceParameter};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use serenity::all::CreateInteractionResponse;
use serenity::json::json;

pub struct ItemEffectModal {
    pub content: String,
}

pub async fn execute_item_effect_modal(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    default_content: String,
) -> Result<Option<ItemEffectModal>, Error> {
    let title = get(poise::Context::Application(ctx), "item_effect__modal_title", None, None);
    let label = get(poise::Context::Application(ctx), "item_effect__modal_field_name", None, None);
    let placeholder = get(poise::Context::Application(ctx), "item_effect__modal_placeholder", None, None);

    let custom_id = format!("{}-{}", ctx.interaction.id, "item_effect_modal");

    let modal_json = json!({
        "type": 9,
        "data": {
            "custom_id": custom_id,
            "title": title,
            "components": [
                {
                    "type": 1,
                    "components": [
                        {
                            "type": 4,
                            "custom_id": "content",
                            "label": label,
                            "style": 2,
                            "value": default_content,
                            "required": false
                        }
                    ]
                },
                {
                    "type": 10,
                    "content": placeholder,
                }
            ]
        }
    });

    ctx.serenity_context.http.create_interaction_response(ctx.interaction.id, &ctx.interaction.token, &modal_json, vec![]).await?;
    ctx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);

    let response = serenity::collector::ModalInteractionCollector::new(ctx.serenity_context)
        .filter(move |m| m.data.custom_id == custom_id)
        .timeout(std::time::Duration::from_secs(600))
        .await;

    if let Some(m) = response {
        let content = m.data.components.iter()
            .flat_map(|row| row.components.iter())
            .find_map(|component| {
                if let serenity::all::ActionRowComponent::InputText(it) = component {
                    if it.custom_id == "content" {
                        return Some(it.value.clone().unwrap_or_default());
                    }
                }
                None
            })
            .unwrap_or_default();

        m.create_response(ctx.serenity_context, CreateInteractionResponse::Acknowledge).await?;
        ctx.has_sent_initial_response.store(true, std::sync::atomic::Ordering::SeqCst);

        Ok(Some(ItemEffectModal { content }))
    } else {
        Ok(None)
    }
}

#[serde_as]
#[derive(ChoiceParameter, Serialize, Deserialize, Debug, Clone)]
pub enum ItemUsage{
    Consumable, //Peut être consommée => Usage unique
    Usable, //Peut être utilisé => Usage multiple
    Placeable, //Peut être placé => Outils ou stockage utilisables liés à un lieu => Usage Unique
    Wearable, //Peut être équipé => Usage "multiples"
    None //Pas d'usage => Pour des items purement décoratif ou de lore
}

#[poise::command(slash_command, subcommands("create", "lookup_subcommand", "item_place", "item_use", "consume", "delete", "populate_test"))]
pub async fn item( _ctx: Context<'_>, ) -> Result<(), Error> { Ok(()) }
