//! Storage row types and low-level helpers.

use hinemos_core::{
    ADMISSION_STATE_AGREED, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED, PARCEL_STATUS_VACANT,
    PlayerState,
};
use serde_json::Value;

use crate::StorageError;

/// Stored SSH identity mapping.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredIdentity {
    /// SSH username.
    pub username: String,
    /// Public key fingerprint.
    pub key_fingerprint: String,
    /// Stable player id used by the runtime.
    pub player_id: String,
    /// True when this identity row was created by the current login.
    pub created: bool,
}

/// Stored password identity mapping.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredPasswordIdentity {
    /// SSH username.
    pub username: String,
    /// Stable player id used by the runtime.
    pub player_id: String,
    /// True when this identity row was created by the current login.
    pub created: bool,
}

/// Stored mail token identity mapping.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredMailAuthToken {
    /// Mail username.
    pub username: String,
    /// Stable player id used by the runtime.
    pub player_id: String,
}

/// Stored account settings summary.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredAccountSettings {
    /// Stable player id used by the runtime.
    pub player_id: String,
    /// Profile display name.
    pub display_name: String,
    /// Full days since the profile was created.
    pub online_days: i32,
    /// True when a password identity exists.
    pub has_password: bool,
    /// True when a mail auth token exists.
    pub has_mail_token: bool,
    /// Current SSH key fingerprint if one is bound.
    pub key_fingerprint: Option<String>,
}

/// Stored admission state for a player profile.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredAdmission {
    /// Stable player id used by the runtime.
    pub player_id: String,
    /// Admission state: pending or agreed.
    pub admission_state: String,
    /// Agreement version accepted by the player, if any.
    pub agreement_version: Option<String>,
    /// Agreement version most recently read by the player, if any.
    pub agreement_read_version: Option<String>,
}

impl StoredAdmission {
    /// Returns true when the profile has been admitted into the main world.
    #[must_use]
    pub fn is_agreed(&self) -> bool {
        self.admission_state == ADMISSION_STATE_AGREED
    }

    /// Returns true when the current agreement version was read.
    #[must_use]
    pub fn has_read_version(&self, version: &str) -> bool {
        self.agreement_read_version.as_deref() == Some(version)
    }
}

/// Stored world message rendered for mailbox and history views.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredWorldMessage {
    /// Message kind: mail, say, or broadcast.
    pub kind: String,
    /// Sender SSH user.
    pub sender_user: String,
    /// Target SSH user or player id when present.
    pub target_user: String,
    /// Message body.
    pub body: String,
    /// Database formatted creation time.
    pub created_at: String,
    /// Database formatted expiry time, empty for persistent mail.
    pub expires_at: Option<String>,
}

/// Stored actionable inbox item for an agent or human player.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredInboxItem {
    /// Database id.
    pub id: i64,
    /// Item kind, for example mail, shop_command, or payment_request.
    pub kind: String,
    /// Recipient SSH user.
    pub recipient_user: String,
    /// Recipient player id.
    pub recipient_player_id: String,
    /// Sender SSH user.
    pub sender_user: String,
    /// Sender player id.
    pub sender_player_id: String,
    /// Short subject for list views.
    pub subject: String,
    /// Full body.
    pub body: String,
    /// Optional source kind for idempotency and threading.
    pub source_kind: Option<String>,
    /// Optional source id for idempotency and threading.
    pub source_id: Option<i64>,
    /// Structured item payload.
    pub payload: Value,
    /// unread, claimed, acked, or archived.
    pub status: String,
    /// Number of processing claims.
    pub attempts: i32,
    /// Database formatted lease expiry, if claimed.
    pub lease_until: Option<String>,
    /// Database formatted creation time.
    pub created_at: String,
}

/// Stored balance for a single account and asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBalance {
    /// Account id that owns the balance.
    pub account_id: String,
    /// Asset symbol, currently always MARK.
    pub asset: String,
    /// Integer amount in the smallest MARK unit.
    pub amount: i64,
}

