/// Protocol-neutral authenticated actor identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIdentity {
    /// Stable username.
    pub user: String,
    /// Stable player/agent id.
    pub player_id: String,
}

/// Protocol-neutral context needed to execute semantic business commands.
#[derive(Debug, Clone, Copy)]
pub struct AppCommandContext<'a> {
    /// Current runtime view id.
    pub current_view: &'a str,
    /// Optional configured mail domain for player-facing addresses.
    pub mail_domain: Option<&'a str>,
    /// One-time token supplied by the protocol adapter for commands that rotate secrets.
    pub generated_token: &'a str,
}

impl AppIdentity {
    /// Builds a protocol-neutral identity.
    #[must_use]
    pub fn new(user: impl Into<String>, player_id: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            player_id: player_id.into(),
        }
    }
}
