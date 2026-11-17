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

    /// List calendars visible to the caller without decrypting any credentials.
    ///
    /// The optional family ID must come from trusted ToolContext. Family roster
    /// membership is resolved live, so stale shares for departed owners vanish.
    pub async fn list_readable_calendars(
        &self,
        user_id: &str,
        family_id: Option<&str>,
    ) -> Result<Vec<agent_storage::ConnectedCalendar>, GoogleCalendarAccessError> {
        let user =
            UserId::from_str(user_id).map_err(|_| GoogleCalendarAccessError::InvalidIdentity)?;
        let (family, owners) = match family_id {
            Some(value) => {
                let family = FamilyId::from_str(value)
                    .map_err(|_| GoogleCalendarAccessError::NotAuthorized)?;
                let context = self
                    .families
                    .context_for_member(user, family)
                    .await
                    .map_err(map_family_error)?;
                let owners = context
                    .members
                    .iter()
                    .map(|member| UserId::from_str(&member.user_id))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| GoogleCalendarAccessError::FamilyUnavailable)?;
                (Some(family), owners)
            }
            None => (None, vec![user]),
        };

        let mut calendars = Vec::new();
        for owner in owners {
            let connections = self
                .connections
                .list_connections(owner)
                .await
                .map_err(|error| GoogleCalendarAccessError::Storage(error.to_string()))?;
            for connection in connections.into_iter().filter(|connection| {
                connection.provider == "google"
                    && connection.status == ExternalConnectionStatus::Active
            }) {
                let connected = self
                    .calendars
                    .list_for_connection(owner, connection.id)
                    .await
                    .map_err(|error| GoogleCalendarAccessError::Storage(error.to_string()))?;
                calendars.extend(connected.into_iter().filter(|calendar| {
                    calendar.selected
                        && (calendar.owner_id == user
                            || (family.is_some()
                                && calendar.family_id == family
                                && calendar.family_access.is_some()))
                }));
            }
        }
        calendars.sort_by(|left, right| {
            left.display_name
                .as_deref()
                .unwrap_or("")
                .cmp(right.display_name.as_deref().unwrap_or(""))
                .then_with(|| left.id.to_string().cmp(&right.id.to_string()))
        });
        Ok(calendars)
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
        let user =
            UserId::from_str(user_id).map_err(|_| GoogleCalendarAccessError::InvalidIdentity)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_storage::{
        CalendarAccessMode, CalendarFamilyAccess, ConnectedCalendarStore, DiscoveredCalendar,
        ExternalConnectionStore, FamilyInvitationStore, FamilyRelationship,
        InMemoryFamilyInvitationStore, InMemoryFamilyStore, NewExternalConnection,
        SqliteConnectedCalendarStore, SqliteExternalConnectionStore,
    };

    #[tokio::test]
    async fn owner_and_current_family_members_can_read_but_removed_members_cannot() {
        let path =
            std::env::temp_dir().join(format!("eme-google-access-{}.sqlite", uuid::Uuid::new_v4()));
        let owner = UserId::new();
        let member = UserId::new();
        let connections = Arc::new(SqliteExternalConnectionStore::open(&path, [0x42; 32]).unwrap());
        let calendars = Arc::new(SqliteConnectedCalendarStore::open(&path).unwrap());
        let connection = connections
            .create_connection(NewExternalConnection {
                owner_id: owner,
                provider: "google".into(),
                provider_subject: "google-subject".into(),
                display_email: None,
                refresh_token: "owner-refresh-token".into(),
                granted_scopes: vec![
                    "https://www.googleapis.com/auth/calendar.events.readonly".into(),
                ],
            })
            .await
            .unwrap();
        let discovered = calendars
            .sync_discovered(
                owner,
                connection.id,
                vec![DiscoveredCalendar {
                    provider_calendar_id: "private-calendar@example.com".into(),
                    display_name: Some("Private".into()),
                    time_zone: Some("Europe/Madrid".into()),
                    provider_access_role: Some("owner".into()),
                }],
            )
            .await
            .unwrap()
            .remove(0);
        calendars
            .update(
                owner,
                discovered.id,
                Some(true),
                Some(CalendarAccessMode::ReadOnly),
            )
            .await
            .unwrap();

        let family_store = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(family_store.clone()));
        let family_service = Arc::new(FamilyService::new(
            family_store,
            invitations as Arc<dyn FamilyInvitationStore>,
        ));
        let family = family_service
            .create_family(owner, "Home", FamilyRelationship::Other)
            .await
            .unwrap();
        family_service
            .add_managed_member(owner, family.id, member, FamilyRelationship::Other)
            .await
            .unwrap();
        calendars
            .share_with_family(
                owner,
                discovered.id,
                family.id,
                CalendarFamilyAccess::Viewer,
            )
            .await
            .unwrap();

        let service = GoogleCalendarAccessService::new(
            calendars.clone(),
            connections.clone(),
            family_service.clone(),
        );
        let calendar_id = discovered.id.to_string();
        let family_id = family.id.to_string();
        let owner_access = service
            .authorize_read(&owner.to_string(), &calendar_id, None)
            .await
            .unwrap();
        assert_eq!(owner_access.refresh_token, "owner-refresh-token");
        let family_access = service
            .authorize_read(&member.to_string(), &calendar_id, Some(&family_id))
            .await
            .unwrap();
        assert_eq!(
            family_access.calendar.family_access,
            Some(CalendarFamilyAccess::Viewer)
        );
        let visible_to_member = service
            .list_readable_calendars(&member.to_string(), Some(&family_id))
            .await
            .unwrap();
        assert_eq!(visible_to_member.len(), 1);
        assert_eq!(visible_to_member[0].id, discovered.id);
        assert!(
            service
                .list_readable_calendars(&member.to_string(), None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            service
                .authorize_read(&member.to_string(), &calendar_id, None)
                .await,
            Err(GoogleCalendarAccessError::NotAuthorized)
        ));

        family_service
            .remove_member(owner, family.id, member)
            .await
            .unwrap();
        assert!(matches!(
            service
                .authorize_read(&member.to_string(), &calendar_id, Some(&family_id))
                .await,
            Err(GoogleCalendarAccessError::NotAuthorized)
        ));
        assert!(matches!(
            service
                .list_readable_calendars(&member.to_string(), Some(&family_id))
                .await,
            Err(GoogleCalendarAccessError::NotAuthorized)
        ));

        drop(service);
        drop(calendars);
        drop(connections);
        let _ = std::fs::remove_file(path);
    }
}
