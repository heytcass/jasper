use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc, Duration};
use oauth2::{
    ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    AuthUrl, TokenUrl, TokenResponse,
    basic::BasicClient, 
    RefreshToken,
    reqwest::async_http_client,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::{info, debug, warn};
use uuid::Uuid;

use crate::database::Event;
use crate::http_utils::{handle_google_api_response, handle_oauth2_response_with_text, parse_json_response};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleCalendarConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub calendar_ids: Vec<String>, // Primary, work, family calendars
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    // token_type field ignored - not needed for our implementation
}

#[derive(Debug, Deserialize)]
struct GoogleCalendarList {
    items: Option<Vec<GoogleCalendarListEntry>>,
}

#[derive(Debug, Deserialize)]
struct GoogleCalendarListEntry {
    id: Option<String>,
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleEventsResponse {
    items: Option<Vec<GoogleEvent>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GoogleEvent {
    id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    start: Option<GoogleEventDateTime>,
    end: Option<GoogleEventDateTime>,
    status: Option<String>,
    attendees: Option<Vec<GoogleEventAttendee>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GoogleEventDateTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    date: Option<String>,
    #[serde(rename = "timeZone")]
    time_zone: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GoogleEventAttendee {
    email: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

pub struct GoogleCalendarService {
    config: GoogleCalendarConfig,
    token_file_path: PathBuf,
    http_client: reqwest::Client,
}

impl GoogleCalendarService {
    pub fn new(config: GoogleCalendarConfig, data_dir: PathBuf) -> Self {
        let token_file_path = data_dir.join("google_calendar_token.json");
        
        Self {
            config,
            token_file_path,
            http_client: reqwest::Client::new(),
        }
    }

    /// Check if we have valid authentication
    pub async fn is_authenticated(&self) -> bool {
        if let Ok(token) = self.load_stored_token().await {
            if let Some(expires_at) = token.expires_at {
                // Check if token is still valid (expires in the future)
                Utc::now() < expires_at
            } else {
                // No expiry means it's likely a long-lived token
                true
            }
        } else {
            false
        }
    }

    /// Get OAuth2 authorization URL for initial setup
    pub fn get_auth_url(&self) -> Result<(String, CsrfToken)> {
        let client = BasicClient::new(
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
            AuthUrl::new("https://accounts.google.com/o/oauth2/auth".to_string())?,
            Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?),
        )
        .set_redirect_uri(RedirectUrl::new(self.config.redirect_uri.clone())?);

        let (auth_url, csrf_token) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("https://www.googleapis.com/auth/calendar.readonly".to_string()))
            .url();

        Ok((auth_url.to_string(), csrf_token))
    }

    /// Exchange authorization code for access token
    pub async fn authenticate_with_code(&mut self, auth_code: &str, _csrf_token: &str) -> Result<()> {
        debug!("Exchanging authorization code for access token using manual token exchange");
        
        // Manual token exchange to avoid oauth2 crate parsing issues
        let token_response = self.exchange_code_manually(auth_code).await?;

        let stored_token = StoredToken {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at: token_response.expires_in.map(|seconds| Utc::now() + Duration::seconds(seconds)),
            scopes: vec!["https://www.googleapis.com/auth/calendar.readonly".to_string()],
        };

        self.store_token(&stored_token).await?;
        
        info!("Google Calendar authentication successful with custom REST API client");
        Ok(())
    }

    /// Refresh expired access token
    async fn refresh_token(&self, current_token: &StoredToken) -> Result<StoredToken> {
        let refresh_token = current_token.refresh_token.as_ref()
            .ok_or_else(|| anyhow!("No refresh token available"))?;

        let client = BasicClient::new(
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
            AuthUrl::new("https://accounts.google.com/o/oauth2/auth".to_string())?,
            Some(TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?),
        );

        let token_result = client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.clone()))
            .request_async(async_http_client)
            .await
            .map_err(|e| anyhow!("Failed to refresh token: {}", e))?;

        let new_token = StoredToken {
            access_token: token_result.access_token().secret().clone(),
            refresh_token: Some(refresh_token.clone()), // Keep existing refresh token
            expires_at: token_result.expires_in().map(|d| Utc::now() + Duration::seconds(d.as_secs() as i64)),
            scopes: current_token.scopes.clone(),
        };

        self.store_token(&new_token).await?;
        info!("Google Calendar token refreshed successfully");
        Ok(new_token)
    }

