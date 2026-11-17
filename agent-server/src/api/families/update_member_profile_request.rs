use chrono::NaiveDate;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpdateMemberProfileRequest {
    pub name: String,
    pub birthday: Option<NaiveDate>,
}
