use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hinemos_storage::{PgStorage, SHOP_BADGE_AWARD_ACTIVE, SHOP_BADGE_AWARD_REVOKED, StorageError};

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
            "hinemos_storage_shop_badges_{}_{}_{}",
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
        eprintln!("skipping shop badge storage test because DATABASE_URL is not configured");
        true
    }
}

async fn storage_with_built_shop() -> (TestDatabase, PgStorage) {
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    storage
        .add_ssh_identity("owner", "test:owner", "player:owner")
        .await
        .expect("owner identity");
    storage
        .add_ssh_identity("newowner", "test:newowner", "player:newowner")
        .await
        .expect("new owner identity");
    storage
        .add_ssh_identity("customer", "test:customer", "player:customer")
        .await
        .expect("customer identity");
    db.query_value(
        "update commercial_parcels
         set owner_user = 'owner',
             owner_player_id = 'player:owner',
             room_user = 'room-N1',
             room_player_id = 'room:parcel:N1',
             status = 'built',
             title = 'Offline Tool Broker',
             description = 'Tools',
             style = 'quiet',
             operator_prompt = 'help',
             custom_commands = '/hello'
         where parcel_id = 'N1'",
    );
    (db, storage)
}

#[tokio::test]
async fn badge_award_lifecycle_is_persisted_and_idempotent() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_shop().await;

    let badge = storage
        .create_shop_badge(
            "N1",
            "player:owner",
            "patron",
            "Good Patron",
            Some("Paid and polite"),
        )
        .await
        .expect("create badge");
    assert_eq!(badge.slug, "patron");

    let updated = storage
        .create_shop_badge("N1", "player:owner", "patron", "Great Patron", None)
        .await
        .expect("update badge");
    assert_eq!(updated.id, badge.id);
    assert_eq!(updated.title, "Great Patron");

    assert!(matches!(
        storage
            .award_shop_badge("N1", "patron", "customer", "player:customer", "owner", None)
            .await,
        Err(StorageError::NotParcelOwner(_))
    ));

    let award = storage
        .award_shop_badge(
            "N1",
            "patron",
            "owner",
            "player:owner",
            "customer",
            Some("first visit"),
        )
        .await
        .expect("award badge");
    assert_eq!(award.recipient_user, "customer");
    assert_eq!(award.badge_title, "Great Patron");
    assert_eq!(award.status, SHOP_BADGE_AWARD_ACTIVE);

    let duplicate = storage
        .award_shop_badge(
            "N1",
            "patron",
            "owner",
            "player:owner",
            "customer",
            Some("ignored duplicate"),
        )
        .await
        .expect("duplicate award is idempotent");
    assert_eq!(duplicate.id, award.id);
    assert_eq!(
        db.query_value("select count(*) from shop_badge_awards where status = 'active'"),
        "1"
    );

    let visible = storage
        .shop_badges_for_target("customer", 10)
        .await
        .expect("badges for target");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].issuer_user, "owner");
    assert_eq!(visible[0].parcel_id, "N1");

    let revoked = storage
        .revoke_shop_badge("N1", "patron", "player:owner", "customer")
        .await
        .expect("revoke badge");
    assert_eq!(revoked.status, SHOP_BADGE_AWARD_REVOKED);
    assert_eq!(
        storage
            .shop_badges_for_player("player:customer", 10)
            .await
            .expect("badges after revoke")
            .len(),
        0
    );

    let reawarded = storage
        .award_shop_badge(
            "N1",
            "patron",
            "owner",
            "player:owner",
            "customer",
            Some("return visit"),
        )
        .await
        .expect("re-award badge");
    assert_ne!(
        reawarded.id, award.id,
        "re-awarding after revoke should append a new audit row"
    );
    assert_eq!(reawarded.status, SHOP_BADGE_AWARD_ACTIVE);
    assert_eq!(
        db.query_value("select count(*) from shop_badge_awards where recipient_user = 'customer'"),
        "2"
    );
    assert_eq!(
        db.query_value("select count(*) from shop_badge_awards where status = 'active'"),
        "1"
    );
    assert_eq!(
        db.query_value("select count(*) from shop_badge_awards where status = 'revoked'"),
        "1"
    );
    assert_eq!(
        db.query_value(&format!(
            "select revoked_at is not null from shop_badge_awards where id = {}",
            award.id
        )),
        "t"
    );
    let visible_after_reaward = storage
        .shop_badges_for_player("player:customer", 10)
        .await
        .expect("badges after re-award");
    assert_eq!(visible_after_reaward.len(), 1);
    assert_eq!(visible_after_reaward[0].id, reawarded.id);
}

#[tokio::test]
async fn badge_owner_permission_follows_current_parcel_owner() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_shop().await;
    storage
        .create_shop_badge("N1", "player:owner", "patron", "Good Patron", None)
        .await
        .expect("create badge");
    db.query_value(
        "update commercial_parcels
         set owner_user = 'newowner',
             owner_player_id = 'player:newowner'
         where parcel_id = 'N1'",
    );

    assert!(matches!(
        storage
            .award_shop_badge("N1", "patron", "owner", "player:owner", "customer", None)
            .await,
        Err(StorageError::NotParcelOwner(_))
    ));
    let award = storage
        .award_shop_badge(
            "N1",
            "patron",
            "newowner",
            "player:newowner",
            "customer",
            None,
        )
        .await
        .expect("new owner can award");
    assert_eq!(award.issuer_user, "newowner");
}
