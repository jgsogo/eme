use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BookUpdate {
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub isbn: Option<Option<String>>,
    pub publisher: Option<Option<String>>,
    pub publication_year: Option<Option<u16>>,
    pub catalogue_url: Option<Option<String>>,
    pub rating: Option<Option<u8>>,
    pub reading_status: Option<ReadingStatus>,
    pub want_to_buy: Option<bool>,
    pub review: Option<Option<String>>,
}
