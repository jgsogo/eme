use super::Device;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct DevicesResponse {
    pub(super) devices: Vec<Device>,
}
