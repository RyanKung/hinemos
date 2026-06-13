//! Storage row types and low-level helpers.

use hinemos_core::{
    ADMISSION_STATE_AGREED, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED, PARCEL_STATUS_VACANT,
    PlayerState,
};
use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::PgPool;
use thiserror::Error;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaymentTarget {
    pub(crate) username: String,
    pub(crate) player_id: String,
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

/// Storage operation errors.
#[derive(Debug, Error)]
pub enum StorageError {
    /// SQLx failed.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// JSON conversion failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Password hashing failed.
    #[error("password hash operation failed: {0}")]
    PasswordHash(String),
    /// Payment amount was not positive.
    #[error("amount must be positive: {0}")]
    InvalidAmount(i64),
    /// Payment target does not exist.
    #[error("payment target not found: {0}")]
    PaymentTargetNotFound(String),
    /// Sender and target resolve to the same account.
    #[error("cannot pay yourself")]
    SelfPayment,
    /// Sender balance is too low.
    #[error("insufficient MARK balance")]
    InsufficientFunds,
    /// Commercial parcel does not exist.
    #[error("parcel not found: {0}")]
    ParcelNotFound(String),
    /// Room binding has no room mailbox principal.
    #[error("room mailbox missing for view: {0}")]
    RoomMailboxMissing(String),
    /// Commercial parcel is already owned.
    #[error("parcel is already owned: {0}")]
    ParcelAlreadyOwned(String),
    /// Player does not own the parcel.
    #[error("you do not own this parcel: {0}")]
    NotParcelOwner(String),
    /// Build field is not recognized.
    #[error("unknown build field: {0}")]
    UnknownBuildField(String),
    /// Build sheet is missing required fields.
    #[error("build is not publishable: {0}")]
    BuildNotPublishable(String),
    /// Parcel is not ready to receive operator commands.
    #[error("parcel is not built: {0}")]
    ParcelNotBuilt(String),
    /// Operator command was not found.
    #[error("shop command not found: {0}")]
    OperatorCommandNotFound(i64),
    /// Payment request was not found.
    #[error("payment request not found: {0}")]
    PaymentRequestNotFound(i64),
    /// Payment request is not pending.
    #[error("payment request is not pending: {0}")]
    PaymentRequestNotPending(i64),
    /// Player is not allowed to act on this payment request.
    #[error("payment request does not belong to this player: {0}")]
    PaymentRequestForbidden(i64),
    /// Inbox item was not found or is not visible to the player.
    #[error("inbox item not found: {0}")]
    InboxItemNotFound(i64),
    /// Inbox filter is not supported.
    #[error("invalid inbox filter: {0}")]
    InvalidInboxFilter(String),
    /// Account setting input is invalid.
    #[error("invalid account setting: {0}")]
    InvalidAccountSetting(String),
}

impl StorageError {
    pub(crate) fn from_password_hash(error: argon2::password_hash::Error) -> Self {
        Self::PasswordHash(error.to_string())
    }
}

pub(crate) async fn seed_commercial_parcels(pool: &PgPool) -> Result<(), StorageError> {
    migrate_legacy_parcel_ids(pool).await?;
    for (district, prefix) in [("north", "N"), ("south", "S")] {
        for position in 1..=10 {
            let parcel_id = format!("{prefix}{position}");
            let view_id = format!("parcel_{parcel_id}");
            let front_view_id = parcel_front_view_id(district, position);
            sqlx::query(
                r#"
                insert into commercial_parcels (parcel_id, view_id, front_view_id, district, position)
                values ($1, $2, $3, $4, $5)
                on conflict (parcel_id) do update
                set front_view_id = excluded.front_view_id
                where commercial_parcels.front_view_id is null
                "#,
            )
            .bind(parcel_id)
            .bind(view_id)
            .bind(front_view_id)
            .bind(district)
            .bind(position)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

fn parcel_front_view_id(district: &str, position: i32) -> String {
    let segment = ((position - 1) / 2) + 1;
    format!("street_{district}_{segment:02}")
}

async fn migrate_legacy_parcel_ids(pool: &PgPool) -> Result<(), StorageError> {
    for (district, prefix) in [("north", "N"), ("south", "S")] {
        for position in (1..=5).rev() {
            let old_id = format!("{district}_{position:02}");
            let new_position = position * 2 - 1;
            let new_id = format!("{prefix}{new_position}");
            let new_view = format!("parcel_{new_id}");
            sqlx::query(
                r#"
                update commercial_parcels
                set parcel_id = $2, view_id = $3, position = $4, updated_at = now()
                where parcel_id = $1
                  and not exists (
                      select 1 from commercial_parcels existing
                      where existing.parcel_id = $2
                  )
                "#,
            )
            .bind(old_id)
            .bind(new_id)
            .bind(new_view)
            .bind(new_position)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

pub(crate) async fn fetch_parcel_by_id(
    pool: &PgPool,
    parcel_id: &str,
) -> Result<StoredParcel, StorageError> {
    let parcel = sqlx::query_as::<_, StoredParcel>(
        r#"
        select parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
               room_user, room_player_id,
               status, title, description, style, operator_prompt, custom_commands
        from commercial_parcels
        where parcel_id = $1
        "#,
    )
    .bind(parcel_id)
    .fetch_optional(pool)
    .await?;

    parcel.ok_or_else(|| StorageError::ParcelNotFound(parcel_id.to_owned()))
}

pub(crate) fn player_account_id(player_id: &str) -> String {
    format!("player:{player_id}")
}

pub(crate) fn player_id_from_password_username(username: &str) -> String {
    format!("password_{}", sanitize_id(username))
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

pub(crate) async fn ensure_player_account(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: &str,
    username: &str,
    player_id: &str,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        insert into world_accounts (account_id, kind, owner_id, display_name)
        values ($1, 'player', $2, $3)
        on conflict (account_id) do update
        set display_name = excluded.display_name
        "#,
    )
    .bind(account_id)
    .bind(player_id)
    .bind(username)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn ensure_balance_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: &str,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        insert into world_balances (account_id, asset, amount)
        values ($1, 'MARK', 0)
        on conflict (account_id, asset) do nothing
        "#,
    )
    .bind(account_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn fetch_balance_pool(
    pool: &PgPool,
    account_id: &str,
) -> Result<StoredBalance, StorageError> {
    let row = sqlx::query(
        r#"
        select account_id, asset, amount
        from world_balances
        where account_id = $1 and asset = 'MARK'
        "#,
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;
    Ok(StoredBalance {
        account_id: row.get("account_id"),
        asset: row.get("asset"),
        amount: row.get("amount"),
    })
}

pub(crate) async fn fetch_balance_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: &str,
) -> Result<StoredBalance, StorageError> {
    let row = sqlx::query(
        r#"
        select account_id, asset, amount
        from world_balances
        where account_id = $1 and asset = 'MARK'
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(StoredBalance {
        account_id: row.get("account_id"),
        asset: row.get("asset"),
        amount: row.get("amount"),
    })
}

pub(crate) async fn credit_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: &str,
    amount: i64,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        update world_balances
        set amount = amount + $2,
            updated_at = now()
        where account_id = $1 and asset = 'MARK'
        "#,
    )
    .bind(account_id)
    .bind(amount)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn debit_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: &str,
    amount: i64,
) -> Result<(), StorageError> {
    let updated = sqlx::query(
        r#"
        update world_balances
        set amount = amount - $2,
            updated_at = now()
        where account_id = $1 and asset = 'MARK' and amount >= $2
        "#,
    )
    .bind(account_id)
    .bind(amount)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(StorageError::InsufficientFunds);
    }
    Ok(())
}

pub(crate) async fn resolve_payment_target(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target: &str,
) -> Result<PaymentTarget, StorageError> {
    let row = sqlx::query(
        r#"
        select username, player_id
        from (
            select username, player_id, last_seen_at
            from ssh_identities
            union all
            select username, player_id, last_seen_at
            from password_identities
            union all
            select username, player_id, last_seen_at
            from mail_auth_tokens
        ) identities
        where username = $1 or player_id = $1
        order by last_seen_at desc
        limit 1
        "#,
    )
    .bind(target)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else {
        return Err(StorageError::PaymentTargetNotFound(target.to_owned()));
    };

    Ok(PaymentTarget {
        username: row.get("username"),
        player_id: row.get("player_id"),
    })
}
