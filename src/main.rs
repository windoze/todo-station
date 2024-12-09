// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use chrono::prelude::*;
use clap::Parser;
use platform_dirs::AppDirs;
use serde::Deserialize;
use slint::{Rgb8Pixel, SharedPixelBuffer, Weak};
use tokio::time::sleep;

mod wallpaper;
mod weather;

use wallpaper::get_wallpaper;
use weather::get_weather;

slint::include_modules!();

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct AppConfig {
    pub weekdays: Vec<String>,
    pub date_format: String,
    pub width: i32,
    pub height: i32,
    pub full_screen: bool,

    pub location: String,
    pub app_id: String,
    pub key_id: String,
    pub signing_key: String,
}

impl Default for AppConfig {
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
            width: 800,
            height: 480,
            full_screen: false,

            location: "101110113".to_string(),
            app_id: include_str!("../test/app-id.txt").trim().to_string(),
            key_id: include_str!("../test/key-id.txt").trim().to_string(),
            signing_key: include_str!("../test/ed25519-private.pem").to_string(),
        }
    }
}

impl AppConfig {
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

fn get_config<P: AsRef<Path>>(config_path: Option<P>) -> anyhow::Result<AppConfig> {
    let config_path: PathBuf = config_path
        .map(|p| p.as_ref().to_path_buf())
        .unwrap_or(get_config_file());
    let config = std::fs::read_to_string(&config_path).unwrap_or_default();
    if config.is_empty() {
        return Ok(AppConfig::default());
    }
    Ok(toml::from_str::<AppConfig>(&config)?)
}

fn main() -> anyhow::Result<()> {
    #[derive(Debug, Clone, Parser)]
    struct Args {
        #[arg(long = "config")]
        config_path: Option<std::path::PathBuf>,
        #[command(flatten)]
        verbose: clap_verbosity_flag::Verbosity,
    }

    let cli = Args::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    let cfg = get_config(cli.config_path)?;

    if cfg.full_screen {
        std::env::set_var("SLINT_FULLSCREEN", "1");
    }

    let ui = AppWindow::new()?;

    ui.global::<AppData>().set_width(cfg.width as f32);
    ui.global::<AppData>().set_height(cfg.height as f32);
    if cfg.full_screen {
        ui.global::<AppData>().set_framed(false);
    }

    let rt = tokio::runtime::Runtime::new()?;

    let handle = ui.as_weak();
    rt.spawn(async move {
        update_wallpaper(handle).await;
    });

    let handle = ui.as_weak();
    let cfg_clone = cfg.clone();
    rt.spawn(async move {
        update_time(handle, cfg_clone).await;
    });

    let handle = ui.as_weak();
    let cfg_clone = cfg.clone();
    rt.spawn(async move {
        update_weather(handle, cfg_clone).await;
    });

    ui.run()?;

    Ok(())
}

async fn update_time(handle: Weak<AppWindow>, cfg: AppConfig) {
    loop {
        // Update time every 100ms
        sleep(Duration::from_millis(100)).await;
        let cfg_clone = cfg.clone();
        handle
            .upgrade_in_event_loop(move |ui| {
                let now = chrono::Local::now();
                ui.global::<AppData>().set_current_time(Time {
                    hour: now.hour() as i32,
                    minute: now.minute() as i32,
                    second: now.second() as i32,
                });

                ui.global::<AppData>().set_current_date(Date {
                    year: now.year(),
                    month: now.month() as i32,
                    day: now.day() as i32,
                });

                ui.global::<AppData>()
                    .set_date_string(cfg_clone.format_date(&now).into());
                ui.global::<AppData>()
                    .set_second_blink_on(now.timestamp_subsec_millis() < 500);
            })
            .unwrap();
    }
}

async fn update_weather(handle: Weak<AppWindow>, cfg: AppConfig) {
    loop {
        // Update time every hour
        if let Ok(weather) =
            get_weather(&cfg.location, &cfg.app_id, &cfg.key_id, &cfg.signing_key).await
        {
            let icon_path = format!("ui/assets/{}.svg", weather.weather_icon);
            if let Ok(content) = tokio::fs::read(&icon_path).await {
                handle
                    .upgrade_in_event_loop(move |ui| {
                        ui.global::<AppData>()
                            .set_temperature(weather.temperature as i32);
                        ui.global::<AppData>().set_high(weather.high as i32);
                        ui.global::<AppData>().set_low(weather.low as i32);
                        if let Ok(image) = slint::Image::load_from_svg_data(&content) {
                            ui.global::<AppData>().set_weather_icon(image);
                        }
                    })
                    .unwrap();
            }
        }

        sleep(Duration::from_secs(3600)).await;
    }
}

async fn update_wallpaper(handle: Weak<AppWindow>) {
    loop {
        if let Ok(wallpaper) = get_wallpaper().await {
            handle
                .upgrade_in_event_loop(move |ui| {
                    let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(
                        wallpaper.as_rgb8().unwrap().as_raw(),
                        wallpaper.width(),
                        wallpaper.height(),
                    );
                    let image = slint::Image::from_rgb8(buffer);
                    ui.global::<AppData>().set_background(image);
                })
                .unwrap();
        }

        sleep(Duration::from_secs(86400)).await;
    }
}
