use crate::{tr_locale};
use serenity::all::{CacheHttp, Color, ComponentInteraction, CreateEmbed, CreateInputText, CreateInteractionResponse, EditMessage, InputTextStyle};
use serenity::client::Context as SerenityContext;
use serenity::utils::CreateQuickModal;
use std::time::Duration;
use crate::database::server::{get_server_by_id, Server};
use crate::database::recipe::Recipe;
use crate::discord::poise_structs::{Context, Error};
use crate::recipe::execute_recipe_modal;
use crate::utility::recipe_parser::RecipeParser;
use crate::utility::reply::{reply, reply_with_args_and_ephemeral};

/// Crée une nouvelle recette (admin ou joueur).
#[poise::command(slash_command, guild_only, rename = "recipe_create")]
pub async fn create(
    ctx: Context<'_>,
    name: String,
    delay: Option<u64>,
    into_wiki: Option<bool>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let server = get_server_by_id(guild_id.get()).await?.ok_or("recipe__server_not_found")?;

    // Vérifier si l'utilisateur est admin ou a le rôle de joueur
    let is_admin = ctx.author_member().await.map_or(false, |m| m.permissions.map_or(false, |p| p.administrator()));
    let has_player_role = if let Some(player_role) = &server.player_role_id {
        ctx.author().has_role(ctx.serenity_context(), guild_id, player_role.id).await.unwrap_or(false)
    } else {
        false
    };

    if !is_admin && !has_player_role {
        return Err("recipe__no_permission".into());
    }

    let existing_recipe = Recipe::get_by_name(server.universe_id, &name).await?;
    let wiki_posts_id = existing_recipe.as_ref().and_then(|r| r.wiki_posts_id.clone());
    let default_content = existing_recipe.map(|r| r.raw_text).unwrap_or_default();

    let modal_result = match ctx {
        poise::Context::Application(app_ctx) => {
            execute_recipe_modal(app_ctx, default_content).await?
        }
        _ => return Err("recipe__slash_only".into()),
    };

    if let Some(modal_data) = modal_result {
        if is_admin {
            let parsed = RecipeParser::parse(&modal_data.content, server.universe_id).await;
            
            match parsed {
                Ok(p) => {
                    let mut recipe = Recipe {
                        _id: None,
                        universe_id: server.universe_id,
                        tool_id: None,
                        ingredients: p.ingredients,
                        result: p.result,
                        tools_needed: p.tools_needed,
                        delay: delay.unwrap_or(0),
                        raw_text: modal_data.content,
                        recipe_name: name.clone(),
                        wiki_posts_id: wiki_posts_id,
                    };

                    sync_recipe_wiki(ctx.serenity_context(), &mut recipe, &server, into_wiki.unwrap_or(false)).await?;
                    
                    recipe.upsert().await?;
                    reply(ctx, Ok("recipe__create_success")).await?;
                }
                Err(e) => {
                    if e.starts_with("recipe__item_not_found:") {
                        let item_name = e.strip_prefix("recipe__item_not_found:").unwrap();
                        let mut args = fluent::FluentArgs::new();
                        args.set("name", item_name.to_string());
                        crate::utility::reply::reply_with_args(ctx, Err("recipe__item_not_found".into()), Some(args)).await?;
                    } else {
                        reply(ctx, Err(e.into())).await?;
                    }
                }
            }
        } else {
            // Pour les joueurs, on envoie aux logs de l'univers
            submit_recipe_for_approval(ctx, name, delay.unwrap_or(0), modal_data.content, server, into_wiki.unwrap_or(false)).await?;
        }
    }

    Ok(())
}

pub static APPROVE_RECIPE_BUTTON_CUSTOM_ID: &str = "recipe__approve";
pub static REJECT_RECIPE_BUTTON_CUSTOM_ID: &str = "recipe__reject";
pub static MODIFY_RECIPE_BUTTON_CUSTOM_ID: &str = "recipe__modify";

