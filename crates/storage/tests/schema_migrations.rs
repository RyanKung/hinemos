use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hinemos_storage::PgStorage;

static TEST_DATABASE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    name: String,
    url: String,
    base_url: String,
}

impl TestDatabase {
    fn create() -> Self {
        let base_url = database_url();
        let name = format!(
            "hinemos_storage_schema_migrations_{}_{}_{}",
            std::process::id(),
            epoch_nanos(),
            TEST_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        run_psql(&base_url, &format!("create schema {name};"));
        let separator = if base_url.contains('?') { '&' } else { '?' };
        Self {
            name: name.clone(),
            url: format!("{base_url}{separator}options=-csearch_path%3D{name}%2Cpublic"),
            base_url,
        }
    }

    fn execute(&self, sql: &str) {
        run_psql(&self.url, sql);
    }

    fn query_value(&self, sql: &str) -> String {
        let output = Command::new("psql")
            .args([&self.url, "--no-align", "--tuples-only", "--command", sql])
            .output()
            .expect("spawn psql");
        assert!(
            output.status.success(),
            "psql query failed: {}\nsql:\n{}",
            String::from_utf8_lossy(&output.stderr),
            sql
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = Command::new("psql")
            .args([
                &self.base_url,
                "--no-align",
                "--tuples-only",
                "--command",
                &format!("drop schema if exists {} cascade;", self.name),
            ])
            .status();
    }
}

fn database_url() -> String {
    maybe_database_url().expect("DATABASE_URL is required in the shell, .env.test, or .env")
}

fn maybe_database_url() -> Option<String> {
    std::env::var("DATABASE_URL")
        .ok()
        .or_else(|| read_env_value(".env.test", "DATABASE_URL"))
        .or_else(|| read_env_value(".env", "DATABASE_URL"))
        .or_else(|| read_env_value("../.env", "DATABASE_URL"))
        .or_else(|| read_env_value("../../.env", "DATABASE_URL"))
}

fn read_env_value(path: &str, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (candidate, value) = line.split_once('=')?;
        if candidate.trim() == key {
            return Some(value.trim().trim_matches('"').trim_matches('\'').to_owned());
        }
    }
    None
}

fn run_psql(url: &str, sql: &str) {
    let output = Command::new("psql")
        .args([url, "--no-align", "--tuples-only", "--command", sql])
        .output()
        .expect("spawn psql");
    assert!(
        output.status.success(),
        "psql command failed: {}\nsql:\n{}",
        String::from_utf8_lossy(&output.stderr),
        sql
    );
}

fn epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos()
}

fn skip_without_database() -> bool {
    if maybe_database_url().is_some() {
        false
    } else {
        eprintln!("skipping schema migration storage test because DATABASE_URL is not configured");
        true
    }
}

