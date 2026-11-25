//! # Service des recettes partagé
//!
//! Enregistrement et suppression d'une recette, sans `poise::Context` : `/recipe create`
//! (administrateur), l'approbation d'une proposition de joueur et l'éditeur de recettes de
//! l'activité (mode admin) appellent ces mêmes fonctions.
//!
//! Une recette est toujours enregistrée sous sa forme texte (`raw_text`, syntaxe du modal de
//! `/recipe create`), analysée par [`RecipeParser`] : l'éditeur de l'activité produit ce texte
//! ([`recipe_text`]), donc la commande slash peut ensuite rouvrir la recette telle quelle.
//!
//! Wiki : un post par serveur de l'univers dans le forum wiki (`rp_wiki_channel_id`). Les posts
//! existants sont mis à jour (ils ne l'étaient pas avant : un `TODO` de la commande slash).

use mongodb::bson::oid::ObjectId;
use serenity::all::{CacheHttp, ChannelId, Colour, CreateEmbed, EditMessage, EditThread, MessageId};

use crate::database::recipe::Recipe;
use crate::discord::poise_structs::Error;
use crate::utility::recipe_parser::RecipeParser;

/// Longueur maximale d'un nom de recette (titre d'un post de forum : 100 caractères).
pub const MAX_RECIPE_NAME: usize = 100;
/// Délai maximal d'un exemplaire : 30 jours.
pub const MAX_RECIPE_DELAY: u64 = 30 * 24 * 3600;
/// Lignes au plus (ingrédients + résultats + outils).
pub const MAX_RECIPE_LINES: usize = 50;
/// Quantité maximale d'une ligne.
pub const MAX_LINE_QUANTITY: u64 = 10_000;

/// Échec de l'enregistrement : texte de la recette invalide (message du [`RecipeParser`], par
/// exemple `recipe__item_not_found:<nom>`), ou autre erreur.
#[derive(Debug)]
pub enum SaveError {
    Parse(String),
    Other(Error),
}

impl From<Error> for SaveError {
    fn from(e: Error) -> Self { SaveError::Other(e) }
}

impl From<mongodb::error::Error> for SaveError {
    fn from(e: mongodb::error::Error) -> Self { SaveError::Other(e.into()) }
}

/// Nom de recette acceptable : non vide, sans saut de ligne, 100 caractères au plus.
pub fn valid_recipe_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n == name && n.chars().count() <= MAX_RECIPE_NAME && !n.chars().any(char::is_control)
}

/// Texte de recette (syntaxe de `/recipe create`) : `< nom quantité` pour un ingrédient,
/// `> nom quantité` pour un résultat, `- nom` pour un outil. La quantité est toujours écrite :
/// un nom qui finit par un nombre (« Potion 2 ») n'est ainsi jamais mal lu.
pub fn recipe_text(ingredients: &[(u64, String)], results: &[(u64, String)], tools: &[String]) -> String {
    let mut lines = Vec::new();
    lines.extend(ingredients.iter().map(|(q, n)| format!("< {n} {q}")));
    lines.extend(results.iter().map(|(q, n)| format!("> {n} {q}")));
    lines.extend(tools.iter().map(|n| format!("- {n}")));
    lines.join("\n")
}

/// Embed du post wiki d'une recette.
fn wiki_embed(recipe: &Recipe) -> CreateEmbed {
    CreateEmbed::new()
        .title(recipe.recipe_name.clone())
        .description(format!("```\n{}\n```", recipe.raw_text))
        .field("Délai/Delay", recipe.delay.to_string(), true)
        .colour(Colour::from_rgb(25, 125, 255))
}

/// Ressource Discord absente (post supprimé à la main, par exemple).
pub fn is_not_found(e: &serenity::Error) -> bool {
    matches!(e, serenity::Error::Http(serenity::all::HttpError::UnsuccessfulRequest(r)) if r.status_code.as_u16() == 404)
}

