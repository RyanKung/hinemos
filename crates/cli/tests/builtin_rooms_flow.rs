mod common;

use common::*;

#[test]
fn built_in_rooms_reply_through_room_runner_end_to_end() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-builtin-rooms");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("rooms{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let world = prepare_builtin_world(&root, &temp);

    let mut server = spawn_hinemos_server_with_options(HinemosServerOptions {
        root: &root,
        host,
        port,
        log_path: &server_log,
        database_url: &test_database.url,
        world: Some(&world),
        admin_socket: None,
        envs: [],
    });
    wait_for_server(host, port, &mut server, &server_log);
    let key = admitted_key(&temp, host, port, &user);

    let queued = queue_all_built_in_room_commands(host, port, &user, &key);
    assert_contains(
        &queued,
        "Workers Society",
        "player can enter Workers Society before queueing commands",
    );
    assert_contains(
        &queued,
        "Hinemos Daily Seer",
        "player can enter Daily Seer before queueing commands",
    );

    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 15 room request(s).",
        "room runner consumes all queued built-in room requests",
    );

    assert_room_reply(
        &test_database,
        &user,
        "room-blackstone_izakaya",
        "The keeper finds 1 gossip note(s) matching storm ledger:",
    );
    assert_room_reply(
        &test_database,
        &user,
        "room-hinemos_school",
        "Credential ready for Agent Basics.",
    );
    assert_room_reply(
        &test_database,
        &user,
        "room-workers_society",
        "Wallet credited. Balance: 1025 MARK.",
    );
    assert_room_reply(
        &test_database,
        &user,
        "room-hinemos_bank",
        "Cash: 60 MARK. Deposit: 40 MARK. Loan debt: 0 MARK.",
    );
    assert_room_reply(
        &test_database,
        &user,
        "room-hinemos_daily_seer",
        "Printed update report: Room Tests.",
    );
    assert_room_reply(
        &test_database,
        &user,
        "room-hinemos_registry",
        "Registry commands:",
    );

    let balance = run_ssh_batch_with_key(host, port, &user, &key, &["/balance", "/quit"]);
    assert_contains(
        &balance,
        "Balance: 1025 MARK",
        "worker wage from the built-in room flow updates wallet balance",
    );
    assert_eq!(
        wage_ledger_summary(&test_database),
        "1:25",
        "workers room finish creates one wage ledger entry",
    );
    assert_eq!(
        broadcast_summary(&test_database),
        "1",
        "newspaper publish creates one public update report",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

fn queue_all_built_in_room_commands(
    host: &str,
    port: u16,
    user: &str,
    key: &std::path::Path,
) -> String {
    run_ssh_batch_with_key(
        host,
        port,
        user,
        key,
        &[
            "/go west",
            "/enter H1",
            "/buy beer",
            "/blame Harbor master hid the storm ledger",
            "/grep storm ledger",
            "/go south",
            "/enter H2",
            "/school programs",
            "/school enroll agent-basics",
            "/school credential agent-basics",
            "/go south",
            "/go east",
            "/go east",
            "/enter H3",
            "/position apply street-sweeper",
            "/position start street-sweeper",
            "/position finish",
            "/go south",
            "/enter H4",
            "/bank balance",
            "/bank deposit 40",
            "/bank balance",
            "/go south",
            "/enter H5",
            "/paper today",
            "/paper publish Room Tests | H1 through H5 replied.",
            "/go south",
            "/enter H6",
            "/marriage help",
            "/quit",
        ],
    )
}

fn assert_room_reply(
    test_database: &TestDatabase,
    recipient_user: &str,
    sender_user: &str,
    body_needle: &str,
) {
    let escaped = body_needle.replace('\'', "''");
    let count = test_database.query_value(&format!(
        "select count(*)
         from inbox_items
         where recipient_user = '{recipient_user}'
           and sender_user = '{sender_user}'
           and body like '%{escaped}%'",
    ));
    assert_eq!(
        count, "1",
        "expected one reply from {sender_user} containing `{body_needle}`"
    );
}

fn wage_ledger_summary(test_database: &TestDatabase) -> String {
    test_database.query_value(
        "select count(*) || ':' || coalesce(sum(amount), 0)
         from world_ledger_entries
         where reason = 'room_wage'
           and memo like 'Workers Society wage for request #%'",
    )
}

fn broadcast_summary(test_database: &TestDatabase) -> String {
    test_database.query_value(
        "select count(*)
         from world_messages
         where sender_user = 'room-hinemos_daily_seer'
           and kind = 'broadcast'
           and body like '%Daily Seer Update: Room Tests%'",
    )
}
