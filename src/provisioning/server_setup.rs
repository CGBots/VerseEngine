//! Installation d'un serveur Discord pour un univers (rôles de base, catégorie des routes et,
//! en installation complète, catégories et salons d'administration et de jeu). Partagée par
//! `/universe_setup`, `/universe_create_universe`, `/universe_add_server` et l'import de carte.
//!
//! Ce qui existe déjà (identifiant enregistré et objet encore présent sur Discord) est réutilisé,
//! jamais recréé (I10, quota de rôles Q8). Tout ce qui est créé passe par le [`Provisioner`] :
//! en cas d'échec, tout est défait, et le document `servers` n'est écrit qu'à la fin, donc
//! inchangé.

use std::collections::HashSet;

use serenity::all::{ChannelId, ChannelType, CreateChannel, CreateForumTag, CreateInvite, EditChannel, GuildId, PermissionOverwrite, RoleId};

use crate::database::server::{Id, IdType, Server};
use crate::discord::channels::{get_admin_category_permission_set, get_road_category_permission_set, get_rp_character_permission_set, get_universal_time_permission_set, ITEM_TAG, PLACE_TAG, SPACE_TAG};
use crate::discord::lib_tuning::guildid::RolePositions;
use crate::discord::roles::{AdminRolePermissions, ModeratorRolePermissions, PlayerRolePermissions, SpectatorRolePermissions};
use crate::provisioning::{ProvisionError, Provisioner};
use crate::translation::get_by_locale;
use crate::universe::setup::setup_sub_command::SetupType;

/// Nombre de salons et de rôles que consomme une installation (budget d'un serveur, § 7.4).
pub fn setup_budget(kind: SetupType) -> (u32, u32) {
    match kind {
        // (salons, rôles)
        SetupType::PartialSetup => (1, 4),
        SetupType::FullSetup => (11, 4),
    }
}

/// Positions des rôles de base (du plus haut au plus bas) juste sous `bot_top`, la position du
/// rôle le plus haut du bot. `None` si le bot est trop bas pour les ordonner (ils restent alors
/// dans l'ordre de création, qui est déjà le bon).
pub fn base_role_positions(bot_top: u64, ordered: [RoleId; 4]) -> Option<Vec<(RoleId, Option<u64>)>> {
    (bot_top > ordered.len() as u64).then(|| {
        ordered.iter().enumerate().map(|(i, r)| (*r, Some(bot_top - 1 - i as u64))).collect()
    })
}

/// Identifiant enregistré, s'il désigne encore un objet présent.
fn still_there(id: Option<Id>, present: &HashSet<u64>) -> Option<u64> {
    id.map(|i| i.id).filter(|i| *i != 0 && present.contains(i))
}

fn channel(name: String, kind: ChannelType, position: u16, permissions: Vec<PermissionOverwrite>, parent: Option<u64>) -> CreateChannel<'static> {
    let mut c = CreateChannel::new(name).kind(kind).position(position).permissions(permissions);
    if let (Some(parent), true) = (parent, kind != ChannelType::Category) {
        c = c.category(parent);
    }
    c
}

