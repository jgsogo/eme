use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    Pending,
    Reading,
    Read,
}

impl ReadingStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Reading => "reading",
            Self::Read => "read",
        }
    }
}

impl TryFrom<&str> for ReadingStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pending" => Ok(Self::Pending),
            "reading" => Ok(Self::Reading),
            "read" => Ok(Self::Read),
            _ => Err(format!("invalid reading status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub publication_year: Option<u16>,
    pub catalogue_url: Option<String>,
    pub rating: Option<u8>,
    pub reading_status: ReadingStatus,
    pub want_to_buy: bool,
    pub review: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookInput {
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    pub publisher: Option<String>,
    pub publication_year: Option<u16>,
    pub catalogue_url: Option<String>,
    pub rating: Option<u8>,
    #[serde(default = "default_status")]
    pub reading_status: ReadingStatus,
    #[serde(default)]
    pub want_to_buy: bool,
    pub review: Option<String>,
}

fn default_status() -> ReadingStatus {
    ReadingStatus::Pending
}

fn deserialize_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookUpdate {
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub isbn: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub publisher: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub publication_year: Option<Option<u16>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub catalogue_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub rating: Option<Option<u8>>,
    pub reading_status: Option<ReadingStatus>,
    pub want_to_buy: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_nullable")]
    pub review: Option<Option<String>>,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_distinguishes_missing_fields_from_explicit_null() {
        let missing: BookUpdate = serde_json::from_str(r#"{"title":"Updated"}"#).unwrap();
        assert_eq!(missing.isbn, None);

        let clear: BookUpdate = serde_json::from_str(r#"{"isbn":null,"review":null}"#).unwrap();
        assert_eq!(clear.isbn, Some(None));
        assert_eq!(clear.review, Some(None));

        let set: BookUpdate = serde_json::from_str(r#"{"isbn":"9788420412146"}"#).unwrap();
        assert_eq!(set.isbn, Some(Some("9788420412146".into())));
    }
}
