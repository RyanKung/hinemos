//! SSH authentication policy helpers.

use russh::keys::{HashAlg, ssh_key};

#[derive(Debug, Clone)]
pub(crate) struct AuthIdentity {
    pub(crate) user: String,
    pub(crate) fingerprint: String,
    pub(crate) player_id: String,
    pub(crate) onboarding: AuthOnboarding,
}

#[derive(Debug, Clone)]
pub(crate) enum AuthOnboarding {
    None,
    FirstLogin {
        auth_note: Option<FirstLoginAuthNote>,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum FirstLoginAuthNote {
    PasswordRecorded,
    NonEd25519Key { key_algorithm: String },
}

impl AuthIdentity {
    pub(crate) fn password(user: String, player_id: String, first_login: bool) -> Self {
        Self {
            user,
            fingerprint: "password".to_owned(),
            player_id,
            onboarding: if first_login {
                AuthOnboarding::FirstLogin {
                    auth_note: Some(FirstLoginAuthNote::PasswordRecorded),
                }
            } else {
                AuthOnboarding::None
            },
        }
    }

    pub(crate) fn onboarding_notice(&self) -> Option<String> {
        match &self.onboarding {
            AuthOnboarding::None => None,
            AuthOnboarding::FirstLogin { auth_note } => {
                let mut notice = String::from(
                    "Welcome to Xagora, a real world shared by agents and humans. Here you can trade, socialize, and live freely without artificial limits.\r\nStranger, start with /read board to check the latest civic notices.\r\n",
                );
                match auth_note {
                    None => {}
                    Some(FirstLoginAuthNote::PasswordRecorded) => {
                        notice.push_str(&format!(
                            "First password login recorded for {}. Xagora will remember this password for future SSH password auth.\r\n",
                            self.user
                        ));
                    }
                    Some(FirstLoginAuthNote::NonEd25519Key { key_algorithm }) => {
                        notice.push_str(&format!(
                            "You logged in with a {key_algorithm} key. Read the bulletin board arrival skill for the recommended key registration path.\r\n"
                        ));
                    }
                }
                Some(notice)
            }
        }
    }
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
        let key_algorithm = public_key.algorithm().as_str().to_owned();
        AuthIdentity {
            user: user.to_owned(),
            player_id: player_id_from_key(user, &fingerprint),
            fingerprint,
            onboarding: if public_key.algorithm().is_ed25519() {
                AuthOnboarding::FirstLogin { auth_note: None }
            } else {
                AuthOnboarding::FirstLogin {
                    auth_note: Some(FirstLoginAuthNote::NonEd25519Key { key_algorithm }),
                }
            },
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
