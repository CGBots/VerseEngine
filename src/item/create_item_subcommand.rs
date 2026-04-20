use futures::TryStreamExt;
use crate::database::items::{get_item_by_name, Item};
use poise::{CreateReply, ChoiceParameter};
use serenity::all::{Attachment, Colour, CreateEmbed, CreateForumPost, CreateMessage, CreateActionRow, CreateButton, ButtonStyle, Color, ComponentInteraction};
use serenity::prelude::Context as SerenityContext;
use crate::database::server::{get_server_by_id, Server};
use crate::discord::channels::ITEM_TAG;
use crate::discord::poise_structs::{Context, Error};
use crate::item::ItemUsage;
use crate::tr;
use crate::utility::reply::{reply, reply_with_args, reply_with_args_and_ephemeral};
use crate::utility::loot_table_parser::VALID_NAME_RE;
use fluent::FluentArgs;

pub static APPROVE_ITEM_BUTTON_CUSTOM_ID: &str = "item__approve";
pub static REJECT_ITEM_BUTTON_CUSTOM_ID: &str = "item__reject";

#[poise::command(slash_command, guild_only, rename="item_create")]
pub async fn create(
    ctx: Context<'_>,
    name: String,
    usage: ItemUsage,
    into_wiki: bool,
    inventory_size: Option<u64>,
    image: Option<Attachment>,
    item_description: Option<String>,
    secret_informations: Option<String>,
) -> Result<(), Error> {
    if !VALID_NAME_RE.is_match(&name) {
        let mut args = FluentArgs::new();
        args.set("name", name);
        let _ = reply_with_args(ctx, Err("create_item__invalid_name".into()), Some(args)).await;
        return Ok(());
    }

    let url = match image{
        None => {None}
        Some(image) => {Some(image.url)}
    };

    let guild_id = ctx.guild_id().unwrap();
    let Some(server) = get_server_by_id(guild_id.get()).await? else {return Err("item__server_not_found".into())};
    let universe_id = server.universe_id;

    // Vérifier si l'utilisateur est admin ou a le rôle de joueur
    let is_admin = ctx.author_member().await.map_or(false, |m| m.permissions.map_or(false, |p| p.administrator()));
    let has_player_role = if let Some(player_role) = &server.player_role_id {
        ctx.author().has_role(ctx.serenity_context(), guild_id, player_role.id).await.unwrap_or(false)
    } else {
        false
    };

    if !is_admin && !has_player_role {
        return Err("item__no_permission".into());
    }

    if get_item_by_name(universe_id, &name).await?.is_some() {
        let _ = reply(ctx, Err("create_item__already_exists".into())).await;
        return Ok(());
    }

    if is_admin {
        let result = Item{
            _id: Default::default(),
            universe_id: universe_id,
            item_name: name.clone(),
            item_usage: usage.clone(),
            effects: vec![],
            description: item_description.clone(),
            image: url.clone(),
            wiki_post_id: None,
            secret_informations: secret_informations,
            inventory_id: None,
            inventory_size: inventory_size.unwrap_or(0),
        }.save().await;

        match result{
            Ok(_) => {}
            Err(_) => { let _ = reply(ctx, Err("create_item__db_error".into())).await; return Ok(()) }
        }

        let embed = CreateEmbed::new()
            .title(name.clone())
            .description(item_description.clone().unwrap_or("".to_string()))
            .field(tr!(ctx.clone(), "item_usage_title"), tr!(ctx.clone(), usage.name()), true)
            .field(tr!(ctx.clone(), "item_inventory_size"), inventory_size.unwrap_or(0).to_string(), true)
            .colour(Colour::from_rgb(25, 125, 255))
            .thumbnail(url.clone().unwrap_or("".to_string()));

        if into_wiki {
            let Ok(servers_cursor) = server.get_other_servers().await else {return Err("item_db_error".into())};
            let servers = servers_cursor.try_collect::<Vec<Server>>().await.unwrap();

            for server in servers{
                if let Some(wiki_channel_id) = server.rp_wiki_channel_id{
                    let Ok(wiki_channel) = ctx.http().get_channel(wiki_channel_id.id.into()).await else {continue};
                    let channel = wiki_channel.guild().unwrap().clone();
                    let Some(item_tag) = channel.available_tags.iter().find(|tag| tag.name == ITEM_TAG) else {continue};
                    let _ = channel.create_forum_post(ctx, CreateForumPost::new(name.clone(), CreateMessage::new().embed(embed.clone())).add_applied_tag(item_tag.id)).await?;
                }
            }
        };

        ctx.send(CreateReply::default().embed(embed)).await?;
    } else {
        // Pour les joueurs, on envoie aux logs de l'univers pour validation
        submit_item_for_approval(
            ctx,
            name,
            usage,
            into_wiki,
            inventory_size,
            url,
            item_description,
            secret_informations,
            server
        ).await?;
    }
    
    Ok(())
}

