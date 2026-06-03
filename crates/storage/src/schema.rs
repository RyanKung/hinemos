//! Database schema migration.

use sqlx::postgres::PgPool;

use crate::StorageError;
use crate::types::seed_commercial_parcels;

pub(crate) async fn migrate(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists player_profiles (
                player_id text primary key,
                display_name text not null,
                admission_state text not null default 'pending',
                agreement_version text,
                agreement_read_version text,
                agreement_read_at timestamptz,
                agreed_at timestamptz,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now()
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            do $$
            declare
                added_admission_state boolean := false;
            begin
                added_admission_state := not exists (
                    select 1
                    from information_schema.columns
                    where table_name = 'player_profiles'
                      and column_name = 'admission_state'
                );

                alter table player_profiles add column if not exists admission_state text not null default 'pending';
                alter table player_profiles add column if not exists agreement_version text;
                alter table player_profiles add column if not exists agreement_read_version text;
                alter table player_profiles add column if not exists agreement_read_at timestamptz;
                alter table player_profiles add column if not exists agreed_at timestamptz;

                if added_admission_state then
                    update player_profiles
                    set admission_state = 'agreed',
                        agreement_version = coalesce(agreement_version, 'legacy'),
                        agreed_at = coalesce(agreed_at, now());
                end if;

                begin
                    alter table player_profiles
                    add constraint player_profiles_admission_state_check
                    check (admission_state in ('pending', 'agreed'));
                exception when duplicate_object then
                    null;
                end;
            end $$;
            "#,
    )
    .execute(pool)
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
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create table if not exists password_identities (
                username text primary key,
                player_id text not null references player_profiles(player_id) on delete cascade,
                password_hash text not null,
                created_at timestamptz not null default now(),
                last_seen_at timestamptz not null default now(),
                unique (player_id)
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create table if not exists mail_auth_tokens (
                username text primary key,
                player_id text not null references player_profiles(player_id) on delete cascade,
                token_hash text not null,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now(),
                last_seen_at timestamptz not null default now(),
                unique (player_id)
            )
            "#,
    )
    .execute(pool)
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
    .execute(pool)
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
                constraint world_messages_expiry_policy check (
                    (kind in ('mail', 'broadcast') and expires_at is null)
                    or (kind = 'say' and expires_at is not null)
                )
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
            r#"
            do $$
            declare
                constraint_name text;
            begin
                for constraint_name in
                    select conname
                    from pg_constraint
                    where conrelid = 'world_messages'::regclass
                      and contype = 'c'
                      and pg_get_constraintdef(oid) like '%expires_at%'
                loop
                    execute format('alter table world_messages drop constraint %I', constraint_name);
                end loop;
            end $$;
            "#,
        )
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
            update world_messages
            set expires_at = null
            where kind = 'broadcast'
              and expires_at is not null
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            do $$
            begin
                alter table world_messages
                add constraint world_messages_expiry_policy check (
                    (kind in ('mail', 'broadcast') and expires_at is null)
                    or (kind = 'say' and expires_at is not null)
                );
            end $$;
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists world_messages_mailbox_idx
            on world_messages (target_user, target_player_id, created_at desc)
            where kind = 'mail'
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists world_messages_live_ttl_idx
            on world_messages (kind, expires_at, created_at desc)
            where kind = 'say'
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists world_messages_news_idx
            on world_messages (created_at desc)
            where kind = 'broadcast'
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create table if not exists inbox_items (
                id bigserial primary key,
                kind text not null,
                recipient_user text not null,
                recipient_player_id text not null,
                sender_user text not null,
                sender_player_id text not null,
                subject text not null,
                body text not null,
                status text not null default 'unread'
                    check (status in ('unread', 'claimed', 'acked', 'archived')),
                source_kind text,
                source_id bigint,
                payload jsonb not null default '{}'::jsonb,
                attempts integer not null default 0,
                lease_until timestamptz,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now(),
                unique (source_kind, source_id, recipient_player_id)
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists inbox_items_recipient_idx
            on inbox_items (recipient_player_id, status, created_at desc)
            "#,
    )
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
    .await?;

    seed_commercial_parcels(pool).await?;

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
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists operator_commands_owner_idx
            on operator_commands (owner_player_id, created_at desc)
            "#,
    )
    .execute(pool)
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
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
            create index if not exists payment_requests_payer_idx
            on payment_requests (payer_player_id, status, created_at desc)
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
