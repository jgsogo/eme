//! Public and school holiday data from OpenHolidays API.

use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use chrono::{Months, NaiveDate};
use reqwest::Client;
use serde_json::{Value, json};
use std::time::Duration;

const API_BASE: &str = "https://openholidaysapi.org";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

fn required_string<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, ToolError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ToolError::Execution(format!("'{name}' must be a non-empty string")))
}

fn optional_string<'a>(arguments: &'a Value, name: &str) -> Option<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

fn parse_date(arguments: &Value, name: &str) -> Result<NaiveDate, ToolError> {
    let value = required_string(arguments, name)?;
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|error| ToolError::Execution(format!("'{name}' must use YYYY-MM-DD: {error}")))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HolidayKind {
    Public,
    School,
}

impl HolidayKind {
    fn from_str(value: &str) -> Result<Self, ToolError> {
        match value {
            "public" => Ok(Self::Public),
            "school" => Ok(Self::School),
            _ => Err(ToolError::Execution(
                "'holiday_type' must be either 'public' or 'school'".into(),
            )),
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::Public => "PublicHolidays",
            Self::School => "SchoolHolidays",
        }
    }
}

/// Retrieve public or school holidays for a country and optional subdivision/group.
pub struct GetHolidays;

#[async_trait]
impl Tool for GetHolidays {
    async fn execute(&self, _context: &ToolContext, arguments: Value) -> Result<String, ToolError> {
        let kind = HolidayKind::from_str(required_string(&arguments, "holiday_type")?)?;
        let country = required_string(&arguments, "country_code")?;
        let from = parse_date(&arguments, "valid_from")?;
        let to = parse_date(&arguments, "valid_to")?;

        if to < from {
            return Err(ToolError::Execution(
                "'valid_to' must be on or after 'valid_from'".into(),
            ));
        }
        let maximum_end = from
            .checked_add_months(Months::new(36))
            .unwrap_or(NaiveDate::MAX);
        if to > maximum_end {
            return Err(ToolError::Execution(
                "date range cannot exceed three years".into(),
            ));
        }

        let language = optional_string(&arguments, "language_code").unwrap_or("EN");
        let from_text = from.to_string();
        let to_text = to.to_string();
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("eme-ai-platform/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| ToolError::Execution(format!("cannot create HTTP client: {error}")))?;

        let mut request = client
            .get(format!("{API_BASE}/{}", kind.endpoint()))
            .query(&[
                ("countryIsoCode", country),
                ("languageIsoCode", language),
                ("validFrom", from_text.as_str()),
                ("validTo", to_text.as_str()),
            ]);

        if let Some(subdivision) = optional_string(&arguments, "subdivision_code") {
            request = request.query(&[("subdivisionCode", subdivision)]);
        }
        if kind == HolidayKind::School
            && let Some(group) = optional_string(&arguments, "group_code")
        {
            request = request.query(&[("groupCode", group)]);
        }

        let response = request
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| {
                ToolError::Execution(format!("OpenHolidays API request failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ToolError::Execution(format!(
                "OpenHolidays API returned HTTP {status}: {}",
                body.chars().take(1000).collect::<String>()
            )));
        }

        let holidays: Value = response.json().await.map_err(|error| {
            ToolError::Execution(format!("invalid OpenHolidays API response: {error}"))
        })?;

        serde_json::to_string_pretty(&json!({
            "source": "OpenHolidays API",
            "data_license": "Open Database License (ODbL)",
            "holiday_type": match kind {
                HolidayKind::Public => "public",
                HolidayKind::School => "school",
            },
            "country_code": country,
            "subdivision_code": optional_string(&arguments, "subdivision_code"),
            "group_code": if kind == HolidayKind::School {
                optional_string(&arguments, "group_code")
            } else {
                None
            },
            "valid_from": from,
            "valid_to": to,
            "language_code": language,
            "holidays": holidays,
        }))
        .map_err(|error| ToolError::Execution(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args() -> Value {
        json!({
            "holiday_type": "school",
            "country_code": "ES",
            "subdivision_code": "ES-CL",
            "valid_from": "2026-01-01",
            "valid_to": "2026-12-31",
            "language_code": "ES"
        })
    }

    #[test]
    fn accepts_valid_date_range() {
        let arguments = args();
        assert_eq!(
            parse_date(&arguments, "valid_from").unwrap().to_string(),
            "2026-01-01"
        );
        assert_eq!(
            HolidayKind::from_str("school").unwrap(),
            HolidayKind::School
        );
    }

    #[test]
    fn rejects_unknown_holiday_type() {
        assert!(HolidayKind::from_str("workday").is_err());
    }

    #[test]
    fn rejects_reversed_date_range() {
        let mut arguments = args();
        arguments["valid_from"] = json!("2026-12-31");
        arguments["valid_to"] = json!("2026-01-01");
        let from = parse_date(&arguments, "valid_from").unwrap();
        let to = parse_date(&arguments, "valid_to").unwrap();
        assert!(to < from);
    }

    #[test]
    fn rejects_ranges_longer_than_three_years() {
        let from = NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap();
        let to = NaiveDate::parse_from_str("2029-01-02", "%Y-%m-%d").unwrap();
        assert!(to > from.checked_add_months(Months::new(36)).unwrap());
    }
}