async fn submit_item_for_approval(
    ctx: Context<'_>,
    name: String,
    usage: ItemUsage,
    into_wiki: bool,
    inventory_size: Option<u64>,
    image_url: Option<String>,
    item_description: Option<String>,
    secret_informations: Option<String>,
    server: Server,
) -> Result<(), Error> {
    use crate::database::universe::get_servers_from_universe;
    use crate::tr_locale;

    let locale = ctx.locale().unwrap_or("fr");

    let buttons = vec![
        CreateActionRow::Buttons(vec![
            CreateButton::new(format!("item:approve:{}", into_wiki)).label(tr_locale!(locale, APPROVE_ITEM_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
            CreateButton::new(format!("item:reject:{}", into_wiki)).label(tr_locale!(locale, REJECT_ITEM_BUTTON_CUSTOM_ID)).style(ButtonStyle::Danger),
        ])
    ];

    let mut embed = CreateEmbed::new()
        .title(format!("{} : {}", tr_locale!(locale, "create_item__validation_title"), name))
        .description(item_description.clone().unwrap_or_default())
        .field(tr_locale!(locale, "item_usage_title"), tr_locale!(locale, usage.name()), true)
        .field(tr_locale!(locale, "item_inventory_size"), inventory_size.unwrap_or(0).to_string(), true)
        .field(tr_locale!(locale, "create_item__creator_field"), ctx.author().name.clone(), true)
        .field(tr_locale!(locale, "create_item__into_wiki_field"), if into_wiki { "Oui/Yes" } else { "Non/No" }, true)
        .colour(Color::from_rgb(25, 125, 255));

    if let Some(url) = &image_url {
        embed = embed.thumbnail(url);
    }
    
    if let Some(secret) = &secret_informations {
        embed = embed.field(tr_locale!(locale, "create_item__secret_field"), secret, false);
    }

    let servers_cursor = get_servers_from_universe(&server.universe_id).await?;
    let servers = servers_cursor.try_collect::<Vec<Server>>().await?;

    for server in servers {
        if let Some(log_channel_id) = server.log_channel_id {
            let channel_id = serenity::all::ChannelId::new(log_channel_id.id);
            let _ = channel_id.send_message(&ctx, 
                CreateMessage::new()
                    .embed(embed.clone())
                    .components(buttons.clone()),
            ).await;
        }
    }

    reply_with_args_and_ephemeral(ctx, Ok("create_item__submit_success"), None, true).await?;
    Ok(())
}

pub async fn approve_item(ctx: SerenityContext, component_interaction: ComponentInteraction, into_wiki: bool) -> Result<&'static str, Error> {
    let guild_id = component_interaction.guild_id.ok_or("item__guild_only")?;
    let server = get_server_by_id(guild_id.get()).await?.ok_or("item__server_not_found")?;
    
    let is_moderator = if let Some(member) = &component_interaction.member {
        member.permissions.map_or(false, |p| p.administrator()) || 
        server.moderator_role_id.map_or(false, |r| member.roles.contains(&r.id.into()))
    } else {
        false
    };

    if !is_moderator {
        return Err("item__no_permission".into());
    }

    let embed = component_interaction.message.embeds.first().ok_or("item__no_embed")?;
    let name = embed.title.as_ref().and_then(|t| t.split(" : ").last()).ok_or("item__invalid_embed")?.to_string();
    let description = if embed.description.as_deref().unwrap_or_default().is_empty() { None } else { embed.description.clone() };
    
    let usage_str = embed.fields.iter().find(|f| {
        let name = f.name.to_lowercase();
        name.contains("usage")
    })
        .map(|f| f.value.clone()).ok_or("item__no_usage")?;
    
    // Convertir le nom traduit de l'usage en enum ItemUsage
    let usage = if usage_str.contains("Consumable") || usage_str.contains("Consommable") { ItemUsage::Consumable }
    else if usage_str.contains("Usable") || usage_str.contains("Utilisable") { ItemUsage::Usable }
    else if usage_str.contains("Placeable") || usage_str.contains("Plaçable") { ItemUsage::Placeable }
    else if usage_str.contains("Wearable") || usage_str.contains("Equipable") { ItemUsage::Wearable }
    else { ItemUsage::None };

    let inventory_size = embed.fields.iter().find(|f| f.name.contains("Inventaire") || f.name.contains("Inventory"))
        .and_then(|f| f.value.parse::<u64>().ok()).unwrap_or(0);
    
    let into_wiki = into_wiki;
    
    let image = embed.thumbnail.as_ref().map(|t| t.url.clone());
    
    let secret_informations = embed.fields.iter().find(|f| f.name.contains("Secret"))
        .map(|f| f.value.clone());

    let item = Item {
        _id: Default::default(),
        universe_id: server.universe_id,
        item_name: name.clone(),
        item_usage: usage.clone(),
        effects: vec![],
        description: description.clone(),
        image: image.clone(),
        wiki_post_id: None,
        secret_informations,
        inventory_id: None,
        inventory_size,
    };

    item.save().await?;

    if into_wiki {
        use crate::database::universe::get_servers_from_universe;
        let servers_cursor = get_servers_from_universe(&server.universe_id).await?;
        let servers = servers_cursor.try_collect::<Vec<Server>>().await?;
        
        let wiki_embed = CreateEmbed::new()
            .title(name.clone())
            .description(description.unwrap_or_default())
            .field("Usage", usage.name(), true) // On utilise usage.name() ici car sync_recipe_wiki semble faire de même (non, sync_recipe_wiki utilise tr!)
            .field("Inventaire / Inventory", inventory_size.to_string(), true)
            .colour(Colour::from_rgb(25, 125, 255))
            .thumbnail(image.unwrap_or_default());

        for s in servers {
            if let Some(wiki_channel_id) = s.rp_wiki_channel_id {
                if let Ok(wiki_channel) = ctx.http.get_channel(wiki_channel_id.id.into()).await {
                    if let Some(channel) = wiki_channel.guild() {
                        let item_tag = channel.available_tags.iter().find(|tag| tag.name == ITEM_TAG);
                        let mut post = CreateForumPost::new(name.clone(), CreateMessage::new().embed(wiki_embed.clone()));
                        if let Some(tag) = item_tag {
                            post = post.add_applied_tag(tag.id);
                        }
                        let _ = channel.create_forum_post(&ctx, post).await;
                    }
                }
            }
        }
    }

    let mut new_embed = CreateEmbed::from(embed.clone());
    new_embed = new_embed.color(Color::from_rgb(0, 255, 0));
    
    component_interaction.channel_id.edit_message(&ctx, component_interaction.message.id, 
        serenity::all::EditMessage::new().embed(new_embed).components(vec![])
    ).await?;

    component_interaction.create_response(&ctx, serenity::all::CreateInteractionResponse::Acknowledge).await?;

    Ok("create_item__approved")
}

pub async fn reject_item(ctx: SerenityContext, component_interaction: ComponentInteraction, _into_wiki: bool) -> Result<&'static str, Error> {
    let guild_id = component_interaction.guild_id.ok_or("item__guild_only")?;
    let server = get_server_by_id(guild_id.get()).await?.ok_or("item__server_not_found")?;
    
    let is_moderator = if let Some(member) = &component_interaction.member {
        member.permissions.map_or(false, |p| p.administrator()) || 
        server.moderator_role_id.map_or(false, |r| member.roles.contains(&r.id.into()))
    } else {
        false
    };

    if !is_moderator {
        return Err("item__no_permission".into());
    }

    let embed = component_interaction.message.embeds.first().ok_or("item__no_embed")?;
    let mut new_embed = CreateEmbed::from(embed.clone());
    new_embed = new_embed.color(Color::from_rgb(255, 0, 0));
    
    component_interaction.channel_id.edit_message(&ctx, component_interaction.message.id, 
        serenity::all::EditMessage::new().embed(new_embed).components(vec![])
    ).await?;

    component_interaction.create_response(&ctx, serenity::all::CreateInteractionResponse::Acknowledge).await?;

    Ok("create_item__rejected")
}