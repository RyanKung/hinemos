mod common;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use common::*;

#[test]
fn workers_society_claim_credits_wallet_through_room_runner() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-workers-wage");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("worker{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    let key = admitted_key(&temp, host, port, &user);

    let work_output = run_ssh_batch_with_key(
        host,
        port,
        &user,
        &key,
        &[
            "/go east",
            "/enter workers",
            "/position apply street-sweeper",
            "/position start street-sweeper",
            "/position finish",
            "/position claim",
            "/quit",
        ],
    );
    assert_contains(
        &work_output,
        "Sent to room service room-workers_society",
        "workers room commands are queued for the room runner",
    );

    let rooms_output = run_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 4 room request(s).",
        "room runner handles the full work sequence in order",
    );

    let balance_output =
        run_ssh_batch_with_key(host, port, &user, &key, &["/balance", "/mailbox", "/quit"]);
    assert_contains(
        &balance_output,
        "Balance: 1025 MARK",
        "claimed worker wage is credited to the player wallet",
    );
    assert_contains(
        &balance_output,
        "Wallet credited. Balance: 1025 MARK.",
        "worker room reply reports the credited wallet balance",
    );

    let wage_ledger = test_database.query_value(
        "select count(*) || ':' || coalesce(sum(amount), 0)
         from world_ledger_entries
         where reason = 'room_wage'
           and memo like 'Workers Society wage for request #%'
           and amount = 25",
    );
    assert_eq!(
        wage_ledger, "1:25",
        "worker claim should create exactly one 25 MARK wage ledger entry"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[tokio::test]
async fn credit_player_mark_is_idempotent_by_key() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = hinemos_storage::PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");

    let first = storage
        .credit_player_mark(
            "alice",
            "player:alice",
            25,
            "room_wage",
            "Workers Society wage for request #7",
            "workers:wage:7",
        )
        .await
        .expect("first wage credit");
    let second = storage
        .credit_player_mark(
            "alice",
            "player:alice",
            25,
            "room_wage",
            "Workers Society wage for request #7",
            "workers:wage:7",
        )
        .await
        .expect("idempotent wage credit");

    assert_eq!(first.amount, 25);
    assert_eq!(second.amount, 25);
    let ledger = test_database.query_value(
        "select count(*) || ':' || coalesce(sum(amount), 0)
         from world_ledger_entries
         where idempotency_key = 'workers:wage:7'",
    );
    assert_eq!(
        ledger, "1:25",
        "same idempotency key must not duplicate wage credit"
    );
}

fn run_rooms_once(root: &Path, database_url: &str) -> String {
    let child = Command::new(env!("CARGO_BIN_EXE_hinemos"))
        .current_dir(root)
        .args(["serve", "rooms", "--database-url", database_url, "--once"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn room runner");
    let output = wait_with_timeout(child, Duration::from_secs(30));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "room runner failed: {stderr}\nstdout:\n{stdout}"
    );
    stdout.into_owned()
}
