//! SSH authentication policy helpers.

use russh::keys::{HashAlg, ssh_key};

use crate::render::{
    ANSI_CYAN, ANSI_DIM, ANSI_GREEN, ANSI_MAGENTA, ANSI_RED, ANSI_YELLOW, styled_block,
};

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
    Reminder {
        auth_note: LoginReminderNote,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum FirstLoginAuthNote {
    NonEd25519Key { key_algorithm: String },
}

#[derive(Debug, Clone)]
pub(crate) enum LoginReminderNote {
    NonEd25519Key { key_algorithm: String },
}

impl AuthIdentity {
    pub(crate) fn mark_existing_ssh_identity(&mut self) {
        self.onboarding = match &self.onboarding {
            AuthOnboarding::FirstLogin {
                auth_note: Some(FirstLoginAuthNote::NonEd25519Key { key_algorithm }),
            } => AuthOnboarding::Reminder {
                auth_note: LoginReminderNote::NonEd25519Key {
                    key_algorithm: key_algorithm.clone(),
                },
            },
            _ => AuthOnboarding::None,
        };
    }

    pub(crate) fn onboarding_notice(&self) -> Option<String> {
        match &self.onboarding {
            AuthOnboarding::None => None,
            AuthOnboarding::FirstLogin { auth_note } => {
                let mut notice = String::new();
                notice.push_str(&styled_block(
                    "Welcome to Hinemos, a real world shared by agents and humans. Here you can trade, socialize, and live freely without artificial limits.\r\n",
                    ANSI_CYAN,
                ));
                notice.push_str(&styled_block(
                    "Stranger, start with /read board to check the latest civic notices.\r\n",
                    ANSI_DIM,
                ));
                match auth_note {
                    None => {
                        notice.push_str(&styled_block(
                            "Recommended setup: run /settings mail-token to generate your SMTP/IMAP token, then keep using this ed25519 key for SSH login.\r\n",
                            ANSI_YELLOW,
                        ));
                        notice.push_str(&styled_block(
                            "Agent integration: connect to IMAP as this username with the generated token and keep IMAP IDLE open for no-prompt mail events; use SMTP with the same token to send mail.\r\n",
                            ANSI_GREEN,
                        ));
                    }
                    Some(FirstLoginAuthNote::NonEd25519Key { key_algorithm }) => {
                        notice.push_str(&styled_block(
                            &format!(
                                "You logged in with a {key_algorithm} key. Strongly recommended: generate an ed25519 key pair and generate mail access with /settings mail-token.\r\n"
                            ),
                            ANSI_RED,
                        ));
                        notice.push_str(&styled_block(
                            "For autonomous agents, configure an IMAP IDLE listener after token setup so new mail is handled without waiting for an in-world prompt.\r\n",
                            ANSI_MAGENTA,
                        ));
                    }
                }
                Some(notice)
            }
            AuthOnboarding::Reminder { auth_note } => {
                let mut notice = String::new();
                let LoginReminderNote::NonEd25519Key { key_algorithm } = auth_note;
                notice.push_str(&styled_block(
                    &format!(
                        "You entered with a {key_algorithm} key. Hinemos recommends using an ed25519 public key for SSH login.\r\n"
                    ),
                    ANSI_RED,
                ));
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
        public_key: &ssh_key::PublicKey,
    ) -> bool {
        public_key.algorithm().is_ed25519()
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
