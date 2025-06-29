use std::{fs::read, sync::Arc};

use crate::device_code_flow;
use futures::StreamExt;
use log::{debug, warn};
use platform_dirs::AppDirs;
use serde::{Deserialize, Serialize};

use crate::config::get_client;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenCache {
    access_token: String,
    expires_on: chrono::DateTime<chrono::Utc>,
    refresh_token: String,
}

lazy_static::lazy_static! {
    static ref TOKEN_CACHE: tokio::sync::Mutex<TokenCache> = tokio::sync::Mutex::new(TokenCache {
        access_token: "".to_string(),
        expires_on: chrono::DateTime::UNIX_EPOCH,
        refresh_token: "".to_string(),
    });
}

trait CacheSingleton {
    async fn is_expired(&self) -> bool;
    async fn expire_on(&self) -> chrono::DateTime<chrono::Utc>;
    async fn get_access_token(&self) -> String;
    async fn get_refresh_token(&self) -> String;
    async fn assign(&self, access_token: String, expires_in: u64, refresh_token: String);
    async fn load(&self) -> anyhow::Result<()>;
    async fn save(&self) -> anyhow::Result<()>;
}

impl CacheSingleton for tokio::sync::Mutex<TokenCache> {
    async fn is_expired(&self) -> bool {
        debug!("Checking if token cache is expired");
        let cache = self.lock().await;
        debug!("Token cache expires on {}", cache.expires_on);
        debug!(
            "Token cache expired: {}",
            cache.expires_on <= chrono::Utc::now() - chrono::Duration::seconds(30)
        );
        cache.expires_on <= chrono::Utc::now() - chrono::Duration::seconds(30)
    }

    async fn expire_on(&self) -> chrono::DateTime<chrono::Utc> {
        let cache = self.lock().await;
        cache.expires_on
    }

    async fn get_access_token(&self) -> String {
        let cache = self.lock().await;
        cache.access_token.clone()
    }

    async fn get_refresh_token(&self) -> String {
        let cache = self.lock().await;
        cache.refresh_token.clone()
    }

    async fn assign(&self, access_token: String, expires_in: u64, refresh_token: String) {
        let mut cache = self.lock().await;
        cache.access_token = access_token;
        cache.expires_on = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);
        cache.refresh_token = refresh_token;
    }

    async fn load(&self) -> anyhow::Result<()> {
        debug!("Loading token cache");
        let path = AppDirs::new(Some("todo-station"), false).unwrap().state_dir;
        debug!(
            "Reading token cache from {}",
            path.join("token_cache.json").display()
        );
        let cache = read(path.join("token_cache.json"))?;
        let cache: TokenCache = serde_json::from_slice(&cache)?;
        let mut token_cache = self.lock().await;
        *token_cache = cache;
        debug!("Token cache loaded");
        Ok(())
    }

    async fn save(&self) -> anyhow::Result<()> {
        debug!("Saving token cache");
        let path = AppDirs::new(Some("todo-station"), false).unwrap().state_dir;
        debug!("Creating cache directory at {path:?}");
        std::fs::create_dir_all(&path)?;
        let cache = {
            let token_cache = self.lock().await;
            serde_json::to_vec(&*token_cache)?
        };
        debug!("Writing token cache to file");
        std::fs::write(path.join("token_cache.json"), cache)?;
        debug!("Token cache saved");
        Ok(())
    }
}

async fn do_get_token(app_id: String) -> anyhow::Result<String> {
    let access_token = {
        debug!("Acquiring token with device code flow");
        if TOKEN_CACHE.is_expired().await {
            debug!("Token cache is expired, acquiring new token with device code flow");
            let client = Arc::new(get_client());
            let phase1 = device_code_flow::start(
                client.clone(),
                "consumers",
                &app_id,
                &["openid", "offline_access", "user.read", "Calendars.Read"],
            )
            .await?;
            debug!("Phase 1 done, waiting for user to authorize");
            println!("{}", phase1.message());
            let (access_token, expires_in, refresh_token) = loop {
                match phase1.stream().next().await {
                    Some(Ok(resp)) => {
                        break (
                            resp.access_token().to_owned(),
                            resp.expires_in,
                            resp.refresh_token()
                                .ok_or(anyhow::anyhow!(
                                    "Failed to extract refresh token from auth response"
                                ))?
                                .to_owned(),
                        );
                    }
                    Some(Err(err)) => {
                        if err.to_string().contains("authorization_pending") {
                            debug!("Authorization pending");
                            continue;
                        }
                        return Err(err.into());
                    }
                    None => {
                        warn!("No response, token acquisition failed");
                        return Err(anyhow::anyhow!("No response"));
                    }
                }
            };
            debug!("User authorized, token cache updated");
            TOKEN_CACHE
                .assign(
                    access_token.secret().to_string(),
                    expires_in,
                    refresh_token.secret().to_string(),
                )
                .await;
        }
        TOKEN_CACHE.get_access_token().await
    };
    if TOKEN_CACHE.get_access_token().await.is_empty() {
        return Err(anyhow::anyhow!("Failed to get access token"));
    }
    TOKEN_CACHE.save().await?;
    Ok(access_token)
}

async fn refresh_token(app_id: String) -> anyhow::Result<String> {
    debug!("Refreshing token");
    let access_token = {
        let refresh_token = TOKEN_CACHE.get_refresh_token().await;
        let client = Arc::new(get_client());
        debug!("Refreshing token with refresh token");
        let resp = client
            .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
            .form(&[
                ("client_id", app_id.as_str()),
                ("scope", "openid offline_access user.read Calendars.Read"),
                ("refresh_token", &refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        let body = resp.text().await?;
        let token: serde_json::Value = serde_json::from_str(&body)?;
        let access_token = token["access_token"]
            .as_str()
            .ok_or(anyhow::anyhow!("Failed to get access token"))?;
        let expires_in = token["expires_in"]
            .as_u64()
            .ok_or(anyhow::anyhow!("Failed to get expiration"))?;
        debug!("Token refreshed, updating cache");
        TOKEN_CACHE
            .assign(access_token.to_string(), expires_in, refresh_token)
            .await;
        TOKEN_CACHE.get_access_token().await
    };
    TOKEN_CACHE.save().await?;
    Ok(access_token)
}

pub async fn get_token(app_id: String) -> anyhow::Result<String> {
    debug!("Getting token for app id {app_id}");
    if TOKEN_CACHE.get_access_token().await.is_empty() {
        debug!("Token cache is empty");
        match TOKEN_CACHE.load().await {
            Ok(_) => {
                if TOKEN_CACHE.is_expired().await {
                    debug!("Token cache is expired");
                    if let Ok(token) = refresh_token(app_id.clone()).await {
                        debug!("Token refreshed");
                        Ok(token)
                    } else {
                        do_get_token(app_id.clone()).await
                    }
                } else {
                    debug!(
                        "Token cache is valid until {}",
                        TOKEN_CACHE.expire_on().await
                    );
                    Ok(TOKEN_CACHE.get_access_token().await)
                }
            }
            Err(err) => {
                warn!("Failed to load token cache: {err}");
                do_get_token(app_id).await
            }
        }
    } else {
        debug!("Token cache is not empty");
        if TOKEN_CACHE.is_expired().await {
            debug!("Token cache is expired");
            refresh_token(app_id).await
        } else {
            debug!("Token cache is valid");
            Ok(TOKEN_CACHE.get_access_token().await)
        }
    }
}