/// Installe `server` (déjà lié à son univers en base) selon `kind`, avec les noms dans la langue
/// `locale`. Met à jour `server` et son document à la fin.
pub async fn setup_server(p: &mut Provisioner, server: &mut Server, kind: SetupType, locale: &str) -> Result<(), ProvisionError> {
    let guild = server.server_id;
    let http = p.http().clone();
    let name = |key: &str| get_by_locale(locale, key, None, None);

    let Ok(roles) = http.get_guild_roles(GuildId::new(guild)).await else {
        return Err(p.fail("partial_setup__get_guild_roles_error").await);
    };
    let Ok(channels) = http.get_channels(GuildId::new(guild)).await else {
        return Err(p.fail("setup__channel_setup_failed").await);
    };
    let role_ids: HashSet<u64> = roles.iter().map(|r| r.id.get()).collect();
    let channel_ids: HashSet<u64> = channels.iter().map(|c| c.id.get()).collect();

    // Rôles de base.
    let wanted = [
        (server.admin_role_id, "admin_role_name", *AdminRolePermissions, "setup__admin_role_not_created"),
        (server.moderator_role_id, "moderator_role_name", *ModeratorRolePermissions, "setup__moderator_role_not_created"),
        (server.spectator_role_id, "spectator_role_name", *SpectatorRolePermissions, "setup__spectator_role_not_created"),
        (server.player_role_id, "player_role_name", *PlayerRolePermissions, "setup__player_role_not_created"),
    ];
    let mut base_roles = Vec::with_capacity(4);
    let mut created_roles = false;
    for (existing, key, permissions, error) in wanted {
        match still_there(existing, &role_ids) {
            Some(id) => base_roles.push(RoleId::new(id)),
            None => match p.create_role(guild, &name(key), Some(permissions)).await {
                Ok(r) => {
                    created_roles = true;
                    base_roles.push(r.id);
                }
                Err(e) => {
                    eprintln!("[setup] guild {guild}: {key}: {e:?}");
                    return Err(p.fail(error).await);
                }
            },
        }
    }
    let [admin, moderator, spectator, player] = [base_roles[0], base_roles[1], base_roles[2], base_roles[3]];
    let everyone = RoleId::new(guild);

    // Ordre des rôles : admin, modérateur, spectateur, joueur, juste sous le rôle le plus haut du
    // bot. Discord interdit de déplacer le rôle du bot ou un rôle au-dessus de lui (50013) : seuls
    // ces quatre rôles sont déplacés. Seulement si l'un d'eux vient d'être créé.
    if created_roles {
        let bot_roles = match http.get_current_user().await {
            Ok(me) => http.get_member(GuildId::new(guild), me.id).await.ok().map(|m| m.roles),
            Err(_) => None,
        };
        let Some(bot_roles) = bot_roles else { return Err(p.fail("setup__reorder_went_wrong").await) };
        let bot_top = roles.iter().filter(|r| bot_roles.contains(&r.id)).map(|r| r.position as u64).max().unwrap_or(0);
        if let Some(positions) = base_role_positions(bot_top, [admin, moderator, spectator, player]) {
            if let Err(e) = GuildId::new(guild).reorder_roles(http.as_ref(), positions).await {
                eprintln!("[setup] guild {guild}: reorder roles: {e:?}");
                return Err(p.fail("setup__reorder_went_wrong").await);
            }
        }
    }

    // Catégorie des routes.
    let road_category = match still_there(server.road_category_id, &channel_ids) {
        Some(id) => id,
        None => {
            let perms = get_road_category_permission_set(everyone, player, spectator, moderator);
            match p.create_channel(guild, channel(name("road_channel_name"), ChannelType::Category, 0, perms, None)).await {
                Ok(c) => c.id.get(),
                Err(e) => {
                    eprintln!("[setup] guild {guild}: road category: {e:?}");
                    return Err(p.fail("setup__road_category_not_created").await);
                }
            }
        }
    };

    let mut updated = server.clone();
    updated
        .admin_role_id(Id { id: admin.get(), id_type: IdType::Role })
        .moderator_role_id(Id { id: moderator.get(), id_type: IdType::Role })
        .spectator_role_id(Id { id: spectator.get(), id_type: IdType::Role })
        .player_role_id(Id { id: player.get(), id_type: IdType::Role })
        .everyone_role_id(Id { id: everyone.get(), id_type: IdType::Role })
        .road_category_id(Id { id: road_category, id_type: IdType::Category });

    if let SetupType::FullSetup = kind {
        complementary(p, &mut updated, &channel_ids, locale, (everyone, spectator, player, moderator), road_category).await?;
    }

    if updated.update().await.is_err() {
        return Err(p.fail("setup__server_update_failed").await);
    }
    *server = updated;

    // Invitation permanente du serveur (pour les voyages entre serveurs). Facultative.
    if server.universal_invite_url.is_none() {
        if let Ok(mut chans) = http.get_channels(GuildId::new(guild)).await {
            chans.sort_by_key(|c| c.position);
            if let Some(target) = chans.iter().find(|c| c.kind == ChannelType::Text) {
                if let Ok(invite) = target.id.create_invite(http.as_ref(), CreateInvite::new().max_age(0).max_uses(0).unique(true)).await {
                    server.universal_invite_url = Some(invite.url());
                    let _ = server.update().await;
                }
            }
        }
    }
    Ok(())
}

