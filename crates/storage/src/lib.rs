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

        sqlx::query(
            r#"
            create table if not exists commercial_parcels (
                parcel_id text primary key,
                view_id text not null unique,
                district text not null,
                position integer not null,
                owner_user text,
                owner_player_id text,
                status text not null default 'vacant'
                    check (status in ('vacant', 'claimed', 'built')),
                title text,
                description text,
                style text,
                operator_prompt text,
                custom_commands text,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now(),
                unique (district, position)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        seed_commercial_parcels(&self.pool).await?;

        sqlx::query(
            r#"
            create table if not exists operator_commands (
                id bigserial primary key,
                view_id text not null,
                parcel_id text not null,
                sender_user text not null,
                sender_player_id text not null,
                owner_user text not null,
                owner_player_id text not null,
                raw_input text not null,
                status text not null default 'pending'
                    check (status in ('pending', 'delivered', 'handled')),
                created_at timestamptz not null default now(),
                delivered_at timestamptz
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create index if not exists operator_commands_owner_idx
            on operator_commands (owner_player_id, created_at desc)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create table if not exists payment_requests (
                id bigserial primary key,
                operator_command_id bigint not null references operator_commands(id) on delete cascade,
                parcel_id text not null,
                payer_user text not null,
                payer_player_id text not null,
                payee_user text not null,
                payee_player_id text not null,
                asset text not null check (asset = 'MARK'),
                amount bigint not null check (amount > 0),
                memo text not null default '',
                delivery text not null,
                status text not null default 'pending'
                    check (status in ('pending', 'paid', 'cancelled')),
                ledger_id bigint references world_ledger_entries(id),
                created_at timestamptz not null default now(),
                paid_at timestamptz
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            create index if not exists payment_requests_payer_idx
            on payment_requests (payer_player_id, status, created_at desc)
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

    /// Lists all commercial parcels.
    pub async fn list_commercial_parcels(&self) -> Result<Vec<StoredParcel>, StorageError> {
        let parcels = sqlx::query_as::<_, StoredParcel>(
            r#"
            select parcel_id, view_id, district, position, owner_user, owner_player_id,
                   status, title, description, style, operator_prompt, custom_commands
            from commercial_parcels
            order by district, position
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(parcels)
    }

    /// Loads a commercial parcel by parcel id.
    pub async fn commercial_parcel(&self, parcel_id: &str) -> Result<StoredParcel, StorageError> {
        fetch_parcel_by_id(&self.pool, parcel_id).await
    }

    /// Loads a commercial parcel by view id.
    pub async fn commercial_parcel_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<StoredParcel>, StorageError> {
        let parcel = sqlx::query_as::<_, StoredParcel>(
            r#"
            select parcel_id, view_id, district, position, owner_user, owner_player_id,
                   status, title, description, style, operator_prompt, custom_commands
            from commercial_parcels
            where view_id = $1
            "#,
        )
        .bind(view_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(parcel)
    }

    /// Claims a free commercial parcel.
    pub async fn claim_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
    ) -> Result<StoredParcel, StorageError> {
        let updated = sqlx::query_as::<_, StoredParcel>(
            r#"
            update commercial_parcels
            set owner_user = $2,
                owner_player_id = $3,
                status = 'claimed',
                updated_at = now()
            where parcel_id = $1
              and owner_player_id is null
            returning parcel_id, view_id, district, position, owner_user, owner_player_id,
                      status, title, description, style, operator_prompt, custom_commands
            "#,
        )
        .bind(parcel_id)
        .bind(owner_user)
        .bind(owner_player_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(parcel) = updated {
            return Ok(parcel);
        }

        let existing = self.commercial_parcel(parcel_id).await?;
        if existing.owner_player_id.is_some() {
            Err(StorageError::ParcelAlreadyOwned(parcel_id.to_owned()))
        } else {
            Ok(existing)
        }
    }

    /// Transfers a commercial parcel to another known player.
    pub async fn transfer_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<StoredParcel, StorageError> {
        let mut tx = self.pool.begin().await?;
        let target = resolve_payment_target(&mut tx, target).await?;
        let updated = sqlx::query_as::<_, StoredParcel>(
            r#"
            update commercial_parcels
            set owner_user = $3,
                owner_player_id = $4,
                updated_at = now()
            where parcel_id = $1
              and owner_player_id = $2
            returning parcel_id, view_id, district, position, owner_user, owner_player_id,
                      status, title, description, style, operator_prompt, custom_commands
            "#,
        )
        .bind(parcel_id)
        .bind(owner_player_id)
        .bind(&target.username)
        .bind(&target.player_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        updated.ok_or_else(|| StorageError::NotParcelOwner(parcel_id.to_owned()))
    }

    /// Updates one build sheet field for an owned parcel.
    pub async fn update_parcel_build_field(
        &self,
        view_id: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<StoredParcel, StorageError> {
        let column = match field {
            "title" => "title",
            "description" => "description",
            "style" => "style",
            "prompt" => "operator_prompt",
            "commands" => "custom_commands",
            _ => return Err(StorageError::UnknownBuildField(field.to_owned())),
        };
        let query = format!(
            "update commercial_parcels set {column} = $3, updated_at = now() \
             where view_id = $1 and owner_player_id = $2 \
             returning parcel_id, view_id, district, position, owner_user, owner_player_id, \
                       status, title, description, style, operator_prompt, custom_commands"
        );
        let updated = sqlx::query_as::<_, StoredParcel>(&query)
            .bind(view_id)
            .bind(owner_player_id)
            .bind(value)
            .fetch_optional(&self.pool)
            .await?;

        updated.ok_or_else(|| StorageError::NotParcelOwner(view_id.to_owned()))
    }

    /// Publishes an owned parcel build sheet.
    pub async fn publish_parcel_build(
        &self,
        view_id: &str,
        owner_player_id: &str,
    ) -> Result<StoredParcel, StorageError> {
        let updated = sqlx::query_as::<_, StoredParcel>(
            r#"
            update commercial_parcels
            set status = 'built',
                updated_at = now()
            where view_id = $1
              and owner_player_id = $2
              and coalesce(title, '') <> ''
              and coalesce(description, '') <> ''
            returning parcel_id, view_id, district, position, owner_user, owner_player_id,
                      status, title, description, style, operator_prompt, custom_commands
            "#,
        )
        .bind(view_id)
        .bind(owner_player_id)
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or_else(|| StorageError::BuildNotPublishable(view_id.to_owned()))
    }

    /// Stores a raw visitor command for a shop operator.
    pub async fn save_operator_command(
        &self,
        parcel: &StoredParcel,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        delivered: bool,
    ) -> Result<StoredOperatorCommand, StorageError> {
        let owner_user = parcel
            .owner_user
            .as_deref()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id.clone()))?;
        let owner_player_id = parcel
            .owner_player_id
            .as_deref()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id.clone()))?;
        let status = if delivered { "delivered" } else { "pending" };
        let command = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            insert into operator_commands (
                view_id, parcel_id, sender_user, sender_player_id,
                owner_user, owner_player_id, raw_input, status, delivered_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8,
                    case when $8 = 'delivered' then now() else null end)
            returning id, view_id, parcel_id, sender_user, sender_player_id,
                      owner_user, owner_player_id, raw_input, status,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(&parcel.view_id)
        .bind(&parcel.parcel_id)
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(owner_user)
        .bind(owner_player_id)
        .bind(raw_input)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;
        Ok(command)
    }

    /// Loads recent raw visitor commands for shops owned by a player.
    pub async fn recent_operator_commands(
        &self,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredOperatorCommand>, StorageError> {
        let commands = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            select id, view_id, parcel_id, sender_user, sender_player_id,
                   owner_user, owner_player_id, raw_input, status,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from operator_commands
            where owner_player_id = $1
            order by id desc
            limit $2
            "#,
        )
        .bind(owner_player_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(commands)
    }

    /// Creates a payment request from a shop command owned by the operator.
    pub async fn create_payment_request(
        &self,
        operator_command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<StoredPaymentRequest, StorageError> {
        if amount <= 0 {
            return Err(StorageError::InvalidAmount(amount));
        }
        let mut tx = self.pool.begin().await?;
        let command = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            select id, view_id, parcel_id, sender_user, sender_player_id,
                   owner_user, owner_player_id, raw_input, status,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from operator_commands
            where id = $1
            for update
            "#,
        )
        .bind(operator_command_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StorageError::OperatorCommandNotFound(operator_command_id))?;

        if command.owner_player_id != owner_player_id {
            return Err(StorageError::NotParcelOwner(command.parcel_id));
        }

        let memo = format!("shop command #{}", command.id);
        let request = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            insert into payment_requests (
                operator_command_id, parcel_id, payer_user, payer_player_id,
                payee_user, payee_player_id, asset, amount, memo, delivery
            )
            values ($1, $2, $3, $4, $5, $6, 'MARK', $7, $8, $9)
            returning id, operator_command_id, parcel_id, payer_user, payer_player_id,
                      payee_user, payee_player_id, asset, amount, memo, delivery,
                      status, ledger_id,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(command.id)
        .bind(command.parcel_id)
        .bind(command.sender_user)
        .bind(command.sender_player_id)
        .bind(command.owner_user)
        .bind(command.owner_player_id)
        .bind(amount)
        .bind(memo)
        .bind(delivery)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            update operator_commands
            set status = 'handled'
            where id = $1
            "#,
        )
        .bind(command.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(request)
    }

    /// Loads pending payment requests for a player.
    pub async fn pending_payment_requests(
        &self,
        payer_player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredPaymentRequest>, StorageError> {
        let requests = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            select id, operator_command_id, parcel_id, payer_user, payer_player_id,
                   payee_user, payee_player_id, asset, amount, memo, delivery,
                   status, ledger_id,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from payment_requests
            where payer_player_id = $1
              and status = 'pending'
            order by id desc
            limit $2
            "#,
        )
        .bind(payer_player_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(requests)
    }

    /// Accepts a pending payment request, transfers MARK, and returns the paid request.
    pub async fn accept_payment_request(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<(StoredPaymentRequest, i64), StorageError> {
        let mut tx = self.pool.begin().await?;
        let request = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            select id, operator_command_id, parcel_id, payer_user, payer_player_id,
                   payee_user, payee_player_id, asset, amount, memo, delivery,
                   status, ledger_id,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from payment_requests
            where id = $1
            for update
            "#,
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StorageError::PaymentRequestNotFound(request_id))?;

        if request.payer_player_id != payer_player_id {
            return Err(StorageError::PaymentRequestForbidden(request_id));
        }
        if request.status != "pending" {
            return Err(StorageError::PaymentRequestNotPending(request_id));
        }

        let sender_account_id = player_account_id(payer_player_id);
        let target_account_id = player_account_id(&request.payee_player_id);
        ensure_player_account(&mut tx, &sender_account_id, payer_user, payer_player_id).await?;
        ensure_player_account(
            &mut tx,
            &target_account_id,
            &request.payee_user,
            &request.payee_player_id,
        )
        .await?;
        ensure_balance_row(&mut tx, &sender_account_id).await?;
        ensure_balance_row(&mut tx, &target_account_id).await?;

        debit_balance(&mut tx, &sender_account_id, request.amount).await?;
        credit_balance(&mut tx, &target_account_id, request.amount).await?;
        let ledger_id = sqlx::query(
            r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount, reason, memo
            )
            values ('MARK', $1, $2, $3, 'shop_payment_request', $4)
            returning id
            "#,
        )
        .bind(&sender_account_id)
        .bind(&target_account_id)
        .bind(request.amount)
        .bind(&request.memo)
        .fetch_one(&mut *tx)
        .await?
        .get::<i64, _>("id");

        let paid = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            update payment_requests
            set status = 'paid',
                ledger_id = $2,
                paid_at = now()
            where id = $1
            returning id, operator_command_id, parcel_id, payer_user, payer_player_id,
                      payee_user, payee_player_id, asset, amount, memo, delivery,
                      status, ledger_id,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(request_id)
        .bind(ledger_id)
        .fetch_one(&mut *tx)
        .await?;
        let sender_balance = fetch_balance_tx(&mut tx, &sender_account_id).await?.amount;
        tx.commit().await?;
        Ok((paid, sender_balance))
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
}

async fn seed_commercial_parcels(pool: &PgPool) -> Result<(), StorageError> {
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

async fn fetch_parcel_by_id(pool: &PgPool, parcel_id: &str) -> Result<StoredParcel, StorageError> {
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
