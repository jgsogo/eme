mod types;
use types::*;

use chrono::{DateTime, LocalResult, NaiveDate, TimeZone};
use chrono_tz::Tz;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

use agent_runtime::{Tool, ToolError};
use async_trait::async_trait;

const API_BASE_URL: &str = "https://www.googleapis.com/calendar/v3";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

impl GoogleCalendarClient {
    async fn from_environment() -> Result<Self, GoogleCalendarError> {
        let client_id = required_env("GOOGLE_OAUTH_CLIENT_ID")?;
        let client_secret = required_env("GOOGLE_OAUTH_CLIENT_SECRET")?;
        let refresh_token = required_env("GOOGLE_OAUTH_REFRESH_TOKEN")?;
        let client = Client::new();
        let response = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error_description")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(body);
            return Err(GoogleCalendarError::OAuth(format!(
                "HTTP {status}: {detail}"
            )));
        }
        let token: TokenResponse = serde_json::from_str(&body)?;
        Ok(Self {
            client,
            access_token: token.access_token,
            base_url: API_BASE_URL.to_owned(),
        })
    }

    #[cfg(test)]
    fn with_base_url(access_token: String, base_url: String) -> Self {
        Self {
            client: Client::new(),
            access_token,
            base_url,
        }
    }

    async fn events_for_day(
        &self,
        calendar_id: &str,
        date: &str,
        time_zone_override: Option<&str>,
    ) -> Result<Value, GoogleCalendarError> {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|error| GoogleCalendarError::InvalidTime(error.to_string()))?;
        let zone_name = match time_zone_override {
            Some(zone) => zone.to_owned(),
            None => self.calendar_time_zone(calendar_id).await?,
        };
        let zone: Tz = zone_name.parse().map_err(|_| {
            GoogleCalendarError::InvalidTime(format!("unknown IANA timezone '{zone_name}'"))
        })?;
        let next_day = date
            .succ_opt()
            .ok_or_else(|| GoogleCalendarError::InvalidTime("date is out of range".into()))?;
        let start = local_midnight(zone, date)?;
        let end = local_midnight(zone, next_day)?;
        self.events_for_bounds(calendar_id, &start, &end, Some(zone_name))
            .await
    }

    async fn events_for_period(
        &self,
        calendar_id: &str,
        time_min: &str,
        time_max: &str,
    ) -> Result<Value, GoogleCalendarError> {
        let start = DateTime::parse_from_rfc3339(time_min)
            .map_err(|error| GoogleCalendarError::InvalidTime(error.to_string()))?;
        let end = DateTime::parse_from_rfc3339(time_max)
            .map_err(|error| GoogleCalendarError::InvalidTime(error.to_string()))?;
        if start >= end {
            return Err(GoogleCalendarError::InvalidTime(
                "'time_min' must be earlier than 'time_max'".into(),
            ));
        }
        self.events_for_bounds(calendar_id, time_min, time_max, None)
            .await
    }

    async fn calendar_time_zone(&self, calendar_id: &str) -> Result<String, GoogleCalendarError> {
        let url = self.calendar_url(calendar_id, false)?;
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        let calendar: Calendar = self.decode_response(response).await?;
        Ok(calendar.time_zone)
    }

    fn calendar_url(&self, calendar_id: &str, events: bool) -> Result<Url, GoogleCalendarError> {
        let mut url = Url::parse(&format!("{}/", self.base_url.trim_end_matches('/')))
            .map_err(|error| GoogleCalendarError::InvalidTime(error.to_string()))?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| {
                GoogleCalendarError::InvalidTime("invalid Google Calendar API base URL".into())
            })?;
            segments.pop_if_empty().push("calendars").push(calendar_id);
            if events {
                segments.push("events");
            }
        }
        Ok(url)
    }

    async fn events_for_bounds(
        &self,
        calendar_id: &str,
        time_min: &str,
        time_max: &str,
        time_zone: Option<String>,
    ) -> Result<Value, GoogleCalendarError> {
        let url = self.calendar_url(calendar_id, true)?;
        let mut events = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .get(url.clone())
                .bearer_auth(&self.access_token)
                .query(&[
                    ("timeMin", time_min),
                    ("timeMax", time_max),
                    ("singleEvents", "true"),
                    ("orderBy", "startTime"),
                    ("maxResults", "2500"),
                ]);
            if let Some(token) = page_token.as_deref() {
                request = request.query(&[("pageToken", token)]);
            }
            let response = request.send().await?;
            let page: EventsPage = self.decode_response(response).await?;
            events.extend(page.items);
            page_token = page.next_page_token;
            if page_token.is_none() {
                break;
            }
        }
        Ok(json!({
            "calendar_id": calendar_id,
            "time_zone": time_zone,
            "time_min": time_min,
            "time_max": time_max,
            "event_count": events.len(),
            "events": events
        }))
    }

    async fn decode_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, GoogleCalendarError> {
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or(body);
            return Err(GoogleCalendarError::Api { status, detail });
        }
        Ok(serde_json::from_str(&body)?)
    }
}