    /// Fetch events from Google Calendar using direct REST API
    pub async fn fetch_events(&mut self, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> Result<Vec<(String, Vec<Event>)>> {
        let token = self.get_valid_token().await?;
        let mut events_by_calendar = Vec::new();

        // Fetch from each configured calendar
        for calendar_id in &self.config.calendar_ids {
            debug!("Fetching events from calendar: {}", calendar_id);
            
            match self.fetch_calendar_events_rest(&token.access_token, calendar_id, start_time, end_time).await {
                Ok(events) => {
                    info!("Fetched {} events from calendar {}", events.len(), calendar_id);
                    events_by_calendar.push((calendar_id.clone(), events));
                }
                Err(e) => {
                    warn!("Failed to fetch events from calendar {}: {}", calendar_id, e);
                    // Continue with other calendars
                }
            }
        }

        let total_events: usize = events_by_calendar.iter().map(|(_, events)| events.len()).sum();
        info!("Total events fetched from Google Calendar: {}", total_events);
        Ok(events_by_calendar)
    }

    /// Get a valid access token, refreshing if necessary
    async fn get_valid_token(&self) -> Result<StoredToken> {
        let token = self.load_stored_token().await?;
        
        // Check if token needs refresh
        if let Some(expires_at) = token.expires_at {
            if Utc::now() + Duration::minutes(5) >= expires_at {
                debug!("Access token expired, refreshing...");
                return self.refresh_token(&token).await;
            }
        }
        
        Ok(token)
    }

    /// Fetch calendar events using Google Calendar REST API directly
    async fn fetch_calendar_events_rest(
        &self,
        access_token: &str,
        calendar_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events",
            urlencoding::encode(calendar_id)
        );

        let response = self.http_client
            .get(&url)
            .bearer_auth(access_token)
            .query(&[
                ("timeMin", &start_time.to_rfc3339()),
                ("timeMax", &end_time.to_rfc3339()),
                ("singleEvents", &"true".to_string()),
                ("orderBy", &"startTime".to_string()),
                ("maxResults", &"250".to_string()),
            ])
            .send()
            .await
            .map_err(|e| anyhow!("Google Calendar API request failed: {}", e))?;

        let response = handle_google_api_response(response).await?;
        let events_response: GoogleEventsResponse = parse_json_response(response, "Google Calendar events response").await?;

        let mut converted_events = Vec::new();

        if let Some(items) = events_response.items {
            for gcal_event in items {
                if let Ok(event) = self.convert_google_event_rest(gcal_event, calendar_id).await {
                    converted_events.push(event);
                }
            }
        }

        Ok(converted_events)
    }

    async fn convert_google_event_rest(
        &self,
        gcal_event: GoogleEvent,
        _google_calendar_id: &str,
    ) -> Result<Event> {
        // Serialize the raw data first before we start moving fields
        let raw_data = serde_json::to_string(&gcal_event)?;
        
        let source_id = gcal_event.id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
        let title = gcal_event.summary.clone();
        let description = gcal_event.description.clone();
        let location = gcal_event.location.clone();

        // Detect if this is an all-day event
        let is_all_day = gcal_event.start.as_ref()
            .map(|start| start.date.is_some() && start.date_time.is_none())
            .unwrap_or(false);

        // Handle start time
        let start_time = if let Some(start) = &gcal_event.start {
            if let Some(datetime_str) = &start.date_time {
                DateTime::parse_from_rfc3339(datetime_str)
                    .map_err(|e| anyhow!("Invalid start datetime: {}", e))?
                    .with_timezone(&Utc)
            } else if let Some(date_str) = &start.date {
                // All-day event - parse date and set to start of day in local timezone
                // For all-day events, we want to preserve the local date, not convert to UTC midnight
                let naive_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| anyhow!("Invalid start date: {}", e))?;
                let naive_datetime = naive_date.and_hms_opt(0, 0, 0)
                    .ok_or_else(|| anyhow!("Invalid date"))?;
                // Store as naive UTC to preserve the date boundary
                DateTime::from_naive_utc_and_offset(naive_datetime, Utc)
            } else {
                return Err(anyhow!("Event has no start time"));
            }
        } else {
            return Err(anyhow!("Event has no start time"));
        };

