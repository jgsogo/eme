use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Device {
    #[serde(rename = "accountName")]
    pub(crate) account_name: String,
    #[serde(rename = "deviceType")]
    pub(crate) device_type: String,
    #[serde(rename = "serialNumber")]
    pub(crate) serial_number: String,
    #[serde(rename = "deviceFamily")]
    #[allow(dead_code)]
    pub(crate) device_family: Option<String>,
}
