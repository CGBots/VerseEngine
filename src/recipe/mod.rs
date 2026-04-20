
use crate::recipe::create_subcommand::create;
use crate::recipe::craft_subcommand::craft;
use crate::recipe::craft_stop_subcommand::stop;
use crate::discord::poise_structs::{Context, Error, Data};
use crate::translation::get;
use serenity::all::CreateInteractionResponse;
use serenity::json::json;

pub mod create_subcommand;
pub mod craft_subcommand;
pub mod craft_stop_subcommand;

pub struct RecipeModal {
    pub content: String,
}

pub async fn execute_recipe_modal(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    default_content: String,
) -> Result<Option<RecipeModal>, Error> {
    let title = get(poise::Context::Application(ctx), "recipe__modal_title", None, None);
    let label = get(poise::Context::Application(ctx), "recipe__modal_field_name", None, None);

    let custom_id = format!("{}-{}", ctx.interaction.id, "recipe_modal");

    let recipe_instructions = get(poise::Context::Application(ctx), "recipe__recipe_instructions", None, None);

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
                            "required": true
                        }
                    ]
                },
                {
                    "type": 10,
                    "content": recipe_instructions
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

        Ok(Some(RecipeModal { content }))
    } else {
        Ok(None)
    }
}

#[poise::command(slash_command, subcommands("create", "craft", "stop"), subcommand_required, guild_only)]
pub async fn recipe(_ctx: Context<'_>) -> Result<(), Error>{
    Ok(())
}