async fn submit_recipe_for_approval(
    ctx: Context<'_>,
    name: String,
    delay: u64,
    content: String,
    server: Server,
    into_wiki: bool,
) -> Result<(), Error> {
    use serenity::all::{CreateActionRow, CreateButton, ButtonStyle, CreateEmbed, CreateEmbedFooter, CreateMessage, Color};
    use crate::database::universe::get_servers_from_universe;
    use futures::TryStreamExt;
    use crate::tr_locale;

    let locale = ctx.locale().unwrap_or("fr");

    let buttons = vec![
        CreateActionRow::Buttons(vec![
            CreateButton::new(APPROVE_RECIPE_BUTTON_CUSTOM_ID).label(tr_locale!(locale, APPROVE_RECIPE_BUTTON_CUSTOM_ID)).style(ButtonStyle::Success),
            CreateButton::new(MODIFY_RECIPE_BUTTON_CUSTOM_ID).label(tr_locale!(locale, MODIFY_RECIPE_BUTTON_CUSTOM_ID)).style(ButtonStyle::Primary),
            CreateButton::new(REJECT_RECIPE_BUTTON_CUSTOM_ID).label(tr_locale!(locale, REJECT_RECIPE_BUTTON_CUSTOM_ID)).style(ButtonStyle::Danger),
        ])
    ];

    let embed = CreateEmbed::new()
        .title(format!("{} : {}", tr_locale!(locale, "recipe__validation_title"), name))
        .description(format!("```\n{}\n```", content))
        .field(tr_locale!(locale, "recipe__delay_field"), delay.to_string(), true)
        .field(tr_locale!(locale, "recipe__creator_field"), ctx.author().name.clone(), true)
        .field(tr_locale!(locale, "recipe__into_wiki_field"), if into_wiki { "Oui/Yes" } else { "Non/No" }, true)
        .footer(CreateEmbedFooter::new(ctx.author().id.to_string()))
        .color(Color::from_rgb(255, 165, 0)); // Orange pour en attente

    let notification_text = tr_locale!(locale, "recipe__submit_notification");
    
    let mut servers = get_servers_from_universe(&server.universe_id).await?;
    while let Some(s) = servers.try_next().await? {
        if let Some(log_channel) = s.log_channel_id {
            let _ = ctx.serenity_context().http.send_message(
                log_channel.id.into(),
                vec![],
                &CreateMessage::new()
                    .content(&notification_text)
                    .embed(embed.clone())
                    .components(buttons.clone()),
            ).await;
        }
    }

    reply_with_args_and_ephemeral(ctx, Ok("recipe__submit_success"), None, true).await?;
    Ok(())
}

pub async fn approve_recipe(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    // Logique d'approbation (similaire à accept_character)
    // 1. Vérifier les permissions de modérateur
    let guild_id = component_interaction.guild_id.ok_or("recipe__guild_only")?;
    let server = get_server_by_id(guild_id.get()).await?.ok_or("recipe__server_not_found")?;
    
    // Vérification simplifiée pour l'instant, on peut réutiliser verify_moderator_permission de character si besoin
    let is_moderator = if let Some(member) = &component_interaction.member {
        member.permissions.map_or(false, |p| p.administrator()) || 
        server.moderator_role_id.map_or(false, |r| member.roles.contains(&r.id.into()))
    } else {
        false
    };

    if !is_moderator {
        return Err("recipe__no_permission".into());
    }

    let embed = component_interaction.message.embeds.first().ok_or("recipe__no_embed")?;
    let recipe_name = embed.title.as_ref().and_then(|t| t.split(" : ").last()).ok_or("recipe__invalid_embed")?.to_string();
    let content = embed.description.as_ref().ok_or("recipe__no_content")?
        .trim_start_matches("```\n").trim_start_matches("```")
        .trim_end_matches("\n```").trim_end_matches("```")
        .to_string();
    let delay = embed.fields.iter().find(|f| f.name.contains("Délai") || f.name.contains("Delay"))
        .and_then(|f| f.value.parse::<u64>().ok()).unwrap_or(0);
    
    let into_wiki = embed.fields.iter().find(|f| f.name.contains("Wiki"))
        .map(|f| f.value.contains("Oui") || f.value.contains("Yes")).unwrap_or(false);

    let parsed = RecipeParser::parse(&content, server.universe_id).await?;
    
    let existing_recipe = Recipe::get_by_name(server.universe_id, &recipe_name).await?;
    let wiki_posts_id = existing_recipe.as_ref().and_then(|r| r.wiki_posts_id.clone());

    let mut recipe = Recipe {
        _id: None,
        universe_id: server.universe_id,
        tool_id: None,
        ingredients: parsed.ingredients,
        result: parsed.result,
        tools_needed: parsed.tools_needed,
        delay,
        raw_text: content,
        recipe_name,
        wiki_posts_id: wiki_posts_id,
    };

    sync_recipe_wiki(&ctx, &mut recipe, &server, into_wiki).await?;
    
    recipe.upsert().await?;

    // Mettre à jour l'embed pour montrer qu'il est approuvé
    let mut new_embed = CreateEmbed::from(embed.clone());
    new_embed = new_embed.color(Color::from_rgb(0, 255, 0));
    
    component_interaction.channel_id.edit_message(&ctx, component_interaction.message.id, 
        EditMessage::new().embed(new_embed).components(vec![])
    ).await?;

    component_interaction.create_response(&ctx, CreateInteractionResponse::Acknowledge).await?;

    Ok("recipe__approved")
}

