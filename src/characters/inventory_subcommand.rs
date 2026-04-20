use serenity::all::{CreateInteractionResponse, CreateInteractionResponseMessage};
use crate::database::characters::{get_character_by_user_id, Character, get_character_by_id};
use crate::database::inventory::Inventory;
use crate::database::items::get_item_by_id;
use crate::database::universe::{get_universe_by_server_id, Universe, get_universe_by_id};
use crate::discord::poise_structs::{Context, Error};
use crate::utility::carousel::{CarouselConfig, CarouselPage, create_carousel_embed, create_carousel_components, paginate_text};
use fluent::FluentArgs;

/// Renvoie l'inventaire du personnage sous forme de liste paginée en DM.
#[poise::command(slash_command, guild_only, rename = "character_inventory")]
pub async fn inventory_subcommand(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().ok_or("inventory__not_in_guild")?.get();
    let universe = get_universe_by_server_id(guild_id).await?.ok_or("loot_table__universe_not_found")?;
    let character = get_character_by_user_id(universe.universe_id, ctx.author().id.get()).await?.ok_or("loot_table__character_not_found")?;
    
    let (embed, components) = create_inventory_page(
        0,
        &character,
        &universe,
        &ctx.locale().unwrap_or("fr")
    ).await?;

    ctx.send(poise::CreateReply::default().embed(embed).components(components).ephemeral(true)).await?;

    Ok(())
}

pub async fn create_inventory_page(
    page_idx: usize,
    character: &Character,
    universe: &Universe,
    locale: &str,
) -> Result<(serenity::all::CreateEmbed, Vec<serenity::all::CreateActionRow>), Error> {
    let inventory_entries = Inventory::get_by_character_id(universe.universe_id, character._id).await?;

    let mut items_text = Vec::new();
    for entry in inventory_entries {
        if let Some(item) = get_item_by_id(entry.item_id).await? {
            let id_str = entry._id.map(|oid| oid.to_hex()).unwrap_or_else(|| "N/A".to_string());
            items_text.push(format!("- {}x {} (ID: `{}`)", entry.quantity, item.item_name, id_str));
        }
    }

    // Pagination logic
    let items_per_page = 15;
    let empty_msg = crate::translation::get_by_locale(locale, "inventory__empty_description", None, None);
    let pages = paginate_text(&items_text, items_per_page, &empty_msg);
    
    let total_pages = pages.len();
    let current_page = page_idx.min(total_pages - 1);

    // Build the page
    let mut title_args = FluentArgs::new();
    title_args.set("character_name", character.name.to_string());
    
    let mut footer_args = FluentArgs::new();
    footer_args.set("current", current_page + 1);
    footer_args.set("total", total_pages);

    let carousel_page = CarouselPage {
        title: crate::translation::get_by_locale(locale, "inventory__title", None, Some(&title_args)),
        description: pages[current_page].clone(),
        fields: vec![
            (crate::translation::get_by_locale(locale, "inventory__universe_field", None, None), universe.name.clone(), true)
        ],
        footer: crate::translation::get_by_locale(locale, "inventory__page_footer", None, Some(&footer_args)),
        color: serenity::all::Colour::BLUE,
    };

    let carousel_config = CarouselConfig {
        prefix: "inv".to_string(),
        current_page,
        total_pages,
        metadata: vec![character._id.to_hex(), universe.universe_id.to_hex()],
    };

    let embed = create_carousel_embed(carousel_page);
    let components = create_carousel_components(carousel_config, locale);

    Ok((embed, components))
}

pub async fn handle_inventory_interaction(
    ctx: serenity::all::Context,
    component: serenity::all::ComponentInteraction,
    character_id_hex: &str,
    universe_id_hex: &str,
    page: usize,
) -> Result<&'static str, Error> {
    let character_id = mongodb::bson::oid::ObjectId::parse_str(character_id_hex).map_err(|_| "Invalid character ID")?;
    let universe_id = mongodb::bson::oid::ObjectId::parse_str(universe_id_hex).map_err(|_| "Invalid universe ID")?;

    let character = get_character_by_id(character_id).await?.ok_or("Character not found")?;
    let universe = get_universe_by_id(universe_id).await?.ok_or("Universe not found")?;
    
    let (embed, components) = create_inventory_page(
        page,
        &character,
        &universe,
        component.locale.as_str()
    ).await?;

    component.create_response(&ctx, CreateInteractionResponse::UpdateMessage(
        CreateInteractionResponseMessage::new()
            .embed(embed)
            .components(components)
    )).await?;

    Ok("")
}
