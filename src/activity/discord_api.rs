//! Appels à l'API Discord nécessaires à l'authentification de l'activité.
//! Le client secret et le token bot ne sortent jamais de ce module.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use serenity::all::{GuildId, Http, UserId};

use crate::activity::config::{ActivityConfig, Secret};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug)]
pub enum DiscordError {
    /// Code OAuth refusé / instance inconnue : erreur imputable au client.
    Rejected,
    /// Discord indisponible ou réponse inattendue.
    Upstream(String),
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
pub struct DiscordUser {
    #[serde_as(as = "DisplayFromStr")]
    pub id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct InstanceLocation {
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub guild_id: Option<u64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[serde(default)]
    pub channel_id: Option<u64>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ActivityInstance {
    pub instance_id: String,
    pub location: Option<InstanceLocation>,
    #[serde_as(as = "Vec<DisplayFromStr>")]
    #[serde(default)]
    pub users: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub roles: Vec<u64>,
    pub guild_owner_id: u64,
    /// (id du rôle, permissions) pour tous les rôles de la guilde.
    pub guild_roles: Vec<(u64, u64)>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TokenGrant {
    pub access_token: String,
    pub expires_in: u64,
    pub scope: String,
}

/// Serveur où le bot est présent.
#[derive(Debug, Clone, PartialEq)]
pub struct GuildSummary {
    pub id: u64,
    pub name: String,
}

/// Nombre de salons et de rôles d'un serveur (budget de l'import, limites de Discord).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildCounts {
    pub channels: u32,
    pub roles: u32,
}

/// Abstraction pour pouvoir tester les routes sans appeler Discord.
pub trait DiscordGateway: Send + Sync {
    fn exchange_code<'a>(&'a self, code: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>>;
    fn current_user<'a>(&'a self, access_token: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>>;
    fn activity_instance<'a>(&'a self, instance_id: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>>;
    /// `Ok(None)` si l'utilisateur n'est pas membre de la guilde.
    fn member<'a>(&'a self, guild_id: u64, user_id: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>>;
    /// Serveurs du bot (au plus `limit`).
    fn bot_guilds<'a>(&'a self, limit: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>>;
    /// Salons et rôles actuels d'un serveur.
    fn guild_counts<'a>(&'a self, guild_id: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>>;
}

pub struct LiveDiscord {
    client: reqwest::Client,
    base: String,
    client_id: u64,
    client_secret: Secret,
    bot_token: Secret,
    bot_http: Arc<Http>,
}

impl LiveDiscord {
    pub fn new(cfg: &ActivityConfig, bot_http: Arc<Http>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("VerseEngine-Activity (https://github.com/CGBots/VerseEngine)")
            .build()
            .expect("client HTTP");
        Self {
            client,
            base: cfg.discord_api_base.clone(),
            client_id: cfg.client_id,
            client_secret: cfg.client_secret.clone(),
            bot_token: cfg.bot_token.clone(),
            bot_http,
        }
    }
}

fn upstream(e: impl std::fmt::Display) -> DiscordError { DiscordError::Upstream(e.to_string()) }

#[derive(Deserialize)]
struct RawGrant {
    access_token: String,
    #[serde(default)]
    expires_in: u64,
    #[serde(default)]
    scope: String,
}

impl DiscordGateway for LiveDiscord {
    fn exchange_code<'a>(&'a self, code: &'a str) -> BoxFuture<'a, Result<TokenGrant, DiscordError>> {
        Box::pin(async move {
            let body = format!(
                "client_id={}&client_secret={}&grant_type=authorization_code&code={}",
                self.client_id,
                urlencoding::encode(self.client_secret.expose()),
                urlencoding::encode(code)
            );
            let res = self
                .client
                .post(format!("{}/oauth2/token", self.base))
                .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await
                .map_err(upstream)?;
            match res.status() {
                s if s.is_success() => {
                    let g: RawGrant = res.json().await.map_err(upstream)?;
                    Ok(TokenGrant { access_token: g.access_token, expires_in: g.expires_in, scope: g.scope })
                }
                s if s.is_client_error() && s.as_u16() != 429 => Err(DiscordError::Rejected),
                s => Err(DiscordError::Upstream(format!("oauth2/token: {s}"))),
            }
        })
    }

    fn current_user<'a>(&'a self, access_token: &'a str) -> BoxFuture<'a, Result<DiscordUser, DiscordError>> {
        Box::pin(async move {
            let res = self.client.get(format!("{}/users/@me", self.base)).bearer_auth(access_token).send().await.map_err(upstream)?;
            if res.status().as_u16() == 401 { return Err(DiscordError::Rejected); }
            if !res.status().is_success() { return Err(DiscordError::Upstream(format!("users/@me: {}", res.status()))); }
            res.json().await.map_err(upstream)
        })
    }

    fn activity_instance<'a>(&'a self, instance_id: &'a str) -> BoxFuture<'a, Result<ActivityInstance, DiscordError>> {
        Box::pin(async move {
            let url = format!(
                "{}/applications/{}/activity-instances/{}",
                self.base,
                self.client_id,
                urlencoding::encode(instance_id)
            );
            let res = self
                .client
                .get(url)
                .header(reqwest::header::AUTHORIZATION, format!("Bot {}", self.bot_token.expose()))
                .send()
                .await
                .map_err(upstream)?;
            if res.status().as_u16() == 404 { return Err(DiscordError::Rejected); }
            if !res.status().is_success() { return Err(DiscordError::Upstream(format!("activity-instances: {}", res.status()))); }
            res.json().await.map_err(upstream)
        })
    }

    fn member<'a>(&'a self, guild_id: u64, user_id: u64) -> BoxFuture<'a, Result<Option<MemberInfo>, DiscordError>> {
        Box::pin(async move {
            let member = match self.bot_http.get_member(GuildId::new(guild_id), UserId::new(user_id)).await {
                Ok(m) => m,
                Err(serenity::Error::Http(serenity::all::HttpError::UnsuccessfulRequest(e))) if e.status_code.as_u16() == 404 => {
                    return Ok(None)
                }
                Err(e) => return Err(upstream(e)),
            };
            let guild = self.bot_http.get_guild(GuildId::new(guild_id)).await.map_err(upstream)?;
            Ok(Some(MemberInfo {
                roles: member.roles.iter().map(|r| r.get()).collect(),
                guild_owner_id: guild.owner_id.get(),
                guild_roles: guild.roles.values().map(|r| (r.id.get(), r.permissions.bits())).collect(),
            }))
        })
    }

    fn bot_guilds<'a>(&'a self, limit: usize) -> BoxFuture<'a, Result<Vec<GuildSummary>, DiscordError>> {
        Box::pin(async move {
            let mut out = vec![];
            let mut after = None;
            while out.len() < limit {
                let page = self
                    .bot_http
                    .get_guilds(after.map(serenity::all::GuildPagination::After), Some(200))
                    .await
                    .map_err(upstream)?;
                let last = page.last().map(|g| g.id);
                out.extend(page.into_iter().map(|g| GuildSummary { id: g.id.get(), name: g.name }));
                match last {
                    Some(id) if out.len() % 200 == 0 => after = Some(id),
                    _ => break,
                }
            }
            out.truncate(limit);
            Ok(out)
        })
    }

    fn guild_counts<'a>(&'a self, guild_id: u64) -> BoxFuture<'a, Result<GuildCounts, DiscordError>> {
        Box::pin(async move {
            let channels = self.bot_http.get_channels(GuildId::new(guild_id)).await.map_err(upstream)?.len() as u32;
            let roles = self.bot_http.get_guild_roles(GuildId::new(guild_id)).await.map_err(upstream)?.len() as u32;
            Ok(GuildCounts { channels, roles })
        })
    }
}
