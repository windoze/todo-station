use chrono::{Days, Local, Timelike};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use token::get_token;

use crate::config::get_client;

mod token;

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
            let group_date = item
                .start
                .date_time
                .and_utc()
                .with_timezone(&Local)
                .date_naive();
            let group_name = group_date.format("%m月%d日").to_string();
            let group = groups
                .entry(group_date)
                .or_insert_with(|| TodoItemGroupData {
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

pub async fn get_todo_list(app_id: String) -> anyhow::Result<Vec<TodoItemGroupData>> {
    info!("Getting todo list");
    let token = get_token(app_id).await?;
    let client = get_client();
    let start_of_the_day = Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .format("%Y-%m-%dT%H:%M:%S%.fZ");
    let end_of_the_day = Local::now()
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
    debug!("Requesting todo list from {}", url);
    // println!("curl -H 'Authorization: Bearer {}' '{}' ", token, url);
    let resp = client.get(&url).bearer_auth(token).send().await?;
    let body = resp.text().await?;
    let items: CalendarItems = serde_json::from_str(&body)?;
    info!(
        "Todo list retrieved, {} items in next 7 days",
        items.value.len()
    );
    Ok(items.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Needs interactive login"]
    async fn test_get_todo_list() {
        let app_id = std::env::var("AAD_APP_ID").unwrap().to_string();
        let todo_list = get_todo_list(app_id).await.unwrap();
        println!("{:?}", todo_list);
    }
}
