use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hinemos_app::RoleCardUpdate;
use hinemos_core::{Gender, MbtiType};
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
            "hinemos_storage_role_card_{}_{}_{}",
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
        eprintln!("skipping account role-card storage test because DATABASE_URL is not configured");
        true
    }
}

async fn storage_with_player() -> (TestDatabase, PgStorage) {
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    storage
        .add_ssh_identity("ada", "test:fingerprint", "player:ada")
        .await
        .expect("add ssh identity");
    (db, storage)
}

fn assert_valid_mbti(value: Option<&str>) {
    let value = value.expect("MBTI should be present");
    assert!(
        MbtiType::parse(value).is_some(),
        "expected a valid MBTI value, got {value}"
    );
}

#[tokio::test]
async fn role_card_defaults_updates_and_admission_gate_are_persisted() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_player().await;

    let settings = storage
        .account_settings("ada", "player:ada")
        .await
        .expect("account settings");
    assert_eq!(settings.display_name, "ada");
    assert_eq!(settings.gender, "none");
    assert_valid_mbti(settings.mbti.as_deref());
    assert_eq!(settings.self_intro, None);
    assert!(
        storage
            .player_admission("player:ada")
            .await
            .expect("admission")
            .role_card_is_complete()
    );

    storage
        .mark_agreement_read("player:ada", "2026-06-03")
        .await
        .expect("mark agreement read");
    storage
        .admit_player("player:ada", "2026-06-03")
        .await
        .expect("random default MBTI permits admission after reading agreement");

    storage
        .update_role_card("player:ada", RoleCardUpdate::Name("Ada Role".to_owned()))
        .await
        .expect("update name");
    storage
        .update_role_card("player:ada", RoleCardUpdate::Gender(Gender::Female))
        .await
        .expect("update gender");
    storage
        .update_role_card("player:ada", RoleCardUpdate::Mbti(MbtiType::Infp))
        .await
        .expect("update mbti");
    storage
        .update_role_card(
            "player:ada",
            RoleCardUpdate::Intro(Some("Building quiet tools".to_owned())),
        )
        .await
        .expect("update intro");

    let settings = storage
        .account_settings("ada", "player:ada")
        .await
        .expect("updated account settings");
    assert_eq!(settings.display_name, "Ada Role");
    assert_eq!(settings.gender, "female");
    assert_eq!(settings.mbti.as_deref(), Some("INFP"));
    assert_eq!(settings.self_intro.as_deref(), Some("Building quiet tools"));
    assert_eq!(
        storage
            .player_admission("player:ada")
            .await
            .expect("admission")
            .mbti
            .as_deref(),
        Some("INFP")
    );

    assert!(
        storage
            .player_admission("player:ada")
            .await
            .expect("admission")
            .is_agreed()
    );
}

#[tokio::test]
async fn role_card_storage_rejects_invalid_free_text() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_player().await;

    assert!(matches!(
        storage
            .update_role_card("player:ada", RoleCardUpdate::Name("Ada\nRole".to_owned()))
            .await,
        Err(StorageError::InvalidAccountSetting(_))
    ));
    assert!(matches!(
        storage
            .update_role_card(
                "player:ada",
                RoleCardUpdate::Intro(Some("multi\nline".to_owned())),
            )
            .await,
        Err(StorageError::InvalidAccountSetting(_))
    ));
}

#[tokio::test]
async fn edited_role_card_name_survives_returning_ssh_identity() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_player().await;

    storage
        .update_role_card("player:ada", RoleCardUpdate::Name("Ada Role".to_owned()))
        .await
        .expect("update name");
    let identity = storage
        .authenticate_ssh_identity("ada", "test:fingerprint", "player:replacement")
        .await
        .expect("authenticate existing ssh identity")
        .expect("existing ssh identity");
    assert!(!identity.created);
    assert_eq!(identity.player_id, "player:ada");

    let settings = storage
        .account_settings("ada", "player:ada")
        .await
        .expect("account settings after returning login");
    assert_eq!(settings.display_name, "Ada Role");
}

#[tokio::test]
async fn admission_rejects_invalid_default_role_card_name() {
    if skip_without_database() {
        return;
    }
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    let long_user = "a".repeat(65);

    storage
        .add_ssh_identity(&long_user, "test:long-user", "player:long")
        .await
        .expect("add long username identity");
    storage
        .mark_agreement_read("player:long", "2026-06-03")
        .await
        .expect("mark agreement read");

    let admission = storage
        .player_admission("player:long")
        .await
        .expect("admission");
    assert!(!admission.role_card_name_is_valid());
    assert!(admission.role_card_has_mbti());
    assert!(!admission.role_card_is_complete());
    assert!(matches!(
        storage.admit_player("player:long", "2026-06-03").await,
        Err(StorageError::InvalidAccountSetting(_))
    ));

    storage
        .update_role_card("player:long", RoleCardUpdate::Name("Valid Role".to_owned()))
        .await
        .expect("update valid name");
    storage
        .admit_player("player:long", "2026-06-03")
        .await
        .expect("admit after valid role-card name");
}

#[tokio::test]
async fn duplicate_role_card_name_returns_account_setting_error() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_player().await;
    storage
        .add_ssh_identity("bob", "test:bob", "player:bob")
        .await
        .expect("add bob identity");

    let error = storage
        .update_role_card("player:ada", RoleCardUpdate::Name("bob".to_owned()))
        .await
        .expect_err("duplicate role-card name should fail");
    assert!(matches!(error, StorageError::InvalidAccountSetting(_)));
    assert_eq!(
        error.to_string(),
        "invalid account setting: role-card name is already taken"
    );
}