#[tokio::test]
async fn migrate_copies_legacy_shop_tables_to_parcel_tables() {
    if skip_without_database() {
        return;
    }

    let db = TestDatabase::create();
    db.execute(
        r#"
        create table commercial_parcels (
            parcel_id text primary key,
            view_id text not null unique,
            front_view_id text,
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
        );
        create table shop_mailing_lists (
            id bigserial primary key,
            parcel_id text not null references commercial_parcels(parcel_id) on delete cascade,
            owner_player_id text not null,
            slug text not null,
            title text not null,
            description text,
            status text not null default 'open'
                check (status in ('open', 'closed')),
            created_at timestamptz not null default now(),
            updated_at timestamptz not null default now(),
            unique (parcel_id, slug)
        );
        create table shop_mailing_list_subscriptions (
            id bigserial primary key,
            list_id bigint not null references shop_mailing_lists(id) on delete cascade,
            subscriber_user text not null,
            subscriber_player_id text not null,
            status text not null default 'active'
                check (status in ('active', 'unsubscribed')),
            created_at timestamptz not null default now(),
            updated_at timestamptz not null default now(),
            unique (list_id, subscriber_player_id)
        );
        create table shop_mailing_list_posts (
            id bigserial primary key,
            list_id bigint not null references shop_mailing_lists(id) on delete cascade,
            sender_user text not null,
            sender_player_id text not null,
            subject text not null,
            body text not null,
            recipient_count bigint not null check (recipient_count >= 0),
            created_at timestamptz not null default now()
        );
        create table shop_mailing_list_deliveries (
            id bigserial primary key,
            post_id bigint not null references shop_mailing_list_posts(id) on delete cascade,
            recipient_user text not null,
            recipient_player_id text not null,
            inbox_item_id bigint,
            created_at timestamptz not null default now(),
            unique (post_id, recipient_player_id)
        );
        create table shop_badges (
            id bigserial primary key,
            parcel_id text not null references commercial_parcels(parcel_id) on delete cascade,
            owner_player_id text not null,
            slug text not null,
            title text not null,
            description text,
            created_at timestamptz not null default now(),
            updated_at timestamptz not null default now(),
            unique (parcel_id, slug)
        );
        create table shop_badge_awards (
            id bigserial primary key,
            badge_id bigint not null references shop_badges(id) on delete cascade,
            issuer_user text not null,
            issuer_player_id text not null,
            recipient_user text not null,
            recipient_player_id text not null,
            note text,
            status text not null default 'active'
                check (status in ('active', 'revoked')),
            awarded_at timestamptz not null default now(),
            revoked_at timestamptz,
            updated_at timestamptz not null default now()
        );

        insert into commercial_parcels (
            parcel_id, view_id, front_view_id, district, position,
            owner_user, owner_player_id, room_user, room_player_id,
            status, title, description, style, operator_prompt, custom_commands
        )
        values (
            'legacy-01', 'legacy-view', 'legacy-front', 'legacy', 7,
            'legacy-owner', 'player:legacy-owner', 'legacy-room', 'player:legacy-room',
            'built', 'Legacy Paper', 'legacy description', 'ink', 'legacy prompt', '/paper submit'
        );
        insert into shop_mailing_lists (
            id, parcel_id, owner_player_id, slug, title, description, status
        )
        values (
            42, 'legacy-01', 'player:legacy-owner', 'weekly',
            'Legacy Weekly', 'old weekly', 'open'
        );
        insert into shop_mailing_list_subscriptions (
            id, list_id, subscriber_user, subscriber_player_id, status
        )
        values (43, 42, 'subscriber', 'player:subscriber', 'active');
        insert into shop_mailing_list_posts (
            id, list_id, sender_user, sender_player_id, subject, body, recipient_count
        )
        values (44, 42, 'editor', 'player:editor', 'Issue 1', 'Body', 1);
        insert into shop_mailing_list_deliveries (
            id, post_id, recipient_user, recipient_player_id
        )
        values (45, 44, 'subscriber', 'player:subscriber');
        insert into shop_badges (
            id, parcel_id, owner_player_id, slug, title, description
        )
        values (
            46, 'legacy-01', 'player:legacy-owner', 'press-pass',
            'Press Pass', 'old badge'
        );
        insert into shop_badge_awards (
            id, badge_id, issuer_user, issuer_player_id,
            recipient_user, recipient_player_id, note, status
        )
        values (
            47, 46, 'editor', 'player:editor',
            'reporter', 'player:reporter', 'legacy award', 'active'
        );
        "#,
    );

    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate legacy schema");

    assert_eq!(
        db.query_value(
            "select owner_player_id || '|' || status || '|' || front_view_id || '|' || custom_commands
             from parcels
             where parcel_id = 'legacy-01'"
        ),
        "player:legacy-owner|built|legacy-front|/paper submit"
    );
    assert_eq!(
        db.query_value(
            "select list.id || '|' || sub.id || '|' || post.id || '|' || delivery.id || '|' ||
                    list.slug || '|' || post.subject
             from parcel_mailing_lists list
             join parcel_mailing_list_subscriptions sub on sub.list_id = list.id
             join parcel_mailing_list_posts post on post.list_id = list.id
             join parcel_mailing_list_deliveries delivery on delivery.post_id = post.id
             where list.parcel_id = 'legacy-01'"
        ),
        "42|43|44|45|weekly|Issue 1"
    );
    assert_eq!(
        db.query_value(
            "select badge.id || '|' || award.id || '|' || badge.slug || '|' || award.note
             from parcel_badges badge
             join parcel_badge_awards award on award.badge_id = badge.id
             where badge.parcel_id = 'legacy-01'"
        ),
        "46|47|press-pass|legacy award"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from unnest(array[
                 'commercial_parcels',
                 'shop_mailing_lists',
                 'shop_mailing_list_subscriptions',
                 'shop_mailing_list_posts',
                 'shop_mailing_list_deliveries',
                 'shop_badges',
                 'shop_badge_awards'
             ]) legacy(name)
             where to_regclass(format('%I.%I', current_schema(), legacy.name)) is not null"
        ),
        "0"
    );
    assert_eq!(
        db.query_value("select nextval(pg_get_serial_sequence('parcel_mailing_lists', 'id')) > 42"),
        "t"
    );
    assert_eq!(
        db.query_value("select nextval(pg_get_serial_sequence('parcel_badges', 'id')) > 46"),
        "t"
    );
}