fn is_calendar_allowed(calendar_id: &str, allowed_ids: &str) -> bool {
    !calendar_id.eq_ignore_ascii_case("primary")
        && allowed_ids
            .split(',')
            .map(str::trim)
            .any(|allowed_id| !allowed_id.is_empty() && allowed_id == calendar_id)
}

fn required_env(name: &str) -> Result<String, GoogleCalendarError> {
    std::env::var(name).map_err(|_| GoogleCalendarError::OAuth(format!("{name} env var not set")))
}

fn local_midnight(zone: Tz, date: NaiveDate) -> Result<String, GoogleCalendarError> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| GoogleCalendarError::InvalidTime("invalid local midnight".into()))?;
    let local = match zone.from_local_datetime(&midnight) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(first, _) => first,
        LocalResult::None => {
            return Err(GoogleCalendarError::InvalidTime(format!(
                "local midnight does not exist for {date} in {zone}"
            )));
        }
    };
    Ok(local.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{bearer_token, method, path, query_param},
    };

    #[test]
    fn calendar_allowlist_requires_an_exact_match_and_rejects_primary() {
        let allowed = "family@example.com, shared@example.com";
        assert!(is_calendar_allowed("family@example.com", allowed));
        assert!(is_calendar_allowed("shared@example.com", allowed));
        assert!(!is_calendar_allowed("other@example.com", allowed));
        assert!(!is_calendar_allowed("primary", allowed));
        assert!(!is_calendar_allowed("PRIMARY", allowed));
        assert!(!is_calendar_allowed("family@example.com.evil", allowed));
    }

    #[tokio::test]
    async fn gets_selected_calendar_day_and_follows_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/calendars/team@example.com"))
            .and(bearer_token("token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"timeZone":"Europe/Madrid"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/calendars/team@example.com/events"))
            .and(query_param("timeMin", "2026-11-14T00:00:00+01:00"))
            .and(query_param("timeMax", "2026-11-15T00:00:00+01:00"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items":[{"id":"one"}],"nextPageToken":"next"
            })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/calendars/team@example.com/events"))
            .and(query_param("pageToken", "next"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items":[{"id":"two"}]})))
            .mount(&server)
            .await;

        let client = GoogleCalendarClient::with_base_url("token".into(), server.uri());
        let events = client
            .events_for_day("team@example.com", "2026-11-14", None)
            .await
            .unwrap();
        assert_eq!(events["event_count"], 2);
        assert_eq!(events["calendar_id"], "team@example.com");
        assert_eq!(events["time_zone"], "Europe/Madrid");
    }

    #[tokio::test]
    async fn rejects_reversed_period() {
        let server = MockServer::start().await;
        let client = GoogleCalendarClient::with_base_url("token".into(), server.uri());
        let error = client
            .events_for_period(
                "calendar-id",
                "2026-11-15T00:00:00Z",
                "2026-11-14T00:00:00Z",
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("earlier"));
    }

    #[test]
    fn local_midnight_uses_daylight_saving_offset() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        assert_eq!(
            local_midnight("Europe/Madrid".parse().unwrap(), date).unwrap(),
            "2026-11-14T00:00:00+01:00"
        );
    }
}
