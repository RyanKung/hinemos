#![deny(missing_docs)]

//! Postgres-backed persistence for accounts and player state.

mod accounts;
mod app_traits;
mod error;
mod identities;
mod messages;
mod parcels;
mod room_mail;
mod schema;
mod storage_badges;
mod storage_ext;
mod storage_mailing_lists;
mod storage_memory;
mod storage_parcel;
mod storage_payments;
mod storage_rooms;
mod types;

use std::future::Future;
use std::pin::Pin;

use hinemos_core::PlayerState;
use sqlx::postgres::{PgPool, PgPoolOptions};

pub use error::StorageError;
pub use hinemos_core::{
    ADMISSION_STATE_AGREED, ADMISSION_STATE_PENDING, INBOX_FILTER_ALL, INBOX_FILTER_CLAIMED,
    INBOX_FILTER_DONE, INBOX_FILTER_OPEN, INBOX_FILTER_UNREAD, INBOX_STATUS_ACKED,
    INBOX_STATUS_ARCHIVED, INBOX_STATUS_CLAIMED, INBOX_STATUS_UNREAD,
    OPERATOR_COMMAND_STATUS_DELIVERED, OPERATOR_COMMAND_STATUS_HANDLED,
    OPERATOR_COMMAND_STATUS_PENDING, PARCEL_BADGE_AWARD_ACTIVE, PARCEL_BADGE_AWARD_REVOKED,
    PARCEL_MAILING_LIST_STATUS_CLOSED, PARCEL_MAILING_LIST_STATUS_OPEN,
    PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE, PARCEL_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED,
    PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED, PARCEL_STATUS_VACANT,
    PARCEL_WORK_DESK_STATUS_CLOSED, PARCEL_WORK_DESK_STATUS_OPEN, PARCEL_WORK_ITEM_CANCELLED,
    PARCEL_WORK_ITEM_CLAIMED, PARCEL_WORK_ITEM_DONE, PARCEL_WORK_ITEM_QUEUED,
    PARCEL_WORK_SHIFT_ACTIVE, PARCEL_WORK_SHIFT_ENDED, PARCEL_WORK_STAFF_ACTIVE,
    PARCEL_WORK_STAFF_REMOVED, PAYMENT_REQUEST_STATUS_CANCELLED, PAYMENT_REQUEST_STATUS_PAID,
    PAYMENT_REQUEST_STATUS_PENDING,
};
pub(crate) use messages::NewInboxItem;
pub use types::{
    NewMemoryAtom, NewMemoryEvent, StoredAccountSettings, StoredAdmission, StoredAgentSelfModel,
    StoredBalance, StoredHungerState, StoredIdentity, StoredInboxItem, StoredMailAuthToken,
    StoredMemoryAtom, StoredMemoryEvent, StoredOperatorCommand, StoredParcel,
    StoredParcelBadgeAward, StoredParcelBadgeDefinition, StoredParcelCommandRoute,
    StoredParcelJobGuide, StoredParcelMailingList, StoredParcelMailingListPost,
    StoredParcelMailingListSubscriber, StoredParcelMailingListSubscription, StoredParcelShift,
    StoredParcelStaff, StoredParcelWorkDesk, StoredParcelWorkItem, StoredPasswordIdentity,
    StoredPaymentRequest, StoredRoomBinding, StoredRoomBindingKind, StoredRoomCommandPolicy,
    StoredServiceRoom, StoredSocialEdge, StoredTransfer, StoredWorldMessage,
};

/// Single in-world test currency used by the current ledger.
pub const TEST_CURRENCY: &str = "MARK";

const INITIAL_MARK_GRANT: i64 = 1_000;

/// Parameters for registering or updating an externally hosted service room.
#[derive(Debug, Clone, Copy)]
pub struct ServiceRoomUpsert<'a> {
    /// Stable view identifier for the external room.
    pub view_id: &'a str,
    /// Front/street view where the room entrance is advertised.
    pub front_view_id: Option<&'a str>,
    /// Entity identifier for the room entrance in the front view.
    pub front_entity_id: Option<&'a str>,
    /// Human-readable in-world address.
    pub address: Option<&'a str>,
    /// Display label for the room.
    pub label: Option<&'a str>,
    /// Comma-separated aliases that enter this room.
    pub enter_aliases: Option<&'a str>,
    /// Mail user owned by the external room service.
    pub room_user: &'a str,
    /// Player/principal id owned by the external room service.
    pub room_player_id: &'a str,
    /// Optional status text shown at the room entrance.
    pub status_text: Option<&'a str>,
    /// Optional command list supplied by the room service.
    pub custom_commands: Option<&'a str>,
    /// Optional command list that counts as hunger recovery.
    pub recovery_commands: Option<&'a str>,
    /// Whether the room is currently enabled.
    pub enabled: bool,
}

/// Async player-state persistence boundary.
pub trait PlayerStateStore {
    /// Loads a player state if one has been saved.
    fn load_player_state<'a>(
        &'a self,
        player_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PlayerState>, StorageError>> + Send + 'a>>;

    /// Saves the current player state.
    fn save_player_state<'a>(
        &'a self,
        player: &'a PlayerState,
    ) -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>>;
}

/// Database-backed storage facade.
#[derive(Debug, Clone)]
pub struct PgStorage {
    pub(crate) pool: PgPool,
}

impl PlayerStateStore for PgStorage {
    fn load_player_state<'a>(
        &'a self,
        player_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PlayerState>, StorageError>> + Send + 'a>> {
        Box::pin(async move { PgStorage::load_player_state(self, player_id).await })
    }

    fn save_player_state<'a>(
        &'a self,
        player: &'a PlayerState,
    ) -> Pin<Box<dyn Future<Output = Result<(), StorageError>> + Send + 'a>> {
        Box::pin(async move { PgStorage::save_player_state(self, player).await })
    }
}

impl PgStorage {
    /// Returns the underlying Postgres connection pool for extension crates.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Connects to Postgres.
    pub async fn connect(database_url: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Creates the initial schema if it does not already exist.
    pub async fn migrate(&self) -> Result<(), StorageError> {
        schema::migrate(&self.pool).await
    }
}
