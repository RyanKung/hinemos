use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hinemos_core::{
    PARCEL_MAILING_LISTS_PER_PARCEL_MAX, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED,
    PARCEL_STATUS_VACANT,
};
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
            "hinemos_storage_parcel_mailing_lists_{}_{}_{}",
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
        eprintln!(
            "skipping parcel mailing-list storage test because DATABASE_URL is not configured"
        );
        true
    }
}

async fn storage_with_built_parcel() -> (TestDatabase, PgStorage) {
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
    storage
        .claim_parcel("E1-C0-01", "owner", "player:owner")
        .await
        .expect("claim grid parcel parcel");
    db.query_value(
        "update parcels
         set owner_user = 'owner',
             owner_player_id = 'player:owner',
             status = 'built',
             title = 'Offline Tool Broker',
             description = 'Tools',
             style = 'quiet',
             operator_prompt = 'help',
             custom_commands = '/hello'
         where parcel_id = 'E1-C0-01'",
    );
    (db, storage)
}

#[tokio::test]
async fn generated_grid_parcels_are_virtual_until_claimed_and_canonicalized() {
    if skip_without_database() {
        return;
    }
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");

    let virtual_parcels = storage
        .parcels_by_front_view("grid_road_xp1_y0")
        .await
        .expect("virtual parcels");
    let parcel_ids = virtual_parcels
        .iter()
        .map(|parcel| parcel.parcel_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        parcel_ids,
        vec!["E1-C0-01", "E1-C0-02", "E1-C0-03", "E1-C0-04"]
    );
    assert!(
        virtual_parcels
            .iter()
            .all(|parcel| parcel.status == PARCEL_STATUS_VACANT)
    );

    let virtual_detail = storage
        .parcel_by_id("e1-c0-1")
        .await
        .expect("canonical virtual parcel detail");
    assert_eq!(virtual_detail.parcel_id, "E1-C0-01");
    assert_eq!(virtual_detail.status, PARCEL_STATUS_VACANT);

    let claimed = storage
        .claim_parcel("e1-c0-1", "owner", "player:owner")
        .await
        .expect("claim canonicalized parcel");
    assert_eq!(claimed.parcel_id, "E1-C0-01");
    assert_eq!(claimed.status, PARCEL_STATUS_CLAIMED);

    let mail = storage
        .set_room_mail_auth_token("E1-C0-1", "player:owner", "token")
        .await
        .expect("room mail token uses canonical parcel");
    assert_eq!(mail.username, "room-E1-C0-01");

    let stored = storage
        .parcel_by_id("E1-C0-1")
        .await
        .expect("canonical stored parcel detail");
    assert_eq!(stored.parcel_id, "E1-C0-01");
    assert_eq!(stored.owner_player_id.as_deref(), Some("player:owner"));
    assert_eq!(stored.room_user.as_deref(), Some("room-E1-C0-01"));

    db.query_value(
        "update parcels
         set status = 'built',
             title = 'Grid Parcel'
         where parcel_id = 'E1-C0-01'",
    );
    let list = storage
        .create_parcel_mailing_list("E1-C0-1", "player:owner", "updates", "Grid Updates")
        .await
        .expect("mailing-list create canonicalizes parcel id");
    assert_eq!(list.parcel_id, "E1-C0-01");
    let listed = storage
        .parcel_mailing_lists("e1-c0-1", "player:owner")
        .await
        .expect("mailing-list list canonicalizes parcel id");
    assert_eq!(listed.len(), 1);
    let subscription = storage
        .subscribe_parcel_mailing_list("E1-C0-1", "updates", "customer", "player:customer")
        .await
        .expect("mailing-list subscribe canonicalizes parcel id");
    assert_eq!(subscription.parcel_id, "E1-C0-01");

    let by_view = storage
        .parcel_by_view("parcel_E1-C0-01")
        .await
        .expect("parcel by view")
        .expect("generated parcel binding");
    assert_eq!(by_view.parcel_id, "E1-C0-01");
    assert_eq!(by_view.owner_player_id.as_deref(), Some("player:owner"));

    let overlaid = storage
        .parcels_by_front_view("grid_road_xp1_y0")
        .await
        .expect("front-view overlay");
    let claimed_overlay = overlaid
        .iter()
        .find(|parcel| parcel.parcel_id == "E1-C0-01")
        .expect("claimed parcel overlay");
    assert_eq!(
        claimed_overlay.owner_player_id.as_deref(),
        Some("player:owner")
    );
    assert_eq!(claimed_overlay.status, PARCEL_STATUS_BUILT);
}

