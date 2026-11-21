//! Invitation typeRaw bearer token

use super::InvitationStorageError;
use sha2::{Digest, Sha256};

/// Raw bearer token. Its Debug implementation deliberately never reveals it.
#[derive(Clone, PartialEq, Eq)]
pub struct InvitationToken(String);

impl InvitationToken {
    pub(crate) fn generate() -> Self {
        // Two independent UUIDv4 values provide 244 bits of CSPRNG-backed entropy.
        Self(format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ))
    }

    pub(crate) fn hash(&self) -> String {
        use std::fmt::Write as _;

        let digest = Sha256::digest(self.0.as_bytes());
        let mut hash = String::with_capacity(digest.len() * 2);
        for byte in digest.iter() {
            // Writing to a String is infallible.
            write!(&mut hash, "{byte:02x}").expect("writing to String cannot fail");
        }
        hash
    }

    /// Parse a token received from an untrusted transport. Tokens are fixed-length
    /// lowercase/uppercase hexadecimal strings; malformed values are rejected
    /// before any identity provisioning takes place.
    pub fn parse(value: &str) -> Result<Self, InvitationStorageError> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(InvitationStorageError::NotFound);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for InvitationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InvitationToken([REDACTED])")
    }
}
