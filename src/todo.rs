use std::{fs::read, sync::Arc};

use azure_identity::device_code_flow;
use chrono::{Days, Local, Timelike};
use futures::StreamExt;
use log::{debug, warn};
use platform_dirs::AppDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Time {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

#[derive(Debug, Clone)]
pub struct TodoItemData {
    pub text: String,
    pub start_time: Time,
    pub end_time: Time,
    pub active: bool,
    pub show_time: bool,
}

#[derive(Debug, Clone)]
pub struct TodoItemGroupData {
    pub group_name: String,
    pub items: Vec<TodoItemData>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarItem {
    subject: String,
    start: TimeWithZone,
    end: TimeWithZone,
    is_all_day: bool,
    is_cancelled: bool,
}

impl From<CalendarItem> for TodoItemData {
    fn from(val: CalendarItem) -> Self {
        // Assume UTC, and it should be
        let start_time = val.start.date_time.and_utc().with_timezone(&Local);
        let end_time = val.end.date_time.and_utc().with_timezone(&Local);
        let start_time = Time {
            hour: start_time.hour() as i32,
            minute: start_time.minute() as i32,
            second: start_time.second() as i32,
        };
        let end_time = Time {
            hour: end_time.hour() as i32,
            minute: end_time.minute() as i32,
            second: end_time.second() as i32,
        };
        TodoItemData {
            text: val.subject,
            start_time,
            end_time,
            active: !val.is_cancelled,
            show_time: !val.is_all_day,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarItems {
    value: Vec<CalendarItem>,
}

impl From<CalendarItems> for Vec<TodoItemGroupData> {
    fn from(val: CalendarItems) -> Self {
        let mut groups = std::collections::BTreeMap::new();
        for item in val.value {
            let group_date = item.start.date_time.and_utc().with_timezone(&Local).date_naive();
            let group_name = group_date.format("%m月%d日").to_string();
            let group = groups.entry(group_date).or_insert_with(|| TodoItemGroupData {
                group_name,
                items: vec![],
                active: true,
            });
            group.items.push(item.into());
        }
        groups.into_values().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimeWithZone {
    date_time: chrono::NaiveDateTime,
    time_zone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenCache {
    access_token: String,
    expires_on: chrono::DateTime<chrono::Utc>,
    refresh_token: String,
}

impl TokenCache {
    fn is_expired(&self) -> bool {
        self.expires_on < chrono::Utc::now()
    }
}

lazy_static::lazy_static! {
    static ref TOKEN_CACHE: tokio::sync::Mutex<TokenCache> = tokio::sync::Mutex::new(TokenCache {
        access_token: "".to_string(),
        expires_on: chrono::Utc::now(),
        refresh_token: "".to_string(),
    });
}

async fn do_get_token(app_id: String) -> anyhow::Result<String> {
    let access_token = {
        let mut cache = TOKEN_CACHE.lock().await;
        if cache.expires_on < chrono::Utc::now() {
            let client = Arc::new(reqwest::Client::new());
            let phrase1 = device_code_flow::start(
                client.clone(),
                "consumers",
                &app_id,
                &["openid", "offline_access", "user.read", "Calendars.Read"],
            )
            .await
            .unwrap();
            println!("{}", phrase1.message());
            let (access_token, expires_in, refresh_token) = loop {
                match phrase1.stream().next().await {
                    Some(Ok(resp)) => {
                        break (
                            resp.access_token().to_owned(),
                            resp.expires_in,
                            resp.refresh_token().unwrap().to_owned(),
                        );
                    }
                    Some(Err(err)) => {
                        if err.to_string().contains("authorization_pending") {
                            continue;
                        }
                        return Err(err.into());
                    }
                    None => {
                        return Err(anyhow::anyhow!("No response"));
                    }
                }
            };
            cache.access_token = access_token.secret().to_string();
            cache.expires_on = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);
            cache.refresh_token = refresh_token.secret().to_string();
        }
        cache.access_token.clone()
    };
    save_token_cache().await?;
    Ok(access_token)
}

async fn load_token_cache() -> anyhow::Result<()> {
    debug!("Loading token cache");
    let path = AppDirs::new(Some("todo-station"), false).unwrap().state_dir;
    let cache = read(path.join("token_cache.json"))?;
    let cache: TokenCache = serde_json::from_slice(&cache)?;
    let mut token_cache = TOKEN_CACHE.lock().await;
    *token_cache = cache;
    debug!("Token cache loaded");
    Ok(())
}

async fn save_token_cache() -> anyhow::Result<()> {
    debug!("Saving token cache");
    let path = AppDirs::new(Some("todo-station"), false).unwrap().state_dir;
    std::fs::create_dir_all(&path)?;
    let token_cache = TOKEN_CACHE.lock().await;
    let cache = serde_json::to_vec(&*token_cache)?;
    std::fs::write(path.join("token_cache.json"), cache)?;
    debug!("Token cache saved");
    Ok(())
}

async fn refresh_token(app_id: String) -> anyhow::Result<String> {
    debug!("Refreshing token");
    let access_token = {
        let mut cache = TOKEN_CACHE.lock().await;
        let client = Arc::new(reqwest::Client::new());
        let resp = client
            .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
            .form(&[
                ("client_id", app_id.as_str()),
                ("scope", "openid offline_access user.read Calendars.Read"),
                ("refresh_token", &cache.refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        let body = resp.text().await?;
        let token: serde_json::Value = serde_json::from_str(&body)?;
        let access_token = token["access_token"].as_str().unwrap();
        let expires_in = token["expires_in"].as_i64().unwrap();
        cache.access_token = access_token.to_string();
        cache.expires_on = chrono::Utc::now() + chrono::Duration::seconds(expires_in);
        cache.access_token.clone()
    };
    save_token_cache().await?;
    Ok(access_token)
}

async fn get_token(app_id: String) -> anyhow::Result<String> {
    let empty = {
        let cache = TOKEN_CACHE.lock().await;
        cache.access_token.is_empty()
    };
    if empty {
        debug!("Token cache is empty");
        match load_token_cache().await {
            Ok(_) => {
                let expired = {
                    let cache = TOKEN_CACHE.lock().await;
                    cache.is_expired()
                };
                if expired {
                    debug!("Token cache is expired");
                    if let Ok(token) = refresh_token(app_id.clone()).await {
                        debug!("Token refreshed");
                        Ok(token)
                    } else {
                        do_get_token(app_id.clone()).await
                    }
                } else {
                    debug!("Token cache is valid");
                    let access_token = {
                        let cache = TOKEN_CACHE.lock().await;
                        cache.access_token.clone()
                    };
                    Ok(access_token)
                }
            }
            Err(err) => {
                warn!("Failed to load token cache: {}", err);
                do_get_token(app_id).await
            }
        }
    } else {
        debug!("Token cache is not empty");
        let expired = {
            let cache = TOKEN_CACHE.lock().await;
            cache.expires_on < chrono::Utc::now()
        };
        if expired {
            debug!("Token cache is expired");
            refresh_token(app_id).await
        } else {
            debug!("Token cache is valid");
            let cache = TOKEN_CACHE.lock().await;
            Ok(cache.access_token.clone())
        }
    }
}

pub async fn get_todo_list(app_id: String) -> anyhow::Result<Vec<TodoItemGroupData>> {
    let token = get_token(app_id).await?;
    let client = reqwest::Client::new();
    let start_of_the_day = chrono::Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .format("%Y-%m-%dT%H:%M:%S%.fZ");
    let end_of_the_day = chrono::Local::now()
        .checked_add_days(Days::new(7))
        .unwrap()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .format("%Y-%m-%dT%H:%M:%S%.fZ");
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/calendarview?startDateTime={}&endDateTime={}",
        start_of_the_day, end_of_the_day
    );
    // println!("curl -H 'Authorization: Bearer {}' '{}' ", token, url);
    let resp = client.get(&url).bearer_auth(token).send().await?;
    let body = resp.text().await?;
    // println!("{}", body);
    let items: CalendarItems = serde_json::from_str(&body)?;
    Ok(items.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_todo_list() {
        let app_id = include_str!("../test/todo-app-id.txt").trim().to_string();
        let todo_list = get_todo_list(app_id).await.unwrap();
        println!("{:?}", todo_list);
    }
}
