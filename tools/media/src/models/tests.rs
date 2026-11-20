//! Invariants for provider-independent media domain types.
use agent_storage::FamilyId;

use super::{MediaProfile, MediaProfileId, ProfilePreferences, WatchedPolicy};

#[test]
fn managed_media_profile_does_not_require_an_eme_user() {
    let profile = MediaProfile::new(FamilyId::new(), "Child");
    assert!(profile.user_id.is_none());
    assert!(profile.active);
}

#[test]
fn profile_preferences_are_explicit_and_bound_to_a_profile() {
    let profile_id = MediaProfileId::new();
    let preferences = ProfilePreferences::new(profile_id);
    assert_eq!(preferences.profile_id, profile_id);
    assert!(preferences.liked_genres.is_empty());
    assert!(preferences.disliked_genres.is_empty());
}

#[test]
fn watched_policy_defaults_to_any_participant_and_has_stable_wire_names() {
    assert_eq!(WatchedPolicy::default(), WatchedPolicy::AnyParticipant);
    assert_eq!(
        serde_json::to_string(&WatchedPolicy::AnyParticipant).unwrap(),
        "\"any_participant\""
    );
    assert_eq!(
        serde_json::to_string(&WatchedPolicy::AllParticipants).unwrap(),
        "\"all_participants\""
    );
    assert_eq!(
        serde_json::to_string(&WatchedPolicy::IncludeAll).unwrap(),
        "\"include_all\""
    );
}
