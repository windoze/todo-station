use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Local};
use log::debug;
use platform_dirs::AppDirs;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WindowConfig {
    pub weekdays: Vec<String>,
    pub date_format: String,
    pub full_screen: bool,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WeatherConfig {
    pub location: String,
    pub app_id: String,
    pub key_id: String,
    pub signing_key: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoConfig {
    pub app_id: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    pub window: WindowConfig,
    pub weather: WeatherConfig,
    pub todo: TodoConfig,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            weekdays: vec![
                "星期日".to_string(),
                "星期一".to_string(),
                "星期二".to_string(),
                "星期三".to_string(),
                "星期四".to_string(),
                "星期五".to_string(),
                "星期六".to_string(),
            ],
            date_format: "%Y年%m月%d日，%A".to_string(),
            full_screen: false,
        }
    }
}

impl WindowConfig {
    pub fn format_date(&self, date: &DateTime<Local>) -> String {
        let binding = "".to_string();
        let weekday = self
            .weekdays
            .get(date.weekday() as usize)
            .unwrap_or(&binding);
        // chrono does not support %A for locale weekday names
        let format = self.date_format.replace("%A", weekday);
        date.format(&format).to_string()
    }
}

fn get_config_file() -> PathBuf {
    let app_dirs = AppDirs::new(Some("todo-station"), false).unwrap();
    app_dirs.config_dir.join("config.toml")
}

pub fn get_config<P: AsRef<Path>>(config_path: Option<P>) -> anyhow::Result<AppConfig> {
    let config_path: PathBuf = config_path
        .map(|p| p.as_ref().to_path_buf())
        .unwrap_or(get_config_file());
    debug!("Config path: {:?}", config_path);
    let config = std::fs::read_to_string(&config_path).unwrap_or_default();
    if config.is_empty() {
        debug!("No config file found, using default config");
        return Ok(AppConfig::default());
    }
    Ok(toml::from_str::<AppConfig>(&config)?)
}
