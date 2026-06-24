//! Database schema migration.

use sqlx::postgres::PgPool;

use crate::StorageError;
use crate::accounts::{SYSTEM_LEDGER_ADJUSTMENT_ACCOUNT_ID, SYSTEM_MARK_ACCOUNT_ID};
use crate::parcels::seed_commercial_parcels;

pub(crate) async fn migrate(pool: &PgPool) -> Result<(), StorageError> {
    migrate_player_profiles(pool).await?;
    migrate_identity_tables(pool).await?;
    migrate_user_accounts(pool).await?;
    migrate_world_messages(pool).await?;
    migrate_inbox_items(pool).await?;
    migrate_ledger(pool).await?;
    migrate_commercial_parcels(pool).await?;
    migrate_service_rooms(pool).await?;
    migrate_shop_payments(pool).await?;
    migrate_marriage_certificates(pool).await?;
    migrate_memory_events(pool).await?;
    migrate_memory_atoms(pool).await?;
    migrate_social_memory(pool).await?;
    Ok(())
}

async fn migrate_marriage_certificates(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists marriage_certificates (
                id bigserial primary key,
                party_a_user text not null,
                party_a_player_id text not null,
                party_b_user text not null,
                party_b_player_id text not null,
                status text not null default 'active'
                    check (status in ('active', 'divorced')),
                fee_amount bigint not null check (fee_amount > 0),
                fee_ledger_ids bigint[] not null default array[]::bigint[],
                certificate_text text not null,
                issued_at timestamptz not null default now(),
                divorced_at timestamptz,
                created_at timestamptz not null default now(),
                constraint marriage_distinct_players
                    check (party_a_player_id <> party_b_player_id)
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "alter table marriage_certificates add column if not exists divorced_at timestamptz",
    )
    .execute(pool)
    .await?;
    repair_status_check_constraint(
        pool,
        "marriage_certificates",
        "marriage_certificates_status_check",
    )
    .await?;

    sqlx::query(
        r#"
            create unique index if not exists marriage_certificates_active_pair_idx
            on marriage_certificates (party_a_player_id, party_b_player_id)
            where status = 'active'
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists marriage_certificates_active_lookup_idx
            on marriage_certificates (status, party_a_player_id, party_b_player_id)
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create table if not exists marriage_certificate_participants (
                certificate_id bigint not null references marriage_certificates(id) on delete cascade,
                player_id text not null,
                username text not null,
                status text not null default 'active'
                    check (status in ('active', 'divorced')),
                created_at timestamptz not null default now(),
                primary key (certificate_id, player_id)
            )
            "#,
    )
    .execute(pool)
    .await?;
    repair_status_check_constraint(
        pool,
        "marriage_certificate_participants",
        "marriage_certificate_participants_status_check",
    )
    .await?;

    sqlx::query(
        r#"
            insert into marriage_certificate_participants (
                certificate_id, player_id, username, status
            )
            select id, party_a_player_id, party_a_user, status
            from marriage_certificates
            on conflict (certificate_id, player_id) do nothing
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            insert into marriage_certificate_participants (
                certificate_id, player_id, username, status
            )
            select id, party_b_player_id, party_b_user, status
            from marriage_certificates
            on conflict (certificate_id, player_id) do nothing
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create unique index if not exists marriage_participants_active_player_idx
            on marriage_certificate_participants (player_id)
            where status = 'active'
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn repair_status_check_constraint(
    pool: &PgPool,
    table_name: &str,
    constraint_name: &str,
) -> Result<(), StorageError> {
    let sql = format!(
        r#"
        do $$
        declare
            existing_name text;
        begin
            for existing_name in
                select conname
                from pg_constraint
                where conrelid = '{table_name}'::regclass
                  and contype = 'c'
                  and pg_get_constraintdef(oid) like '%status%'
            loop
                execute format('alter table {table_name} drop constraint %I', existing_name);
            end loop;

            alter table {table_name}
            add constraint {constraint_name}
            check (status in ('active', 'divorced'));
        end $$;
        "#
    );
    sqlx::query(&sql).execute(pool).await?;
    Ok(())
}

async fn migrate_player_profiles(pool: &PgPool) -> Result<(), StorageError> {
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

    Ok(())
}

async fn migrate_identity_tables(pool: &PgPool) -> Result<(), StorageError> {
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
            create table if not exists view_presence (
                player_id text primary key references player_profiles(player_id) on delete cascade,
                username text not null,
                view_id text not null,
                last_seen_at timestamptz not null default now()
            )
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_user_accounts(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists user_accounts (
                username text primary key,
                player_id text not null unique references player_profiles(player_id) on delete cascade,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now()
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            insert into user_accounts (username, player_id, created_at, updated_at)
            select distinct on (username) username, player_id, created_at, last_seen_at
            from password_identities
            order by username, last_seen_at desc
            on conflict (username) do nothing
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            insert into user_accounts (username, player_id, created_at, updated_at)
            select distinct on (username) username, player_id, created_at, last_seen_at
            from ssh_identities
            order by username, last_seen_at desc
            on conflict (username) do nothing
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            do $$
            begin
                alter table ssh_identities drop constraint if exists ssh_identities_player_id_key;
            end $$;
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create unique index if not exists player_profiles_display_name_unique_idx
            on player_profiles (display_name)
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_world_messages(pool: &PgPool) -> Result<(), StorageError> {
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

    repair_world_message_expiry_constraint(pool).await?;

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

    Ok(())
}

async fn repair_world_message_expiry_constraint(pool: &PgPool) -> Result<(), StorageError> {
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

    Ok(())
}

async fn migrate_inbox_items(pool: &PgPool) -> Result<(), StorageError> {
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

    Ok(())
}

async fn migrate_ledger(pool: &PgPool) -> Result<(), StorageError> {
    migrate_world_accounts(pool).await?;
    migrate_world_balances(pool).await?;
    migrate_world_ledger_entries(pool).await?;
    backfill_legacy_ledger_edges(pool).await?;
    normalize_legacy_self_payment_entries(pool).await?;
    enforce_ledger_entry_constraints(pool).await?;
    create_ledger_indexes(pool).await?;
    Ok(())
}

async fn migrate_world_accounts(pool: &PgPool) -> Result<(), StorageError> {
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
            insert into world_accounts (account_id, kind, owner_id, display_name)
            values
                ($1, 'system', 'system', 'System MARK issuance'),
                ($2, 'system', 'system', 'Legacy ledger adjustment')
            on conflict (account_id) do update
            set display_name = excluded.display_name
            "#,
    )
    .bind(SYSTEM_MARK_ACCOUNT_ID)
    .bind(SYSTEM_LEDGER_ADJUSTMENT_ACCOUNT_ID)
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_world_balances(pool: &PgPool) -> Result<(), StorageError> {
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

    Ok(())
}

async fn migrate_world_ledger_entries(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists world_ledger_entries (
                id bigserial primary key,
                asset text not null check (asset = 'MARK'),
                debit_account_id text not null references world_accounts(account_id),
                credit_account_id text not null references world_accounts(account_id),
                amount bigint not null check (amount > 0),
                reason text not null,
                memo text not null default '',
                idempotency_key text unique,
                created_at timestamptz not null default now(),
                constraint world_ledger_distinct_accounts
                    check (debit_account_id <> credit_account_id)
            )
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn backfill_legacy_ledger_edges(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            update world_ledger_entries
            set debit_account_id = $1
            where debit_account_id is null
              and credit_account_id is not null
            "#,
    )
    .bind(SYSTEM_MARK_ACCOUNT_ID)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            update world_ledger_entries
            set credit_account_id = $1
            where credit_account_id is null
              and debit_account_id is not null
            "#,
    )
    .bind(SYSTEM_MARK_ACCOUNT_ID)
    .execute(pool)
    .await?;

    Ok(())
}

async fn normalize_legacy_self_payment_entries(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount,
                reason, memo, idempotency_key, created_at
            )
            select asset,
                   case when credit_account_id = $1 then $2 else $1 end,
                   credit_account_id,
                   amount,
                   reason,
                   memo,
                   'system:migration:legacy_self_payment_offset:' || id,
                   created_at
            from world_ledger_entries
            where debit_account_id = credit_account_id
            on conflict (idempotency_key) do nothing
            "#,
    )
    .bind(SYSTEM_LEDGER_ADJUSTMENT_ACCOUNT_ID)
    .bind(SYSTEM_MARK_ACCOUNT_ID)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            update world_ledger_entries
            set credit_account_id = case when credit_account_id = $1 then $2 else $1 end
            where debit_account_id = credit_account_id
            "#,
    )
    .bind(SYSTEM_LEDGER_ADJUSTMENT_ACCOUNT_ID)
    .bind(SYSTEM_MARK_ACCOUNT_ID)
    .execute(pool)
    .await?;

    Ok(())
}

async fn enforce_ledger_entry_constraints(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            alter table world_ledger_entries
            alter column debit_account_id set not null,
            alter column credit_account_id set not null
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            do $$
            begin
                if not exists (
                    select 1
                    from pg_constraint
                    where conrelid = 'world_ledger_entries'::regclass
                      and conname = 'world_ledger_distinct_accounts'
                ) then
                    alter table world_ledger_entries
                    add constraint world_ledger_distinct_accounts
                    check (debit_account_id <> credit_account_id);
                end if;
            end
            $$;
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn create_ledger_indexes(pool: &PgPool) -> Result<(), StorageError> {
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

    Ok(())
}

async fn migrate_commercial_parcels(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists commercial_parcels (
                parcel_id text primary key,
                view_id text not null unique,
                district text not null,
                position integer not null,
                owner_user text,
                owner_player_id text,
                room_user text,
                room_player_id text,
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

    sqlx::query("alter table commercial_parcels add column if not exists room_user text")
        .execute(pool)
        .await?;

    sqlx::query("alter table commercial_parcels add column if not exists room_player_id text")
        .execute(pool)
        .await?;

    sqlx::query("alter table commercial_parcels add column if not exists front_view_id text")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
            update commercial_parcels
            set front_view_id = format('street_%s_%s', district, lpad((((position - 1) / 2) + 1)::text, 2, '0'))
            where front_view_id is null
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create unique index if not exists commercial_parcels_room_user_idx
            on commercial_parcels (room_user)
            where room_user is not null
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create unique index if not exists commercial_parcels_room_player_idx
            on commercial_parcels (room_player_id)
            where room_player_id is not null
            "#,
    )
    .execute(pool)
    .await?;

    seed_commercial_parcels(pool).await?;

    Ok(())
}

async fn migrate_service_rooms(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists service_rooms (
                view_id text primary key,
                front_view_id text,
                front_entity_id text,
                address text,
                label text,
                enter_aliases text,
                room_user text not null unique,
                room_player_id text not null unique,
                status_text text,
                custom_commands text,
                enabled boolean not null default true,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now()
            )
            "#,
    )
    .execute(pool)
    .await?;

    for column in [
        "front_view_id text",
        "front_entity_id text",
        "address text",
        "label text",
        "enter_aliases text",
        "status_text text",
        "custom_commands text",
        "enabled boolean not null default true",
    ] {
        sqlx::query(&format!(
            "alter table service_rooms add column if not exists {column}"
        ))
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn migrate_shop_payments(pool: &PgPool) -> Result<(), StorageError> {
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

async fn migrate_memory_events(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists memory_events (
                id bigserial primary key,
                agent_id text not null,
                occurred_at timestamptz not null default now(),
                source text not null,
                event_type text not null,
                actors jsonb not null default '[]'::jsonb,
                content text not null,
                world_refs jsonb not null default '{}'::jsonb,
                salience double precision not null default 0.5
                    check (salience >= 0.0 and salience <= 1.0),
                embedding jsonb,
                created_at timestamptz not null default now()
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists memory_events_agent_time_idx
            on memory_events (agent_id, occurred_at desc, id desc)
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists memory_events_actor_gin_idx
            on memory_events using gin (actors)
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_memory_atoms(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists memory_atoms (
                id bigserial primary key,
                agent_id text not null,
                kind text not null
                    check (kind in ('episodic', 'social', 'self', 'norm', 'goal', 'preference', 'commitment')),
                subject text not null,
                predicate text not null,
                object jsonb not null default '{}'::jsonb,
                summary text not null,
                evidence_event_ids bigint[] not null default array[]::bigint[],
                confidence double precision not null default 0.5
                    check (confidence >= 0.0 and confidence <= 1.0),
                importance double precision not null default 0.5
                    check (importance >= 0.0 and importance <= 1.0),
                emotional_valence double precision not null default 0.0
                    check (emotional_valence >= -1.0 and emotional_valence <= 1.0),
                embedding jsonb,
                created_at timestamptz not null default now(),
                updated_at timestamptz not null default now(),
                expires_at timestamptz,
                unique (agent_id, kind, subject, predicate)
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists memory_atoms_agent_kind_idx
            on memory_atoms (agent_id, kind, importance desc, updated_at desc)
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists memory_atoms_subject_idx
            on memory_atoms (agent_id, subject, importance desc, updated_at desc)
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn migrate_social_memory(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
            create table if not exists social_edges (
                agent_id text not null,
                target_id text not null,
                trust double precision not null default 0.0
                    check (trust >= -1.0 and trust <= 1.0),
                affinity double precision not null default 0.0
                    check (affinity >= -1.0 and affinity <= 1.0),
                obligation double precision not null default 0.0
                    check (obligation >= 0.0 and obligation <= 1.0),
                rivalry double precision not null default 0.0
                    check (rivalry >= 0.0 and rivalry <= 1.0),
                familiarity double precision not null default 0.0
                    check (familiarity >= 0.0 and familiarity <= 1.0),
                tags text[] not null default array[]::text[],
                evidence_memory_ids bigint[] not null default array[]::bigint[],
                updated_at timestamptz not null default now(),
                primary key (agent_id, target_id)
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create table if not exists agent_self_models (
                agent_id text not null,
                version bigint not null,
                identity jsonb not null default '{}'::jsonb,
                current_state jsonb not null default '{}'::jsonb,
                style jsonb not null default '{}'::jsonb,
                derived_from_memory_ids bigint[] not null default array[]::bigint[],
                created_at timestamptz not null default now(),
                primary key (agent_id, version)
            )
            "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
            create index if not exists agent_self_models_latest_idx
            on agent_self_models (agent_id, version desc)
            "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
