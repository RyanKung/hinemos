use thiserror::Error;

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
    /// A player cannot marry themself.
    #[error("cannot register marriage with yourself")]
    SelfMarriage,
    /// Sender balance is too low.
    #[error("insufficient MARK balance")]
    InsufficientFunds,
    /// Marriage partner is not present in the registry room.
    #[error("marriage partner is not present in registry: {0}")]
    MarriagePartnerNotPresent(String),
    /// One of the players already has an active marriage.
    #[error("active marriage already exists for: {0}")]
    MarriageAlreadyActive(String),
    /// Player has no active marriage certificate to dissolve.
    #[error("no active marriage found for: {0}")]
    NoActiveMarriage(String),
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
    /// Mailing-list input is invalid.
    #[error("invalid mailing list: {0}")]
    InvalidMailingList(String),
    /// Mailing list was not found.
    #[error("mailing list not found: {parcel_id}/{slug}")]
    MailingListNotFound {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Mailing list already exists for a shop parcel.
    #[error("mailing list already exists: {parcel_id}/{slug}")]
    MailingListAlreadyExists {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Mailing list is closed to new subscriptions.
    #[error("mailing list is closed: {parcel_id}/{slug}")]
    MailingListClosed {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Player is already actively subscribed.
    #[error("already subscribed to mailing list: {parcel_id}/{slug}")]
    MailingListAlreadySubscribed {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Mailing list has no active subscribers.
    #[error("mailing list has no active subscribers: {parcel_id}/{slug}")]
    MailingListNoSubscribers {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
}

impl StorageError {
    pub(crate) fn from_password_hash(error: argon2::password_hash::Error) -> Self {
        Self::PasswordHash(error.to_string())
    }
}
