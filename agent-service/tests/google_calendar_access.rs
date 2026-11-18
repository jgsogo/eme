//! Regression tests for the Google Calendar authorization boundary.

use std::sync::Arc;

use agent_storage::{
    CalendarAccessMode, CalendarFamilyAccess, ConnectedCalendarId, ConnectedCalendarStore,
    DiscoveredCalendar, ExternalConnectionId, ExternalConnectionStore, FamilyInvitationStore,
    FamilyRelationship, InMemoryFamilyInvitationStore, InMemoryFamilyStore, NewExternalConnection,
    SqliteConnectedCalendarStore, SqliteExternalConnectionStore, UserId,
};
use uuid::Uuid;

use agent_service::{FamilyService, GoogleCalendarAccessError, GoogleCalendarAccessService};

const READ_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events.readonly";
const WRITE_SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";

struct Fixture {
    path: std::path::PathBuf,
    owner: UserId,
    member: UserId,
    connection_id: ExternalConnectionId,
    calendar_id: ConnectedCalendarId,
    family_id: agent_storage::FamilyId,
    connections: Arc<SqliteExternalConnectionStore>,
    calendars: Arc<SqliteConnectedCalendarStore>,
    families: Arc<FamilyService>,
    access: GoogleCalendarAccessService,
}

impl Fixture {
    async fn new(scopes: Vec<String>, access_mode: CalendarAccessMode) -> Self {
        let path = std::env::temp_dir().join(format!(
            "eme-google-auth-regression-{}.sqlite",
            Uuid::new_v4()
        ));
        let owner = UserId::new();
        let member = UserId::new();
        let connections = Arc::new(SqliteExternalConnectionStore::open(&path, [0x42; 32]).unwrap());
        let calendars = Arc::new(SqliteConnectedCalendarStore::open(&path).unwrap());
        let connection = connections
            .create_connection(NewExternalConnection {
                owner_id: owner,
                provider: "google".into(),
                provider_subject: "google-subject".into(),
                display_email: Some("owner@example.com".into()),
                refresh_token: "owner-refresh-token".into(),
                granted_scopes: scopes,
            })
            .await
            .unwrap();
        let calendar = calendars
            .sync_discovered(
                owner,
                connection.id,
                vec![DiscoveredCalendar {
                    provider_calendar_id: "private-calendar@example.com".into(),
                    display_name: Some("Private calendar".into()),
                    time_zone: Some("Europe/Madrid".into()),
                    provider_access_role: Some("owner".into()),
                }],
            )
            .await
            .unwrap()
            .remove(0);
        calendars
            .update(owner, calendar.id, Some(true), Some(access_mode))
            .await
            .unwrap();

        let family_store = Arc::new(InMemoryFamilyStore::default());
        let invitations = Arc::new(InMemoryFamilyInvitationStore::new(family_store.clone()));
        let families = Arc::new(FamilyService::new(
            family_store,
            invitations as Arc<dyn FamilyInvitationStore>,
        ));
        let family = families
            .create_family(owner, "Home", FamilyRelationship::Other)
            .await
            .unwrap();
        families
            .add_managed_member(owner, family.id, member, FamilyRelationship::Other)
            .await
            .unwrap();

        let access = GoogleCalendarAccessService::new(
            calendars.clone(),
            connections.clone(),
            families.clone(),
        );
        Self {
            path,
            owner,
            member,
            connection_id: connection.id,
            calendar_id: calendar.id,
            family_id: family.id,
            connections,
            calendars,
            families,
            access,
        }
    }

    fn calendar_id(&self) -> String {
        self.calendar_id.to_string()
    }