pub async fn reject_recipe(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let guild_id = component_interaction.guild_id.ok_or("recipe__guild_only")?;
    let server = get_server_by_id(guild_id.get()).await?.ok_or("recipe__server_not_found")?;
    
    let is_moderator = if let Some(member) = &component_interaction.member {
        member.permissions.map_or(false, |p| p.administrator()) || 
        server.moderator_role_id.map_or(false, |r| member.roles.contains(&r.id.into()))
    } else {
        false
    };

    if !is_moderator {
        return Err("recipe__no_permission".into());
    }

    let embed = component_interaction.message.embeds.first().ok_or("recipe__no_embed")?;
    let mut new_embed = CreateEmbed::from(embed.clone());
    new_embed = new_embed.color(Color::from_rgb(255, 0, 0));
    
    component_interaction.channel_id.edit_message(&ctx, component_interaction.message.id, 
        EditMessage::new().embed(new_embed).components(vec![])
    ).await?;

    component_interaction.create_response(&ctx, CreateInteractionResponse::Acknowledge).await?;

    Ok("recipe__rejected")
}

pub async fn modify_recipe_interaction(ctx: SerenityContext, component_interaction: ComponentInteraction) -> Result<&'static str, Error> {
    let guild_id = component_interaction.guild_id.ok_or("recipe__guild_only")?;
    let server = get_server_by_id(guild_id.get()).await?.ok_or("recipe__server_not_found")?;
    
    let is_moderator = if let Some(member) = &component_interaction.member {
        member.permissions.map_or(false, |p| p.administrator()) || 
        server.moderator_role_id.map_or(false, |r| member.roles.contains(&r.id.into()))
    } else {
        false
    };

    if !is_moderator {
        return Err("recipe__no_permission".into());
    }

    let embed = component_interaction.message.embeds.first().ok_or("recipe__no_embed")?;
    let _recipe_name = embed.title.as_ref().and_then(|t| t.split(" : ").last()).ok_or("recipe__invalid_embed")?.to_string();
    let content = embed.description.as_ref().ok_or("recipe__no_content")?
        .trim_start_matches("```\n").trim_start_matches("```")
        .trim_end_matches("\n```").trim_end_matches("```")
        .to_string();
    let _delay = embed.fields.iter().find(|f| f.name.contains("Délai") || f.name.contains("Delay"))
        .and_then(|f| f.value.parse::<u64>().ok()).unwrap_or(0);

    let locale = component_interaction.locale.as_str();

    // Ouvrir un modal pré-rempli
    let modal_title = tr_locale!(locale, "recipe__modal_title");
    let label = tr_locale!(locale, "recipe__modal_field_name");

    let modal = CreateQuickModal::new(modal_title)
        .field(CreateInputText::new(InputTextStyle::Paragraph, label, "content")
            .value(content)
            .required(true)
            .style(InputTextStyle::Paragraph)
        )
        .timeout(Duration::from_secs(600));

    let response = component_interaction.quick_modal(&ctx, modal).await?;
    if let Some(modal_response) = response {
        let new_content = modal_response.inputs.first().cloned().unwrap_or_default();
        modal_response.interaction.create_response(&ctx, CreateInteractionResponse::Acknowledge).await?;

        // Mettre à jour l'embed avec le nouveau contenu
        let mut new_embed = CreateEmbed::from(embed.clone());
        new_embed = new_embed.description(format!("```\n{}\n```", new_content));
        
        component_interaction.channel_id.edit_message(&ctx, component_interaction.message.id, 
            EditMessage::new().embed(new_embed)
        ).await?;
    }

    Ok("recipe__modified")
}

