use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct Device {
    #[serde(rename = "accountName")]
    pub(super) account_name: String,
    #[serde(rename = "deviceType")]
    pub(super) device_type: String,
    #[serde(rename = "serialNumber")]
    pub(super) serial_number: String,
    #[serde(rename = "deviceFamily")]
    #[allow(dead_code)]
    pub(super) device_family: Option<String>,
}