/// Completed MARK transfer summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTransfer {
    /// Ledger row id.
    pub ledger_id: i64,
    /// Asset symbol, currently always MARK.
    pub asset: String,
    /// Transferred amount.
    pub amount: i64,
    /// Debited account.
    pub sender_account_id: String,
    /// Credited account.
    pub target_account_id: String,
    /// Resolved target user.
    pub target_user: String,
    /// Transfer memo.
    pub memo: String,
    /// Sender balance after transfer.
    pub sender_balance: i64,
}

/// Commercial parcel state.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredParcel {
    /// Stable parcel id, for example N1.
    pub parcel_id: String,
    /// Static RON view id overlaid by this parcel.
    pub view_id: String,
    /// Street view id where the parcel entrance is visible.
    pub front_view_id: String,
    /// Parcel district: north or south.
    pub district: String,
    /// One-based door number in the district.
    pub position: i32,
    /// Owning SSH username.
    pub owner_user: Option<String>,
    /// Owning player id.
    pub owner_player_id: Option<String>,
    /// Room-owned mail username.
    pub room_user: Option<String>,
    /// Room-owned mail player id.
    pub room_player_id: Option<String>,
    /// vacant, claimed, or built.
    pub status: String,
    /// Built shop title.
    pub title: Option<String>,
    /// Built shop description.
    pub description: Option<String>,
    /// Owner-authored style note.
    pub style: Option<String>,
    /// Owner-authored operator prompt.
    pub operator_prompt: Option<String>,
    /// Owner-authored custom command help.
    pub custom_commands: Option<String>,
}

impl StoredParcel {
    /// Returns true when the parcel has been built.
    #[must_use]
    pub fn is_built(&self) -> bool {
        self.status == PARCEL_STATUS_BUILT
    }

    /// Returns true when the parcel has been claimed but not built yet.
    #[must_use]
    pub fn is_claimed(&self) -> bool {
        self.status == PARCEL_STATUS_CLAIMED
    }

    /// Returns true when the parcel is vacant.
    #[must_use]
    pub fn is_vacant(&self) -> bool {
        self.status == PARCEL_STATUS_VACANT
    }
}

/// Externally hosted room service registration.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredServiceRoom {
    /// Runtime view id handled by this service.
    pub view_id: String,
    /// Street view id where the room entrance is visible.
    pub front_view_id: Option<String>,
    /// Entity id for the visible entrance object.
    pub front_entity_id: Option<String>,
    /// Short address token for entering from the street.
    pub address: Option<String>,
    /// Player-facing room label.
    pub label: Option<String>,
    /// Additional enter aliases separated by whitespace, comma, newline, or semicolon.
    pub enter_aliases: Option<String>,
    /// Room-owned mail username.
    pub room_user: String,
    /// Room-owned mail player id.
    pub room_player_id: String,
    /// Player-facing status text appended to the room observation.
    pub status_text: Option<String>,
    /// Data-authored command help, one command per line or semicolon.
    pub custom_commands: Option<String>,
    /// Whether this registration is active.
    pub enabled: bool,
}

/// Source table behind a unified room binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredRoomBindingKind {
    /// A commercial parcel backed by `commercial_parcels`.
    CommercialParcel,
    /// An externally hosted service room backed by `service_rooms`.
    ServiceRoom,
}

/// Command forwarding policy for a room binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredRoomCommandPolicy {
    /// Forward all unhandled input to the room mailbox.
    ForwardAll,
    /// Forward only listed extension commands.
    ForwardListed(Vec<String>),
}

