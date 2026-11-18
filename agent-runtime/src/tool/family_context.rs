//! Trusted family membership details resolved by the host platform.

use chrono::NaiveDate;
use serde::Serialize;

/// One member of the family selected for a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyMemberContext {
    /// Stable EME user identifier exposed as the model-facing `recipient_id`.
    #[serde(rename = "recipient_id")]
    pub user_id: String,
    /// Display name from the platform identity record, when available.
    pub name: Option<String>,
    /// Calendar date of birth from the member's EME user profile, when available.
    pub birthday: Option<NaiveDate>,
    /// Telegram account identifier linked to this EME user, when available.
    #[serde(skip)]
    pub telegram_id: Option<String>,
    /// Platform permission within the family (owner, admin, member).
    pub role: String,
    /// Family relationship (father, mother, son, daughter, etc.).
    pub relationship: String,
}

/// Trusted family information resolved by the platform before a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyContext {
    pub family_id: String,
    pub family_name: String,
    /// The authenticated user's permission within this family.
    pub family_role: String,
    /// The authenticated user's relationship within this family.
    pub relationship: String,
    /// Complete active family roster.
    pub members: Vec<FamilyMemberContext>,
}

#[cfg(test)]
mod tests {
    use super::FamilyMemberContext;

    #[test]
    fn serializes_recipient_id_without_exposing_telegram_identity() {
        let member = FamilyMemberContext {
            user_id: "internal-user-id".to_owned(),
            name: Some("Javier".to_owned()),
            birthday: None,
            telegram_id: Some("123456789".to_owned()),
            role: "owner".to_owned(),
            relationship: "father".to_owned(),
        };

        let value = serde_json::to_value(member).unwrap();

        assert_eq!(value["recipient_id"], "internal-user-id");
        assert!(value.get("user_id").is_none());
        assert!(value.get("telegram_id").is_none());
    }
}
