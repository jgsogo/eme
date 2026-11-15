use super::Device;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct DevicesResponse {
    pub(crate) devices: Vec<Device>,
}
