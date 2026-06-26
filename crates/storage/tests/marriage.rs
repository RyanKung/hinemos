use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hinemos_storage::{PgStorage, StorageError};

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
            "hinemos_storage_marriage_{}_{}_{}",
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

async fn storage_with_players() -> (TestDatabase, PgStorage) {
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    seed_player(&db, "alice", "player:alice", "hinemos_registry");
    seed_player(&db, "bob", "player:bob", "hinemos_registry");
    storage
        .ensure_player_wallet("alice", "player:alice")
        .await
        .expect("alice wallet");
    storage
        .ensure_player_wallet("bob", "player:bob")
        .await
        .expect("bob wallet");
    (db, storage)
}

fn skip_without_database() -> bool {
    if maybe_database_url().is_some() {
        false
    } else {
        eprintln!("skipping marriage storage test because DATABASE_URL is not configured");
        true
    }
}

fn seed_player(db: &TestDatabase, user: &str, player_id: &str, view_id: &str) {
    db.query_value(&format!(
        "insert into player_profiles (player_id, display_name, admission_state)
         values ('{player_id}', '{user}', 'agreed');
         insert into ssh_identities (username, key_fingerprint, player_id)
         values ('{user}', 'test:{user}', '{player_id}');
         insert into view_presence (player_id, username, view_id, last_seen_at)
         values ('{player_id}', '{user}', '{view_id}', now())
         on conflict (player_id) do update
         set username = excluded.username,
             view_id = excluded.view_id,
             last_seen_at = excluded.last_seen_at;"
    ));
}

#[tokio::test]
async fn register_marriage_charges_both_players_and_persists_certificate() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_players().await;

    let certificate = storage
        .register_marriage("alice", "player:alice", "bob", 25, "hinemos_registry")
        .await
        .expect("register marriage");

    assert!(certificate.certificate_text.contains("alice"));
    assert!(certificate.certificate_text.contains("bob"));
    assert_eq!(certificate.fee_amount, 25);
    assert_eq!(
        storage
            .player_balance("player:alice")
            .await
            .expect("alice balance")
            .amount,
        975
    );
    assert_eq!(
        storage
            .player_balance("player:bob")
            .await
            .expect("bob balance")
            .amount,
        975
    );
    assert_eq!(
        db.query_value("select count(*) from marriage_certificates where status = 'active'"),
        "1"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from marriage_certificate_participants
             where status = 'active'"
        ),
        "2"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where kind = 'marriage_certificate'
               and sender_user = 'room-hinemos_registry'"
        ),
        "2"
    );
}

#[tokio::test]
async fn register_marriage_rolls_back_when_target_is_not_present() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_players().await;
    db.query_value("update view_presence set view_id = 'official_street' where username = 'bob'");

    let error = storage
        .register_marriage("alice", "player:alice", "bob", 25, "hinemos_registry")
        .await
        .expect_err("target should not be present");

    assert!(matches!(error, StorageError::MarriagePartnerNotPresent(_)));
    assert_eq!(
        storage
            .player_balance("player:alice")
            .await
            .expect("alice balance")
            .amount,
        1000
    );
    assert_eq!(
        db.query_value("select count(*) from marriage_certificates"),
        "0"
    );
}

#[tokio::test]
async fn register_marriage_rejects_duplicate_active_marriage() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_players().await;
    storage
        .register_marriage("alice", "player:alice", "bob", 25, "hinemos_registry")
        .await
        .expect("first marriage");

    let error = storage
        .register_marriage("alice", "player:alice", "bob", 25, "hinemos_registry")
        .await
        .expect_err("duplicate marriage should fail");

    assert!(matches!(error, StorageError::MarriageAlreadyActive(_)));
}

#[tokio::test]
async fn register_marriage_rejects_self_marriage() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_players().await;

    let error = storage
        .register_marriage("alice", "player:alice", "alice", 25, "hinemos_registry")
        .await
        .expect_err("self marriage should fail");

    assert!(matches!(error, StorageError::SelfMarriage));
}

#[tokio::test]
async fn divorce_marriage_marks_certificate_inactive_and_allows_remarriage() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_players().await;
    let certificate = storage
        .register_marriage("alice", "player:alice", "bob", 25, "hinemos_registry")
        .await
        .expect("register marriage");

    let divorce = storage
        .divorce_marriage("alice", "player:alice")
        .await
        .expect("divorce marriage");

    assert_eq!(divorce.id, certificate.id);
    assert_eq!(divorce.status, "divorced");
    assert_eq!(
        db.query_value("select count(*) from marriage_certificates where status = 'active'"),
        "0"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from marriage_certificate_participants
             where status = 'active'"
        ),
        "0"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where kind = 'marriage_divorce'
               and sender_user = 'room-hinemos_registry'"
        ),
        "2"
    );

    storage
        .register_marriage("alice", "player:alice", "bob", 25, "hinemos_registry")
        .await
        .expect("remarriage after divorce");
}

#[tokio::test]
async fn divorce_marriage_rejects_players_without_active_marriage() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_players().await;

    let error = storage
        .divorce_marriage("alice", "player:alice")
        .await
        .expect_err("divorce without active marriage should fail");

    assert!(matches!(error, StorageError::NoActiveMarriage(_)));
}
