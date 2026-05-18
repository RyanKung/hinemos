//! SSH authentication policy helpers.

use russh::keys::{HashAlg, ssh_key};

#[derive(Debug, Clone)]
pub(crate) struct AuthIdentity {
    pub(crate) user: String,
    pub(crate) fingerprint: String,
    pub(crate) player_id: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PublicKeyAuthPolicy;

impl PublicKeyAuthPolicy {
    pub(crate) fn accepts_public_key_offer(
        &self,
        _user: &str,
        _public_key: &ssh_key::PublicKey,
    ) -> bool {
        true
    }

    pub(crate) fn authorize(&self, user: &str, public_key: &ssh_key::PublicKey) -> AuthIdentity {
        let fingerprint = public_key.fingerprint(HashAlg::Sha256).to_string();
        AuthIdentity {
            user: user.to_owned(),
            player_id: player_id_from_key(user, &fingerprint),
            fingerprint,
        }
    }
}

fn player_id_from_key(user: &str, fingerprint: &str) -> String {
    format!("ssh_{}_{}", sanitize_id(user), sanitize_id(fingerprint))
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
