//! Storage row types and low-level helpers.

use hinemos_core::PlayerState;
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
        self.admission_state == "agreed"
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
    /// Stable parcel id, for example north_01.
    pub parcel_id: String,
    /// Static RON view id overlaid by this parcel.
    pub view_id: String,
    /// Parcel district: north or south.
    pub district: String,
    /// One-based position away from the crossroads.
    pub position: i32,
    /// Owning SSH username.
    pub owner_user: Option<String>,
    /// Owning player id.
    pub owner_player_id: Option<String>,
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
    for district in ["north", "south"] {
        for position in 1..=5 {
            let parcel_id = format!("{district}_{position:02}");
            let view_id = format!("parcel_{parcel_id}");
            sqlx::query(
                r#"
                insert into commercial_parcels (parcel_id, view_id, district, position)
                values ($1, $2, $3, $4)
                on conflict (parcel_id) do nothing
                "#,
            )
            .bind(parcel_id)
            .bind(view_id)
            .bind(district)
            .bind(position)
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
        select parcel_id, view_id, district, position, owner_user, owner_player_id,
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
