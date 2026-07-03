mod common;

use std::time::Duration;

use common::*;

#[test]
fn h6_registry_registers_present_players_and_issues_certificate() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-marriage-registry");
    let host = "127.0.0.1";
    let port = free_local_port();
    let alice = format!("alice{}_{}", std::process::id(), epoch_seconds());
    let bob = format!("bob{}_{}", std::process::id(), epoch_seconds());
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
    let alice_key = admitted_key(&temp, host, port, &alice);
    let bob_key = admitted_key(&temp, host, port, &bob);

    let mut alice_session = SshSession::spawn_with_key(host, port, &alice, &alice_key);
    alice_session.wait_for_stdout("Available:", Duration::from_secs(10));
    enter_registry(&mut alice_session);

    let mut bob_session = SshSession::spawn_with_key(host, port, &bob, &bob_key);
    bob_session.wait_for_stdout("Available:", Duration::from_secs(10));
    enter_registry(&mut bob_session);

    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from view_presence
             where username in ('{alice}', '{bob}')
               and view_id = 'hinemos_registry'"
        )),
        "2",
        "entering H6 immediately records durable registry presence for both players"
    );

    alice_session.write_line(&format!("/marriage register {bob}"));
    alice_session.wait_for_stdout(
        "Sent to room service room-hinemos_registry",
        Duration::from_secs(10),
    );
    let register_request_id = latest_room_request_id(&alice_session);

    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 1 room request(s).",
        "registry room runner handles one marriage request",
    );

    read_room_reply(&mut alice_session, register_request_id);
    alice_session.wait_for_stdout("Marriage registered", Duration::from_secs(10));
    alice_session.wait_for_stdout("Marriage Certificate", Duration::from_secs(10));
    alice_session.write_line("/marriage certificate");
    alice_session.wait_for_stdout(
        "Sent to room service room-hinemos_registry",
        Duration::from_secs(10),
    );
    let certificate_request_id = latest_room_request_id(&alice_session);
    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 1 room request(s).",
        "registry room runner handles certificate lookup",
    );
    read_room_reply(&mut alice_session, certificate_request_id);
    alice_session.wait_for_stdout("Issued:", Duration::from_secs(10));
    alice_session.write_line("/balance");
    alice_session.wait_for_stdout("Balance: 975 MARK", Duration::from_secs(10));
    alice_session.write_line("/marriage divorce");
    alice_session.wait_for_stdout(
        "Sent to room service room-hinemos_registry",
        Duration::from_secs(10),
    );
    let divorce_request_id = latest_room_request_id(&alice_session);
    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 1 room request(s).",
        "registry room runner handles divorce filing",
    );
    read_room_reply(&mut alice_session, divorce_request_id);
    alice_session.wait_for_stdout("Marriage dissolved", Duration::from_secs(10));
    alice_session.write_line("/marriage certificate");
    alice_session.wait_for_stdout(
        "Sent to room service room-hinemos_registry",
        Duration::from_secs(10),
    );
    let post_divorce_certificate_request_id = latest_room_request_id(&alice_session);
    let rooms_output = run_hinemos_rooms_once(&root, &test_database.url);
    assert_contains(
        &rooms_output,
        "Processed 1 room request(s).",
        "registry room runner handles post-divorce certificate lookup",
    );
    read_room_reply(&mut alice_session, post_divorce_certificate_request_id);
    alice_session.wait_for_stdout(
        "No active marriage certificate on file.",
        Duration::from_secs(10),
    );
    alice_session.write_line("/quit");

    bob_session.write_line("/mailbox");
    bob_session.wait_for_stdout("marriage_certificate", Duration::from_secs(10));
    let certificate_item_id = latest_mailbox_item_id(&bob_session, "marriage_certificate");
    bob_session.write_line(&format!("/mail read {certificate_item_id}"));
    bob_session.wait_for_stdout("Marriage Certificate", Duration::from_secs(10));
    bob_session.write_line("/mailbox");
    bob_session.wait_for_stdout("marriage_divorce", Duration::from_secs(10));
    let divorce_item_id = latest_mailbox_item_id(&bob_session, "marriage_divorce");
    bob_session.write_line(&format!("/mail read {divorce_item_id}"));
    bob_session.wait_for_stdout("Marriage dissolved", Duration::from_secs(10));
    bob_session.write_line("/balance");
    bob_session.wait_for_stdout("Balance: 975 MARK", Duration::from_secs(10));
    bob_session.write_line("/quit");

    let alice_output = alice_session.wait_success(Duration::from_secs(10));
    assert_contains(&alice_output, &bob, "alice sees bob on issued certificate");
    let bob_output = bob_session.wait_success(Duration::from_secs(10));
    assert_contains(&bob_output, &alice, "bob sees alice on issued certificate");

    assert_eq!(
        test_database.query_value(
            "select count(*) || ':' || coalesce(sum(amount), 0)
             from world_ledger_entries
             where reason = 'marriage_registration_fee'"
        ),
        "2:50",
        "registry fee creates two 25 MARK ledger entries"
    );
    assert_eq!(
        test_database.query_value(
            "select count(*) || ':' || coalesce(sum((status = 'active')::int), 0)
             from marriage_certificates"
        ),
        "1:0",
        "divorce keeps certificate history but clears active marriage"
    );
    assert_eq!(
        test_database
            .query_value("select count(*) from inbox_items where kind = 'marriage_divorce'"),
        "2",
        "both parties receive a divorce notice"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

fn read_room_reply(session: &mut SshSession, request_id: i64) {
    let needle = format!("Re: #{request_id}");
    session.write_line("/mailbox");
    session.wait_for_stdout(&needle, Duration::from_secs(10));
    let item_id = newest_mailbox_item_id(session, needle);
    session.write_line(&format!("/mail read {item_id}"));
}

fn latest_room_request_id(session: &SshSession) -> i64 {
    session
        .stdout_text()
        .lines()
        .rev()
        .find_map(|line| {
            let request = line.rsplit_once("request #")?.1;
            request
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("expected room request id in SSH output"))
}

fn latest_mailbox_item_id(session: &SshSession, kind: &str) -> i64 {
    newest_mailbox_item_id(session, kind)
}

fn newest_mailbox_item_id(session: &SshSession, needle: impl AsRef<str>) -> i64 {
    let needle = needle.as_ref();
    let stdout = session.stdout_text();
    let mailbox_block = stdout.rsplit("Mailbox").next().unwrap_or(&stdout);
    mailbox_block
        .lines()
        .find_map(|line| {
            if !line.contains(needle) {
                return None;
            }
            line.split_once('#')?
                .1
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("expected mailbox item containing {needle} in SSH output"))
}

fn enter_registry(session: &mut SshSession) {
    session.write_line("/go east");
    session.wait_for_stdout("East Hinemos Blvd", Duration::from_secs(10));
    session.write_line("/enter H6");
    session.wait_for_stdout(
        "You enter Hinemos Registry Office.",
        Duration::from_secs(10),
    );
}
