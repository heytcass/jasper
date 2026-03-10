use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use crate::config::TravelConfig;
use crate::significance_engine::CalendarEventSummary;

/// Result of a travel time calculation for a single event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TravelTimeResult {
    pub duration_seconds: i64,
    pub duration_in_traffic_seconds: Option<i64>,
    pub distance_meters: i64,
    pub origin: String,
    pub destination: String,
    pub travel_mode: String,
}

/// Cache key: (origin_lower, destination_lower, travel_mode, departure_hour)
type CacheKey = (String, String, String, i64);

struct CachedRoute {
    result: TravelTimeResult,
    fetched_at: DateTime<Utc>,
}

/// Google Routes API response structures
#[derive(Debug, Deserialize)]
struct RoutesResponse {
    routes: Option<Vec<Route>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Route {
    /// Duration string like "165s"
    duration: Option<String>,
    /// Static duration (without traffic) like "150s"
    static_duration: Option<String>,
    /// Distance in meters
    distance_meters: Option<i64>,
}

pub struct TravelTimeService {
    api_key: String,
    home_address: String,
    travel_mode: String,
    lookahead_hours: u32,
    cache_duration_minutes: u32,
    client: reqwest::Client,
    cache: RwLock<HashMap<CacheKey, CachedRoute>>,
    /// Set to true if the API returns 403 (API not enabled) to avoid hammering
    api_disabled: AtomicBool,
}

impl TravelTimeService {
    pub fn new(config: &TravelConfig) -> Self {
        Self {
            api_key: config.google_api_key.clone(),
            home_address: config.home_address.clone(),
            travel_mode: config.travel_mode.clone(),
            lookahead_hours: config.lookahead_hours,
            cache_duration_minutes: config.cache_duration_minutes,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            cache: RwLock::new(HashMap::new()),
            api_disabled: AtomicBool::new(false),
        }
    }

    /// Calculate travel times for a batch of calendar events.
    /// Filters to only own-calendar events with physical locations within the lookahead window.
    /// Returns a map of event ID -> TravelTimeResult.
    pub async fn get_travel_times_for_events(
        &self,
        events: &[CalendarEventSummary],
    ) -> HashMap<String, TravelTimeResult> {
        if self.api_disabled.load(Ordering::Relaxed) {
            return HashMap::new();
        }

        let now = Utc::now();
        let lookahead_cutoff = now + chrono::Duration::hours(self.lookahead_hours as i64);

        // Filter to eligible events
        let eligible: Vec<_> = events
            .iter()
            .filter(|e| {
                e.is_own_calendar
                    && !e.is_all_day
                    && e.start_time > now
                    && e.start_time <= lookahead_cutoff
                    && e.location
                        .as_ref()
                        .is_some_and(|l| Self::is_physical_location(l))
            })
            .collect();

        if eligible.is_empty() {
            return HashMap::new();
        }

        debug!(
            "Calculating travel times for {} eligible events",
            eligible.len()
        );

        let mut map = HashMap::new();
        for event in eligible {
            let destination = event.location.as_ref().unwrap();
            match self
                .get_or_fetch(&self.home_address, destination, event.start_time)
                .await
            {
                Ok(tt) => {
                    debug!(
                        "Travel time to '{}': {} min",
                        tt.destination,
                        tt.duration_seconds / 60
                    );
                    map.insert(event.id.clone(), tt);
                }
                Err(e) => {
                    debug!("Failed to get travel time for event {}: {}", event.id, e);
                }
            }
        }

        map
    }

    /// Check cache, then call API if needed
    async fn get_or_fetch(
        &self,
        origin: &str,
        destination: &str,
        departure_time: DateTime<Utc>,
    ) -> anyhow::Result<TravelTimeResult> {
        // Round departure to nearest hour for cache key stability
        let departure_hour = departure_time.timestamp() / 3600;
        let cache_key = (
            origin.to_lowercase(),
            destination.to_lowercase(),
            self.travel_mode.clone(),
            departure_hour,
        );

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                let age = Utc::now() - cached.fetched_at;
                if age.num_minutes() < self.cache_duration_minutes as i64 {
                    debug!("Travel time cache hit for '{}'", destination);
                    return Ok(cached.result.clone());
                }
            }
        }

        // Cache miss — call the API
        let result = self
            .call_routes_api(origin, destination, departure_time)
            .await?;

