use crate::*;

/// Protocol-neutral event emitted by application handlers.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// Plain text intended for human clients.
    Text(String),
    /// Structured world observation.
    Observation(JsonObservation),
    /// Structured world observation produced by executing one command.
    CommandObservation {
        /// Command that produced the observation.
        command: SemanticCommand,
        /// Resulting observation.
        observation: JsonObservation,
    },
    /// Persist a player state and update protocol presence.
    PersistPlayerState(PlayerState),
    /// Prompt should be rendered by the protocol adapter.
    Prompt,
    /// Session should close with the given process-style exit status.
    CloseSession(i32),
    /// Room directory caches should be invalidated by the protocol adapter.
    InvalidateRoomCache,
    /// A commercial parcel cache entry should be invalidated by the protocol adapter.
    InvalidateCommercialParcelCache {
        /// Parcel view id.
        view_id: String,
        /// Front/street view where the parcel is visible.
        front_view_id: String,
    },
    /// A single inbox item cache entry should be invalidated by the protocol adapter.
    InvalidateInboxItem {
        /// Inbox item id to remove from caches.
        item_id: i64,
    },
    /// Deliver a live text message to a player id if they are online.
    LiveMessage {
        /// Target player id.
        target_player_id: String,
        /// Text delivered to the target connection.
        text: String,
    },
    /// Deliver a live text message to everyone in a room view.
    LiveViewMessage {
        /// Target room view id.
        view_id: String,
        /// Text delivered to the target connections.
        text: String,
    },
    /// Deliver a live inbox notice to a player id if they are online.
    LiveInboxNotice {
        /// Target player id.
        target_player_id: String,
        /// Inbox notice payload.
        notice: LiveInboxNotice,
    },
    /// Ensure an admitted player has a wallet, then move them into the world.
    EnsureWalletAndEnter {
        /// Stable username.
        user: String,
        /// Stable player id.
        player_id: String,
        /// Accepted agreement version.
        agreement_version: String,
        /// View where the player should enter after admission.
        target_view: String,
    },
    /// Move the current player to another view and render the resulting observation.
    Relocate {
        /// Target view id.
        target_view: String,
        /// Optional movement direction for observation events.
        direction: Option<hinemos_core::Direction>,
        /// Optional message attached to the resulting observation.
        message: Option<String>,
    },
}

/// Minimal inbox payload needed for protocol adapters to render live notices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInboxNotice {
    /// Inbox item id.
    pub id: i64,
    /// Inbox item kind.
    pub kind: String,
    /// Sender username.
    pub sender_user: String,
    /// Inbox subject.
    pub subject: String,
    /// Inbox body.
    pub body: String,
}

/// Result of checking a command while admission is pending.
#[derive(Debug, Clone, PartialEq)]
pub enum PendingAdmissionCommandOutcome {
    /// The player is already admitted; normal command handling should continue.
    NotPending,
    /// The pending player may continue through normal command handling.
    Allow(Vec<UiEvent>),
    /// The command was handled and normal command handling should stop.
    Block(Vec<UiEvent>),
}

impl LiveInboxNotice {
    /// Builds a live notice payload from an inbox item view.
    #[must_use]
    pub fn from_item(item: &impl InboxItemView) -> Self {
        Self {
            id: item.id(),
            kind: item.kind().to_owned(),
            sender_user: item.sender_user().to_owned(),
            subject: item.subject().to_owned(),
            body: item.body().to_owned(),
        }
    }
}

/// Accumulates protocol-neutral UI events for handlers that are being extracted.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct UiEvents {
    events: Vec<UiEvent>,
}

impl UiEvents {
    /// Appends one event.
    pub fn push(&mut self, event: UiEvent) {
        self.events.push(event);
    }

    /// Appends a text event.
    pub fn text(&mut self, text: impl Into<String>) {
        self.events.push(UiEvent::Text(text.into()));
    }

    /// Appends an observation event.
    pub fn observation(&mut self, observation: JsonObservation) {
        self.events.push(UiEvent::Observation(observation));
    }

    /// Appends a prompt event.
    pub fn prompt(&mut self) {
        self.events.push(UiEvent::Prompt);
    }

    /// Appends a close-session event.
    pub fn close_session(&mut self, status: i32) {
        self.events.push(UiEvent::CloseSession(status));
    }

    /// Returns the accumulated events.
    #[must_use]
    pub fn into_vec(self) -> Vec<UiEvent> {
        self.events
    }

    /// Returns true when the accumulated events close the session.
    #[must_use]
    pub fn contains_close_session(&self) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event, UiEvent::CloseSession(_)))
    }
}

impl From<Vec<UiEvent>> for UiEvents {
    fn from(events: Vec<UiEvent>) -> Self {
        Self { events }
    }
}
