use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hinemos_core::SHOP_MAILING_LISTS_PER_PARCEL_MAX;
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
            "hinemos_storage_shop_mailing_lists_{}_{}_{}",
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
        eprintln!("skipping shop mailing-list storage test because DATABASE_URL is not configured");
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
    storage
        .add_ssh_identity("late", "test:late", "player:late")
        .await
        .expect("late identity");
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
async fn mailing_list_subscription_delivery_and_retry_are_persisted() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_shop().await;

    let list = storage
        .create_shop_mailing_list("N1", "player:owner", "updates", "Shop Updates")
        .await
        .expect("create list");
    assert_eq!(list.slug, "updates");
    assert!(matches!(
        storage
            .create_shop_mailing_list("N1", "player:owner", "updates", "Duplicate")
            .await,
        Err(StorageError::MailingListAlreadyExists { .. })
    ));

    storage
        .subscribe_shop_mailing_list("N1", "updates", "customer", "player:customer")
        .await
        .expect("subscribe");
    assert!(matches!(
        storage
            .subscribe_shop_mailing_list("N1", "updates", "customer", "player:customer")
            .await,
        Err(StorageError::MailingListAlreadySubscribed { .. })
    ));
    assert_eq!(
        storage
            .shop_mailing_list_subscriptions("player:customer")
            .await
            .expect("subscriptions")
            .len(),
        1
    );

    let sent = storage
        .send_shop_mailing_list_post(
            "N1",
            "updates",
            "owner",
            "player:owner",
            "Weekly Deal",
            "Body",
        )
        .await
        .expect("send post");
    assert_eq!(sent.post.recipient_count, 1);
    assert_eq!(sent.deliveries.len(), 1);
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where kind = 'mail'
               and source_kind = 'shop_mailing_list_post'
               and source_id = (select max(id) from shop_mailing_list_posts)"
        ),
        "1"
    );

    storage
        .deliver_shop_mailing_list_post(sent.post.id)
        .await
        .expect("retry delivery");
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where kind = 'mail'
               and source_kind = 'shop_mailing_list_post'
               and source_id = (select max(id) from shop_mailing_list_posts)"
        ),
        "1",
        "retrying delivery should reuse the recipient inbox item"
    );

    storage
        .unsubscribe_shop_mailing_list("N1", "updates", "customer", "player:customer")
        .await
        .expect("unsubscribe");
    assert!(matches!(
        storage
            .send_shop_mailing_list_post("N1", "updates", "owner", "player:owner", "No One", "Body")
            .await,
        Err(StorageError::MailingListNoSubscribers { .. })
    ));

    storage
        .subscribe_shop_mailing_list("N1", "updates", "customer", "player:customer")
        .await
        .expect("resubscribe");
    storage
        .close_shop_mailing_list("N1", "updates", "player:owner")
        .await
        .expect("close list");
    assert!(matches!(
        storage
            .subscribe_shop_mailing_list("N1", "updates", "late", "player:late")
            .await,
        Err(StorageError::MailingListClosed { .. })
    ));
    storage
        .unsubscribe_shop_mailing_list("N1", "updates", "customer", "player:customer")
        .await
        .expect("unsubscribe after close");
}

#[tokio::test]
async fn mailing_list_count_is_limited_per_parcel() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_built_shop().await;

    for index in 0..SHOP_MAILING_LISTS_PER_PARCEL_MAX {
        storage
            .create_shop_mailing_list(
                "N1",
                "player:owner",
                &format!("list-{index}"),
                &format!("List {index}"),
            )
            .await
            .expect("create list under limit");
    }

    assert!(matches!(
        storage
            .create_shop_mailing_list("N1", "player:owner", "overflow", "Overflow")
            .await,
        Err(StorageError::InvalidMailingList(message))
            if message.contains("mailing-list limit reached")
    ));
}

#[tokio::test]
async fn mailing_list_owner_permission_follows_current_parcel_owner() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_shop().await;
    storage
        .create_shop_mailing_list("N1", "player:owner", "updates", "Shop Updates")
        .await
        .expect("create list");
    storage
        .subscribe_shop_mailing_list("N1", "updates", "customer", "player:customer")
        .await
        .expect("subscribe");
    db.query_value(
        "update commercial_parcels
         set owner_user = 'newowner',
             owner_player_id = 'player:newowner'
         where parcel_id = 'N1'",
    );

    assert!(matches!(
        storage
            .send_shop_mailing_list_post(
                "N1",
                "updates",
                "owner",
                "player:owner",
                "Old Owner",
                "Body"
            )
            .await,
        Err(StorageError::NotParcelOwner(_))
    ));
    let sent = storage
        .send_shop_mailing_list_post(
            "N1",
            "updates",
            "newowner",
            "player:newowner",
            "New Owner",
            "Body",
        )
        .await
        .expect("new owner can send");
    assert_eq!(sent.post.recipient_count, 1);
}
