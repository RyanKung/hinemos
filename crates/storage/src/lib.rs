#![deny(missing_docs)]

//! Postgres-backed persistence for accounts and player state.

mod messages;
mod schema;
mod types;

use std::future::Future;
use std::pin::Pin;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use hinemos_core::PlayerState;
use rand_core::OsRng;
use serde_json::json;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};

use messages::NewInboxItem;
use types::{
    PlayerStateRow, credit_balance, debit_balance, ensure_balance_row, ensure_player_account,
    fetch_balance_pool, fetch_balance_tx, fetch_parcel_by_id, player_account_id,
    player_id_from_password_username, resolve_payment_target,
};
pub use types::{
    StorageError, StoredAccountSettings, StoredAdmission, StoredBalance, StoredIdentity,
    StoredInboxItem, StoredMailAuthToken, StoredOperatorCommand, StoredParcel,
    StoredPasswordIdentity, StoredPaymentRequest, StoredTransfer, StoredWorldMessage,
};

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

    /// Adds an SSH public-key identity to an existing canonical account.
    pub async fn add_ssh_identity(
        &self,
        username: &str,
        key_fingerprint: &str,
        player_id: &str,
    ) -> Result<StoredIdentity, StorageError> {
        self.ensure_user_account(username, player_id).await?;
        if let Some(owner) = self.ssh_key_owner(key_fingerprint).await?
            && owner != username
        {
            return Err(StorageError::InvalidAccountSetting(format!(
                "ssh key is already bound to {owner}"
            )));
        }

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
            set last_seen_at = now()
            returning username, key_fingerprint, player_id, (xmax = 0) as created
            "#,
        )
        .bind(username)
        .bind(key_fingerprint)
        .bind(player_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(identity)
    }

    /// Authenticates an SSH public-key identity, preserving stored player bindings.
    ///
    /// A new SSH key may create a user only when the username is unused. Existing
    /// usernames must bind new keys from an already-authenticated session.
    pub async fn authenticate_ssh_identity(
        &self,
        username: &str,
        key_fingerprint: &str,
        fallback_player_id: &str,
    ) -> Result<Option<StoredIdentity>, StorageError> {
        if let Some(identity) = sqlx::query_as::<_, StoredIdentity>(
            r#"
            update ssh_identities
            set last_seen_at = now()
            where username = $1 and key_fingerprint = $2
            returning username, key_fingerprint, player_id, false as created
            "#,
        )
        .bind(username)
        .bind(key_fingerprint)
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(Some(identity));
        }

        if self.user_player_id(username).await?.is_some() {
            return Ok(None);
        }

        self.add_ssh_identity(username, key_fingerprint, fallback_player_id)
            .await
            .map(Some)
    }

    /// Replaces the SSH public key bound to an existing player identity.
    pub async fn replace_ssh_identity(
        &self,
        username: &str,
        player_id: &str,
        key_fingerprint: &str,
    ) -> Result<StoredIdentity, StorageError> {
        let mut tx = self.pool.begin().await?;
        self.ensure_user_account(username, player_id).await?;
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
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            delete from ssh_identities
            where username = $1 or player_id = $2
            "#,
        )
        .bind(username)
        .bind(player_id)
        .execute(&mut *tx)
        .await?;

        let identity = sqlx::query_as::<_, StoredIdentity>(
            r#"
            insert into ssh_identities (username, key_fingerprint, player_id)
            values ($1, $2, $3)
            returning username, key_fingerprint, player_id, true as created
            "#,
        )
        .bind(username)
        .bind(key_fingerprint)
        .bind(player_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(identity)
    }

    /// Authenticates or creates the SSH password identity for a username.
    pub async fn authenticate_password_identity(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<StoredPasswordIdentity>, StorageError> {
        if password.is_empty() {
            return Ok(None);
        }

        if let Some(identity) = self
            .verify_existing_password_identity(username, password)
            .await?
        {
            return Ok(Some(identity));
        }

        self.create_password_identity(username, password).await
    }

    /// Sets or rotates the SSH password login secret for an existing player.
    pub async fn set_password_identity(
        &self,
        username: &str,
        player_id: &str,
        password: &str,
    ) -> Result<StoredPasswordIdentity, StorageError> {
        if password.is_empty() {
            return Err(StorageError::InvalidAccountSetting(
                "password must not be empty".to_owned(),
            ));
        }
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(StorageError::from_password_hash)?
            .to_string();

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

        let identity = sqlx::query_as::<_, StoredPasswordIdentity>(
            r#"
            insert into password_identities (username, player_id, password_hash)
            values ($1, $2, $3)
            on conflict (username) do update
            set player_id = excluded.player_id,
                password_hash = excluded.password_hash,
                last_seen_at = now()
            returning username, player_id, false as created
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(identity)
    }

    /// Sets or rotates the SMTP/IMAP auth token for a player.
    pub async fn set_mail_auth_token(
        &self,
        username: &str,
        player_id: &str,
        token: &str,
    ) -> Result<StoredMailAuthToken, StorageError> {
        if token.is_empty() {
            return Err(StorageError::InvalidAccountSetting(
                "mail auth token must not be empty".to_owned(),
            ));
        }
        let salt = SaltString::generate(&mut OsRng);
        let token_hash = Argon2::default()
            .hash_password(token.as_bytes(), &salt)
            .map_err(StorageError::from_password_hash)?
            .to_string();

        self.ensure_user_account(username, player_id).await?;

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

        let token = sqlx::query_as::<_, StoredMailAuthToken>(
            r#"
            insert into mail_auth_tokens (username, player_id, token_hash)
            values ($1, $2, $3)
            on conflict (username) do update
            set player_id = excluded.player_id,
                token_hash = excluded.token_hash,
                updated_at = now()
            returning username, player_id
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(token_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(token)
    }

    /// Authenticates a SMTP/IMAP username with its dedicated mail auth token.
    pub async fn verify_mail_auth_token(
        &self,
        username: &str,
        token: &str,
    ) -> Result<Option<StoredMailAuthToken>, StorageError> {
        if token.is_empty() {
            return Ok(None);
        }
        let Some(row) = sqlx::query(
            r#"
            select username, player_id, token_hash
            from mail_auth_tokens
            where username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let token_hash: String = row.get("token_hash");
        let parsed_hash =
            PasswordHash::new(&token_hash).map_err(StorageError::from_password_hash)?;
        if Argon2::default()
            .verify_password(token.as_bytes(), &parsed_hash)
            .is_err()
        {
            return Ok(None);
        }

        sqlx::query(
            r#"
            update mail_auth_tokens
            set last_seen_at = now()
            where username = $1
            "#,
        )
        .bind(username)
        .execute(&self.pool)
        .await?;

        Ok(Some(StoredMailAuthToken {
            username: row.get("username"),
            player_id: row.get("player_id"),
        }))
    }

    /// Loads account settings for display in SSH sessions.
    pub async fn account_settings(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<StoredAccountSettings, StorageError> {
        let settings = sqlx::query_as::<_, StoredAccountSettings>(
            r#"
            select
                profile.player_id,
                profile.display_name,
                greatest(1, floor(extract(epoch from (now() - profile.created_at)) / 86400)::int + 1) as online_days,
                exists (
                    select 1 from password_identities password
                    where password.username = $1 and password.player_id = $2
                ) as has_password,
                exists (
                    select 1 from mail_auth_tokens token
                    where token.username = $1 and token.player_id = $2
                ) as has_mail_token,
                (
                    select key_fingerprint
                    from ssh_identities ssh
                    where ssh.username = $1 and ssh.player_id = $2
                    order by ssh.last_seen_at desc
                    limit 1
                ) as key_fingerprint
            from player_profiles profile
            where profile.player_id = $2
            "#,
        )
        .bind(username)
        .bind(player_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(settings)
    }

    /// Loads admission state for a player profile.
    pub async fn player_admission(&self, player_id: &str) -> Result<StoredAdmission, StorageError> {
        let admission = sqlx::query_as::<_, StoredAdmission>(
            r#"
            select player_id, admission_state, agreement_version, agreement_read_version
            from player_profiles
            where player_id = $1
            "#,
        )
        .bind(player_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(admission)
    }

    /// Records that the player read the current agreement version.
    pub async fn mark_agreement_read(
        &self,
        player_id: &str,
        agreement_version: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            update player_profiles
            set agreement_read_version = $2,
                agreement_read_at = now(),
                updated_at = now()
            where player_id = $1
            "#,
        )
        .bind(player_id)
        .bind(agreement_version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Marks the player as admitted after accepting the current agreement version.
    pub async fn admit_player(
        &self,
        player_id: &str,
        agreement_version: &str,
    ) -> Result<(), StorageError> {
        let result = sqlx::query(
            r#"
            update player_profiles
            set admission_state = 'agreed',
                agreement_version = $2,
                agreed_at = now(),
                updated_at = now()
            where player_id = $1
              and agreement_read_version = $2
            "#,
        )
        .bind(player_id)
        .bind(agreement_version)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::InvalidAccountSetting(
                "read the current agreement before agreeing".to_owned(),
            ));
        }
        Ok(())
    }

    async fn verify_existing_password_identity(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<StoredPasswordIdentity>, StorageError> {
        let Some(row) = sqlx::query(
            r#"
            select username, player_id, password_hash
            from password_identities
            where username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let password_hash: String = row.get("password_hash");
        let parsed_hash =
            PasswordHash::new(&password_hash).map_err(StorageError::from_password_hash)?;
        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_err()
        {
            return Ok(None);
        }

        sqlx::query(
            r#"
            update password_identities
            set last_seen_at = now()
            where username = $1
            "#,
        )
        .bind(username)
        .execute(&self.pool)
        .await?;

        Ok(Some(StoredPasswordIdentity {
            username: row.get("username"),
            player_id: row.get("player_id"),
            created: false,
        }))
    }

    async fn create_password_identity(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Option<StoredPasswordIdentity>, StorageError> {
        if self.user_player_id(username).await?.is_some() {
            return Ok(None);
        }
        let player_id = player_id_from_password_username(username);
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(StorageError::from_password_hash)?
            .to_string();

        self.ensure_user_account(username, &player_id).await?;

        sqlx::query(
            r#"
            insert into player_profiles (player_id, display_name)
            values ($1, $2)
            on conflict (player_id) do update
            set display_name = excluded.display_name,
                updated_at = now()
            "#,
        )
        .bind(&player_id)
        .bind(username)
        .execute(&self.pool)
        .await?;

        let inserted = sqlx::query_as::<_, StoredPasswordIdentity>(
            r#"
            insert into password_identities (username, player_id, password_hash)
            values ($1, $2, $3)
            on conflict (username) do nothing
            returning username, player_id, true as created
            "#,
        )
        .bind(username)
        .bind(&player_id)
        .bind(password_hash)
        .fetch_optional(&self.pool)
        .await?;

        if inserted.is_some() {
            return Ok(inserted);
        }

        self.verify_existing_password_identity(username, password)
            .await
    }

    async fn user_player_id(&self, username: &str) -> Result<Option<String>, StorageError> {
        let player_id = sqlx::query_scalar::<_, String>(
            r#"
            select player_id
            from user_accounts
            where username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(player_id)
    }

    async fn ssh_key_owner(&self, key_fingerprint: &str) -> Result<Option<String>, StorageError> {
        let username = sqlx::query_scalar::<_, String>(
            r#"
            select username
            from ssh_identities
            where key_fingerprint = $1
            limit 1
            "#,
        )
        .bind(key_fingerprint)
        .fetch_optional(&self.pool)
        .await?;
        Ok(username)
    }

    async fn ensure_user_account(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<(), StorageError> {
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

        let row = sqlx::query(
            r#"
            insert into user_accounts (username, player_id)
            values ($1, $2)
            on conflict (username) do update
            set updated_at = now()
            returning player_id
            "#,
        )
        .bind(username)
        .bind(player_id)
        .fetch_one(&self.pool)
        .await?;
        let stored_player_id: String = row.get("player_id");
        if stored_player_id != player_id {
            return Err(StorageError::InvalidAccountSetting(format!(
                "username {username} belongs to another player"
            )));
        }
        Ok(())
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
        let subject = format!("Shop command for {}", parcel.parcel_id);
        self.create_inbox_item(NewInboxItem {
            kind: "shop_command",
            recipient_user: owner_user,
            recipient_player_id: owner_player_id,
            sender_user,
            sender_player_id,
            subject: &subject,
            body: raw_input,
            source_kind: Some("operator_command"),
            source_id: Some(command.id),
            payload: json!({
                "parcelId": parcel.parcel_id,
                "viewId": parcel.view_id,
                "commandId": command.id,
                "rawInput": raw_input
            }),
        })
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
        let subject = format!("Payment request #{}", request.id);
        let body = format!(
            "{} requests {} {} for shop command #{} in {}. Accept with /pay accept {}.",
            request.payee_user,
            request.amount,
            request.asset,
            request.operator_command_id,
            request.parcel_id,
            request.id
        );
        self.create_inbox_item(NewInboxItem {
            kind: "payment_request",
            recipient_user: &request.payer_user,
            recipient_player_id: &request.payer_player_id,
            sender_user: &request.payee_user,
            sender_player_id: &request.payee_player_id,
            subject: &subject,
            body: &body,
            source_kind: Some("payment_request"),
            source_id: Some(request.id),
            payload: json!({
                "requestId": request.id,
                "operatorCommandId": request.operator_command_id,
                "parcelId": request.parcel_id,
                "amount": request.amount,
                "asset": request.asset
            }),
        })
        .await?;
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
}