/// Unified view/mailbox/entry binding for parcel rooms and service rooms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRoomBinding {
    /// Binding source.
    pub kind: StoredRoomBindingKind,
    /// Room view id entered by the player.
    pub view_id: String,
    /// Street/front view where the entrance is visible.
    pub front_view_id: String,
    /// Optional entity that must be visible in the front view for this entrance.
    pub front_entity_id: Option<String>,
    /// Short player-entered address.
    pub address: String,
    /// Player-facing label.
    pub label: String,
    /// Player-facing status text for externally hosted rooms.
    pub status_text: Option<String>,
    /// Data-authored command help for externally hosted rooms.
    pub custom_commands: Option<String>,
    /// Player-facing entrance line shown in front-view observations.
    pub entry_text: String,
    /// Optional label used to replace the authored ASCII placeholder.
    pub ascii_label: Option<String>,
    /// Parcel owner username when this binding comes from a commercial parcel.
    pub owner_user: Option<String>,
    /// Parcel status when this binding comes from a commercial parcel.
    pub parcel_status: Option<String>,
    /// Parcel title when this binding comes from a commercial parcel.
    pub parcel_title: Option<String>,
    /// Parcel description when this binding comes from a commercial parcel.
    pub parcel_description: Option<String>,
    /// Parcel style note when this binding comes from a commercial parcel.
    pub parcel_style: Option<String>,
    /// Parcel operator prompt when this binding comes from a commercial parcel.
    pub parcel_operator_prompt: Option<String>,
    /// Parcel custom command help when this binding comes from a commercial parcel.
    pub parcel_custom_commands: Option<String>,
    /// Explicit enter aliases.
    pub enter_aliases: Vec<String>,
    /// Mailbox username for room-owned workflows.
    pub room_user: Option<String>,
    /// Mailbox player id for room-owned workflows.
    pub room_player_id: Option<String>,
    /// Owning player id for commercial parcel rooms.
    pub owner_player_id: Option<String>,
    /// Input forwarding policy.
    pub command_policy: StoredRoomCommandPolicy,
}

impl StoredRoomBinding {
    /// Builds a binding for a commercial parcel.
    #[must_use]
    pub fn from_parcel(parcel: StoredParcel) -> Self {
        let address = parcel.parcel_id.clone();
        let parcel_title = parcel.title.clone();
        let label = parcel_title
            .clone()
            .unwrap_or_else(|| parcel.parcel_id.clone());
        let entry_label = match parcel.status.as_str() {
            PARCEL_STATUS_BUILT => parcel_title
                .as_deref()
                .unwrap_or(&parcel.parcel_id)
                .to_owned(),
            PARCEL_STATUS_CLAIMED => format!(
                "{} claimed by {}",
                parcel.parcel_id,
                parcel.owner_user.as_deref().unwrap_or("unknown")
            ),
            _ => format!("{} {}", parcel.parcel_id, PARCEL_STATUS_VACANT),
        };
        let ascii_label = parcel.is_built().then(|| label.clone());
        let enter_aliases = parcel_title.clone().into_iter().collect();
        Self {
            kind: StoredRoomBindingKind::CommercialParcel,
            view_id: parcel.view_id,
            front_view_id: parcel.front_view_id,
            front_entity_id: None,
            address,
            label,
            status_text: None,
            custom_commands: None,
            entry_text: format!("- {entry_label}. Enter: /enter {}.", parcel.parcel_id),
            ascii_label,
            owner_user: parcel.owner_user,
            parcel_status: Some(parcel.status),
            parcel_title,
            parcel_description: parcel.description,
            parcel_style: parcel.style,
            parcel_operator_prompt: parcel.operator_prompt,
            parcel_custom_commands: parcel.custom_commands,
            enter_aliases,
            room_user: parcel.room_user,
            room_player_id: parcel.room_player_id,
            owner_player_id: parcel.owner_player_id,
            command_policy: StoredRoomCommandPolicy::ForwardAll,
        }
    }