    fn family_id(&self) -> String {
        self.family_id.to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[tokio::test]
async fn another_users_calendar_cannot_be_accessed_without_a_matching_family_share() {
    let fixture = Fixture::new(
        vec![READ_SCOPE.into(), WRITE_SCOPE.into()],
        CalendarAccessMode::ReadWrite,
    )
    .await;
    let calendar_id = fixture.calendar_id();

    assert!(matches!(
        fixture
            .access
            .authorize_read(&fixture.member.to_string(), &calendar_id, None)
            .await,
        Err(GoogleCalendarAccessError::NotAuthorized)
    ));
    assert!(matches!(
        fixture
            .access
            .authorize_read(
                &fixture.member.to_string(),
                &calendar_id,
                Some(&Uuid::new_v4().to_string()),
            )
            .await,
        Err(GoogleCalendarAccessError::NotAuthorized)
    ));

    let other_family = fixture
        .families
        .create_family(fixture.owner, "Other", FamilyRelationship::Other)
        .await
        .unwrap();
    fixture
        .families
        .add_managed_member(
            fixture.owner,
            other_family.id,
            fixture.member,
            FamilyRelationship::Other,
        )
        .await
        .unwrap();
    fixture
        .calendars
        .share_with_family(
            fixture.owner,
            fixture.calendar_id,
            fixture.family_id,
            CalendarFamilyAccess::Viewer,
        )
        .await
        .unwrap();

    // Being a member of another family does not authorize a share from this one.
    assert!(matches!(
        fixture
            .access
            .authorize_read(
                &fixture.member.to_string(),
                &calendar_id,
                Some(&other_family.id.to_string()),
            )
            .await,
        Err(GoogleCalendarAccessError::NotAuthorized)
    ));
}

#[tokio::test]
async fn deselection_and_unsharing_take_effect_on_the_next_authorization() {
    let fixture = Fixture::new(
        vec![READ_SCOPE.into(), WRITE_SCOPE.into()],
        CalendarAccessMode::ReadWrite,
    )
    .await;
    let calendar_id = fixture.calendar_id();
    let family_id = fixture.family_id();

    fixture
        .calendars
        .share_with_family(
            fixture.owner,
            fixture.calendar_id,
            fixture.family_id,
            CalendarFamilyAccess::Viewer,
        )
        .await
        .unwrap();
    assert!(
        fixture
            .access
            .authorize_read(&fixture.member.to_string(), &calendar_id, Some(&family_id))
            .await
            .is_ok()
    );

    fixture
        .calendars
        .unshare_from_family(fixture.owner, fixture.calendar_id)
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .access
            .authorize_read(&fixture.member.to_string(), &calendar_id, Some(&family_id))
            .await,
        Err(GoogleCalendarAccessError::NotAuthorized)
    ));

    fixture
        .calendars
        .share_with_family(
            fixture.owner,
            fixture.calendar_id,
            fixture.family_id,
            CalendarFamilyAccess::Viewer,
        )
        .await
        .unwrap();
    fixture
        .calendars
        .update(fixture.owner, fixture.calendar_id, Some(false), None)
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .access
            .authorize_read(&fixture.member.to_string(), &calendar_id, Some(&family_id))
            .await,
        Err(GoogleCalendarAccessError::NotFound)
    ));
    assert!(matches!(
        fixture
            .access
            .authorize_read(&fixture.owner.to_string(), &calendar_id, None)
            .await,
        Err(GoogleCalendarAccessError::NotFound)
    ));
}

#[tokio::test]
async fn google_scopes_are_intersected_with_eme_read_write_permissions() {
    let fixture = Fixture::new(
        vec![READ_SCOPE.into(), WRITE_SCOPE.into()],
        CalendarAccessMode::ReadWrite,
    )
    .await;
    let calendar_id = fixture.calendar_id();

    assert!(
        fixture
            .access
            .authorize_write(&fixture.owner.to_string(), &calendar_id, None)
            .await
            .is_ok()
    );

    fixture
        .connections
        .update_credentials(
            fixture.owner,
            fixture.connection_id,
            None,
            vec![READ_SCOPE.into()],
        )
        .await
        .unwrap();

    assert!(
        fixture
            .access
            .authorize_read(&fixture.owner.to_string(), &calendar_id, None)
            .await
            .is_ok()
    );
    assert!(matches!(
        fixture
            .access
            .authorize_write(&fixture.owner.to_string(), &calendar_id, None)
            .await,
        Err(GoogleCalendarAccessError::InsufficientWriteScopes)
    ));
}