        // Handle end time
        let end_time = if let Some(end) = &gcal_event.end {
            if let Some(datetime_str) = &end.date_time {
                Some(DateTime::parse_from_rfc3339(datetime_str)
                    .map_err(|e| anyhow!("Invalid end datetime: {}", e))?
                    .with_timezone(&Utc))
            } else if let Some(date_str) = &end.date {
                // All-day event ends at end of day in local timezone
                let naive_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                    .map_err(|e| anyhow!("Invalid end date: {}", e))?;
                let naive_datetime = naive_date.and_hms_opt(23, 59, 59)
                    .ok_or_else(|| anyhow!("Invalid date"))?;
                Some(DateTime::from_naive_utc_and_offset(naive_datetime, Utc))
            } else {
                None
            }
        } else {
            None
        };

        let participants = if let Some(attendees) = &gcal_event.attendees {
            let attendee_emails: Vec<String> = attendees.iter()
                .filter_map(|a| a.email.clone())
                .collect();
            if !attendee_emails.is_empty() {
                Some(serde_json::to_string(&attendee_emails)?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Event {
            id: 0, // Will be set by database
            source_id,
            calendar_id: 0, // Will be set when storing in database
            title,
            description,
            start_time: start_time.timestamp(),
            end_time: end_time.map(|t| t.timestamp()),
            location,
            event_type: Some("google_calendar".to_string()),
            participants,
            raw_data_json: Some(raw_data),
            is_all_day: Some(is_all_day),
        })
    }

    async fn store_token(&self, token: &StoredToken) -> Result<()> {
        let token_json = serde_json::to_string_pretty(token)?;
        fs::write(&self.token_file_path, token_json).await?;
        debug!("Stored Google Calendar token to {:?}", self.token_file_path);
        Ok(())
    }

    async fn load_stored_token(&self) -> Result<StoredToken> {
        let token_data = fs::read_to_string(&self.token_file_path).await?;
        let token: StoredToken = serde_json::from_str(&token_data)?;
        Ok(token)
    }

    /// Get calendar metadata for database storage
    pub async fn get_calendar_metadata(&mut self, calendar_id: &str) -> Result<(String, String, Option<String>)> {
        let token = self.get_valid_token().await?;

        let response = self.http_client
            .get(&format!("https://www.googleapis.com/calendar/v3/calendars/{}", urlencoding::encode(calendar_id)))
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| anyhow!("Google Calendar API request failed: {}", e))?;

        let response = handle_google_api_response(response).await?;
        let calendar_info: GoogleCalendarListEntry = parse_json_response(response, "Google Calendar response").await?;

        Ok((
            calendar_id.to_string(),
            calendar_info.summary.unwrap_or_else(|| calendar_id.to_string()),
            None, // We'll infer color from the calendar ID patterns
        ))
    }

    /// Get calendar list for configuration using direct REST API
    pub async fn list_calendars(&mut self) -> Result<Vec<(String, String)>> {
        let token = self.get_valid_token().await?;

        let response = self.http_client
            .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| anyhow!("Google Calendar API request failed: {}", e))?;

        let response = handle_google_api_response(response).await?;
        let calendar_list: GoogleCalendarList = parse_json_response(response, "Google Calendar list response").await?;

        let mut calendars = Vec::new();
        if let Some(items) = calendar_list.items {
            for calendar in items {
                if let (Some(id), Some(summary)) = (calendar.id, calendar.summary) {
                    calendars.push((id, summary));
                }
            }
        }

        Ok(calendars)
    }

    /// Manual token exchange to avoid oauth2 crate JSON parsing issues
    async fn exchange_code_manually(&self, auth_code: &str) -> Result<GoogleTokenResponse> {
        debug!("Manual token exchange called with auth_code length: {}", auth_code.len());
        let client = reqwest::Client::new();
        
        let params = [
            ("client_id", &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("code", &auth_code.to_string()),
            ("grant_type", &"authorization_code".to_string()),
            ("redirect_uri", &self.config.redirect_uri),
        ];

        debug!("Sending token exchange request to Google OAuth2 endpoint");
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

        // Handle OAuth2 response and get text for debugging
        let response_text = handle_oauth2_response_with_text(response).await?;
        debug!("Raw Google token response: {}", response_text);

        let token_response: GoogleTokenResponse = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse token response: {} - Raw response: {}", e, response_text))?;

        debug!("Token exchange successful, received access token");
        Ok(token_response)
    }
}

impl Default for GoogleCalendarConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: "http://localhost:8080/auth/callback".to_string(),
            calendar_ids: vec!["primary".to_string()],
        }
    }
}