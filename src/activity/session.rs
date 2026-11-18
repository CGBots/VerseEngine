//! Sessions de l'activité : jeton opaque aléatoire (256 bits, CSPRNG) remis au client ;
//! seule son empreinte SHA-256 est conservée en mémoire.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use mongodb::bson::oid::ObjectId;
use rand::Rng;
use sha2::{Digest, Sha256};

use crate::activity::permissions::AccessLevel;

const MAX_SESSIONS: usize = 50_000;

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: u64,
    pub username: String,
    pub guild_id: u64,
    pub universe_id: ObjectId,
    pub instance_id: String,
    /// Langue du client Discord du joueur (D12), normalisée : `fr` ou `en-US`.
    pub locale: &'static str,
    /// Niveau calculé à l'authentification ; les routes admin le revérifient.
    pub access: AccessLevel,
    pub expires_at: Instant,
}

#[derive(Debug)]
pub struct SessionStore {
    ttl: Duration,
    sessions: RwLock<HashMap<[u8; 32], Session>>,
}

fn digest(token: &str) -> [u8; 32] { Sha256::digest(token.as_bytes()).into() }

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes.iter().flat_map(|b| [HEX[(b >> 4) as usize] as char, HEX[(b & 0xf) as usize] as char]).collect()
}

impl SessionStore {
    pub fn new(ttl: Duration) -> Self { Self { ttl, sessions: RwLock::new(HashMap::new()) } }

    pub fn ttl(&self) -> Duration { self.ttl }

    /// Crée une session et renvoie le jeton en clair (à transmettre une seule fois au client).
    pub fn create(&self, mut session: Session) -> String {
        let mut raw = [0u8; 32];
        rand::rng().fill_bytes(&mut raw);
        let token = to_hex(&raw);
        let now = Instant::now();
        session.expires_at = now + self.ttl;
        let mut map = self.sessions.write().unwrap_or_else(|p| p.into_inner());
        if map.len() >= MAX_SESSIONS {
            map.retain(|_, s| s.expires_at > now);
        }
        // Une seule session active par (utilisateur, instance) : l'ancienne est révoquée.
        map.retain(|_, s| !(s.user_id == session.user_id && s.instance_id == session.instance_id));
        map.insert(digest(&token), session);
        token
    }

    pub fn get(&self, token: &str) -> Option<Session> { self.get_at(token, Instant::now()) }

    pub fn get_at(&self, token: &str, now: Instant) -> Option<Session> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let key = digest(token);
        let session = self.sessions.read().unwrap_or_else(|p| p.into_inner()).get(&key).cloned()?;
        if session.expires_at <= now {
            self.sessions.write().unwrap_or_else(|p| p.into_inner()).remove(&key);
            return None;
        }
        Some(session)
    }

    #[allow(dead_code)] // déconnexion explicite (phase b)
    pub fn revoke(&self, token: &str) {
        self.sessions.write().unwrap_or_else(|p| p.into_inner()).remove(&digest(token));
    }
}

#[cfg(test)]
pub(crate) fn test_session(user_id: u64) -> Session {
    Session {
        user_id,
        username: "test".into(),
        guild_id: 1,
        universe_id: ObjectId::new(),
        instance_id: "i-1".into(),
        locale: "fr",
        access: AccessLevel::Player,
        expires_at: Instant::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_get() {
        let store = SessionStore::new(Duration::from_secs(60));
        let token = store.create(test_session(42));
        assert_eq!(token.len(), 64);
        assert_eq!(store.get(&token).unwrap().user_id, 42);
        assert!(store.get("deadbeef").is_none());
        assert!(store.get(&"0".repeat(64)).is_none());
    }

    #[test]
    fn tokens_are_unique_and_not_stored_in_clear() {
        let store = SessionStore::new(Duration::from_secs(60));
        let a = store.create(test_session(1));
        let b = store.create(Session { instance_id: "i-2".into(), ..test_session(1) });
        assert_ne!(a, b);
        let map = store.sessions.read().unwrap();
        assert!(map.keys().all(|k| to_hex(k) != a && to_hex(k) != b));
    }

    #[test]
    fn expiry_and_revocation() {
        let store = SessionStore::new(Duration::from_secs(60));
        let token = store.create(test_session(1));
        assert!(store.get_at(&token, Instant::now() + Duration::from_secs(61)).is_none());
        let token = store.create(test_session(2));
        store.revoke(&token);
        assert!(store.get(&token).is_none());
    }

    #[test]
    fn new_session_replaces_old_for_same_instance() {
        let store = SessionStore::new(Duration::from_secs(60));
        let old = store.create(test_session(7));
        let new = store.create(test_session(7));
        assert!(store.get(&old).is_none());
        assert!(store.get(&new).is_some());
    }
}