#[tokio::test]
async fn disconnecting_the_google_connection_revokes_calendar_access_immediately() {
    let fixture = Fixture::new(vec![READ_SCOPE.into()], CalendarAccessMode::ReadOnly).await;
    let calendar_id = fixture.calendar_id();

    assert!(
        fixture
            .access
            .authorize_read(&fixture.owner.to_string(), &calendar_id, None)
            .await
            .is_ok()
    );
    fixture
        .connections
        .revoke_connection(fixture.owner, fixture.connection_id)
        .await
        .unwrap();

    assert!(matches!(
        fixture
            .access
            .authorize_read(&fixture.owner.to_string(), &calendar_id, None)
            .await,
        Err(GoogleCalendarAccessError::NotFound)
    ));
    assert!(
        fixture
            .access
            .list_readable_calendars(&fixture.owner.to_string(), None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn supports_multiple_google_accounts_and_independent_calendar_permissions() {
    let fixture = Fixture::new(
        vec![READ_SCOPE.into(), WRITE_SCOPE.into()],
        CalendarAccessMode::ReadWrite,
    )
    .await;

    let second_connection = fixture
        .connections
        .create_connection(NewExternalConnection {
            owner_id: fixture.owner,
            provider: "google".into(),
            provider_subject: "second-google-subject".into(),
            display_email: Some("second@example.com".into()),
            refresh_token: "second-refresh-token".into(),
            granted_scopes: vec![READ_SCOPE.into(), WRITE_SCOPE.into()],
        })
        .await
        .unwrap();

    let second_calendars = fixture
        .calendars
        .sync_discovered(
            fixture.owner,
            second_connection.id,
            vec![
                DiscoveredCalendar {
                    provider_calendar_id: "readonly@example.com".into(),
                    display_name: Some("Second account read-only".into()),
                    time_zone: Some("Europe/Madrid".into()),
                    provider_access_role: Some("owner".into()),
                },
                DiscoveredCalendar {
                    provider_calendar_id: "writable@example.com".into(),
                    display_name: Some("Second account writable".into()),
                    time_zone: Some("Europe/Madrid".into()),
                    provider_access_role: Some("owner".into()),
                },
            ],
        )
        .await
        .unwrap();

    let read_only = fixture
        .calendars
        .update(
            fixture.owner,
            second_calendars[0].id,
            Some(true),
            Some(CalendarAccessMode::ReadOnly),
        )
        .await
        .unwrap();
    let read_write = fixture
        .calendars
        .update(
            fixture.owner,
            second_calendars[1].id,
            Some(true),
            Some(CalendarAccessMode::ReadWrite),
        )
        .await
        .unwrap();

    let visible = fixture
        .access
        .list_readable_calendars(&fixture.owner.to_string(), None)
        .await
        .unwrap();
    assert_eq!(visible.len(), 3);
    assert!(
        visible
            .iter()
            .any(|calendar| calendar.id == fixture.calendar_id)
    );
    assert!(visible.iter().any(|calendar| calendar.id == read_only.id));
    assert!(visible.iter().any(|calendar| calendar.id == read_write.id));

    assert!(matches!(
        fixture
            .access
            .authorize_write(&fixture.owner.to_string(), &read_only.id.to_string(), None,)
            .await,
        Err(GoogleCalendarAccessError::WriteNotAuthorized)
    ));
    assert!(
        fixture
            .access
            .authorize_write(&fixture.owner.to_string(), &read_write.id.to_string(), None,)
            .await
            .is_ok()
    );
}