#[tokio::test]
async fn legacy_static_vacant_parcels_are_removed_without_deleting_built_history() {
    if skip_without_database() {
        return;
    }
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");

    db.query_value(
        "insert into parcels
            (parcel_id, view_id, front_view_id, district, position, status)
         values
            ('N1', 'parcel_N1', 'street_north_01', 'north', 1, 'vacant'),
            ('N3', 'parcel_N3', 'street_north_02', 'north', 3, 'built')",
    );
    storage.migrate().await.expect("rerun migration cleanup");

    let legacy_count = db.query_value(
        "select count(*)
         from parcels
         where district in ('north', 'south')
           and owner_player_id is null
           and status = 'vacant'",
    );
    let built_count = db.query_value(
        "select count(*)
         from parcels
         where parcel_id = 'N3'
           and status = 'built'",
    );

    assert_eq!(legacy_count, "0");
    assert_eq!(built_count, "1");
    assert!(matches!(
        storage.parcel_by_id("N1").await,
        Err(StorageError::ParcelNotFound(parcel_id)) if parcel_id == "N1"
    ));
}

#[tokio::test]
async fn generated_grid_origin_parcel_is_not_virtual_or_claimable() {
    if skip_without_database() {
        return;
    }
    let db = TestDatabase::create();
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");

    let detail = storage
        .parcel_by_id("C0-C0-01")
        .await
        .expect_err("origin parcel should not exist");
    let claim = storage
        .claim_parcel("C0-C0-01", "owner", "player:owner")
        .await
        .expect_err("origin parcel should not be claimable");
    let stored_count = db.query_value(
        "select count(*)
         from parcels
         where parcel_id = 'C0-C0-01'",
    );

    assert!(matches!(
        detail,
        StorageError::ParcelNotFound(parcel_id) if parcel_id == "C0-C0-01"
    ));
    assert!(matches!(
        claim,
        StorageError::ParcelNotFound(parcel_id) if parcel_id == "C0-C0-01"
    ));
    assert_eq!(stored_count, "0");
}

#[tokio::test]
async fn mailing_list_subscription_delivery_and_retry_are_persisted() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_parcel().await;

    let list = storage
        .create_parcel_mailing_list("E1-C0-01", "player:owner", "updates", "Parcel Updates")
        .await
        .expect("create list");
    assert_eq!(list.slug, "updates");
    assert!(matches!(
        storage
            .create_parcel_mailing_list("E1-C0-01", "player:owner", "updates", "Duplicate")
            .await,
        Err(StorageError::MailingListAlreadyExists { .. })
    ));

    storage
        .subscribe_parcel_mailing_list("E1-C0-01", "updates", "customer", "player:customer")
        .await
        .expect("subscribe");
    assert!(matches!(
        storage
            .subscribe_parcel_mailing_list("E1-C0-01", "updates", "customer", "player:customer")
            .await,
        Err(StorageError::MailingListAlreadySubscribed { .. })
    ));
    assert_eq!(
        storage
            .parcel_mailing_list_subscriptions("player:customer")
            .await
            .expect("subscriptions")
            .len(),
        1
    );

    let sent = storage
        .send_parcel_mailing_list_post(
            "E1-C0-01",
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
               and source_kind = 'parcel_mailing_list_post'
               and source_id = (select max(id) from parcel_mailing_list_posts)"
        ),
        "1"
    );

    assert!(matches!(
        storage
            .send_parcel_mailing_list_post(
                "E1-C0-01",
                "updates",
                "late",
                "player:late",
                "Not Joined",
                "Body"
            )
            .await,
        Err(StorageError::MailingListNotMember { .. })
    ));
    let chat = storage
        .send_parcel_mailing_list_post(
            "Offline Tool Broker",
            "updates",
            "customer",
            "player:customer",
            "Parcel chat: updates",
            "Hello from a member",
        )
        .await
        .expect("member can post group chat");
    assert_eq!(chat.post.recipient_count, 1);
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where kind = 'mail'
               and source_kind = 'parcel_mailing_list_post'
               and sender_user = 'customer'
               and subject = 'Parcel chat: updates'
               and body like '%Reply: /parcel chat E1-C0-01 updates -- <message>%'"
        ),
        "1",
        "member chat delivery should include a reply command"
    );

    storage
        .deliver_parcel_mailing_list_post(sent.post.id)
        .await
        .expect("retry delivery");
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where kind = 'mail'
               and source_kind = 'parcel_mailing_list_post'
               and source_id = (select max(id) from parcel_mailing_list_posts)"
        ),
        "1",
        "retrying delivery should reuse the recipient inbox item"
    );

    storage
        .unsubscribe_parcel_mailing_list("E1-C0-01", "updates", "customer", "player:customer")
        .await
        .expect("unsubscribe");
    assert!(matches!(
        storage
            .send_parcel_mailing_list_post(
                "E1-C0-01",
                "updates",
                "owner",
                "player:owner",
                "No One",
                "Body"
            )
            .await,
        Err(StorageError::MailingListNoSubscribers { .. })
    ));

    storage
        .subscribe_parcel_mailing_list("E1-C0-01", "updates", "customer", "player:customer")
        .await
        .expect("resubscribe");
    storage
        .close_parcel_mailing_list("E1-C0-01", "updates", "player:owner")
        .await
        .expect("close list");
    assert!(matches!(
        storage
            .subscribe_parcel_mailing_list("E1-C0-01", "updates", "late", "player:late")
            .await,
        Err(StorageError::MailingListClosed { .. })
    ));
    storage
        .unsubscribe_parcel_mailing_list("E1-C0-01", "updates", "customer", "player:customer")
        .await
        .expect("unsubscribe after close");
}

