use agent_runtime::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
pub struct CurrentDateTime;
pub struct GetTimezone;
pub struct ConvertTimezone;
pub struct CalculateDateDifference;
pub struct GetDateInfo;
pub struct ParseDateTime;
fn output(v: Value) -> Result<String, ToolError> {
    serde_json::to_string_pretty(&v).map_err(|e| ToolError::Execution(e.to_string()))
}
fn req<'a>(a: &'a Value, k: &str) -> Result<&'a str, ToolError> {
    a.get(k)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::Execution(format!("'{k}' must be a non-empty string")))
}
fn tz(s: &str) -> Result<Tz, ToolError> {
    s.parse()
        .map_err(|_| ToolError::Execution(format!("unknown IANA timezone '{s}'")))
}
fn naive(s: &str) -> Result<NaiveDateTime, ToolError> {
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Ok(d);
        }
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        .map_err(|e| ToolError::Execution(format!("invalid datetime '{s}': {e}")))
}
fn instant(s: &str, z: Tz) -> Result<DateTime<Utc>, ToolError> {
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Ok(d.with_timezone(&Utc));
    }
    z.from_local_datetime(&naive(s)?)
        .single()
        .map(|d| d.with_timezone(&Utc))
        .ok_or_else(|| {
            ToolError::Execution(format!(
                "local datetime ambiguous or nonexistent in {}",
                z.name()
            ))
        })
}
#[async_trait]
impl Tool for CurrentDateTime {
    async fn execute(&self, _context: &ToolContext, _: Value) -> Result<String, ToolError> {
        let n = Utc::now();
        output(
            json!({"datetime":n.to_rfc3339_opts(SecondsFormat::Millis,true),"timezone":"UTC","unix_timestamp":n.timestamp(),"unix_timestamp_millis":n.timestamp_millis()}),
        )
    }
}
#[async_trait]
impl Tool for GetTimezone {
    async fn execute(&self, _context: &ToolContext, a: Value) -> Result<String, ToolError> {
        let z = tz(a.get("timezone").and_then(Value::as_str).unwrap_or("UTC"))?;
        let n = Utc::now().with_timezone(&z);
        output(
            json!({"timezone":z.name(),"datetime":n.to_rfc3339(),"utc_offset":n.offset().to_string(),"abbreviation":n.format("%Z").to_string()}),
        )
    }
}
#[async_trait]
impl Tool for ConvertTimezone {
    async fn execute(&self, _context: &ToolContext, a: Value) -> Result<String, ToolError> {
        let f = tz(req(&a, "from_timezone")?)?;
        let t = tz(req(&a, "to_timezone")?)?;
        let d = instant(req(&a, "datetime")?, f)?.with_timezone(&t);
        output(
            json!({"datetime":d.to_rfc3339_opts(SecondsFormat::Millis,false),"timezone":t.name(),"utc_offset":d.offset().to_string(),"unix_timestamp":d.timestamp()}),
        )
    }
}
#[async_trait]
impl Tool for CalculateDateDifference {
    async fn execute(&self, _context: &ToolContext, a: Value) -> Result<String, ToolError> {
        let s = instant(req(&a, "start")?, chrono_tz::UTC)?;
        let e = instant(req(&a, "end")?, chrono_tz::UTC)?;
        let n = e.signed_duration_since(s).num_seconds();
        output(
            json!({"start":s.to_rfc3339(),"end":e.to_rfc3339(),"difference_seconds":n,"difference_days":n as f64/86400.0,"calendar_days":e.date_naive().signed_duration_since(s.date_naive()).num_days(),"absolute_seconds":n.unsigned_abs()}),
        )
    }
}
#[async_trait]
impl Tool for GetDateInfo {
    async fn execute(&self, _context: &ToolContext, a: Value) -> Result<String, ToolError> {
        let d = NaiveDate::parse_from_str(req(&a, "date")?, "%Y-%m-%d")
            .map_err(|e| ToolError::Execution(format!("date must use YYYY-MM-DD: {e}")))?;
        let w = d.iso_week();
        let next = if d.month() == 12 {
            NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
        }
        .unwrap();
        let leap = d.year() % 4 == 0 && (d.year() % 100 != 0 || d.year() % 400 == 0);
        output(
            json!({"date":d.to_string(),"weekday":d.format("%A").to_string(),"weekday_number_monday":d.weekday().number_from_monday(),"day_of_year":d.ordinal(),"iso_week":w.week(),"iso_week_year":w.year(),"month":d.month(),"month_name":d.format("%B").to_string(),"days_in_month":(next-d).num_days()+d.day() as i64-1,"year":d.year(),"leap_year":leap,"days_in_year":if leap{366}else{365}}),
        )
    }
}
#[async_trait]
impl Tool for ParseDateTime {
    async fn execute(&self, _context: &ToolContext, a: Value) -> Result<String, ToolError> {
        let s = req(&a, "value")?;
        let z = tz(a.get("timezone").and_then(Value::as_str).unwrap_or("UTC"))?;
        let d = if let Some(f) = a.get("format").and_then(Value::as_str) {
            let n = NaiveDateTime::parse_from_str(s, f)
                .or_else(|_| {
                    NaiveDate::parse_from_str(s, f).map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                })
                .map_err(|e| ToolError::Execution(format!("cannot parse '{s}' with '{f}': {e}")))?;
            z.from_local_datetime(&n)
                .single()
                .map(|d| d.with_timezone(&Utc))
                .ok_or_else(|| {
                    ToolError::Execution("local datetime ambiguous or nonexistent".into())
                })?
        } else {
            instant(s, z)?
        };
        let l = d.with_timezone(&z);
        output(
            json!({"input":s,"datetime":l.to_rfc3339_opts(SecondsFormat::Millis,false),"date":l.date_naive().to_string(),"timezone":z.name(),"unix_timestamp":d.timestamp(),"unix_timestamp_millis":d.timestamp_millis()}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn timezone_conversion() {
        let v:Value=serde_json::from_str(&ConvertTimezone.execute(&ToolContext::default(), json!({"datetime":"2026-01-15T12:00:00","from_timezone":"Europe/Madrid","to_timezone":"America/New_York"})).await.unwrap()).unwrap();
        assert_eq!(v["datetime"], "2026-01-15T06:00:00.000-05:00");
    }
    #[tokio::test]
    async fn leap_day_difference() {
        let v: Value = serde_json::from_str(
            &CalculateDateDifference
                .execute(
                    &ToolContext::default(),
                    json!({"start":"2024-02-28","end":"2024-03-01"}),
                )
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(v["calendar_days"], 2);
    }
    #[tokio::test]
    async fn calendar_metadata() {
        let v: Value = serde_json::from_str(
            &GetDateInfo
                .execute(&ToolContext::default(), json!({"date":"2024-02-29"}))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(v["leap_year"], true);
        assert_eq!(v["day_of_year"], 60);
    }
    #[tokio::test]
    async fn custom_parse() {
        assert!(ParseDateTime.execute(&ToolContext::default(), json!({"value":"15/01/2026 12:30","format":"%d/%m/%Y %H:%M","timezone":"Europe/Madrid"})).await.is_ok());
    }
}
