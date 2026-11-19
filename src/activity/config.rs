//! Configuration de l'API de l'activité, lue **uniquement** depuis l'environnement.

use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Secret dont l'affichage `Debug`/`Display` est masqué.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
    pub fn expose(&self) -> &str { &self.0 }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("Secret(***)") }
}

#[derive(Debug, Clone)]
pub struct ActivityConfig {
    pub bind: SocketAddr,
    pub client_id: u64,
    pub client_secret: Secret,
    pub bot_token: Secret,
    pub static_dir: Option<PathBuf>,
    pub session_ttl: Duration,
    /// Base de l'API Discord (surchargée en test).
    pub discord_api_base: String,
}

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Disabled,
    Missing(&'static str),
    Invalid(&'static str),
}

impl ActivityConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let enabled = get("ACTIVITY_ENABLED").map(|v| matches!(v.trim(), "1" | "true" | "TRUE" | "yes")).unwrap_or(false);
        if !enabled {
            return Err(ConfigError::Disabled);
        }
        let need = |k: &'static str| get(k).filter(|v| !v.trim().is_empty()).ok_or(ConfigError::Missing(k));
        let client_id = need("DISCORD_CLIENT_ID")?.trim().parse().map_err(|_| ConfigError::Invalid("DISCORD_CLIENT_ID"))?;
        let client_secret = Secret::new(need("DISCORD_CLIENT_SECRET")?);
        let bot_token = Secret::new(need("DISCORD_TOKEN")?);
        let bind = get("ACTIVITY_BIND")
            .unwrap_or_else(|| "127.0.0.1:8787".into())
            .parse()
            .map_err(|_| ConfigError::Invalid("ACTIVITY_BIND"))?;
        let ttl = match get("ACTIVITY_SESSION_TTL_SECS") {
            Some(v) => v.trim().parse::<u64>().ok().filter(|s| (60..=86_400).contains(s)).ok_or(ConfigError::Invalid("ACTIVITY_SESSION_TTL_SECS"))?,
            None => 3600,
        };
        let static_dir = Some(PathBuf::from(get("ACTIVITY_STATIC_DIR").unwrap_or_else(|| "web/activity/dist".into())));
        Ok(Self {
            bind,
            client_id,
            client_secret,
            bot_token,
            static_dir,
            session_ttl: Duration::from_secs(ttl),
            discord_api_base: "https://discord.com/api/v10".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<ActivityConfig, ConfigError> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ActivityConfig::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn disabled_by_default() {
        assert_eq!(cfg(&[]).unwrap_err(), ConfigError::Disabled);
    }

    #[test]
    fn requires_secrets() {
        assert_eq!(cfg(&[("ACTIVITY_ENABLED", "true")]).unwrap_err(), ConfigError::Missing("DISCORD_CLIENT_ID"));
        assert_eq!(
            cfg(&[("ACTIVITY_ENABLED", "true"), ("DISCORD_CLIENT_ID", "abc")]).unwrap_err(),
            ConfigError::Invalid("DISCORD_CLIENT_ID")
        );
    }

    #[test]
    fn defaults_and_secret_redaction() {
        let c = cfg(&[
            ("ACTIVITY_ENABLED", "true"),
            ("DISCORD_CLIENT_ID", "123"),
            ("DISCORD_CLIENT_SECRET", "s3cr3t"),
            ("DISCORD_TOKEN", "bot-token"),
        ])
        .unwrap();
        assert_eq!(c.bind.to_string(), "127.0.0.1:8787");
        assert_eq!(c.session_ttl, Duration::from_secs(3600));
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("s3cr3t") && !dbg.contains("bot-token"));
    }

    #[test]
    fn rejects_out_of_range_ttl() {
        let r = cfg(&[
            ("ACTIVITY_ENABLED", "1"),
            ("DISCORD_CLIENT_ID", "1"),
            ("DISCORD_CLIENT_SECRET", "s"),
            ("DISCORD_TOKEN", "t"),
            ("ACTIVITY_SESSION_TTL_SECS", "5"),
        ]);
        assert_eq!(r.unwrap_err(), ConfigError::Invalid("ACTIVITY_SESSION_TTL_SECS"));
    }
}
