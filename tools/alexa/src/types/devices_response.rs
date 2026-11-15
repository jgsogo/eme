use super::super::*;

#[derive(Debug, Deserialize)]
pub(super) struct DevicesResponse {
    pub(super) devices: Vec<Device>,
}
