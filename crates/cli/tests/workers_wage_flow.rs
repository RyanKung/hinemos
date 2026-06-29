mod common;

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

    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
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
           and amount = 25
           and debit_account_id = 'system:mark'
           and credit_account_id like 'player:%'",
    );
    assert_eq!(
        wage_ledger, "1:25",
        "worker claim should create exactly one two-sided 25 MARK wage ledger entry"
    );

    let one_sided_entries = test_database.query_value(
        "select count(*)
         from world_ledger_entries
         where debit_account_id is null
            or credit_account_id is null",
    );
    assert_eq!(
        one_sided_entries, "0",
        "ledger entries must always have both sides"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[tokio::test]
async fn hungry_player_buys_bread_through_blackstone_and_resumes_interacting() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-hunger-bread-loop");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("hungry{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    let key = admitted_key(&temp, host, port, &user);
    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    let player_account_id = format!("player:{player_id}");
    let storage = hinemos_storage::PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");
    storage
        .record_hunger_interaction(&player_id, hinemos_app::HUNGER_THRESHOLD_POINTS)
        .await
        .expect("seed player hunger at threshold");

    let blocked = run_ssh_batch_with_key(
        host,
        port,
        &user,
        &key,
        &["/say too hungry to work", "/quit"],
    );
    assert_contains(
        &blocked,
        "You are too hungry to keep working.",
        "hungry player with MARK is blocked before recovery",
    );

    let queued = run_ssh_batch_with_key(
        host,
        port,
        &user,
        &key,
        &["/go west", "/enter H1", "/buy bread", "/quit"],
    );
    assert_contains(
        &queued,
        "Sent to room service room-blackstone_izakaya",
        "bread purchase is forwarded to Blackstone instead of being blocked",
    );

    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 1 room request(s).",
        "room runner handles the queued bread purchase",
    );

    let balance = run_ssh_batch_with_key(host, port, &user, &key, &["/balance", "/quit"]);
    assert_contains(
        &balance,
        "Balance: 980 MARK",
        "Blackstone bread purchase debits the player wallet",
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*) || ':' || coalesce(sum(amount), 0)
             from world_ledger_entries
             where reason = 'room_food'
               and memo like 'Blackstone Izakaya food for request #%'
               and amount = 20
               and debit_account_id = '{player_account_id}'
               and credit_account_id = 'system:mark'"
        )),
        "1:20",
        "Blackstone bread purchase creates one room food debit"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select hunger_points from player_hunger where player_id = '{player_id}'"
        )),
        "0",
        "Blackstone bread effect restores hunger"
    );

    let allowed = run_ssh_batch_with_key(host, port, &user, &key, &["/say fed and ready", "/quit"]);
    assert_contains(
        &allowed,
        "You say: fed and ready",
        "meaningful SSH command is allowed after hunger recovery",
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
         where idempotency_key = 'workers:wage:7'
           and debit_account_id = 'system:mark'
           and credit_account_id is not null",
    );
    assert_eq!(
        ledger, "1:25",
        "same idempotency key must not duplicate two-sided wage credit"
    );

    let one_sided_entries = test_database.query_value(
        "select count(*)
         from world_ledger_entries
         where debit_account_id is null
            or credit_account_id is null",
    );
    assert_eq!(
        one_sided_entries, "0",
        "system credits must not create one-sided ledger entries"
    );
}

#[tokio::test]
async fn food_debit_is_idempotent_and_hunger_restores() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = hinemos_storage::PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");
    storage
        .ensure_player_wallet("alice", "player:alice")
        .await
        .expect("wallet");
    storage
        .record_hunger_interaction("player:alice", 12)
        .await
        .expect("hunger");

    let first = storage
        .debit_player_mark(
            "alice",
            "player:alice",
            20,
            "room_food",
            "Blackstone Izakaya food for request #9",
            "blackstone:food:9",
        )
        .await
        .expect("first food debit");
    let second = storage
        .debit_player_mark(
            "alice",
            "player:alice",
            20,
            "room_food",
            "Blackstone Izakaya food for request #9",
            "blackstone:food:9",
        )
        .await
        .expect("idempotent food debit");
    let restored = storage
        .restore_player_hunger("player:alice", "bread")
        .await
        .expect("restore hunger");

    assert_eq!(first.amount, 980);
    assert_eq!(second.amount, 980);
    assert_eq!(restored.hunger_points, 0);
    let ledger = test_database.query_value(
        "select count(*) || ':' || coalesce(sum(amount), 0)
         from world_ledger_entries
         where idempotency_key = 'blackstone:food:9'
           and debit_account_id = 'player:player:alice'
           and credit_account_id = 'system:mark'",
    );
    assert_eq!(
        ledger, "1:20",
        "same food idempotency key must not duplicate debit ledger entries"
    );
}

