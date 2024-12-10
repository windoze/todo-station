use log::{debug, info};
use serde::Deserialize;

const WALLPAPER_URL_BASE: &str = "https://www.bing.com";
const WALLPAPER_URL_JSON: &str =
    "https://www.bing.com/HPImageArchive.aspx?format=js&idx=0&n=1&mkt=en-US";

#[derive(Debug, Deserialize)]
struct Wallpaper {
    url: String,
}

#[derive(Debug, Deserialize)]
struct WallpaperResponse {
    images: Vec<Wallpaper>,
}

pub async fn get_wallpaper() -> anyhow::Result<image::DynamicImage> {
    info!("Fetching wallpaper from Bing");
    let resp = reqwest::Client::new().get(WALLPAPER_URL_JSON).send().await?;
    let wallpaper = resp.json::<WallpaperResponse>().await?;

    let image_url = format!("{}{}", WALLPAPER_URL_BASE, wallpaper.images[0].url);
    debug!("Wallpaper URL: {}", image_url);
    let bytes = reqwest::Client::new().get(&image_url).send().await?.bytes().await?;

    info!("Wallpaper fetched successfully");
    Ok(image::load_from_memory(&bytes)?) 
}

#[cfg(test)]
mod tests {
    use image::GenericImageView;

    use super::*;

    #[tokio::test]
    async fn test_get_wallpaper() {
        let wallpaper = get_wallpaper().await.unwrap();
        assert_eq!(wallpaper.dimensions(), (1920, 1080));
    }
}