/// Publie, met à jour ou retire les posts wiki de la recette. `renamed` : le nom a changé (le
/// titre des posts existants est alors modifié ; Discord limite fortement les renommages).
pub async fn sync_recipe_wiki(ctx: &impl CacheHttp, recipe: &mut Recipe, universe_id: ObjectId, into_wiki: bool, renamed: bool) -> Result<(), Error> {
    use crate::database::universe::get_servers_from_universe;
    use crate::discord::channels::RECIPE_TAG;
    use futures::TryStreamExt;
    use serenity::all::{CreateForumPost, CreateForumTag, CreateMessage, EditChannel};

    let mut wiki_posts = recipe.wiki_posts_id.clone().unwrap_or_default();

    if !into_wiki {
        // Supprimer les posts existants
        for (_guild_id, message_id) in wiki_posts.drain(..) {
            // On tente de supprimer, on ignore les erreurs si le post est déjà supprimé
            let _ = ctx.http().delete_message(message_id.into(), message_id.into(), None).await;
            // Un post de forum est un fil : le supprimer supprime aussi son premier message.
            let _ = ctx.http().delete_channel(message_id.into(), None).await;
        }
        recipe.wiki_posts_id = None;
        return Ok(());
    }

    let embed = wiki_embed(recipe);

    // Posts existants : le premier message d'un post de forum a l'identifiant du fil.
    let mut kept = Vec::with_capacity(wiki_posts.len());
    for (guild_id, thread_id) in wiki_posts.drain(..) {
        let thread = ChannelId::new(thread_id);
        if renamed {
            let _ = thread.edit_thread(ctx, EditThread::new().name(recipe.recipe_name.clone())).await;
        }
        match thread.edit_message(ctx, MessageId::new(thread_id), EditMessage::new().embed(embed.clone())).await {
            // Post supprimé : il sera recréé ci-dessous.
            Err(e) if is_not_found(&e) => {}
            _ => kept.push((guild_id, thread_id)),
        }
    }
    wiki_posts = kept;

    let mut servers_cursor = get_servers_from_universe(&universe_id).await?;
    while let Some(server_doc) = servers_cursor.try_next().await? {
        if let Some(wiki_channel_id) = server_doc.rp_wiki_channel_id {
            if wiki_posts.iter().any(|(guild_id, _)| *guild_id == server_doc.server_id) {
                continue;
            }

            if let Ok(wiki_channel) = ctx.http().get_channel(wiki_channel_id.id.into()).await {
                let Some(mut channel) = wiki_channel.guild() else { continue };

                let mut recipe_tag = channel.available_tags.iter().find(|tag| {
                    tag.name.to_lowercase() == RECIPE_TAG.to_lowercase() || tag.name == "Recette"
                }).map(|t| t.id);

                if recipe_tag.is_none() {
                    // L'emoji des étiquettes existantes n'est pas recopié (structure peu stable).
                    let mut tags: Vec<CreateForumTag> = channel.available_tags.iter().map(|tag| {
                        CreateForumTag::new(tag.name.clone()).moderated(tag.moderated)
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

async fn parsed_recipe(universe_id: ObjectId, name: &str, delay: u64, raw_text: String) -> Result<Recipe, SaveError> {
    let parsed = RecipeParser::parse(&raw_text, universe_id).await.map_err(SaveError::Parse)?;
    Ok(Recipe {
        _id: None,
        universe_id,
        tool_id: None,
        ingredients: parsed.ingredients,
        result: parsed.result,
        tools_needed: parsed.tools_needed,
        delay,
        raw_text,
        recipe_name: name.to_string(),
        wiki_posts_id: None,
    })
}

/// Enregistre une recette par son nom (`/recipe create` et approbation) : la recette du même nom
/// est remplacée, ses posts wiki conservés.
pub async fn save_recipe(ctx: &impl CacheHttp, universe_id: ObjectId, name: &str, delay: u64, raw_text: String, into_wiki: bool) -> Result<(), SaveError> {
    let mut recipe = parsed_recipe(universe_id, name, delay, raw_text).await?;
    recipe.wiki_posts_id = Recipe::get_by_name(universe_id, name).await?.and_then(|r| r.wiki_posts_id);
    sync_recipe_wiki(ctx, &mut recipe, universe_id, into_wiki, false).await?;
    recipe.upsert().await?;
    Ok(())
}

/// Crée (`id = None`) ou modifie une recette par son identifiant (éditeur de l'activité) :
/// renommer est possible, deux recettes ne portent jamais le même nom.
pub async fn save_recipe_by_id(ctx: &impl CacheHttp, universe_id: ObjectId, id: Option<ObjectId>, name: &str, delay: u64, raw_text: String, into_wiki: bool) -> Result<Recipe, SaveError> {
    let existing = match id {
        Some(id) => Some(Recipe::get_by_id(universe_id, id).await?.ok_or_else(|| Error::from("recipe__not_found"))?),
        None => None,
    };
    if let Some(other) = Recipe::get_by_name(universe_id, name).await? {
        if other._id != existing.as_ref().and_then(|r| r._id) {
            return Err(Error::from("recipe__name_taken").into());
        }
    }
    let mut recipe = parsed_recipe(universe_id, name, delay, raw_text).await?;
    let renamed = existing.as_ref().is_some_and(|r| r.recipe_name != name);
    recipe.wiki_posts_id = existing.as_ref().and_then(|r| r.wiki_posts_id.clone());
    sync_recipe_wiki(ctx, &mut recipe, universe_id, into_wiki, renamed).await?;
    match existing.and_then(|r| r._id) {
        Some(id) => {
            recipe._id = Some(id);
            recipe.replace_by_id().await?;
        }
        None => {
            let inserted = recipe.clone().save().await?;
            recipe._id = inserted.inserted_id.as_object_id();
        }
    }
    Ok(recipe)
}

/// Supprime une recette et ses posts wiki. Refusé tant qu'un craft de cette recette est en
/// cours : sa fin relit la recette pour donner les résultats.
pub async fn delete_recipe(ctx: &impl CacheHttp, universe_id: ObjectId, id: ObjectId) -> Result<String, Error> {
    let mut recipe = Recipe::get_by_id(universe_id, id).await?.ok_or("recipe__not_found")?;
    let crafts = crate::database::craft::PlayerCraft::get_active_crafts(universe_id).await?;
    if crafts.iter().any(|c| c.recipe_id == id) {
        return Err("recipe__in_use".into());
    }
    sync_recipe_wiki(ctx, &mut recipe, universe_id, false, false).await?;
    Recipe::delete_by_id(universe_id, id).await?;
    Ok(recipe.recipe_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipe_text_uses_the_slash_command_syntax() {
        let text = recipe_text(&[(2, "Bois".into()), (1, "Potion 2".into())], &[(1, "Chaise".into())], &["Marteau".into()]);
        assert_eq!(text, "< Bois 2\n< Potion 2 1\n> Chaise 1\n- Marteau");
    }

    #[test]
    fn recipe_names_are_bounded_and_single_line() {
        assert!(valid_recipe_name("Pain de seigle"));
        assert!(!valid_recipe_name(""));
        assert!(!valid_recipe_name("  "));
        assert!(!valid_recipe_name(" Pain"), "espaces autour refusés : le nom sert de clé");
        assert!(!valid_recipe_name("Pain\nnoir"));
        assert!(valid_recipe_name(&"é".repeat(MAX_RECIPE_NAME)));
        assert!(!valid_recipe_name(&"é".repeat(MAX_RECIPE_NAME + 1)));
    }

    /// Cycle complet contre MongoDB, sans Discord (univers sans serveur : aucun post wiki) :
    /// création et renommage d'une recette par identifiant, nom unique, recette en cours de craft
    /// non supprimable, puis objet modifié et supprimé par le service des objets.
    #[test]
    #[ignore = "integration test: needs a running MongoDB (MONGODB_* env vars); run with --ignored"]
    fn recipes_and_items_round_trip_through_the_shared_services() {
        use crate::database::items::{get_item_by_id, Item};
        use crate::database::universe::Universe;
        use crate::item::service::{delete_item, update_item, ItemFields, WikiAction};
        use crate::item::ItemUsage;
        use crate::test_support::block_on;

        block_on(async {
            let http = serenity::all::Http::new("dummy");
            let universe_id = ObjectId::new();
            Universe { universe_id, name: "Test admin panel".into(), creator_id: 1, global_time_modifier: 100, time_origin_timestamp: 0, creation_timestamp: 0, fog_enabled: false }
                .insert_universe()
                .await
                .unwrap();
            let item = |name: &str| Item {
                _id: ObjectId::new(),
                universe_id,
                item_name: name.into(),
                item_usage: ItemUsage::None,
                effects: vec![],
                description: None,
                secret_informations: None,
                image: None,
                wiki_post_id: None,
                inventory_id: None,
                inventory_size: 0,
            };
            let (wood, chair) = (item("Bois"), item("Chaise"));
            wood.clone().save().await.unwrap();
            chair.clone().save().await.unwrap();

            let text = recipe_text(&[(2, "Bois".into())], &[(1, "Chaise".into())], &[]);
            let created = save_recipe_by_id(&http, universe_id, None, "Chaise simple", 60, text.clone(), false).await.unwrap();
            let id = created._id.unwrap();
            assert_eq!(created.ingredients, vec![(2, wood._id)]);
            let other = save_recipe_by_id(&http, universe_id, None, "Autre", 0, text.clone(), false).await.unwrap();
            assert!(matches!(
                save_recipe_by_id(&http, universe_id, Some(id), "Autre", 0, text.clone(), false).await,
                Err(SaveError::Other(e)) if e.to_string() == "recipe__name_taken"
            ));
            let renamed = save_recipe_by_id(&http, universe_id, Some(id), "Chaise rustique", 120, text.clone(), false).await.unwrap();
            assert_eq!(renamed._id, Some(id), "même recette, même identifiant");
            let stored = Recipe::get_by_id(universe_id, id).await.unwrap().unwrap();
            assert_eq!((stored.recipe_name.as_str(), stored.delay), ("Chaise rustique", 120));
            assert!(matches!(
                save_recipe_by_id(&http, universe_id, None, "Vide", 0, "# rien".into(), false).await,
                Err(SaveError::Parse(m)) if m == "recipe__empty_recipe"
            ));

            // Une recette en cours de craft ne se supprime pas.
            let craft = crate::craft::logic::new_player_craft(universe_id, 7, 1, id, 60, 1, vec![], 0);
            craft.clone().insert().await.unwrap();
            assert_eq!(delete_recipe(&http, universe_id, id).await.unwrap_err().to_string(), "recipe__in_use");
            craft.remove().await.unwrap();
            assert_eq!(delete_recipe(&http, universe_id, id).await.unwrap(), "Chaise rustique");
            assert!(Recipe::get_by_id(universe_id, id).await.unwrap().is_none());

            // Objet : modifié (nom inchangé), puis supprimé ; la recette qui l'utilise est signalée.
            let fields = ItemFields { usage: ItemUsage::Usable, description: Some("Du chêne".into()), secret: None, image: None, inventory_size: 3, effects_raw: String::new() };
            update_item(&http, universe_id, wood._id, fields, WikiAction::Keep, "fr").await.unwrap();
            let stored = get_item_by_id(wood._id).await.unwrap().unwrap();
            assert_eq!((stored.item_name.as_str(), stored.description.as_deref(), stored.inventory_size), ("Bois", Some("Du chêne"), 3));
            assert!(matches!(stored.item_usage, ItemUsage::Usable));
            let deleted = delete_item(&http, universe_id, "Bois", "fr").await.unwrap();
            assert_eq!(deleted.affected_recipes, vec!["Autre".to_string()]);
            assert!(get_item_by_id(wood._id).await.unwrap().is_none());

            // Nettoyage.
            delete_recipe(&http, universe_id, other._id.unwrap()).await.unwrap();
            crate::database::items::Item::delete(universe_id, "Chaise").await.unwrap();
            let db = crate::database::db_client::get_db_client().await;
            db.database(crate::database::db_namespace::VERSEENGINE_DB_NAME)
                .collection::<mongodb::bson::Document>("universes")
                .delete_one(mongodb::bson::doc! {"_id": universe_id})
                .await
                .ok();
        });
    }
}
