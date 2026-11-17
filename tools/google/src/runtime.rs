//! Runtime dependencies for user-scoped Google Calendar tools.

use std::sync::Arc;

use agent_service::GoogleCalendarAccessService;
use agent_storage::ExternalConnectionId;

use crate::{GoogleOAuthConfig, GoogleOAuthError, GoogleOAuthTokenManager};

/// Server-composed configuration for Google Calendar tool execution.
///
/// Contains secrets and trusted services; never serialize this value or expose
/// it to the model.
#[derive(Clone)]
pub struct GoogleCalendarRuntimeConfig {
    pub(crate) oauth: GoogleOAuthConfig,
    pub(crate) access: Arc<GoogleCalendarAccessService>,
    token_manager: Arc<GoogleOAuthTokenManager>,
}

impl GoogleCalendarRuntimeConfig {
    pub fn new(
        oauth: GoogleOAuthConfig,
        access: Arc<GoogleCalendarAccessService>,
        token_manager: Arc<GoogleOAuthTokenManager>,
    ) -> Self {
        Self { oauth, access, token_manager }
    }

    /// Refresh credentials through the shared per-connection coordinator.
    pub(crate) async fn refresh_access_token(
        &self,
        connection_id: ExternalConnectionId,
        refresh_token: &str,
    ) -> Result<String, GoogleOAuthError> {
        self.token_manager
            .refresh_access_token(connection_id, &self.oauth, refresh_token)
            .await
    }
}
