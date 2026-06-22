use sqlx::Row;
use sqlx::postgres::PgPool;

use crate::{StorageError, StoredBalance};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaymentTarget {
    pub(crate) username: String,
    pub(crate) player_id: String,
}

pub(crate) const SYSTEM_MARK_ACCOUNT_ID: &str = "system:mark";
pub(crate) const SYSTEM_LEDGER_ADJUSTMENT_ACCOUNT_ID: &str = "system:ledger-adjustment";

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

pub(crate) async fn ensure_system_account(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        insert into world_accounts (account_id, kind, owner_id, display_name)
        values ($1, 'system', 'system', 'System MARK issuance')
        on conflict (account_id) do update
        set display_name = excluded.display_name
        "#,
    )
    .bind(SYSTEM_MARK_ACCOUNT_ID)
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
