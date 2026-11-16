//! Invitation records, token handling, and persistence contract.

mod family_invitation;
mod family_invitation_store;
mod family_invitation_summary;
mod invitation_storage_error;
mod invitation_target_identity;
mod invitation_token;
#[cfg(test)]
mod token_tests {

    use super::*;

    #[test]
    fn parses_only_fixed_length_hexadecimal_tokens() {
        let token = InvitationToken::generate();
        assert_eq!(InvitationToken::parse(token.expose()).unwrap(), token);
        assert!(InvitationToken::parse("not-a-token").is_err());
        assert!(InvitationToken::parse(&token.expose()[..63]).is_err());
        assert!(InvitationToken::parse(&format!("{}z", &token.expose()[..63])).is_err());
    }
}


pub use family_invitation::FamilyInvitation;
pub use family_invitation_store::FamilyInvitationStore;
pub use family_invitation_summary::FamilyInvitationSummary;
pub use invitation_storage_error::InvitationStorageError;
pub use invitation_target_identity::InvitationTargetIdentity;
pub use invitation_token::InvitationToken;
