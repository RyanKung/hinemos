#![deny(missing_docs)]

//! Postgres-backed persistence for accounts and player state.

mod messages;
mod schema;
mod storage_ext;
mod storage_memory;
mod storage_payments;
mod types;

use std::future::Future;
use std::pin::Pin;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use hinemos_core::PlayerState;
use rand_core::OsRng;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};

use messages::NewInboxItem;
pub(crate) use types::PlayerStateRow;
use types::player_id_from_password_username;
pub use types::{
    NewMemoryAtom, NewMemoryEvent, StorageError, StoredAccountSettings, StoredAdmission,
    StoredAgentSelfModel, StoredBalance, StoredIdentity, StoredInboxItem, StoredMailAuthToken,
    StoredMemoryAtom, StoredMemoryEvent, StoredOperatorCommand, StoredParcel,
    StoredPasswordIdentity, StoredPaymentRequest, StoredServiceRoom, StoredSocialEdge,
    StoredTransfer, StoredWorldMessage,
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

fn room_mail_user(parcel_id: &str) -> String {
    format!("room-{parcel_id}")
}

fn room_mail_player_id(parcel_id: &str) -> String {
    format!("room:{parcel_id}")
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

    /// Sets or rotates the SMTP/IMAP auth token for a room mailbox.
    pub async fn set_room_mail_auth_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<StoredMailAuthToken, StorageError> {
        if token.is_empty() {
            return Err(StorageError::InvalidAccountSetting(
                "room mail auth token must not be empty".to_owned(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let room_user = room_mail_user(parcel_id);
        let room_player_id = room_mail_player_id(parcel_id);
        let parcel = sqlx::query(
            r#"
            update commercial_parcels
            set room_user = coalesce(room_user, $3),
                room_player_id = coalesce(room_player_id, $4),
                updated_at = now()
            where parcel_id = $1
              and owner_player_id = $2
            returning coalesce(room_user, $3) as room_user,
                      coalesce(room_player_id, $4) as room_player_id
            "#,
        )
        .bind(parcel_id)
        .bind(owner_player_id)
        .bind(&room_user)
        .bind(&room_player_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(parcel) = parcel else {
            return Err(StorageError::NotParcelOwner(parcel_id.to_owned()));
        };
        let room_user: String = parcel.get("room_user");
        let room_player_id: String = parcel.get("room_player_id");

        let salt = SaltString::generate(&mut OsRng);
        let token_hash = Argon2::default()
            .hash_password(token.as_bytes(), &salt)
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
        .bind(&room_player_id)
        .bind(&room_user)
        .execute(&mut *tx)
        .await?;

        let auth = sqlx::query_as::<_, StoredMailAuthToken>(
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
        .bind(room_user)
        .bind(room_player_id)
        .bind(token_hash)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(auth)
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
}