#[tokio::test]
async fn migration_converts_legacy_one_sided_ledger_entries() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);

    seed_legacy_ledger_fixture(&test_database);
    let storage = hinemos_storage::PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate legacy ledger");

    assert_legacy_one_sided_entries_converted(&test_database);
    assert_legacy_self_payment_entries_balanced(&test_database);
    assert_no_invalid_ledger_edges(&test_database);
}

fn seed_legacy_ledger_fixture(test_database: &TestDatabase) {
    test_database.query_value(
        "create table world_accounts (
             account_id text primary key,
             kind text not null check (kind in ('player', 'room', 'system')),
             owner_id text,
             display_name text not null,
             created_at timestamptz not null default now()
         );
         create table world_ledger_entries (
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
         );
         insert into world_accounts (account_id, kind, owner_id, display_name)
         values
             ('player:legacy_credit', 'player', 'legacy_credit', 'Legacy Credit'),
             ('player:legacy_debit', 'player', 'legacy_debit', 'Legacy Debit'),
             ('player:legacy_self', 'player', 'legacy_self', 'Legacy Self'),
             ('system:mark', 'system', 'system', 'System MARK issuance');
         insert into world_ledger_entries (
             asset, debit_account_id, credit_account_id, amount, reason, memo
         )
         values
             ('MARK', null, 'player:legacy_credit', 7, 'legacy_credit', 'legacy credit'),
             ('MARK', 'player:legacy_debit', null, 3, 'legacy_debit', 'legacy debit'),
             ('MARK', 'player:legacy_self', 'player:legacy_self', 11, 'legacy_self', 'legacy self'),
             ('MARK', 'system:mark', 'system:mark', 13, 'legacy_system_self', 'legacy system self');",
    );
}

fn assert_legacy_one_sided_entries_converted(test_database: &TestDatabase) {
    let converted_entries = test_database.query_value(
        "select count(*)
         from world_ledger_entries
         where (debit_account_id = 'system:mark'
                and credit_account_id = 'player:legacy_credit')
            or (debit_account_id = 'player:legacy_debit'
                and credit_account_id = 'system:mark')",
    );
    assert_eq!(
        converted_entries, "2",
        "legacy one-sided entries should be converted to explicit system-account entries"
    );
}

fn assert_legacy_self_payment_entries_balanced(test_database: &TestDatabase) {
    let player_self_payment_entries = test_database.query_value(
        "select count(*) || ':' || coalesce(sum(amount), 0)
         from world_ledger_entries
         where (debit_account_id = 'player:legacy_self'
                and credit_account_id = 'system:ledger-adjustment')
            or (debit_account_id = 'system:ledger-adjustment'
                and credit_account_id = 'player:legacy_self')",
    );
    assert_eq!(
        player_self_payment_entries, "2:22",
        "legacy player self-payments should become balancing distinct-account entries"
    );

    let system_self_payment_entries = test_database.query_value(
        "select count(*) || ':' || coalesce(sum(amount), 0)
         from world_ledger_entries
         where (debit_account_id = 'system:mark'
                and credit_account_id = 'system:ledger-adjustment')
            or (debit_account_id = 'system:ledger-adjustment'
                and credit_account_id = 'system:mark')",
    );
    assert_eq!(
        system_self_payment_entries, "2:26",
        "legacy system self-payments should also become distinct-account entries"
    );
}

fn assert_no_invalid_ledger_edges(test_database: &TestDatabase) {
    let self_payment_entries = test_database.query_value(
        "select count(*)
         from world_ledger_entries
         where debit_account_id = credit_account_id",
    );
    assert_eq!(
        self_payment_entries, "0",
        "migration should leave no self-payment ledger entries"
    );

    let nullable_ledger_edges = test_database.query_value(
        "select count(*)
         from information_schema.columns
         where table_schema = current_schema()
           and table_name = 'world_ledger_entries'
           and column_name in ('debit_account_id', 'credit_account_id')
           and is_nullable = 'YES'",
    );
    assert_eq!(
        nullable_ledger_edges, "0",
        "ledger account edge columns should become non-nullable"
    );
}