/// Installation complète : catégories et salons d'administration et de jeu.
async fn complementary(
    p: &mut Provisioner,
    server: &mut Server,
    present: &HashSet<u64>,
    locale: &str,
    (everyone, spectator, player, moderator): (RoleId, RoleId, RoleId, RoleId),
    road_category: u64,
) -> Result<(), ProvisionError> {
    let guild = server.server_id;
    let name = |key: &str| get_by_locale(locale, key, None, None);

    macro_rules! resolve {
        ($field:expr, $create:expr, $error:literal) => {
            match still_there($field, present) {
                Some(id) => id,
                None => match p.create_channel(guild, $create).await {
                    Ok(c) => c.id.get(),
                    Err(e) => {
                        eprintln!("[setup] guild {guild}: {}: {e:?}", $error);
                        return Err(p.fail($error).await);
                    }
                },
            }
        };
    }

    let admin_category = resolve!(server.admin_category_id, channel(name("admin_category_name"), ChannelType::Category, 0, get_admin_category_permission_set(everyone, spectator, player, moderator), None), "setup__admin_category_not_created");
    let nrp_category = resolve!(server.nrp_category_id, channel(name("nrp_category_name"), ChannelType::Category, 1, vec![], None), "setup__nrp_category_not_created");
    let rp_category = resolve!(server.rp_category_id, channel(name("rp_category_name"), ChannelType::Category, 1, vec![], None), "setup__rp_category_not_created");
    let log = resolve!(server.log_channel_id, channel(name("log_channel_name"), ChannelType::Text, 0, vec![], Some(admin_category)), "setup__log_channel_not_created");
    let commands = resolve!(server.commands_channel_id, channel(name("commands_channel_name"), ChannelType::Text, 0, vec![], Some(admin_category)), "setup__commands_channel_not_created");
    let moderation = resolve!(server.moderation_channel_id, channel(name("moderation_channel_name"), ChannelType::Text, 0, vec![], Some(admin_category)), "setup__moderation_channel_not_created");
    let nrp_general = resolve!(server.nrp_general_channel_id, channel(name("nrp_general_channel_name"), ChannelType::Text, 0, vec![], Some(nrp_category)), "setup__nrp_general_channel_not_created");
    let character = resolve!(server.rp_character_channel_id, channel(name("rp_character_channel_name"), ChannelType::Text, 0, get_rp_character_permission_set(player), Some(rp_category)), "setup__rp_character_channel_not_created");
    let wiki_existed = still_there(server.rp_wiki_channel_id, present).is_some();
    let wiki = resolve!(server.rp_wiki_channel_id, channel(name("rp_wiki_channel_name"), ChannelType::Forum, 0, vec![], Some(rp_category)), "setup__wiki_channel_not_created");
    if !wiki_existed {
        let tags = vec![CreateForumTag::new(PLACE_TAG), CreateForumTag::new(SPACE_TAG), CreateForumTag::new(ITEM_TAG)];
        if let Err(e) = ChannelId::new(wiki).edit(p.http().as_ref(), EditChannel::new().available_tags(tags)).await {
            eprintln!("[setup] guild {guild}: wiki tags: {e:?}");
        }
    }
    let time = resolve!(server.universal_time_channel_id, channel(name("universal_time_channel_name"), ChannelType::Text, 0, get_universal_time_permission_set(everyone), Some(rp_category)), "setup__universal_time_channel_not_created");

    server
        .admin_category_id(Id { id: admin_category, id_type: IdType::Category })
        .nrp_category_id(Id { id: nrp_category, id_type: IdType::Category })
        .rp_category_id(Id { id: rp_category, id_type: IdType::Category })
        .log_channel_id(Id { id: log, id_type: IdType::Channel })
        .commands_channel_id(Id { id: commands, id_type: IdType::Channel })
        .moderation_channel_id(Id { id: moderation, id_type: IdType::Channel })
        .nrp_general_channel_id(Id { id: nrp_general, id_type: IdType::Channel })
        .rp_character_channel_id(Id { id: character, id_type: IdType::Channel })
        .rp_wiki_channel_id(Id { id: wiki, id_type: IdType::Channel })
        .universal_time_channel_id(Id { id: time, id_type: IdType::Channel });

    // Ordre des catégories (cosmétique, un échec ne fait pas échouer l'installation).
    let order: Vec<(ChannelId, u64)> = [admin_category, nrp_category, rp_category, road_category]
        .into_iter()
        .enumerate()
        .map(|(i, id)| (ChannelId::new(id), i as u64))
        .collect();
    if let Err(e) = GuildId::new(guild).reorder_channels(p.http().as_ref(), order).await {
        eprintln!("[setup] guild {guild}: reorder channels: {e:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ids_still_present_are_reused() {
        let present: HashSet<u64> = [5, 6].into_iter().collect();
        assert_eq!(still_there(Some(Id { id: 5, id_type: IdType::Role }), &present), Some(5));
        assert_eq!(still_there(Some(Id { id: 9, id_type: IdType::Role }), &present), None);
        assert_eq!(still_there(Some(Id { id: 0, id_type: IdType::Role }), &present), None);
        assert_eq!(still_there(None, &present), None);
    }

    #[test]
    fn a_category_never_gets_a_parent() {
        let c = channel("x".into(), ChannelType::Category, 0, vec![], Some(3));
        let json = serde_json::to_value(&c).unwrap();
        assert!(json.get("parent_id").is_none());
        let t = channel("y".into(), ChannelType::Text, 0, vec![], Some(3));
        assert_eq!(serde_json::to_value(&t).unwrap()["parent_id"], "3");
    }

    #[test]
    fn base_roles_go_right_under_the_bot_in_order() {
        let r = [RoleId::new(1), RoleId::new(2), RoleId::new(3), RoleId::new(4)];
        let p = base_role_positions(10, r).unwrap();
        assert_eq!(p, vec![(r[0], Some(9)), (r[1], Some(8)), (r[2], Some(7)), (r[3], Some(6))]);
        assert_eq!(base_role_positions(5, r).unwrap().last(), Some(&(r[3], Some(1))));
        assert!(base_role_positions(4, r).is_none(), "the bot is too low");
    }

    #[test]
    fn the_setup_budget_is_counted() {
        assert_eq!(setup_budget(SetupType::PartialSetup), (1, 4));
        assert_eq!(setup_budget(SetupType::FullSetup), (11, 4));
    }
}
