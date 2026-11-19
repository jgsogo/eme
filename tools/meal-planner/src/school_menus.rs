//! Date-keyed, manually managed school lunch menu domain model.
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchoolMenuStatus {
    Known,
    NotPublished,
    Closed,
    Unknown,
}
impl SchoolMenuStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Known => "known",
            Self::NotPublished => "not_published",
            Self::Closed => "closed",
            Self::Unknown => "unknown",
        }
    }
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "known" => Some(Self::Known),
            "not_published" => Some(Self::NotPublished),
            "closed" => Some(Self::Closed),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuItem {
    pub name: String,
    pub notes: Option<String>,
    pub allergens: Option<Vec<String>>,
    pub dietary_alternatives: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuDraft {
    pub date: NaiveDate,
    pub status: SchoolMenuStatus,
    pub items: Vec<SchoolMenuItem>,
    pub notes: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchoolMenuEntry {
    pub id: i64,
    #[serde(flatten)]
    pub draft: SchoolMenuDraft,
    pub created_at: i64,
    pub updated_at: i64,
}
impl SchoolMenuDraft {
    pub(crate) fn validate(mut self) -> Result<Self, String> {
        match self.status {
            SchoolMenuStatus::Known if self.items.is_empty() => {
                return Err("a school menu with status `known` must contain at least one item".into());
            }
            // A closure is authoritative: discard any menu items supplied by
            // the caller so an existing menu is cleared when this draft is saved.
            SchoolMenuStatus::Closed => self.items.clear(),
            SchoolMenuStatus::NotPublished | SchoolMenuStatus::Unknown
                if !self.items.is_empty() =>
            {
                return Err(format!(
                    "a school menu with status `{}` cannot contain menu items; set `items` to an empty array ([]). Menu items are only valid when status is `known`",
                    self.status.as_str()
                ));
            }
            _ => {}
        }
        for item in &mut self.items {
            item.name = item.name.trim().to_owned();
            if item.name.is_empty() {
                return Err("school menu item name must not be empty".into());
            }
            item.notes = clean(item.notes.take());
            item.allergens = item.allergens.take().map(normalize);
            item.dietary_alternatives = normalize(std::mem::take(&mut item.dietary_alternatives));
        }
        self.notes = clean(self.notes.take());
        Ok(self)
    }
}
fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}
fn normalize(v: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for s in v {
        let s = s.trim();
        if !s.is_empty() && !out.iter().any(|x: &String| x.eq_ignore_ascii_case(s)) {
            out.push(s.to_owned())
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 2).unwrap()
    }

    #[test]
    fn menu_states_are_explicit() {
        assert!(
            SchoolMenuDraft {
                date: date(),
                status: SchoolMenuStatus::Known,
                items: vec![],
                notes: None
            }
            .validate()
            .is_err()
        );
        for status in [
            SchoolMenuStatus::Unknown,
            SchoolMenuStatus::NotPublished,
            SchoolMenuStatus::Closed,
        ] {
            assert!(
                SchoolMenuDraft {
                    date: date(),
                    status,
                    items: vec![],
                    notes: None
                }
                .validate()
                .is_ok()
            )
        }
    }

    #[test]
    fn closing_school_discards_supplied_menu_items_and_preserves_notes() {
        let draft = SchoolMenuDraft {
            date: date(),
            status: SchoolMenuStatus::Closed,
            items: vec![SchoolMenuItem {
                name: "Festivo / Sin servicio de comedor".into(),
                notes: None,
                allergens: None,
                dietary_alternatives: vec![],
            }],
            notes: Some("Festivo".into()),
        };

        let normalized = draft.validate().unwrap();

        assert!(normalized.items.is_empty());
        assert_eq!(normalized.notes.as_deref(), Some("Festivo"));
    }

    #[test]
    fn non_known_status_with_items_returns_actionable_error() {
        let error = SchoolMenuDraft {
            date: date(),
            status: SchoolMenuStatus::NotPublished,
            items: vec![SchoolMenuItem {
                name: "Some dish".into(),
                notes: None,
                allergens: None,
                dietary_alternatives: vec![],
            }],
            notes: None,
        }
        .validate()
        .unwrap_err();

        assert!(error.contains("set `items` to an empty array ([])"));
        assert!(error.contains("status is `known`"));
    }
}
