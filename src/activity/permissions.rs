//! Calcul du niveau d'accès d'un membre à partir de données **lues par le bot**
//! (jamais fournies par le client).

use serde::Serialize;

/// Bit ADMINISTRATOR des permissions Discord.
pub const ADMINISTRATOR: u64 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    Player,
    Moderator,
    Admin,
}

impl AccessLevel {
    /// Accès au mode administrateur de l'activité (❓Q3 : modérateurs inclus pour l'instant,
    /// les actions destructives exigeront `Admin`).
    pub fn can_use_admin_mode(self) -> bool { self >= AccessLevel::Moderator }
    #[allow(dead_code)] // routes admin (phase d)
    pub fn can_edit_world(self) -> bool { self == AccessLevel::Admin }
}

/// Entrées du calcul, toutes issues de l'API Discord via le token du bot et de `Server`.
pub struct AccessInput<'a> {
    pub user_id: u64,
    pub guild_owner_id: u64,
    /// Rôles du membre.
    pub member_roles: &'a [u64],
    /// Rôles de la guilde avec leurs permissions (`@everyone` a l'id de la guilde).
    pub guild_roles: &'a [(u64, u64)],
    pub guild_id: u64,
    pub admin_role_id: Option<u64>,
    pub moderator_role_id: Option<u64>,
}

pub fn compute_access(input: &AccessInput) -> AccessLevel {
    if input.user_id == input.guild_owner_id {
        return AccessLevel::Admin;
    }
    let has_role = |r: Option<u64>| r.is_some_and(|r| input.member_roles.contains(&r));
    let has_admin_permission = input
        .guild_roles
        .iter()
        .filter(|(id, _)| *id == input.guild_id || input.member_roles.contains(id))
        .any(|(_, perms)| perms & ADMINISTRATOR != 0);
    if has_admin_permission || has_role(input.admin_role_id) {
        AccessLevel::Admin
    } else if has_role(input.moderator_role_id) {
        AccessLevel::Moderator
    } else {
        AccessLevel::Player
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(user: u64, roles: &'a [u64], guild_roles: &'a [(u64, u64)]) -> AccessInput<'a> {
        AccessInput {
            user_id: user,
            guild_owner_id: 1,
            member_roles: roles,
            guild_roles,
            guild_id: 1000,
            admin_role_id: Some(10),
            moderator_role_id: Some(20),
        }
    }

    #[test]
    fn owner_is_admin() {
        assert_eq!(compute_access(&input(1, &[], &[])), AccessLevel::Admin);
    }

    #[test]
    fn roles_map_to_levels() {
        assert_eq!(compute_access(&input(2, &[10], &[])), AccessLevel::Admin);
        assert_eq!(compute_access(&input(2, &[20], &[])), AccessLevel::Moderator);
        assert_eq!(compute_access(&input(2, &[30], &[])), AccessLevel::Player);
        assert_eq!(compute_access(&input(2, &[], &[])), AccessLevel::Player);
    }

    #[test]
    fn administrator_permission_counts_only_for_held_roles() {
        let guild_roles = [(1000, 0), (30, ADMINISTRATOR), (40, 0)];
        assert_eq!(compute_access(&input(2, &[30], &guild_roles)), AccessLevel::Admin);
        assert_eq!(compute_access(&input(2, &[40], &guild_roles)), AccessLevel::Player);
        let everyone_admin = [(1000, ADMINISTRATOR)];
        assert_eq!(compute_access(&input(2, &[], &everyone_admin)), AccessLevel::Admin);
    }

    #[test]
    fn unset_roles_grant_nothing() {
        let i = AccessInput { admin_role_id: None, moderator_role_id: None, ..input(2, &[10, 20], &[]) };
        assert_eq!(compute_access(&i), AccessLevel::Player);
    }

    #[test]
    fn admin_mode_rules() {
        assert!(!AccessLevel::Player.can_use_admin_mode());
        assert!(AccessLevel::Moderator.can_use_admin_mode());
        assert!(!AccessLevel::Moderator.can_edit_world());
        assert!(AccessLevel::Admin.can_edit_world());
    }
}