#[tokio::test]
async fn parcel_command_routes_queue_operator_commands_for_in_parcel_workers() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_parcel().await;
    storage
        .create_parcel_work_desk("E1-C0-01", "player:owner", "submissions", "Submissions")
        .await
        .expect("create routed desk");
    storage
        .add_parcel_staff("E1-C0-01", "submissions", "player:owner", "customer")
        .await
        .expect("assign worker");
    let route = storage
        .add_parcel_command_route("E1-C0-01", "player:owner", "submissions", "/hello")
        .await
        .expect("add route");
    assert_eq!(route.command_prefix, "/hello");
    assert_eq!(
        storage
            .parcel_command_routes("E1-C0-01", "player:owner")
            .await
            .expect("list routes")
            .len(),
        1
    );

    let parcel = storage
        .parcel_by_id("E1-C0-01")
        .await
        .expect("load parcel parcel");
    let command = storage
        .save_operator_command(&parcel, "late", "player:late", "/hello newsroom", true)
        .await
        .expect("operator command");
    let routed = storage
        .dispatch_parcel_command_routes(&parcel, command.id)
        .await
        .expect("dispatch route");
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].slug, "submissions");
    assert_eq!(routed[0].desk_title, "Submissions");
    assert_eq!(routed[0].command_prefix, "/hello");
    assert_eq!(routed[0].status, "queued");
    assert_eq!(routed[0].sender_user, "late");
    assert_eq!(
        db.query_value(
            "select count(*)
             from parcel_work_items
             where command_prefix = '/hello'
               and status = 'queued'"
        ),
        "1"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from inbox_items
             where source_kind = 'parcel_mailing_list_post'"
        ),
        "0"
    );
    let listed_without_shift = storage
        .parcel_work_items(
            "E1-C0-01",
            "customer",
            "player:customer",
            Some("submissions"),
            20,
        )
        .await
        .expect_err("worker must start shift before listing work");
    assert!(matches!(
        listed_without_shift,
        StorageError::ParcelShiftNotActive { .. }
    ));
    storage
        .start_parcel_shift("E1-C0-01", "submissions", "customer", "player:customer")
        .await
        .expect("start shift");
    let visible = storage
        .parcel_work_items(
            "E1-C0-01",
            "customer",
            "player:customer",
            Some("submissions"),
            20,
        )
        .await
        .expect("list work during shift");
    assert_eq!(visible.len(), 1);
    let claimed = storage
        .claim_parcel_work("E1-C0-01", "customer", "player:customer", routed[0].id)
        .await
        .expect("claim work");
    assert_eq!(claimed.status, "claimed");
    let done = storage
        .finish_parcel_work(
            "E1-C0-01",
            "customer",
            "player:customer",
            routed[0].id,
            "accepted for daily",
        )
        .await
        .expect("finish work");
    assert_eq!(done.status, "done");
    assert_eq!(done.result.as_deref(), Some("accepted for daily"));
    let visible_after_done = storage
        .parcel_work_items(
            "E1-C0-01",
            "customer",
            "player:customer",
            Some("submissions"),
            20,
        )
        .await
        .expect("list completed work during shift");
    assert_eq!(visible_after_done.len(), 1);
    let completed_item = visible_after_done.first().expect("completed work item");
    assert_eq!(completed_item.status, "done");
    assert_eq!(completed_item.result.as_deref(), Some("accepted for daily"));
    storage
        .remove_parcel_staff("E1-C0-01", "submissions", "player:owner", "customer")
        .await
        .expect("remove worker");
    let listed_after_remove = storage
        .parcel_work_items(
            "E1-C0-01",
            "customer",
            "player:customer",
            Some("submissions"),
            20,
        )
        .await
        .expect_err("removed worker cannot list work during old shift");
    assert!(matches!(
        listed_after_remove,
        StorageError::ParcelShiftNotActive { .. }
    ));
    assert_eq!(
        db.query_value(
            "select count(*)
             from parcel_work_shifts
             where worker_user = 'customer'
               and status = 'active'"
        ),
        "0"
    );

    storage
        .remove_parcel_command_route("E1-C0-01", "player:owner", "submissions", "/hello")
        .await
        .expect("remove route");
    let second = storage
        .save_operator_command(&parcel, "customer", "player:customer", "/hello again", true)
        .await
        .expect("second operator command");
    let routed_after_remove = storage
        .dispatch_parcel_command_routes(&parcel, second.id)
        .await
        .expect("dispatch after remove");
    assert!(routed_after_remove.is_empty());
}