async fn sync_recipe_wiki(ctx: &impl CacheHttp, recipe: &mut Recipe, server: &Server, into_wiki: bool) -> Result<(), Error> {
    // Nettoyage des imports inutilisés détectés par cargo check
    use crate::discord::channels::RECIPE_TAG;
    use serenity::all::{CreateForumPost, CreateMessage, Colour, EditChannel, CreateForumTag};
    use crate::database::universe::get_servers_from_universe;
    use futures::TryStreamExt;

    let mut wiki_posts = recipe.wiki_posts_id.clone().unwrap_or_default();

    if !into_wiki {
        // Supprimer les posts existants
        for (_guild_id, message_id) in wiki_posts.drain(..) {
            // On tente de supprimer, on ignore les erreurs si le post est déjà supprimé
            let _ = ctx.http().delete_message(message_id.into(), message_id.into(), None).await;
            // Note: Discord forum posts sont des threads, delete_message sur le message_id du post
            // pourrait ne pas être suffisant ou correct selon comment serenity le gère.
            // En fait, create_forum_post renvoie un GuildChannel (le thread).
            // Pour supprimer un thread, on utilise delete_channel.
            let _ = ctx.http().delete_channel(message_id.into(), None).await;
        }
        recipe.wiki_posts_id = None;
        return Ok(());
    }
    
    let embed = CreateEmbed::new()
        .title(recipe.recipe_name.clone())
        .description(format!("```\n{}\n```", recipe.raw_text))
        .field("Délai/Delay", recipe.delay.to_string(), true)
        .colour(Colour::from_rgb(25, 125, 255));

    let mut servers_cursor = get_servers_from_universe(&server.universe_id).await?;
    while let Some(server_doc) = servers_cursor.try_next().await? {
        if let Some(wiki_channel_id) = server_doc.rp_wiki_channel_id {
            if wiki_posts.iter().any(|(guild_id, _)| *guild_id == server_doc.server_id) {
                // TODO: Update existing post if needed
                continue; 
            }

            if let Ok(wiki_channel) = ctx.http().get_channel(wiki_channel_id.id.into()).await {
                let Some(mut channel) = wiki_channel.guild() else { continue };
                
                let mut recipe_tag = channel.available_tags.iter().find(|tag| {
                    tag.name.to_lowercase() == RECIPE_TAG.to_lowercase() || tag.name == "Recette"
                }).map(|t| t.id);

                if recipe_tag.is_none() {
                    let mut tags: Vec<CreateForumTag> = channel.available_tags.iter().map(|tag| {
                        let mut t = CreateForumTag::new(tag.name.clone());
                        // On ignore l'emoji pour l'instant car la structure ForumEmoji semble opaque ou changeante
                        // et sa reconstruction pose des problèmes de compilation.
                        t = t.moderated(tag.moderated);
                        t
                    }).collect();
                    
                    tags.push(CreateForumTag::new("Recette"));
                    
                    let edit = EditChannel::new().available_tags(tags);
                    if let Ok(updated_channel) = channel.id.edit(ctx, edit).await {
                        channel = updated_channel;
                        recipe_tag = channel.available_tags.iter().find(|tag| {
                            tag.name == "Recette"
                        }).map(|t| t.id);
                    }
                }
                
                let mut post_builder = CreateForumPost::new(recipe.recipe_name.clone(), CreateMessage::new().embed(embed.clone()));
                if let Some(tag_id) = recipe_tag {
                    post_builder = post_builder.add_applied_tag(tag_id);
                }
                
                if let Ok(post) = channel.create_forum_post(ctx, post_builder).await {
                    wiki_posts.push((server_doc.server_id, post.id.get()));
                }
            }
        }
    }
    
    recipe.wiki_posts_id = if wiki_posts.is_empty() { None } else { Some(wiki_posts) };
    Ok(())
}