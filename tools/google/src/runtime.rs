//! Runtime dependencies for user-scoped Google Calendar tools.

use std::sync::Arc;

use agent_service::GoogleCalendarAccessService;

use crate::GoogleOAuthConfig;

/// Server-composed configuration for Google Calendar tool execution.
///
/// Contains secrets and trusted services; never serialize this value or expose
/// it to the model.
#[derive(Clone)]
pub struct GoogleCalendarRuntimeConfig {
    pub(crate) oauth: GoogleOAuthConfig,
    pub(crate) access: Arc<GoogleCalendarAccessService>,
}

impl GoogleCalendarRuntimeConfig {
    pub fn new(
        oauth: GoogleOAuthConfig,
        access: Arc<GoogleCalendarAccessService>,
    ) -> Self {
        Self { oauth, access }
    }
}
