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
    /// Sender balance is too low.
    #[error("insufficient MARK balance")]
    InsufficientFunds,
    /// Parcel does not exist.
    #[error("parcel not found: {0}")]
    ParcelNotFound(String),
    /// Room binding has no room mailbox principal.
    #[error("room mailbox missing for view: {0}")]
    RoomMailboxMissing(String),
    /// Parcel is already owned.
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
    #[error("parcel command not found: {0}")]
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
    /// Mailing list has no active members.
    #[error("parcel chat has no active members: {parcel_id}/{slug}")]
    MailingListNoSubscribers {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Player is not a member of the parcel mailing list.
    #[error("join this parcel chat before posting: {parcel_id}/{slug}")]
    MailingListNotMember {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Parcel work input is invalid.
    #[error("invalid parcel work: {0}")]
    InvalidShopWork(String),
    /// Parcel work desk was not found.
    #[error("parcel work desk not found: {parcel_id}/{slug}")]
    ShopWorkDeskNotFound {
        /// Parcel id.
        parcel_id: String,
        /// Stable work-desk slug.
        slug: String,
    },
    /// Parcel work desk already exists.
    #[error("parcel work desk already exists: {parcel_id}/{slug}")]
    ShopWorkDeskAlreadyExists {
        /// Parcel id.
        parcel_id: String,
        /// Stable work-desk slug.
        slug: String,
    },
    /// Worker is not assigned to the parcel work desk.
    #[error("parcel worker is not assigned to this desk: {parcel_id}/{slug}")]
    ShopWorkerNotAssigned {
        /// Parcel id.
        parcel_id: String,
        /// Stable work-desk slug.
        slug: String,
    },
    /// Worker has no active in-parcel shift for this desk.
    #[error("no active parcel shift for this desk: {parcel_id}/{slug}")]
    ShopShiftNotActive {
        /// Parcel id.
        parcel_id: String,
        /// Stable work-desk slug.
        slug: String,
    },
    /// Parcel work item was not found.
    #[error("parcel work item not found: {0}")]
    ShopWorkItemNotFound(i64),
    /// Parcel work item is not in a valid state for this operation.
    #[error("parcel work item has invalid state: {0}")]
    ShopWorkItemInvalidState(i64),
    /// Parcel badge input is invalid.
    #[error("invalid parcel badge: {0}")]
    InvalidShopBadge(String),
    /// Parcel badge was not found.
    #[error("parcel badge not found: {parcel_id}/{slug}")]
    ShopBadgeNotFound {
        /// Parcel id.
        parcel_id: String,
        /// Stable badge slug.
        slug: String,
    },
    /// Parcel badge award was not found.
    #[error("parcel badge award not found: {parcel_id}/{slug} for {target}")]
    ShopBadgeAwardNotFound {
        /// Parcel id.
        parcel_id: String,
        /// Stable badge slug.
        slug: String,
        /// Target username or player id.
        target: String,
    },
    /// Parcel badge award is not currently active.
    #[error("parcel badge award is not active: {parcel_id}/{slug} for {target}")]
    ShopBadgeAwardNotActive {
        /// Parcel id.
        parcel_id: String,
        /// Stable badge slug.
        slug: String,
        /// Target username or player id.
        target: String,
    },
}

impl StorageError {
    pub(crate) fn from_password_hash(error: argon2::password_hash::Error) -> Self {
        Self::PasswordHash(error.to_string())
    }
}
