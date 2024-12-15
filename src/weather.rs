use chrono::Duration;
use ed25519_dalek::pkcs8::DecodePrivateKey;
use jwt_compact::{alg::Ed25519, AlgorithmExt, Claims, TimeOptions};
use log::{debug, info};
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::config::get_client;

fn de_float<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => s.parse().map_err(de::Error::custom)?,
        Value::Number(num) => num.as_f64().ok_or(de::Error::custom("Invalid number"))? as f32,
        _ => return Err(de::Error::custom("wrong type")),
    })
}

fn de_int<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => s.parse().map_err(de::Error::custom)?,
        Value::Number(num) => num.as_i64().ok_or(de::Error::custom("Invalid number"))? as i32,
        _ => return Err(de::Error::custom("wrong type")),
    })
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Now {
    pub obs_time: String,
    #[serde(deserialize_with = "de_float")]
    pub temp: f32,
    #[serde(deserialize_with = "de_float")]
    pub feels_like: f32,
    pub icon: String,
    pub text: String,
    #[serde(deserialize_with = "de_int")]
    pub wind360: i32,
    pub wind_dir: String,
    pub wind_scale: String,
    #[serde(deserialize_with = "de_float")]
    pub wind_speed: f32,
    #[serde(deserialize_with = "de_int")]
    pub humidity: i32,
    #[serde(deserialize_with = "de_float")]
    pub precip: f32,
    #[serde(deserialize_with = "de_int")]
    pub pressure: i32,
    #[serde(deserialize_with = "de_float")]
    pub vis: f32,
    #[serde(deserialize_with = "de_int")]
    pub cloud: i32,
    #[serde(deserialize_with = "de_float")]
    pub dew: f32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeatherNow {
    #[serde(deserialize_with = "de_int")]
    pub code: i32,
    pub update_time: String,
    pub fx_link: String,
    pub now: Now,
    pub refer: Refer,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refer {
    pub sources: Vec<String>,
    pub license: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyForecast {
    pub fx_date: String,
    pub sunrise: String,
    pub sunset: String,
    pub moonrise: String,
    pub moonset: String,
    pub moon_phase: String,
    pub moon_phase_icon: String,
    #[serde(deserialize_with = "de_float")]
    pub temp_max: f32,
    #[serde(deserialize_with = "de_float")]
    pub temp_min: f32,
    pub icon_day: String,
    pub text_day: String,
    pub icon_night: String,
    pub text_night: String,
    #[serde(deserialize_with = "de_int")]
    pub wind360_day: i32,
    pub wind_dir_day: String,
    pub wind_scale_day: String,
    #[serde(deserialize_with = "de_float")]
    pub wind_speed_day: f32,
    #[serde(deserialize_with = "de_int")]
    pub wind360_night: i32,
    pub wind_dir_night: String,
    pub wind_scale_night: String,
    #[serde(deserialize_with = "de_float")]
    pub wind_speed_night: f32,
    #[serde(deserialize_with = "de_int")]
    pub humidity: i32,
    #[serde(deserialize_with = "de_float")]
    pub precip: f32,
    #[serde(deserialize_with = "de_int")]
    pub pressure: i32,
    #[serde(deserialize_with = "de_float")]
    pub vis: f32,
    #[serde(deserialize_with = "de_int")]
    pub cloud: i32,
    #[serde(deserialize_with = "de_int")]
    pub uv_index: i32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherDaily {
    #[serde(deserialize_with = "de_int")]
    pub code: i32,
    pub update_time: String,
    pub fx_link: String,
    pub daily: Vec<DailyForecast>,
    pub refer: Refer,
}

#[derive(Debug, Default)]
pub struct Weather {
    pub temperature: f32,
    pub high: f32,
    pub low: f32,
    pub weather_icon: String,
}

fn get_token(app_id: &str, key_id: &str, signing_key: &str) -> anyhow::Result<String> {
    // According to QWeather API, the sub field is the app_id, and uses Ed25519(EdDSA) algorithm
    // The token is signed locally with the private key and sent to the server
    // https://dev.qweather.com/docs/authentication/jwt/

    debug!("Generating token for {}", app_id);

    #[derive(Debug, Clone, Serialize)]
    struct Claim {
        sub: String,
    }

    // Claims include the sub field, the issuance time (iat), and the expiration time (exp)
    let claim = Claims::<Claim>::new(Claim {
        sub: app_id.to_owned(),
    })
    .set_duration_and_issuance(&TimeOptions::default(), Duration::minutes(10));

    // The only useful header field is kid, which is the key_id, the alg field is "EdDSA"
    let header = jwt_compact::Header::empty().with_key_id(key_id.to_owned());
    // The signing key is the private key in PEM format, with or without the header and footer
    let key = if signing_key.starts_with("-----BEGIN") {
        signing_key.to_string()
    } else {
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            signing_key
        )
    };
    debug!("Signing token");
    let signing_key = ed25519_dalek::SigningKey::from_pkcs8_pem(&key)?;
    let result = Ed25519.token(&header, &claim, &signing_key)?;
    debug!("Token generated");
    Ok(result)
}

pub async fn get_weather(
    location: &str,
    app_id: &str,
    key_id: &str,
    signing_key: &str,
) -> anyhow::Result<Weather> {
    info!("Getting weather for {}", location);
    // The operation cannot continue without a token, panic here because it's a unrecoverable error
    // Other errors are mostly recoverable, return a result so the main loop can continue
    let token =
        get_token(app_id, key_id, signing_key).expect("Failed to get token, cannot continue");

    info!("Getting current weather");
    let client = get_client();
    let resp = client
        .get("https://devapi.qweather.com/v7/weather/now")
        .query(&[("location", location)])
        .bearer_auth(&token);
    let now: WeatherNow = resp.send().await?.json().await?;

    info!("Getting weather forecast");
    let resp = client
        .get("https://devapi.qweather.com/v7/weather/7d")
        .query(&[("location", location)])
        .bearer_auth(&token);
    let daily: WeatherDaily = resp.send().await?.json().await?;
    info!("Weather retrieved");
    Ok(Weather {
        temperature: now.now.temp,
        high: daily.daily[0].temp_max,
        low: daily.daily[0].temp_min,
        weather_icon: now.now.icon,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_token() {
        let app = std::env::var("QWEATHER_APP_ID").unwrap();
        let kid = std::env::var("QWEATHER_KEY_ID").unwrap();
        let signing_key = std::env::var("QWEATHER_KEY").unwrap();
        let token = get_token(&app, &kid, &signing_key).unwrap();
        println!("{}", token);
        assert_eq!(token.len(), 206);
    }

    #[tokio::test]
    async fn test_get_weather() {
        let location = "101110113";
        let app = std::env::var("QWEATHER_APP_ID").unwrap();
        let kid = std::env::var("QWEATHER_KEY_ID").unwrap();
        let signing_key = std::env::var("QWEATHER_KEY").unwrap();
        let weather = get_weather(location, &app, &kid, &signing_key)
            .await
            .unwrap();
        println!("{:?}", weather);
        // It should be, right?
        assert!(weather.temperature > -50.0 && weather.temperature < 50.0);
        assert!(weather.high > -50.0 && weather.high < 50.0);
        assert!(weather.low > -50.0 && weather.low < 50.0);
    }
}
