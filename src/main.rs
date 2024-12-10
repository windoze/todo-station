// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{rc::Rc, time::Duration};

use chrono::prelude::*;
use clap::Parser;
use log::{debug, info};
use slint::{ModelRc, Rgb8Pixel, SharedPixelBuffer, VecModel, Weak};
use tokio::time::sleep;

mod config;
mod todo;
mod wallpaper;
mod weather;

use config::{get_config, TodoConfig, WeatherConfig, WindowConfig};
use wallpaper::get_wallpaper;
use weather::get_weather;

slint::include_modules!();

#[derive(rust_embed::Embed)]
#[folder = "ui/assets"]
struct Assets;

async fn update_time(handle: Weak<AppWindow>, cfg: WindowConfig) {
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

async fn update_weather(handle: Weak<AppWindow>, cfg: WeatherConfig) {
    loop {
        // Update weather every hour
        debug!("Getting weather");
        if let Ok(weather) =
            get_weather(&cfg.location, &cfg.app_id, &cfg.key_id, &cfg.signing_key).await
        {
            let icon_path = format!("{}.svg", weather.weather_icon);
            debug!("Weather icon path: {}", icon_path);
            if let Some(file) = Assets::get(&icon_path) {
                handle
                    .upgrade_in_event_loop(move |ui| {
                        ui.global::<AppData>()
                            .set_temperature(weather.temperature as i32);
                        ui.global::<AppData>().set_high(weather.high as i32);
                        ui.global::<AppData>().set_low(weather.low as i32);
                        if let Ok(image) = slint::Image::load_from_svg_data(&file.data) {
                            ui.global::<AppData>().set_weather_icon(image);
                        }
                    })
                    .unwrap();
            }
        }

        // Sleep until the next hour
        let now = chrono::Local::now();
        let begin_of_next_hour = now
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap()
            + chrono::Duration::hours(1);
        debug!("The next run of getting weather is at {}", begin_of_next_hour.to_utc());
        let duration = begin_of_next_hour - now;
        sleep(duration.to_std().unwrap_or_default()).await;
    }
}

async fn update_wallpaper(handle: Weak<AppWindow>) {
    loop {
        // Update wallpaper every day
        debug!("Getting wallpaper");
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

        // Sleep until the next 9AM UTC, it's about the time when the wallpaper changes
        let now = chrono::Utc::now();
        let next_1am = now
            .with_hour(9)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap()
            + chrono::Duration::days(1);
        debug!("The next run of getting wallpaper is at {}", next_1am);
        let duration = next_1am - now;
        sleep(duration.to_std().unwrap_or_default()).await;
    }
}

impl From<todo::Time> for Time {
    fn from(time: todo::Time) -> Self {
        Self {
            hour: time.hour,
            minute: time.minute,
            second: time.second,
        }
    }
}

impl From<todo::TodoItemData> for TodoItemData {
    fn from(item: todo::TodoItemData) -> Self {
        Self {
            active: item.active,
            end_time: item.end_time.into(),
            show_time: item.show_time,
            start_time: item.start_time.into(),
            text: item.text.into(),
        }
    }
}

impl From<todo::TodoItemGroupData> for TodoItemGroupData {
    fn from(list: todo::TodoItemGroupData) -> Self {
        let items: Vec<TodoItemData> = list.items.into_iter().map(|item| item.into()).collect();
        Self {
            active: true,
            items: ModelRc::from(Rc::new(VecModel::from(items))),
            group_name: list.group_name.into(),
        }
    }
}

async fn update_todo(handle: Weak<AppWindow>, cfg: TodoConfig) {
    loop {
        // Update todo every 10 minutes
        debug!("Getting todo list");
        if let Ok(todo) = todo::get_todo_list(cfg.app_id.clone()).await {
            handle
                .upgrade_in_event_loop(move |ui| {
                    let groups: Vec<TodoItemGroupData> =
                        todo.into_iter().map(|list| list.into()).collect();
                    ui.global::<AppData>()
                        .set_todo_list(ModelRc::from(Rc::new(VecModel::from(groups))));
                })
                .unwrap();
        }

        // Sleep for 10 minutes
        sleep(Duration::from_secs(600)).await;
    }
}

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // Attach to parent console if it exists, so that we can see the console output.
        // The app requires the console to display the device code login message.
        unsafe {
            winapi::um::wincon::AttachConsole(winapi::um::wincon::ATTACH_PARENT_PROCESS);
        }
    }
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

    if cfg.window.full_screen {
        std::env::set_var("SLINT_FULLSCREEN", "1");
    }

    let ui = AppWindow::new()?;

    if cfg.window.full_screen {
        ui.global::<AppData>().set_framed(false);
    }

    let rt = tokio::runtime::Runtime::new()?;

    let handle = ui.as_weak();
    rt.spawn(async move {
        info!("Starting wallpaper update task");
        update_wallpaper(handle).await;
    });

    let handle = ui.as_weak();
    let cfg_clone = cfg.window.clone();
    rt.spawn(async move {
        info!("Starting time update task");
        update_time(handle, cfg_clone).await;
    });

    let handle = ui.as_weak();
    let cfg_clone = cfg.weather.clone();
    rt.spawn(async move {
        info!("Starting weather update task");
        update_weather(handle, cfg_clone).await;
    });

    let handle = ui.as_weak();
    let cfg_clone = cfg.todo.clone();
    rt.spawn(async move {
        info!("Starting todo update task");
        update_todo(handle, cfg_clone).await;
    });

    ui.run()?;

    Ok(())
}
