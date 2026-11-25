//! Enregistrement des commandes globales du bot sans effacer la commande « Entry Point » de
//! l'activité.
//!
//! `poise::builtins::register_globally` fait un `PUT /applications/{id}/commands`, qui remplace
//! **toutes** les commandes globales. Quand les activités sont activées dans le portail, Discord
//! crée une commande de type `PRIMARY_ENTRY_POINT` (« Launch ») : sans précaution, chaque
//! démarrage du bot la supprimerait (ou ferait échouer l'enregistrement). On relit donc les
//! commandes existantes et on renvoie la commande Entry Point telle quelle avec les nôtres.

use serenity::all::{Command, CommandType, CreateCommand, EntryPointHandlerType, Http};

/// Recrée à l'identique les commandes Entry Point existantes.
pub fn preserved_entry_points(existing: &[Command]) -> Vec<CreateCommand> {
    existing
        .iter()
        .filter(|c| c.kind == CommandType::PrimaryEntryPoint)
        .map(|c| {
            let mut builder = CreateCommand::new(c.name.clone())
                .kind(CommandType::PrimaryEntryPoint)
                .description(c.description.clone())
                .handler(c.handler.unwrap_or(EntryPointHandlerType::DiscordLaunchActivity))
                .nsfw(c.nsfw);
            for (locale, name) in c.name_localizations.iter().flatten() {
                builder = builder.name_localized(locale.clone(), name.clone());
            }
            for (locale, description) in c.description_localizations.iter().flatten() {
                builder = builder.description_localized(locale.clone(), description.clone());
            }
            if !c.integration_types.is_empty() {
                builder = builder.integration_types(c.integration_types.clone());
            }
            if let Some(contexts) = &c.contexts {
                builder = builder.contexts(contexts.clone());
            }
            if let Some(permissions) = c.default_member_permissions {
                builder = builder.default_member_permissions(permissions);
            }
            builder
        })
        .collect()
}

/// Remplace les commandes globales par `commands` en conservant l'Entry Point de l'activité.
pub async fn register_globally<U, E>(http: &Http, commands: &[poise::Command<U, E>]) -> Result<(), serenity::Error> {
    let mut builders = poise::builtins::create_application_commands(commands);
    let existing = Command::get_global_commands(http).await?;
    builders.extend(preserved_entry_points(&existing));
    Command::set_global_commands(http, builders).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(json: serde_json::Value) -> Command {
        serde_json::from_value(json).unwrap()
    }

    fn base(id: &str, kind: u8, name: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "application_id": "1", "version": "1", "type": kind, "name": name,
            "description": "Lancer l'activité", "default_member_permissions": null, "nsfw": false,
            "integration_types": [0, 1], "contexts": [0, 1, 2], "handler": 2,
            "name_localizations": {"en-US": "Launch"}, "description_localizations": null,
        })
    }

    #[test]
    fn only_the_entry_point_command_is_preserved() {
        let existing = [command(base("10", 1, "travel")), command(base("11", 4, "lancer"))];
        let kept = preserved_entry_points(&existing);
        assert_eq!(kept.len(), 1);
        let json = serde_json::to_value(&kept[0]).unwrap();
        assert_eq!(json["type"], 4);
        assert_eq!(json["name"], "lancer");
        assert_eq!(json["handler"], 2);
        assert_eq!(json["integration_types"], serde_json::json!([0, 1]));
        assert_eq!(json["contexts"], serde_json::json!([0, 1, 2]));
        assert_eq!(json["name_localizations"]["en-US"], "Launch");
    }

    #[test]
    fn nothing_is_added_without_an_entry_point() {
        assert!(preserved_entry_points(&[command(base("10", 1, "travel"))]).is_empty());
        assert!(preserved_entry_points(&[]).is_empty());
    }

    /// Lecture seule contre Discord : les commandes globales de l'application de test se lisent et
    /// les commandes slash ne sont jamais prises pour une Entry Point.
    #[test]
    #[ignore = "integration test: needs DISCORD_TOKEN; run with --ignored"]
    fn global_commands_are_read_from_discord() {
        crate::test_support::block_on(async {
            let http = crate::test_support::discord_http();
            let app = http.get_current_application_info().await.expect("application info");
            http.set_application_id(app.id);
            let existing = Command::get_global_commands(&http).await.expect("global commands");
            let entry_points = existing.iter().filter(|c| c.kind == CommandType::PrimaryEntryPoint).count();
            assert_eq!(preserved_entry_points(&existing).len(), entry_points);
            assert!(existing.iter().any(|c| c.kind == CommandType::ChatInput));
        });
    }
}