    /// Builds a binding for an externally hosted service room.
    #[must_use]
    pub fn from_service_room(room: StoredServiceRoom) -> Option<Self> {
        let front_view_id = room.front_view_id?;
        let address = room.address.unwrap_or_else(|| room.view_id.clone());
        let label = room.label.clone().unwrap_or_else(|| room.view_id.clone());
        let enter_aliases = room
            .enter_aliases
            .as_deref()
            .unwrap_or_default()
            .split([',', ';', '\n', ' '])
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(str::to_owned)
            .collect();
        let listed_commands = room
            .custom_commands
            .as_deref()
            .unwrap_or_default()
            .split(['\n', ';'])
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .map(str::to_owned)
            .collect();
        Some(Self {
            kind: StoredRoomBindingKind::ServiceRoom,
            view_id: room.view_id,
            front_view_id,
            front_entity_id: room.front_entity_id,
            entry_text: format!("- {address} {label}. Enter: /enter {address}."),
            address,
            label,
            status_text: room.status_text,
            custom_commands: room.custom_commands,
            ascii_label: None,
            owner_user: None,
            parcel_status: None,
            parcel_title: None,
            parcel_description: None,
            parcel_style: None,
            parcel_operator_prompt: None,
            parcel_custom_commands: None,
            enter_aliases,
            room_user: Some(room.room_user),
            room_player_id: Some(room.room_player_id),
            owner_player_id: None,
            command_policy: StoredRoomCommandPolicy::ForwardListed(listed_commands),
        })
    }
}

/// Raw visitor command forwarded to a shop operator.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredOperatorCommand {
    /// Database id.
    pub id: i64,
    /// View where the command was entered.
    pub view_id: String,
    /// Parcel id for the shop.
    pub parcel_id: String,
    /// Sender SSH username.
    pub sender_user: String,
    /// Sender player id.
    pub sender_player_id: String,
    /// Shop owner username.
    pub owner_user: String,
    /// Shop owner player id.
    pub owner_player_id: String,
    /// Raw line entered by the visitor.
    pub raw_input: String,
    /// pending, delivered, or handled.
    pub status: String,
    /// Database formatted creation time.
    pub created_at: String,
}

/// Payment request created by a shop operator for a visitor command.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredPaymentRequest {
    /// Database id.
    pub id: i64,
    /// Operator command that produced this request.
    pub operator_command_id: i64,
    /// Parcel id for the shop.
    pub parcel_id: String,
    /// Visitor SSH username.
    pub payer_user: String,
    /// Visitor player id.
    pub payer_player_id: String,
    /// Shop owner SSH username.
    pub payee_user: String,
    /// Shop owner player id.
    pub payee_player_id: String,
    /// Asset symbol, currently always MARK.
    pub asset: String,
    /// Requested amount.
    pub amount: i64,
    /// Payment memo.
    pub memo: String,
    /// Content delivered after payment.
    pub delivery: String,
    /// pending, paid, or cancelled.
    pub status: String,
    /// Ledger row id after payment.
    pub ledger_id: Option<i64>,
    /// Database formatted creation time.
    pub created_at: String,
}

/// Stored active marriage certificate.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredMarriageCertificate {
    /// Database id.
    pub id: i64,
    /// First participant username, canonicalized lexicographically by player id.
    pub party_a_user: String,
    /// First participant player id.
    pub party_a_player_id: String,
    /// Second participant username.
    pub party_b_user: String,
    /// Second participant player id.
    pub party_b_player_id: String,
    /// Certificate status, currently active.
    pub status: String,
    /// Per-player fee amount in MARK.
    pub fee_amount: i64,
    /// Ledger ids for the two fee charges.
    pub fee_ledger_ids: Vec<i64>,
    /// Rendered certificate text.
    pub certificate_text: String,
    /// Database formatted issue time.
    pub issued_at: String,
}

/// New append-only memory event.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NewMemoryEvent {
    /// Stable agent/player id that owns this memory.
    pub agent_id: String,
    /// Event source, for example chat, trade, system, or manual.
    pub source: String,
    /// Event classifier, for example promise_made or trade_executed.
    pub event_type: String,
    /// Actors involved in the event.
    pub actors: Value,
    /// Human-readable event content.
    pub content: String,
    /// References into world systems such as conversation, trade, or location ids.
    pub world_refs: Value,
    /// Event salience from 0.0 to 1.0.
    pub salience: f64,
}

