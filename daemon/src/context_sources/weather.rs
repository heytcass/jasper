use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn, debug};

use super::{
    ContextSource, ContextData, ContextDataType, ContextContent, WeatherContext, WeatherForecast
};

/// OpenWeatherMap API response structures
#[derive(Debug, Deserialize)]
struct OpenWeatherMapResponse {
    coord: Coord,
    weather: Vec<Weather>,
    main: Main,
    visibility: Option<i32>,
    wind: Option<Wind>,
    rain: Option<Rain>,
    snow: Option<Snow>,
    dt: i64,
    sys: Sys,
    timezone: i32,
    id: i64,
    name: String,
    cod: i32,
}

#[derive(Debug, Deserialize)]
struct Coord {
    lon: f64,
    lat: f64,
}

#[derive(Debug, Deserialize)]
struct Weather {
    id: i32,
    main: String,
    description: String,
    icon: String,
}

#[derive(Debug, Deserialize)]
struct Main {
    temp: f64,
    feels_like: f64,
    temp_min: f64,
    temp_max: f64,
    pressure: i32,
    humidity: i32,
    sea_level: Option<i32>,
    grnd_level: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct Wind {
    speed: f64,
    deg: i32,
    gust: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Rain {
    #[serde(rename = "1h")]
    one_hour: Option<f64>,
    #[serde(rename = "3h")]
    three_hour: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Snow {
    #[serde(rename = "1h")]
    one_hour: Option<f64>,
    #[serde(rename = "3h")]
    three_hour: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Sys {
    #[serde(rename = "type")]
    sys_type: Option<i32>,
    id: Option<i64>,
    country: Option<String>,
    sunrise: Option<i64>,
    sunset: Option<i64>,
}

// 5-day forecast API response
#[derive(Debug, Deserialize)]
struct ForecastResponse {
    cod: String,
    message: i32,
    cnt: i32,
    list: Vec<ForecastItem>,
    city: City,
}

#[derive(Debug, Deserialize)]
struct ForecastItem {
    dt: i64,
    main: Main,
    weather: Vec<Weather>,
    clouds: Clouds,
    wind: Wind,
    visibility: i32,
    pop: f64, // Probability of precipitation
    rain: Option<Rain>,
    snow: Option<Snow>,
    sys: ForecastSys,
    dt_txt: String,
}

#[derive(Debug, Deserialize)]
struct Clouds {
    all: i32,
}

#[derive(Debug, Deserialize)]
struct ForecastSys {
    pod: String, // Part of day (n-night, d-day)
}

#[derive(Debug, Deserialize)]
struct City {
    id: i64,
    name: String,
    coord: Coord,
    country: String,
    population: i64,
    timezone: i32,
    sunrise: i64,
    sunset: i64,
}

/// Weather context source with OpenWeatherMap integration
pub struct WeatherContextSource {
    api_key: Option<String>,
    location: String,
    enabled: bool,
    client: Client,
    units: String, // "metric", "imperial", "kelvin"
    cache_duration_minutes: u32,
}

impl WeatherContextSource {
    /// Create a new weather context source
    pub fn new(api_key: Option<String>, location: String) -> Self {
        let enabled = api_key.is_some();
        Self {
            api_key,
            location,
            enabled,
            client: Client::new(),
            units: "imperial".to_string(), // Default to Fahrenheit for US
            cache_duration_minutes: 60, // Cache for 1 hour
        }
    }
    
    /// Create with custom configuration
    pub fn with_config(api_key: Option<String>, location: String, units: String, cache_duration_minutes: u32) -> Self {
        let enabled = api_key.is_some();
        Self {
            api_key,
            location,
            enabled,
            client: Client::new(),
            units,
            cache_duration_minutes,
        }
    }
    
    /// Fetch current weather data from OpenWeatherMap API
    async fn fetch_current_weather(&self) -> Result<OpenWeatherMapResponse> {
        let api_key = self.api_key.as_ref().ok_or_else(|| anyhow!("OpenWeatherMap API key not configured"))?;
        
        let url = format!(
            "https://api.openweathermap.org/data/2.5/weather?q={}&appid={}&units={}",
            self.location, api_key, self.units
        );
        
        debug!("Fetching current weather from: {}", url);
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Weather API request failed with status {}: {}", status, body));
        }
        
        let weather_data: OpenWeatherMapResponse = response.json().await?;
        debug!("Successfully fetched current weather for {}", weather_data.name);
        
        Ok(weather_data)
    }
    
    /// Fetch weather forecast from OpenWeatherMap API
    async fn fetch_forecast(&self) -> Result<ForecastResponse> {
        let api_key = self.api_key.as_ref().ok_or_else(|| anyhow!("OpenWeatherMap API key not configured"))?;
        
        let url = format!(
            "https://api.openweathermap.org/data/2.5/forecast?q={}&appid={}&units={}",
            self.location, api_key, self.units
        );
        
        debug!("Fetching weather forecast from: {}", url);
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow!("Weather forecast API request failed with status {}: {}", status, body));
        }
        
        let forecast_data: ForecastResponse = response.json().await?;
        debug!("Successfully fetched forecast for {}", forecast_data.city.name);
        
        Ok(forecast_data)
    }
    
    /// Convert temperature to display format
    fn format_temperature(&self, temp: f64) -> String {
        match self.units.as_str() {
            "metric" => format!("{:.0}°C", temp),
            "imperial" => format!("{:.0}°F", temp),
            "kelvin" => format!("{:.0}K", temp),
            _ => format!("{:.0}°F", temp), // Default to Fahrenheit
        }
    }
    
    /// Convert OpenWeatherMap forecast to our WeatherForecast format
    fn convert_forecast_items(&self, forecast_items: Vec<ForecastItem>) -> Vec<WeatherForecast> {
        let mut daily_forecasts = HashMap::new();
        
        // Group forecast items by day and find daily highs/lows
        for item in forecast_items {
            let date = DateTime::from_timestamp(item.dt, 0)
                .unwrap_or_else(Utc::now)
                .date_naive();
            
            let entry = daily_forecasts.entry(date).or_insert({
                WeatherForecast {
                    date: date.and_hms_opt(12, 0, 0).unwrap().and_utc(),
                    temperature_high: item.main.temp_max as f32,
                    temperature_low: item.main.temp_min as f32,
                    conditions: item.weather.first().map(|w| w.main.clone()).unwrap_or_else(|| "Unknown".to_string()),
                    precipitation_chance: item.pop as f32,
                    description: item.weather.first().map(|w| w.description.clone()).unwrap_or_else(|| "No description".to_string()),
                }
            });
            
            // Update highs and lows
            entry.temperature_high = entry.temperature_high.max(item.main.temp_max as f32);
            entry.temperature_low = entry.temperature_low.min(item.main.temp_min as f32);
            
            // Update precipitation chance to maximum for the day
            entry.precipitation_chance = entry.precipitation_chance.max(item.pop as f32);
        }
        
        // Convert to sorted vector
        let mut forecasts: Vec<WeatherForecast> = daily_forecasts.into_values().collect();
        forecasts.sort_by(|a, b| a.date.cmp(&b.date));
        
        // Take only the next 5 days
        forecasts.truncate(5);
        
        forecasts
    }
    
    /// Generate weather alerts based on conditions
    fn generate_weather_alerts(&self, current: &OpenWeatherMapResponse, forecast: &[WeatherForecast]) -> Vec<String> {
        let mut alerts = Vec::new();
        
        // Check for severe weather conditions
        if let Some(weather) = current.weather.first() {
            match weather.main.as_str() {
                "Thunderstorm" => alerts.push("Thunderstorms expected - plan indoor activities".to_string()),
                "Snow" => alerts.push("Snow conditions - allow extra travel time".to_string()),
                "Rain" => {
                    if current.rain.as_ref().and_then(|r| r.one_hour).unwrap_or(0.0) > 5.0 {
                        alerts.push("Heavy rain expected - consider rescheduling outdoor plans".to_string());
                    }
                },
                _ => {}
            }
        }
        
        // Check temperature extremes
        if self.units == "imperial" {
            if current.main.temp < 32.0 {
                alerts.push("Freezing temperatures - dress warmly".to_string());
            } else if current.main.temp > 90.0 {
                alerts.push("High temperatures - stay hydrated".to_string());
            }
        } else if self.units == "metric" {
            if current.main.temp < 0.0 {
                alerts.push("Below freezing - dress warmly".to_string());
            } else if current.main.temp > 32.0 {
                alerts.push("High temperatures - stay hydrated".to_string());
            }
        }
        
        // Check for precipitation in upcoming forecast
        for forecast_day in forecast.iter().take(2) { // Check next 2 days
            if forecast_day.precipitation_chance > 0.7 {
                let day_name = if forecast_day.date.date_naive() == Utc::now().date_naive() {
                    "today".to_string()
                } else if forecast_day.date.date_naive() == (Utc::now() + chrono::Duration::days(1)).date_naive() {
                    "tomorrow".to_string()
                } else {
                    forecast_day.date.format("%A").to_string()
                };
                
                alerts.push(format!("High chance of rain {} - bring an umbrella", day_name));
            }
        }
        
        alerts
    }
    
    /// Fetch weather data from OpenWeatherMap API
    async fn fetch_weather_data(&self, _start: DateTime<Utc>, _end: DateTime<Utc>) -> Result<WeatherContext> {
        if !self.enabled {
            return Err(anyhow!("Weather API is not enabled (no API key configured)"));
        }
        
        // If API key is placeholder, return sample data for demonstration
        if self.api_key.as_ref().map_or(true, |key| key == "your_openweathermap_api_key_here") {
            info!("Using demo weather data (API key not configured)");
            return Ok(WeatherContext {
                current_conditions: "Partly cloudy, 72°F (feels like 75°F), 65% humidity".to_string(),
                forecast: vec![
                    WeatherForecast {
                        date: Utc::now(),
                        temperature_high: 75.0,
                        temperature_low: 65.0,
                        conditions: "Partly Cloudy".to_string(),
                        precipitation_chance: 0.2,
                        description: "Pleasant weather with some clouds".to_string(),
                    },
                    WeatherForecast {
                        date: Utc::now() + chrono::Duration::days(1),
                        temperature_high: 78.0,
                        temperature_low: 68.0,
                        conditions: "Sunny".to_string(),
                        precipitation_chance: 0.1,
                        description: "Clear skies and warm temperatures".to_string(),
                    },
                    WeatherForecast {
                        date: Utc::now() + chrono::Duration::days(2),
                        temperature_high: 82.0,
                        temperature_low: 70.0,
                        conditions: "Thunderstorms".to_string(),
                        precipitation_chance: 0.8,
                        description: "Scattered thunderstorms in the afternoon".to_string(),
                    },
                ],
                alerts: vec![
                    "High chance of rain Saturday - bring an umbrella".to_string(),
                    "Thunderstorms expected Saturday - plan indoor activities".to_string(),
                ],
            });
        }
        
        // Fetch current weather and forecast in parallel
        let (current_result, forecast_result) = tokio::join!(
            self.fetch_current_weather(),
            self.fetch_forecast()
        );
        
        let current_weather = current_result?;
        let forecast_response = forecast_result?;
        
        // Convert forecast data
        let forecast = self.convert_forecast_items(forecast_response.list);
        
        // Generate current conditions string
        let current_conditions = if let Some(weather) = current_weather.weather.first() {
            format!(
                "{}, {} (feels like {}), {}% humidity",
                weather.description,
                self.format_temperature(current_weather.main.temp),
                self.format_temperature(current_weather.main.feels_like),
                current_weather.main.humidity
            )
        } else {
            format!("Current temperature: {}", self.format_temperature(current_weather.main.temp))
        };
        
        // Generate weather alerts
        let alerts = self.generate_weather_alerts(&current_weather, &forecast);
        
        info!("Weather data successfully fetched for {}", self.location);
        
        Ok(WeatherContext {
            current_conditions,
            forecast,
            alerts,
        })
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
    
    async fn fetch_context(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<ContextData> {
        info!("Fetching weather context for location: {}", self.location);
        
        let weather_context = self.fetch_weather_data(start, end).await?;
        
        Ok(ContextData {
            source_id: self.source_id().to_string(),
            timestamp: Utc::now(),
            data_type: ContextDataType::Weather,
            priority: 75, // Lower priority
            content: ContextContent::Weather(weather_context),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("location".to_string(), self.location.clone());
                metadata.insert("source_type".to_string(), "weather".to_string());
                metadata
            },
        })
    }
    
    fn priority(&self) -> i32 {
        75 // Lower priority
    }
    
    fn required_config(&self) -> Vec<String> {
        vec!["api_key".to_string(), "location".to_string()]
    }
}