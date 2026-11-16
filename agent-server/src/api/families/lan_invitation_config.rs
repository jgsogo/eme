use std::net::IpAddr;

/// Explicit LAN base URL used to build invitation links. Only private, loopback,
/// or link-local IP addresses are accepted; Host headers are never trusted.
#[derive(Debug, Clone)]
pub struct LanInvitationConfig {
    base_url: String,
}