        // Store in cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                cache_key,
                CachedRoute {
                    result: result.clone(),
                    fetched_at: Utc::now(),
                },
            );
        }

        Ok(result)
    }

    /// Call the Google Routes API computeRoutes endpoint
    async fn call_routes_api(
        &self,
        origin: &str,
        destination: &str,
        departure_time: DateTime<Utc>,
    ) -> anyhow::Result<TravelTimeResult> {
        let is_driving = self.travel_mode == "DRIVE";

        let mut body = serde_json::json!({
            "origin": { "address": origin },
            "destination": { "address": destination },
            "travelMode": self.travel_mode,
            "departureTime": departure_time.to_rfc3339(),
        });

        // routingPreference is only valid for DRIVE and TWO_WHEELER
        if is_driving {
            body.as_object_mut()
                .unwrap()
                .insert("routingPreference".into(), "TRAFFIC_AWARE".into());
        }

        // For driving, request both duration and staticDuration to compare traffic impact
        let field_mask = if is_driving {
            "routes.duration,routes.staticDuration,routes.distanceMeters"
        } else {
            "routes.duration,routes.distanceMeters"
        };

        let response = self
            .client
            .post("https://routes.googleapis.com/directions/v2:computeRoutes")
            .header("X-Goog-Api-Key", &self.api_key)
            .header("X-Goog-FieldMask", field_mask)
            .json(&body)
            .send()
            .await?;

        let status = response.status();

        if status == reqwest::StatusCode::FORBIDDEN {
            error!(
                "Google Routes API returned 403. Enable the Routes API at \
                 https://console.cloud.google.com/apis/library/routes.googleapis.com"
            );
            self.api_disabled.store(true, Ordering::Relaxed);
            anyhow::bail!("Routes API not enabled (403)");
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::BAD_REQUEST {
                debug!(
                    "Routes API could not resolve address '{}': {}",
                    destination, error_text
                );
            } else {
                warn!("Routes API error {}: {}", status, error_text);
            }
            anyhow::bail!("Routes API error {}", status);
        }

        let resp: RoutesResponse = response.json().await?;

        let route = resp
            .routes
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("No routes returned for '{}'", destination))?;

        let duration_seconds = parse_duration_string(route.duration.as_deref())?;
        let distance_meters = route.distance_meters.unwrap_or(0);

        // For driving: duration includes traffic, staticDuration is without traffic
        // We want: duration_seconds = traffic-aware, duration_in_traffic_seconds = same (traffic-aware)
        // The "base" without traffic is staticDuration
        let duration_in_traffic_seconds = if is_driving {
            // duration already includes traffic for DRIVE mode with TRAFFIC_AWARE
            Some(duration_seconds)
        } else {
            None
        };

        // For driving, use staticDuration as the "base" duration
        let base_duration = if is_driving {
            parse_duration_string(route.static_duration.as_deref()).unwrap_or(duration_seconds)
        } else {
            duration_seconds
        };

        Ok(TravelTimeResult {
            duration_seconds: base_duration,
            duration_in_traffic_seconds,
            distance_meters,
            origin: origin.to_string(),
            destination: destination.to_string(),
            travel_mode: self.travel_mode.clone(),
        })
    }

    /// Determine if a location string is a real physical address
    /// (as opposed to a URL, phone number, or "virtual" marker)
    fn is_physical_location(location: &str) -> bool {
        let trimmed = location.trim();
        if trimmed.is_empty() {
            return false;
        }

        let lower = trimmed.to_lowercase();

        // Reject URLs
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return false;
        }

        // Reject common video call patterns
        let video_patterns = [
            "zoom.us",
            "meet.google",
            "teams.microsoft",
            "webex",
            "gotomeeting",
            "whereby.com",
            "around.co",
        ];
        if video_patterns.iter().any(|p| lower.contains(p)) {
            return false;
        }

        // Reject virtual/remote markers
        let virtual_markers = [
            "virtual",
            "remote",
            "online",
            "tbd",
            "tba",
            "zoom",
            "teams",
            "google meet",
            "video call",
            "phone call",
            "dial-in",
            "call-in",
        ];
        if virtual_markers.iter().any(|m| lower == *m) {
            return false;
        }

        // Reject strings that are just phone numbers
        let digits_and_phone_chars: String = trimmed
            .chars()
            .filter(|c| {
                !c.is_ascii_digit() && *c != '-' && *c != '(' && *c != ')' && *c != '+' && *c != ' '
            })
            .collect();
        if digits_and_phone_chars.is_empty() && trimmed.len() >= 7 {
            return false;
        }

        true
    }

    /// Human-readable label for the travel mode
    pub fn travel_mode_label(&self) -> &str {
        match self.travel_mode.as_str() {
            "DRIVE" => "drive",
            "TRANSIT" => "transit",
            "WALK" => "walk",
            "BICYCLE" => "bike ride",
            _ => "travel",
        }
    }
}

/// Parse a Google Routes API duration string like "165s" into seconds
fn parse_duration_string(duration: Option<&str>) -> anyhow::Result<i64> {
    let s = duration.ok_or_else(|| anyhow::anyhow!("Missing duration field"))?;
    let s = s.trim_end_matches('s');
    s.parse::<i64>()
        .map_err(|e| anyhow::anyhow!("Failed to parse duration '{}': {}", s, e))
}
