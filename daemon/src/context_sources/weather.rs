use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

use super::{
    ContextSource, ContextData, ContextDataType, ContextContent, WeatherContext, WeatherForecast
};

// ── Google Weather API response types ──────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentConditionsResponse {
    weather_condition: Option<WeatherCondition>,
    temperature: Option<Temperature>,
    feels_like_temperature: Option<Temperature>,
    relative_humidity: Option<i32>,
    wind: Option<WindInfo>,
    precipitation: Option<Precipitation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForecastResponse {
    forecast_days: Option<Vec<ForecastDay>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForecastDay {
    display_date: Option<DisplayDate>,
    daytime_forecast: Option<DayPartForecast>,
    max_temperature: Option<Temperature>,
    min_temperature: Option<Temperature>,
}

#[derive(Debug, Deserialize)]
struct DisplayDate {
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DayPartForecast {
    weather_condition: Option<WeatherCondition>,
    precipitation: Option<Precipitation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeatherCondition {
    description: Option<LocalizedText>,
    #[serde(rename = "type")]
    condition_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalizedText {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Temperature {
    degrees: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct WindInfo {
    speed: Option<SpeedValue>,
}

#[derive(Debug, Deserialize)]
struct SpeedValue {
    value: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Precipitation {
    probability: Option<PrecipProbability>,
}

#[derive(Debug, Deserialize)]
struct PrecipProbability {
    percent: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlertsResponse {
    alerts: Option<Vec<Alert>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Alert {
    event_name: Option<String>,
    severity: Option<String>,
}

// ── Cached weather result ──────────────────────────────────────────────

struct CachedWeather {
    data: WeatherContext,
    fetched_at: DateTime<Utc>,
}

// ── Weather context source ─────────────────────────────────────────────

pub struct WeatherContextSource {
    google_api_key: String,
    latitude: f64,
    longitude: f64,
    enabled: bool,
    client: Client,
    units: String,
    cache_duration_minutes: u32,
    cache: RwLock<Option<CachedWeather>>,
}

impl WeatherContextSource {
    pub fn new(google_api_key: String, latitude: f64, longitude: f64, units: String, cache_duration_minutes: u32) -> Self {
        let enabled = !google_api_key.is_empty();
        Self {
            google_api_key,
            latitude,
            longitude,
            enabled,
            client: Client::new(),
            units,
            cache_duration_minutes,
            cache: RwLock::new(None),
        }
    }

    /// Fetch current conditions from Google Weather API
    async fn fetch_current_weather(&self) -> Result<CurrentConditionsResponse> {
        let units_system = self.google_units_system();
        let url = format!(
            "https://weather.googleapis.com/v1/currentConditions:lookup?\
             key={}&location.latitude={}&location.longitude={}&unitsSystem={}",
            self.google_api_key, self.latitude, self.longitude, units_system
        );

        debug!("Fetching current weather from Google Weather API");

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Google Weather currentConditions failed ({}): {}", status, body));
        }

        Ok(response.json().await?)
    }

    /// Fetch daily forecast from Google Weather API
    async fn fetch_forecast(&self) -> Result<ForecastResponse> {
        let units_system = self.google_units_system();
        let url = format!(
            "https://weather.googleapis.com/v1/forecast/days:lookup?\
             key={}&location.latitude={}&location.longitude={}&days=5&unitsSystem={}",
            self.google_api_key, self.latitude, self.longitude, units_system
        );

        debug!("Fetching weather forecast from Google Weather API");

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Google Weather forecast failed ({}): {}", status, body));
        }

        Ok(response.json().await?)
    }

    /// Fetch weather alerts from Google Weather API
    async fn fetch_alerts(&self) -> Result<AlertsResponse> {
        let url = format!(
            "https://weather.googleapis.com/v1/publicAlerts:lookup?\
             key={}&location.latitude={}&location.longitude={}",
            self.google_api_key, self.latitude, self.longitude
        );

        debug!("Fetching weather alerts from Google Weather API");

        let response = self.client.get(&url).send().await?;
        if !response.status().is_success() {
            // Alerts endpoint may 404 if no alerts — that's fine
            let status = response.status();
            if status.as_u16() == 404 {
                return Ok(AlertsResponse { alerts: None });
            }
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Google Weather alerts failed ({}): {}", status, body));
        }

        Ok(response.json().await?)
    }

    /// Map config units to Google API unitsSystem parameter
    fn google_units_system(&self) -> &str {
        match self.units.as_str() {
            "imperial" => "IMPERIAL",
            "metric" => "METRIC",
            _ => "IMPERIAL",
        }
    }

    /// Unit suffix for temperature display
    fn temp_unit(&self) -> &str {
        match self.units.as_str() {
            "metric" => "°C",
            _ => "°F",
        }
    }

    /// Convert Google forecast days to our WeatherForecast format
    fn convert_forecast(&self, days: Vec<ForecastDay>) -> Vec<WeatherForecast> {
        days.into_iter()
            .filter_map(|day| {
                let date = day.display_date.as_ref().and_then(|d| {
                    let y = d.year?;
                    let m = d.month?;
                    let d = d.day?;
                    chrono::NaiveDate::from_ymd_opt(y, m, d)
                        .and_then(|nd| nd.and_hms_opt(12, 0, 0))
                        .map(|ndt| ndt.and_utc())
                })?;

                let high = day.max_temperature.as_ref()?.degrees? as f32;
                let low = day.min_temperature.as_ref()?.degrees? as f32;

                let conditions = day.daytime_forecast.as_ref()
                    .and_then(|df| df.weather_condition.as_ref())
                    .and_then(|wc| wc.condition_type.clone())
                    .unwrap_or_else(|| "Unknown".to_string());

                let description = day.daytime_forecast.as_ref()
                    .and_then(|df| df.weather_condition.as_ref())
                    .and_then(|wc| wc.description.as_ref())
                    .and_then(|d| d.text.clone())
                    .unwrap_or_else(|| conditions.clone());

                let precip_chance = day.daytime_forecast.as_ref()
                    .and_then(|df| df.precipitation.as_ref())
                    .and_then(|p| p.probability.as_ref())
                    .and_then(|pp| pp.percent)
                    .unwrap_or(0) as f32 / 100.0;

                Some(WeatherForecast {
                    date,
                    temperature_high: high,
                    temperature_low: low,
                    conditions,
                    precipitation_chance: precip_chance,
                    description,
                })
            })
            .collect()
    }

    /// Fetch and assemble all weather data with TTL-based caching
    async fn fetch_weather_data(&self) -> Result<WeatherContext> {
        if !self.enabled {
            return Err(anyhow!("Weather API not enabled (no Google API key configured)"));
        }

        // Return cached data if still fresh
        {
            let cache = self.cache.read().await;
            if let Some(ref cached) = *cache {
                let age = Utc::now().signed_duration_since(cached.fetched_at);
                if age.num_minutes() < self.cache_duration_minutes as i64 {
                    debug!("Returning cached weather data ({} min old)", age.num_minutes());
                    return Ok(cached.data.clone());
                }
            }
        }

        // Fetch current conditions, forecast, and alerts in parallel
        let (current_result, forecast_result, alerts_result) = tokio::join!(
            self.fetch_current_weather(),
            self.fetch_forecast(),
            self.fetch_alerts()
        );

        let current = current_result?;
        let forecast_resp = forecast_result?;

        // Alerts are best-effort — log failures but don't fail the whole fetch
        let alerts_resp = match alerts_result {
            Ok(a) => a,
            Err(e) => {
                warn!("Failed to fetch weather alerts (non-fatal): {}", e);
                AlertsResponse { alerts: None }
            }
        };

        // Build current conditions string
        let unit = self.temp_unit();
        let condition_text = current.weather_condition.as_ref()
            .and_then(|wc| wc.description.as_ref())
            .and_then(|d| d.text.clone())
            .unwrap_or_else(|| "Unknown conditions".to_string());

        let temp = current.temperature.as_ref()
            .and_then(|t| t.degrees)
            .map(|d| format!("{:.0}{}", d, unit))
            .unwrap_or_default();

        let feels_like = current.feels_like_temperature.as_ref()
            .and_then(|t| t.degrees)
            .map(|d| format!(" (feels like {:.0}{})", d, unit))
            .unwrap_or_default();

        let humidity = current.relative_humidity
            .map(|h| format!(", {}% humidity", h))
            .unwrap_or_default();

        let current_conditions = format!("{}, {}{}{}", condition_text, temp, feels_like, humidity);

        // Convert forecast
        let forecast = forecast_resp.forecast_days
            .map(|days| self.convert_forecast(days))
            .unwrap_or_default();

        // Convert alerts
        let mut alerts: Vec<String> = alerts_resp.alerts
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| {
                let name = a.event_name?;
                let severity = a.severity.unwrap_or_default();
                Some(if severity.is_empty() { name } else { format!("{} ({})", name, severity) })
            })
            .collect();

        // Also generate simple temperature-based alerts like the old implementation
        if let Some(temp_deg) = current.temperature.as_ref().and_then(|t| t.degrees) {
            if self.units == "imperial" {
                if temp_deg < 32.0 {
                    alerts.push("Freezing temperatures — dress warmly".to_string());
                } else if temp_deg > 95.0 {
                    alerts.push("Extreme heat — stay hydrated".to_string());
                }
            } else if temp_deg < 0.0 {
                alerts.push("Below freezing — dress warmly".to_string());
            } else if temp_deg > 35.0 {
                alerts.push("Extreme heat — stay hydrated".to_string());
            }
        }

        info!("Weather data fetched from Google Weather API");

        let result = WeatherContext {
            current_conditions,
            forecast,
            alerts,
        };

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedWeather {
                data: result.clone(),
                fetched_at: Utc::now(),
            });
        }

        Ok(result)
    }
}

#[async_trait]
impl ContextSource for WeatherContextSource {
    fn source_id(&self) -> &str {
        "weather"
    }

    fn display_name(&self) -> &str {
        "Weather"
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    async fn fetch_context(&self, _start: DateTime<Utc>, _end: DateTime<Utc>) -> Result<ContextData> {
        info!("Fetching weather context from Google Weather API");

        let weather_context = self.fetch_weather_data().await?;

        Ok(ContextData {
            source_id: self.source_id().to_string(),
            timestamp: Utc::now(),
            data_type: ContextDataType::Weather,
            priority: 75,
            content: ContextContent::Weather(weather_context),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("latitude".to_string(), self.latitude.to_string());
                metadata.insert("longitude".to_string(), self.longitude.to_string());
                metadata.insert("source_type".to_string(), "google_weather".to_string());
                metadata
            },
        })
    }

    fn priority(&self) -> i32 {
        75
    }

    fn required_config(&self) -> Vec<String> {
        vec!["google_api_key".to_string(), "latitude".to_string(), "longitude".to_string()]
    }
}
