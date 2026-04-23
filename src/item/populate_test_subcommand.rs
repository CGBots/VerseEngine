use crate::database::items::Item;
use crate::database::inventory::{Inventory, HolderType};
use crate::database::server::get_server_by_id;
use crate::database::characters::get_character_by_user_id;
use crate::discord::poise_structs::{Context, Error};
use crate::item::ItemUsage;
use mongodb::bson::oid::ObjectId;

#[poise::command(slash_command, guild_only, required_permissions = "ADMINISTRATOR", rename = "populate_test")]
pub async fn populate_test(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().unwrap().get();
    let user_id = ctx.author().id.get();

    let Some(server) = get_server_by_id(guild_id).await? else {
        return Err("Server not found".into());
    };

    let Some(character) = get_character_by_user_id(server.universe_id, user_id).await? else {
        return Err("Character not found. Please create a character first.".into());
    };

    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for i in 1..=50 {
        let item_name = format!("Test Item {} ({})", i, timestamp);
        
        let item = Item {
            _id: ObjectId::new(),
            universe_id: server.universe_id,
            item_name: item_name.clone(),
            item_usage: ItemUsage::None,
            effects: vec![],
            description: Some(format!("Ceci est l'item de test numéro {}", i)),
            secret_informations: None,
            image: None,
            wiki_post_id: None,
            inventory_id: None,
            inventory_size: 0,
        };

        let item_id = item._id;
        item.save().await?;

        // Ajouter l'item à l'inventaire du personnage
        let _ = Inventory::add_item_to_inventory(
            server.universe_id,
            character._id,
            HolderType::Character,
            item_id,
            1,
        ).await;
    }

    ctx.say("Base de données peuplée avec 50 items de test pour votre personnage !").await?;

    Ok(())
}
