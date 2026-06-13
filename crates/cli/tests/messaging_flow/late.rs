use std::fs;
use std::time::Duration;

use crate::common::*;
use hinemos_admin_protocol::{AdminRequest, unix_admin_call};

#[derive(Clone, Copy)]
struct RoomRegistration<'a> {
    view_id: &'a str,
    front_view_id: &'a str,
    address: &'a str,
    label: &'a str,
    enter_aliases: &'a str,
    room_user: &'a str,
    room_player_id: &'a str,
    status_text: &'a str,
    custom_commands: &'a str,
    enabled: bool,
}

fn write_room_registrations(
    world_dir: &std::path::Path,
    registrations: &[RoomRegistration<'_>],
    context: &str,
) {
    let body = registrations
        .iter()
        .map(room_registration_block)
        .collect::<Vec<_>>()
        .join(",\n");
    fs::write(world_dir.join("rooms.ron"), format!("[\n{body}\n]")).expect(context);
}

fn room_registration_block(registration: &RoomRegistration<'_>) -> String {
    let enabled = if registration.enabled {
        "true"
    } else {
        "false"
    };
    format!(
        r#"    (
        view_id: "{view_id}",
        front_view_id: Some("{front_view_id}"),
        front_entity_id: None,
        address: Some("{address}"),
        label: Some("{label}"),
        enter_aliases: Some("{enter_aliases}"),
        room_user: "{room_user}",
        room_player_id: "{room_player_id}",
        status_text: Some("{status_text}"),
        custom_commands: Some("{custom_commands}"),
        enabled: {enabled},
    )"#,
        view_id = registration.view_id,
        front_view_id = registration.front_view_id,
        address = registration.address,
        label = registration.label,
        enter_aliases = registration.enter_aliases,
        room_user = registration.room_user,
        room_player_id = registration.room_player_id,
        status_text = registration.status_text,
        custom_commands = registration.custom_commands,
        enabled = enabled
    )
}

fn reload_world(admin_socket: &std::path::Path, world_dir: &std::path::Path) {
    unix_admin_call(
        admin_socket,
        &AdminRequest::ReloadWorld {
            world_dir: Some(world_dir.to_path_buf()),
        },
    )
    .expect("admin reload-world");
}

fn assert_service_room_enabled(test_database: &TestDatabase, view_id: &str, expected: &str) {
    let enabled = test_database.query_value(&format!(
        "select enabled from service_rooms where view_id = '{view_id}'"
    ));
    assert_eq!(enabled, expected, "unexpected service room enabled state");
}

