//! Google Calendar list discovery for an individual OAuth connection.
use crate::{GoogleOAuthConfig, GoogleOAuthError, refresh_access_token};
use serde::Deserialize;

const CALENDAR_LIST_URL: &str = "https://www.googleapis.com/calendar/v3/users/me/calendarList";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListPage {
    #[serde(default)]
    items: Vec<GoogleCalendarSummary>,
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleCalendarSummary {
    pub id: String,
    pub summary: Option<String>,
    pub time_zone: Option<String>,
    pub access_role: Option<String>,
}

/// Fetch every calendar visible to the connected Google account.
///
/// The refresh token is supplied by the server-side connection store. It must
/// never be passed to browser clients or agent-facing tools.
pub async fn list_calendars(
    config: &GoogleOAuthConfig,
    refresh_token: &str,
) -> Result<Vec<GoogleCalendarSummary>, GoogleOAuthError> {
    let token = refresh_access_token(config, refresh_token).await?;
    let client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;

    let mut calendars = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut request = client
            .get(CALENDAR_LIST_URL)
            .bearer_auth(&token.access_token)
            .query(&[("maxResults", "250")]);
        if let Some(token) = page_token.as_deref() {
            request = request.query(&[("pageToken", token)]);
        }
        let response = request.send().await
            .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
        let status = response.status();
        let body = response.text().await
            .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
        if !status.is_success() {
            let detail = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| value.pointer("/error/message").and_then(serde_json::Value::as_str).map(str::to_owned))
                .unwrap_or(body);
            return Err(GoogleOAuthError::Request(format!("Google Calendar API returned {status}: {detail}")));
        }
        let page: CalendarListPage = serde_json::from_str(&body)
            .map_err(|error| GoogleOAuthError::Request(error.to_string()))?;
        calendars.extend(page.items);
        page_token = page.next_page_token;
        if page_token.is_none() { break; }
    }
    Ok(calendars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_calendar_list_metadata() {
        let page: CalendarListPage = serde_json::from_value(serde_json::json!({
            "items": [{
                "id": "primary",
                "summary": "Personal",
                "timeZone": "Europe/Madrid",
                "accessRole": "owner"
            }],
            "nextPageToken": "next"
        })).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "primary");
        assert_eq!(page.items[0].time_zone.as_deref(), Some("Europe/Madrid"));
        assert_eq!(page.items[0].access_role.as_deref(), Some("owner"));
        assert_eq!(page.next_page_token.as_deref(), Some("next"));
    }
}
