#![deny(missing_docs)]

//! Postgres-backed persistence for accounts and player state.

use agentopia_core::PlayerState;
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use thiserror::Error;

/// Database-backed storage facade.
#[derive(Debug, Clone)]
pub struct PgStorage {
    pool: PgPool,
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
}
