#![deny(missing_docs)]

//! Postgres-backed persistence for accounts and player state.

use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;
use xagora_core::PlayerState;

/// Single in-world test currency used by the current ledger.
pub const TEST_CURRENCY: &str = "MARK";

const INITIAL_MARK_GRANT: i64 = 1_000;

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
    pool: PgPool,
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
        sqlx::query(
            r#"
            create table if not exists player_profiles (
                player_id text primary key,
                display_name text not null,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists ssh_identities (
                username text not null,
                key_fingerprint text not null,
                player_id text not null references player_profiles(player_id) on delete cascade,
                created_at timestamptz not null default now(),
                last_seen_at timestamptz not null default now(),
                primary key (username, key_fingerprint),
                unique (player_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists player_states (
                player_id text primary key references player_profiles(player_id) on delete cascade,
                current_view text not null,
                inventory jsonb not null default '[]'::jsonb,
                updated_at timestamptz not null default now()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists world_messages (
                id bigserial primary key,
                kind text not null check (kind in ('mail', 'say', 'broadcast')),
                sender_user text not null,
                sender_player_id text not null,
                target_user text,
                target_player_id text,
                target_view text,
                body text not null,
                created_at timestamptz not null default now(),
                expires_at timestamptz,
                check (
                    (kind = 'mail' and expires_at is null)
                    or (kind in ('say', 'broadcast') and expires_at is not null)
                )
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create index if not exists world_messages_mailbox_idx
            on world_messages (target_user, target_player_id, created_at desc)
            where kind = 'mail'
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create index if not exists world_messages_live_ttl_idx
            on world_messages (kind, expires_at, created_at desc)
            where kind in ('say', 'broadcast')
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists world_accounts (
                account_id text primary key,
                kind text not null check (kind in ('player', 'room', 'system')),
                owner_id text,
                display_name text not null,
                created_at timestamptz not null default now()
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists world_balances (
                account_id text not null references world_accounts(account_id) on delete cascade,
                asset text not null check (asset = 'MARK'),
                amount bigint not null check (amount >= 0),
                updated_at timestamptz not null default now(),
                primary key (account_id, asset)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists world_ledger_entries (
                id bigserial primary key,
                asset text not null check (asset = 'MARK'),
                debit_account_id text references world_accounts(account_id),
                credit_account_id text references world_accounts(account_id),
                amount bigint not null check (amount > 0),
                reason text not null,
                memo text not null default '',
                idempotency_key text unique,
                created_at timestamptz not null default now(),
                check (debit_account_id is not null or credit_account_id is not null)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create index if not exists world_ledger_account_idx
            on world_ledger_entries (
                coalesce(debit_account_id, ''),
                coalesce(credit_account_id, ''),
                created_at desc
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Upserts the SSH public-key identity and profile.
    pub async fn upsert_ssh_identity(
        &self,
        username: &str,
        key_fingerprint: &str,
        player_id: &str,
    ) -> Result<StoredIdentity, StorageError> {
        sqlx::query(
            r#"
            insert into player_profiles (player_id, display_name)
            values ($1, $2)
            on conflict (player_id) do update
            set display_name = excluded.display_name,
                updated_at = now()
            "#,
        )
        .bind(player_id)
        .bind(username)
        .execute(&self.pool)
        .await?;

        let identity = sqlx::query_as::<_, StoredIdentity>(
            r#"
            insert into ssh_identities (username, key_fingerprint, player_id)
            values ($1, $2, $3)
            on conflict (username, key_fingerprint) do update
            set player_id = excluded.player_id,
                last_seen_at = now()
            returning username, key_fingerprint, player_id
            "#,
        )
        .bind(username)
        .bind(key_fingerprint)
        .bind(player_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(identity)
    }

    /// Ensures the player has a MARK wallet and receives the one-time test grant.
    pub async fn ensure_player_wallet(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<StoredBalance, StorageError> {
        let mut tx = self.pool.begin().await?;
        let account_id = player_account_id(player_id);
        ensure_player_account(&mut tx, &account_id, username, player_id).await?;
        ensure_balance_row(&mut tx, &account_id).await?;

        let grant_key = format!("initial_mark_grant:{account_id}");
        let inserted = sqlx::query(
            r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount, reason, memo, idempotency_key
            )
            values ('MARK', null, $1, $2, 'initial_grant', 'initial test MARK grant', $3)
            on conflict (idempotency_key) do nothing
            returning id
            "#,
        )
        .bind(&account_id)
        .bind(INITIAL_MARK_GRANT)
        .bind(&grant_key)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();

        if inserted {
            credit_balance(&mut tx, &account_id, INITIAL_MARK_GRANT).await?;
        }

        let balance = fetch_balance_tx(&mut tx, &account_id).await?;
        tx.commit().await?;
        Ok(balance)
    }

    /// Loads a player's MARK balance.
    pub async fn player_balance(&self, player_id: &str) -> Result<StoredBalance, StorageError> {
        let account_id = player_account_id(player_id);
        fetch_balance_pool(&self.pool, &account_id).await
    }

    /// Transfers MARK from a player to a user or player account.
    pub async fn transfer_mark(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<StoredTransfer, StorageError> {
        if amount <= 0 {
            return Err(StorageError::InvalidAmount(amount));
        }

        let mut tx = self.pool.begin().await?;
        let sender_account_id = player_account_id(sender_player_id);
        ensure_player_account(&mut tx, &sender_account_id, sender_user, sender_player_id).await?;
        ensure_balance_row(&mut tx, &sender_account_id).await?;

        let target = resolve_payment_target(&mut tx, target).await?;
        let target_account_id = player_account_id(&target.player_id);
        ensure_player_account(
            &mut tx,
            &target_account_id,
            &target.username,
            &target.player_id,
        )
        .await?;
        ensure_balance_row(&mut tx, &target_account_id).await?;

        if sender_account_id == target_account_id {
            return Err(StorageError::SelfPayment);
        }

        debit_balance(&mut tx, &sender_account_id, amount).await?;
        credit_balance(&mut tx, &target_account_id, amount).await?;
        let ledger_id = sqlx::query(
            r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount, reason, memo
            )
            values ('MARK', $1, $2, $3, 'player_payment', $4)
            returning id
            "#,
        )
        .bind(&sender_account_id)
        .bind(&target_account_id)
        .bind(amount)
        .bind(memo)
        .fetch_one(&mut *tx)
        .await?
        .get::<i64, _>("id");
        let sender_balance = fetch_balance_tx(&mut tx, &sender_account_id).await?.amount;
        tx.commit().await?;

        Ok(StoredTransfer {
            ledger_id,
            asset: TEST_CURRENCY.to_owned(),
            amount,
            sender_account_id,
            target_account_id,
            target_user: target.username,
            memo: memo.to_owned(),
            sender_balance,
        })
    }

    /// Loads a player state if one has been saved.
    pub async fn load_player_state(
        &self,
        player_id: &str,
    ) -> Result<Option<PlayerState>, StorageError> {
        let Some(row) = sqlx::query_as::<_, PlayerStateRow>(
            r#"
            select player_id, current_view, inventory
            from player_states
            where player_id = $1
            "#,
        )
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(row.try_into()?))
    }

    /// Saves the current player state.
    pub async fn save_player_state(&self, player: &PlayerState) -> Result<(), StorageError> {
        let inventory = serde_json::to_value(&player.inventory)?;
        sqlx::query(
            r#"
            insert into player_states (player_id, current_view, inventory)
            values ($1, $2, $3)
            on conflict (player_id) do update
            set current_view = excluded.current_view,
                inventory = excluded.inventory,
                updated_at = now()
            "#,
        )
        .bind(&player.id)
        .bind(&player.current_view)
        .bind(inventory)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Persists a mailbox message. Mail has no expiry.
    pub async fn save_mail_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        body: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into world_messages (
                kind, sender_user, sender_player_id, target_user, target_player_id, body
            )
            values ('mail', $1, $2, $3, $3, $4)
            "#,
        )
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(target)
        .bind(body)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Persists a same-view say message with a 24 hour expiry.
    pub async fn save_say_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target_view: &str,
        body: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into world_messages (
                kind, sender_user, sender_player_id, target_view, body, expires_at
            )
            values ('say', $1, $2, $3, $4, now() + interval '24 hours')
            "#,
        )
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(target_view)
        .bind(body)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Persists a broadcast message with a 24 hour expiry.
    pub async fn save_broadcast_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into world_messages (
                kind, sender_user, sender_player_id, body, expires_at
            )
            values ('broadcast', $1, $2, $3, now() + interval '24 hours')
            "#,
        )
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(body)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Loads recent mailbox messages for a player identity.
    pub async fn recent_mailbox_messages(
        &self,
        username: &str,
        player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredWorldMessage>, StorageError> {
        let messages = sqlx::query_as::<_, StoredWorldMessage>(
            r#"
            select
                kind,
                sender_user,
                coalesce(target_user, '') as target_user,
                body,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from world_messages
            where kind = 'mail'
              and (target_user = $1 or target_player_id = $2)
            order by created_at desc
            limit $3
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Loads recent unexpired say messages for a view.
    pub async fn recent_view_messages(
        &self,
        view_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredWorldMessage>, StorageError> {
        let messages = sqlx::query_as::<_, StoredWorldMessage>(
            r#"
            select
                kind,
                sender_user,
                coalesce(target_user, '') as target_user,
                body,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from world_messages
            where kind = 'say'
              and target_view = $1
              and expires_at > now()
            order by created_at desc
            limit $2
            "#,
        )
        .bind(view_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Loads recent unexpired broadcast messages for news.
    pub async fn recent_news_messages(
        &self,
        limit: i64,
    ) -> Result<Vec<StoredWorldMessage>, StorageError> {
        let messages = sqlx::query_as::<_, StoredWorldMessage>(
            r#"
            select
                kind,
                sender_user,
                coalesce(target_user, '') as target_user,
                body,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from world_messages
            where kind = 'broadcast'
              and expires_at > now()
            order by created_at desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }
}

/// Stored SSH identity mapping.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredIdentity {
    /// SSH username.
    pub username: String,
    /// Public key fingerprint.
    pub key_fingerprint: String,
    /// Stable player id used by the runtime.
    pub player_id: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PaymentTarget {
    username: String,
    player_id: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PlayerStateRow {
    player_id: String,
    current_view: String,
    inventory: Value,
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
}

fn player_account_id(player_id: &str) -> String {
    format!("player:{player_id}")
}

async fn ensure_player_account(
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

async fn ensure_balance_row(
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

async fn fetch_balance_pool(
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

async fn fetch_balance_tx(
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

async fn credit_balance(
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

async fn debit_balance(
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

async fn resolve_payment_target(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target: &str,
) -> Result<PaymentTarget, StorageError> {
    let row = sqlx::query(
        r#"
        select username, player_id
        from ssh_identities
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