#[tokio::test]
async fn parcel_command_routes_use_one_longest_match_per_work_desk() {
    if skip_without_database() {
        return;
    }
    let (db, storage) = storage_with_built_parcel().await;
    storage
        .create_parcel_work_desk("E1-C0-01", "player:owner", "submissions", "Submissions")
        .await
        .expect("create submissions desk");
    storage
        .create_parcel_work_desk("E1-C0-01", "player:owner", "alerts", "Alerts")
        .await
        .expect("create alerts desk");
    storage
        .add_parcel_command_route("E1-C0-01", "player:owner", "submissions", "/paper")
        .await
        .expect("add broad route");
    storage
        .add_parcel_command_route("E1-C0-01", "player:owner", "submissions", "/paper submit")
        .await
        .expect("add specific route");
    storage
        .add_parcel_command_route("E1-C0-01", "player:owner", "submissions", "/PAPER SUBMIT")
        .await
        .expect("add same-length route");
    storage
        .add_parcel_command_route("E1-C0-01", "player:owner", "alerts", "/paper")
        .await
        .expect("add second stream route");

    let parcel = storage
        .parcel_by_id("E1-C0-01")
        .await
        .expect("load parcel parcel");
    let command = storage
        .save_operator_command(
            &parcel,
            "customer",
            "player:customer",
            "/paper submit scoop",
            true,
        )
        .await
        .expect("operator command");
    let routed = storage
        .dispatch_parcel_command_routes(&parcel, command.id)
        .await
        .expect("dispatch routes");

    assert_eq!(routed.len(), 2);
    let submissions = routed
        .iter()
        .find(|item| item.slug == "submissions")
        .expect("submissions dispatch");
    let alerts = routed
        .iter()
        .find(|item| item.slug == "alerts")
        .expect("alerts dispatch");
    assert_eq!(submissions.command_prefix, "/paper submit");
    assert_eq!(alerts.command_prefix, "/paper");
    assert_eq!(
        db.query_value(
            "select count(*)
             from parcel_work_items
             where command_prefix = '/paper submit'"
        ),
        "1"
    );
    assert_eq!(
        db.query_value(
            "select count(*)
             from parcel_work_items
             where command_prefix = '/PAPER SUBMIT'"
        ),
        "0"
    );
}

#[tokio::test]
async fn mailing_list_count_is_limited_per_parcel() {
    if skip_without_database() {
        return;
    }
    let (_db, storage) = storage_with_built_parcel().await;

    for index in 0..PARCEL_MAILING_LISTS_PER_PARCEL_MAX {
        storage
            .create_parcel_mailing_list(
                "E1-C0-01",
                "player:owner",
                &format!("list-{index}"),
                &format!("List {index}"),
            )
            .await
            .expect("create list under limit");
    }

    assert!(matches!(
        storage
            .create_parcel_mailing_list("E1-C0-01", "player:owner", "overflow", "Overflow")
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
    let (db, storage) = storage_with_built_parcel().await;
    storage
        .create_parcel_mailing_list("E1-C0-01", "player:owner", "updates", "Parcel Updates")
        .await
        .expect("create list");
    storage
        .subscribe_parcel_mailing_list("E1-C0-01", "updates", "customer", "player:customer")
        .await
        .expect("subscribe");
    db.query_value(
        "update parcels
         set owner_user = 'newowner',
             owner_player_id = 'player:newowner'
         where parcel_id = 'E1-C0-01'",
    );

    assert!(matches!(
        storage
            .send_parcel_mailing_list_post(
                "E1-C0-01",
                "updates",
                "owner",
                "player:owner",
                "Old Owner",
                "Body"
            )
            .await,
        Err(StorageError::MailingListNotMember { .. })
    ));
    let sent = storage
        .send_parcel_mailing_list_post(
            "E1-C0-01",
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
