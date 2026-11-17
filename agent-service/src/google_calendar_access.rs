//! Resolves per-invocation read access to owner- or family-shared calendars.

use std::{str::FromStr, sync::Arc};

use agent_storage::{
    CalendarFamilyAccess, ConnectedCalendarId, ConnectedCalendarStore, ExternalConnectionStatus,
    ExternalConnectionStore, FamilyId, UserId,
};

use crate::{FamilyService, FamilyServiceError};

use super::{AuthorizedGoogleCalendar, GoogleCalendarAccessError};

/// Checks current EME permissions and resolves the owner's credential for a read.
pub struct GoogleCalendarAccessService {
    calendars: Arc<dyn ConnectedCalendarStore>,
    connections: Arc<dyn ExternalConnectionStore>,
    families: Arc<FamilyService>,
}

impl GoogleCalendarAccessService {
    pub fn new(
        calendars: Arc<dyn ConnectedCalendarStore>,
        connections: Arc<dyn ExternalConnectionStore>,
        families: Arc<FamilyService>,
    ) -> Self {
        Self {
            calendars,
            connections,
            families,
        }
    }

    /// Authorize a read using trusted caller and selected-family context.
    ///
    /// The user and family identifiers must come from ToolContext, never from
    /// tool arguments. The model supplies only the opaque EME calendar ID.
    /// Family membership is revalidated on every invocation.
    pub async fn authorize_read(
        &self,
        user_id: &str,
        calendar_id: &str,
        family_id: Option<&str>,
    ) -> Result<AuthorizedGoogleCalendar, GoogleCalendarAccessError> {
        let user = UserId::from_str(user_id)
            .map_err(|_| GoogleCalendarAccessError::InvalidIdentity)?;
        let calendar_id = ConnectedCalendarId::from_str(calendar_id)
            .map_err(|_| GoogleCalendarAccessError::NotFound)?;

        let calendar = self
            .calendars
            .get_selected(calendar_id)
            .await
            .map_err(|error| GoogleCalendarAccessError::Storage(error.to_string()))?
            .ok_or(GoogleCalendarAccessError::NotFound)?;

        if calendar.owner_id != user {
            let family_id = family_id
                .and_then(|value| FamilyId::from_str(value).ok())
                .ok_or(GoogleCalendarAccessError::NotAuthorized)?;
            if calendar.family_id != Some(family_id)
                || !matches!(
                    calendar.family_access,
                    Some(CalendarFamilyAccess::Viewer | CalendarFamilyAccess::Contributor)
                )
            {
                return Err(GoogleCalendarAccessError::NotAuthorized);
            }

            // Both sides of the share must still belong to this family. This
            // prevents stale shares from surviving either member's departure.
            self.families
                .context_for_member(user, family_id)
                .await
                .map_err(map_family_error)?;
            self.families
                .context_for_member(calendar.owner_id, family_id)
                .await
                .map_err(map_family_error)?;
        }

        let connection = self
            .connections
            .get_connection(calendar.owner_id, calendar.connection_id)
            .await
            .map_err(|error| GoogleCalendarAccessError::Storage(error.to_string()))?
            .filter(|connection| {
                connection.provider == "google"
                    && connection.status == ExternalConnectionStatus::Active
            })
            .ok_or(GoogleCalendarAccessError::NotFound)?;

        let has_read_scope = connection.granted_scopes.iter().any(|scope| {
            matches!(
                scope.as_str(),
                "https://www.googleapis.com/auth/calendar.events.readonly"
                    | "https://www.googleapis.com/auth/calendar.events"
                    | "https://www.googleapis.com/auth/calendar"
            )
        });
        if !has_read_scope {
            return Err(GoogleCalendarAccessError::InsufficientScopes);
        }

        let refresh_token = self
            .connections
            .refresh_token(calendar.owner_id, calendar.connection_id)
            .await
            .map_err(|error| GoogleCalendarAccessError::Storage(error.to_string()))?;

        Ok(AuthorizedGoogleCalendar {
            calendar,
            refresh_token,
        })
    }
}

fn map_family_error(error: FamilyServiceError) -> GoogleCalendarAccessError {
    match error {
        FamilyServiceError::NotFound | FamilyServiceError::NotMember => {
            GoogleCalendarAccessError::NotAuthorized
        }
        _ => GoogleCalendarAccessError::FamilyUnavailable,
    }
}