/// Stored append-only memory event.
#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct StoredMemoryEvent {
    /// Database id.
    pub id: i64,
    /// Stable agent/player id that owns this memory.
    pub agent_id: String,
    /// Database formatted occurrence time.
    pub occurred_at: String,
    /// Event source.
    pub source: String,
    /// Event classifier.
    pub event_type: String,
    /// Actors involved in the event.
    pub actors: Value,
    /// Human-readable event content.
    pub content: String,
    /// References into world systems.
    pub world_refs: Value,
    /// Event salience from 0.0 to 1.0.
    pub salience: f64,
    /// Database formatted creation time.
    pub created_at: String,
}

/// New or updated semantic memory atom.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NewMemoryAtom {
    /// Stable agent/player id that owns this memory.
    pub agent_id: String,
    /// Memory kind: episodic, social, self, norm, goal, preference, or commitment.
    pub kind: String,
    /// Entity this memory is about.
    pub subject: String,
    /// Relation or property being remembered.
    pub predicate: String,
    /// Structured object payload.
    pub object: Value,
    /// Human-readable memory summary.
    pub summary: String,
    /// Event ids that justify this memory.
    pub evidence_event_ids: Vec<i64>,
    /// Confidence from 0.0 to 1.0.
    pub confidence: f64,
    /// Importance from 0.0 to 1.0.
    pub importance: f64,
    /// Emotional valence from -1.0 to 1.0.
    pub emotional_valence: f64,
}

/// Stored semantic memory atom.
#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct StoredMemoryAtom {
    /// Database id.
    pub id: i64,
    /// Stable agent/player id that owns this memory.
    pub agent_id: String,
    /// Memory kind.
    pub kind: String,
    /// Entity this memory is about.
    pub subject: String,
    /// Relation or property being remembered.
    pub predicate: String,
    /// Structured object payload.
    pub object: Value,
    /// Human-readable memory summary.
    pub summary: String,
    /// Event ids that justify this memory.
    pub evidence_event_ids: Vec<i64>,
    /// Confidence from 0.0 to 1.0.
    pub confidence: f64,
    /// Importance from 0.0 to 1.0.
    pub importance: f64,
    /// Emotional valence from -1.0 to 1.0.
    pub emotional_valence: f64,
    /// Database formatted creation time.
    pub created_at: String,
    /// Database formatted update time.
    pub updated_at: String,
    /// Database formatted expiry time.
    pub expires_at: Option<String>,
}

/// Stored social graph edge from one agent to another identity.
#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct StoredSocialEdge {
    /// Stable agent/player id that owns the relationship.
    pub agent_id: String,
    /// Target identity id or handle.
    pub target_id: String,
    /// Trust score from -1.0 to 1.0.
    pub trust: f64,
    /// Affinity score from -1.0 to 1.0.
    pub affinity: f64,
    /// Obligation score from 0.0 to 1.0.
    pub obligation: f64,
    /// Rivalry score from 0.0 to 1.0.
    pub rivalry: f64,
    /// Familiarity score from 0.0 to 1.0.
    pub familiarity: f64,
    /// Relationship tags.
    pub tags: Vec<String>,
    /// Memory ids that justify this edge.
    pub evidence_memory_ids: Vec<i64>,
    /// Database formatted update time.
    pub updated_at: String,
}

/// Stored self-model snapshot loaded when an agent logs in.
#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct StoredAgentSelfModel {
    /// Stable agent/player id that owns this model.
    pub agent_id: String,
    /// Monotonic model version.
    pub version: i64,
    /// Identity and long-term self description.
    pub identity: Value,
    /// Current goals, commitments, conflicts, and focus.
    pub current_state: Value,
    /// Behavioral style knobs.
    pub style: Value,
    /// Memory ids used to derive this model.
    pub derived_from_memory_ids: Vec<i64>,
    /// Database formatted creation time.
    pub created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct PlayerStateRow {
    pub(crate) player_id: String,
    pub(crate) current_view: String,
    pub(crate) inventory: Value,
}

impl TryFrom<PlayerStateRow> for PlayerState {
    type Error = StorageError;

    fn try_from(row: PlayerStateRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.player_id,
            current_view: row.current_view,
            inventory: serde_json::from_value(row.inventory)?,
        })
    }
}