fn wait_for_closed_room_escape(session: &mut SshSession, command: &str) {
    session.write_line(command);
    session.wait_for_stdout(
        "This room is closed. You step back to the street.",
        Duration::from_secs(10),
    );
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn rooms_reload_with_missing_front_view_escapes_players_to_street() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-room-missing-front-reload");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    let room_user = format!("room-missing-front-{}", epoch_seconds());
    let room_player_id = format!("room:missing-front:{}", epoch_seconds());
    let room_address = format!("MF{}", std::process::id());
    let room_view_id = format!("missing_front_room_{}", std::process::id());
    let registration = RoomRegistration {
        view_id: &room_view_id,
        front_view_id: "arrival_street",
        address: &room_address,
        label: "Missing Front View Reload Room",
        enter_aliases: "missing-front-reload",
        room_user: &room_user,
        room_player_id: &room_player_id,
        status_text: "Reloaded missing-front status.",
        custom_commands: "/room ping",
        enabled: true,
    };
    write_room_registrations(&world_dir, &[registration], "write initial rooms.ron");

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!(
        "room_missing_front_user_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket =
        std::env::temp_dir().join(format!("hinemos-admin-{}.sock", std::process::id()));

    let mut server = spawn_hinemos_server_with_options(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        Some(&world_dir),
        Some(&admin_socket),
        [],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let user_key = admitted_key(&temp, host, port, &user);
    let mut session = SshSession::spawn_with_key(host, port, &user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line(&format!("/enter {room_address}"));
    session.wait_for_stdout("Missing Front View Reload Room", Duration::from_secs(10));

    write_room_registrations(
        &world_dir,
        &[RoomRegistration {
            front_view_id: "missing_street",
            ..registration
        }],
        "remove front view from rooms.ron",
    );
    reload_world(&admin_socket, &world_dir);
    assert_service_room_enabled(&test_database, &room_view_id, "f");

    wait_for_closed_room_escape(&mut session, "/look");
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn invalid_or_disabled_room_registration_escapes_players_to_front_view() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hi-invalid-room");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    let room_user = format!("room-invalid-test-{}", epoch_seconds());
    let room_player_id = format!("room:invalid-test:{}", epoch_seconds());
    let room_address = format!("IR{}", std::process::id());
    let room_view_id = format!("invalid_escape_room_{}", std::process::id());
    let missing_front_view_id = format!("missing_front_room_{}", std::process::id());
    let active_room = RoomRegistration {
        view_id: &room_view_id,
        front_view_id: "arrival_street",
        address: &room_address,
        label: "Invalid Escape Test Room",
        enter_aliases: "invalid-escape-test",
        room_user: &room_user,
        room_player_id: &room_player_id,
        status_text: "Invalid escape room status.",
        custom_commands: "/room ping",
        enabled: true,
    };
    write_invalid_escape_rooms(
        &world_dir,
        active_room,
        &missing_front_view_id,
        &room_user,
        &room_player_id,
        &room_address,
    );

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!(
        "invalid_room_user_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket = temp.path.join("admin.sock");

    let mut server = spawn_hinemos_server_with_options(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        Some(&world_dir),
        Some(&admin_socket),
        [],
    );
    wait_for_server(host, port, &mut server, &server_log);

    assert_service_room_enabled(&test_database, &missing_front_view_id, "f");

    let user_key = admitted_key(&temp, host, port, &user);
    let mut session = SshSession::spawn_with_key(host, port, &user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line(&format!("/enter {room_address}"));
    session.wait_for_stdout(
        "You enter Invalid Escape Test Room.",
        Duration::from_secs(10),
    );

    write_room_registrations(
        &world_dir,
        &[RoomRegistration {
            enabled: false,
            ..active_room
        }],
        "disable room in rooms.ron",
    );
    reload_world(&admin_socket, &world_dir);
    assert_service_room_enabled(&test_database, &room_view_id, "f");

    wait_for_closed_room_escape(&mut session, "/look");
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

fn write_invalid_escape_rooms(
    world_dir: &std::path::Path,
    active_room: RoomRegistration<'_>,
    missing_front_view_id: &str,
    room_user: &str,
    room_player_id: &str,
    room_address: &str,
) {
    let missing_room_user = format!("{room_user}-missing");
    let missing_room_player_id = format!("{room_player_id}:missing");
    let missing_room_address = format!("BAD{room_address}");
    let missing_room = RoomRegistration {
        view_id: missing_front_view_id,
        front_view_id: "missing_street",
        address: &missing_room_address,
        label: "Missing Front View Room",
        enter_aliases: "missing-front-room",
        room_user: &missing_room_user,
        room_player_id: &missing_room_player_id,
        status_text: "This should be disabled by front view validation.",
        custom_commands: "/room missing",
        enabled: true,
    };
    write_room_registrations(
        world_dir,
        &[active_room, missing_room],
        "write initial rooms.ron",
    );
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn reloaded_disabled_room_escapes_players_on_help() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hi-disabled-room-help");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    let room_user = format!("room-disabled-help-{}", epoch_seconds());
    let room_player_id = format!("room:disabled-help:{}", epoch_seconds());
    let room_address = format!("DH{}", std::process::id());
    let room_view_id = format!("disabled_help_room_{}", std::process::id());
    let registration = RoomRegistration {
        view_id: &room_view_id,
        front_view_id: "arrival_street",
        address: &room_address,
        label: "Disabled Help Room",
        enter_aliases: "disabled-help",
        room_user: &room_user,
        room_player_id: &room_player_id,
        status_text: "Disabled help room status.",
        custom_commands: "/room ping",
        enabled: true,
    };
    write_room_registrations(&world_dir, &[registration], "write initial rooms.ron");

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!(
        "disabled_help_user_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket = temp.path.join("admin.sock");

    let mut server = spawn_hinemos_server_with_options(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        Some(&world_dir),
        Some(&admin_socket),
        [],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let user_key = admitted_key(&temp, host, port, &user);
    let mut session = SshSession::spawn_with_key(host, port, &user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line(&format!("/enter {room_address}"));
    session.wait_for_stdout("You enter Disabled Help Room.", Duration::from_secs(10));

    write_room_registrations(
        &world_dir,
        &[RoomRegistration {
            enabled: false,
            ..registration
        }],
        "disable room in rooms.ron",
    );
    reload_world(&admin_socket, &world_dir);

    wait_for_closed_room_escape(&mut session, "/help");
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn configured_mail_domain_addresses_deliver_to_local_user() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-mail-domain");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!("domain_sender_{}_{}", std::process::id(), epoch_seconds());
    let target = format!("domain_target_{}_{}", std::process::id(), epoch_seconds());
    let target_address = format!("{target}@hinemos.local");
    let external_address = format!("{target}@example.com");
    let message = format!("domain_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("HINEMOS_MAIL_DOMAIN", "hinemos.local")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let sender_key = admitted_key(&temp, host, port, &sender);
    let target_key = admitted_key(&temp, host, port, &target);

    let mut target_session = SshSession::spawn_with_key(host, port, &target, &target_key);
    target_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let sender_output = run_ssh_batch_with_key(
        host,
        port,
        &sender,
        &sender_key,
        &[
            &format!("/mail {external_address} should_not_deliver"),
            &format!("/mail {target_address} {message}"),
            "/quit",
        ],
    );
    assert_contains(
        &sender_output,
        "external mail domain is not available: example.com; local domain is hinemos.local",
        "external domain is rejected",
    );
    assert_contains(
        &sender_output,
        &format!("You mail {target_address}: {message}"),
        "sender can address configured mail domain",
    );

    target_session.wait_for_stdout(
        &format!("Mail from {sender}@hinemos.local: Private mail"),
        Duration::from_secs(10),
    );
    target_session.wait_for_stdout(&message, Duration::from_secs(10));
    target_session.write_line("/mail read 1");
    target_session.wait_for_stdout("Inbox #1", Duration::from_secs(10));
    target_session.wait_for_stdout(
        &format!("From: {sender}@hinemos.local"),
        Duration::from_secs(10),
    );
    target_session.wait_for_stdout(&message, Duration::from_secs(10));
    target_session.write_line("/quit");
    target_session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn ssh_mailbox_protocol_receives_newmail_without_polling() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-mailbox-protocol");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!("mailbox_sender_{}_{}", std::process::id(), epoch_seconds());
    let target = format!("mailbox_target_{}_{}", std::process::id(), epoch_seconds());
    let message = format!(
        "mailbox_protocol_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("HINEMOS_MAIL_DOMAIN", "hinemos.local")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let sender_key = admitted_key(&temp, host, port, &sender);
    let target_key = admitted_key(&temp, host, port, &target);

    let mut mailbox_session =
        SshSession::spawn_exec_with_key(host, port, &target, &target_key, ["mailbox"]);
    mailbox_session.wait_for_stdout(
        &format!("OK HINEMOS-MAIL ready user {target}@hinemos.local"),
        Duration::from_secs(10),
    );
    mailbox_session.write_line("IDLE");
    mailbox_session.wait_for_stdout("IDLE active", Duration::from_secs(10));

    let sender_output = run_ssh_batch_with_key(
        host,
        port,
        &sender,
        &sender_key,
        &[&format!("/mail {target} {message}"), "/quit"],
    );
    assert_contains(
        &sender_output,
        &format!("You mail {target}: {message}"),
        "sender sees shell mail confirmation",
    );

    mailbox_session.wait_for_stdout(
        &format!("* NEWMAIL 1 KIND mail FROM {sender}@hinemos.local SUBJECT Private mail"),
        Duration::from_secs(10),
    );
    mailbox_session.write_line("READ 1");
    mailbox_session.wait_for_stdout("* MESSAGE 1", Duration::from_secs(10));
    mailbox_session.wait_for_stdout(
        &format!("FROM {sender}@hinemos.local"),
        Duration::from_secs(10),
    );
    mailbox_session.wait_for_stdout(&format!("BODY {message}"), Duration::from_secs(10));
    mailbox_session.write_line("ACK 1");
    mailbox_session.wait_for_stdout("OK ACK 1", Duration::from_secs(10));
    mailbox_session.write_line("QUIT");
    let mailbox_output = mailbox_session.wait_success(Duration::from_secs(10));
    assert_not_contains(
        &mailbox_output,
        "/mail list unread",
        "mailbox protocol does not suggest polling",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn online_agent_can_parse_live_inbox_notice_and_read_mail() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-agent-live-inbox");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!(
        "agent_mail_sender_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let agent = format!(
        "agent_mail_target_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let message = format!(
        "agent_live_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let sender_key = admitted_key(&temp, host, port, &sender);
    let agent_key = admitted_key(&temp, host, port, &agent);

    let mut agent_session = SshSession::spawn_with_key(host, port, &agent, &agent_key);
    agent_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let sender_output = run_ssh_batch_with_key(
        host,
        port,
        &sender,
        &sender_key,
        &[&format!("/mail {agent} {message}"), "/quit"],
    );
    assert_contains(
        &sender_output,
        &format!("You mail {agent}: {message}"),
        "sender sees mail confirmation",
    );

    agent_session.wait_for_stdout(
        &format!("Mail from {sender}: Private mail"),
        Duration::from_secs(10),
    );
    agent_session.wait_for_stdout(&message, Duration::from_secs(10));
    let notice_prefix = "(saved to /mailbox as #";
    agent_session.wait_for_stdout(notice_prefix, Duration::from_secs(10));
    let inbox_id = parse_hash_id(&agent_session.stdout_text(), notice_prefix);

    agent_session.write_line(&format!("/mail read {inbox_id}"));
    agent_session.wait_for_stdout(&format!("Inbox #{inbox_id}"), Duration::from_secs(10));
    agent_session.wait_for_stdout(&message, Duration::from_secs(10));
    agent_session.write_line(&format!("/mail ack {inbox_id}"));
    agent_session.wait_for_stdout(&format!("Acked inbox #{inbox_id}"), Duration::from_secs(10));
    agent_session.write_line(&format!("/mail read {inbox_id}"));
    agent_session.wait_for_stdout(&format!("Inbox #{inbox_id}"), Duration::from_secs(10));
    agent_session.wait_for_stdout("Status: acked", Duration::from_secs(10));
    agent_session.write_line("/quit");
    let agent_output = agent_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &agent_output,
        &format!("Mail from {sender}: Private mail"),
        "agent sees parseable live inbox notice",
    );
    assert_contains(
        &agent_output,
        &format!("Inbox #{inbox_id}"),
        "agent reads the live inbox item it parsed",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn same_view_say_reaches_only_players_in_that_view() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-view-multicast");
    let host = "127.0.0.1";
    let port = free_local_port();
    let speaker = format!("say_speaker_{}_{}", std::process::id(), epoch_seconds());
    let listener = format!("say_listener_{}_{}", std::process::id(), epoch_seconds());
    let outsider = format!("say_outsider_{}_{}", std::process::id(), epoch_seconds());
    let message = format!("view_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let speaker_key = admitted_key(&temp, host, port, &speaker);
    let listener_key = admitted_key(&temp, host, port, &listener);
    let outsider_key = admitted_key(&temp, host, port, &outsider);

    let mut listener_session = SshSession::spawn_with_key(host, port, &listener, &listener_key);
    listener_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut outsider_session = SshSession::spawn_with_key(host, port, &outsider, &outsider_key);
    outsider_session.wait_for_stdout("Available:", Duration::from_secs(10));
    outsider_session.write_line("/go west");
    outsider_session.wait_for_stdout("West Hinemos Blvd", Duration::from_secs(10));

    let speaker_output = run_ssh_batch_with_key(
        host,
        port,
        &speaker,
        &speaker_key,
        &[&format!("/say {message}"), "/history", "/quit"],
    );
    assert_contains(
        &speaker_output,
        &format!("You say: {message}"),
        "speaker sees say confirmation",
    );
    assert_contains(
        &speaker_output,
        &message,
        "speaker sees say in current room history",
    );

    listener_session.wait_for_stdout(
        &format!("[say from {speaker}] {message}"),
        Duration::from_secs(10),
    );
    listener_session.write_line("/quit");
    let listener_output = listener_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &listener_output,
        &format!("[say from {speaker}] {message}"),
        "same-view listener receives live say",
    );

    outsider_session.write_line("/history");
    outsider_session.write_line("/quit");
    let outsider_output = outsider_session.wait_success(Duration::from_secs(10));
    assert_not_contains(
        &outsider_output,
        &message,
        "different-view listener does not receive same-view say",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn observations_show_active_users_in_the_same_view_and_who_lists_all() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-view-who");
    let host = "127.0.0.1";
    let port = free_local_port();
    let observer = format!("who_observer_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let observer_key = admitted_key(&temp, host, port, &observer);
    let mut observer_session = SshSession::spawn_with_key(host, port, &observer, &observer_key);
    observer_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let mut peer_sessions = Vec::new();
    let mut peers = Vec::new();
    for index in 0..11 {
        let peer = format!(
            "who_peer_{index}_{}_{}",
            std::process::id(),
            epoch_seconds()
        );
        let key = admitted_key(&temp, host, port, &peer);
        let session = SshSession::spawn_with_key(host, port, &peer, &key);
        session.wait_for_stdout("Available:", Duration::from_secs(10));
        peer_sessions.push(session);
        peers.push(peer);
    }

    observer_session.write_line("/look");
    observer_session.wait_for_stdout("Online here:", Duration::from_secs(10));
    observer_session.wait_for_stdout("+1 more (use /who)", Duration::from_secs(10));

    observer_session.write_line("/who");
    observer_session.wait_for_stdout(
        "Online here in arrival_street (11):",
        Duration::from_secs(10),
    );
    observer_session.wait_for_stdout(&peers[0], Duration::from_secs(10));
    observer_session.wait_for_stdout(&peers[10], Duration::from_secs(10));

    observer_session.write_line("/quit");
    let observer_output = observer_session.wait_success(Duration::from_secs(10));
    assert_not_contains(
        &observer_output,
        "player_id",
        "ordinary who output does not expose player ids",
    );

    for mut session in peer_sessions {
        session.write_line("/quit");
        let _output = session.wait_success(Duration::from_secs(10));
    }

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn broadcast_reaches_all_online_players_and_persists_in_news() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-broadcast");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!(
        "broadcast_sender_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let listener_a = format!("broadcast_a_{}_{}", std::process::id(), epoch_seconds());
    let listener_b = format!("broadcast_b_{}_{}", std::process::id(), epoch_seconds());
    let message = format!("broadcast_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let sender_key = admitted_key(&temp, host, port, &sender);
    let listener_a_key = admitted_key(&temp, host, port, &listener_a);
    let listener_b_key = admitted_key(&temp, host, port, &listener_b);

    let mut session_a = SshSession::spawn_with_key(host, port, &listener_a, &listener_a_key);
    session_a.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut session_b = SshSession::spawn_with_key(host, port, &listener_b, &listener_b_key);
    session_b.wait_for_stdout("Available:", Duration::from_secs(10));
    session_b.write_line("/go west");
    session_b.wait_for_stdout("West Hinemos Blvd", Duration::from_secs(10));

    let sender_output = run_ssh_batch_with_key(
        host,
        port,
        &sender,
        &sender_key,
        &[&format!("/broadcast {message}"), "/news", "/quit"],
    );
    assert_contains(
        &sender_output,
        &format!("You broadcast: {message}"),
        "sender sees broadcast confirmation",
    );
    assert_contains(&sender_output, &message, "sender sees broadcast in news");

    session_a.wait_for_stdout(
        &format!("[broadcast from {sender}] {message}"),
        Duration::from_secs(10),
    );
    session_b.wait_for_stdout(
        &format!("[broadcast from {sender}] {message}"),
        Duration::from_secs(10),
    );

    session_a.write_line("/news");
    session_a.wait_for_stdout("News", Duration::from_secs(10));
    session_a.wait_for_stdout(&message, Duration::from_secs(10));
    session_a.write_line("/quit");
    let output_a = session_a.wait_success(Duration::from_secs(10));
    assert_contains(
        &output_a,
        &format!("[broadcast from {sender}] {message}"),
        "same-view listener receives broadcast",
    );
    assert_contains(&output_a, &message, "broadcast persists in news");

    session_b.write_line("/quit");
    let output_b = session_b.wait_success(Duration::from_secs(10));
    assert_contains(
        &output_b,
        &format!("[broadcast from {sender}] {message}"),
        "different-view listener receives broadcast",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}
