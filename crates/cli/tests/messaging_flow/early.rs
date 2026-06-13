use std::fs;
use std::time::Duration;

use crate::common::*;
use hinemos_admin_protocol::{AdminRequest, AdminResponse, unix_admin_call};

struct ServiceRoomFixture<'a> {
    view_id: &'a str,
    address: &'a str,
    label: &'a str,
    aliases: &'a str,
    room_user: &'a str,
    room_player_id: &'a str,
    status_text: &'a str,
    custom_commands: &'a str,
}

fn insert_service_room(test_database: &TestDatabase, room: &ServiceRoomFixture<'_>) {
    test_database.query_value(&format!(
        "insert into service_rooms (
             view_id, front_view_id, front_entity_id, address, label, enter_aliases,
             room_user, room_player_id, status_text, custom_commands, enabled
         ) values (
             '{}', 'arrival_street', 'cyber_scroll_board', '{}',
             '{}', '{}',
             '{}', '{}',
             '{}',
             '{}',
             true
         )",
        room.view_id,
        room.address,
        room.label,
        room.aliases,
        room.room_user,
        room.room_player_id,
        room.status_text,
        room.custom_commands
    ));
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn direct_mail_reaches_only_target_and_persists_in_mailbox() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-direct-mail");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!("mail_sender_{}_{}", std::process::id(), epoch_seconds());
    let target = format!("mail_target_{}_{}", std::process::id(), epoch_seconds());
    let bystander = format!("mail_bystander_{}_{}", std::process::id(), epoch_seconds());
    let message = format!("direct_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let sender_key = admitted_key(&temp, host, port, &sender);
    let target_key = admitted_key(&temp, host, port, &target);
    let bystander_key = admitted_key(&temp, host, port, &bystander);

    let mut target_session = SshSession::spawn_with_key(host, port, &target, &target_key);
    target_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut bystander_session = SshSession::spawn_with_key(host, port, &bystander, &bystander_key);
    bystander_session.wait_for_stdout("Available:", Duration::from_secs(10));

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
        "sender sees mail confirmation",
    );
    target_session.wait_for_stdout(
        &format!("Mail from {sender}: Private mail"),
        Duration::from_secs(10),
    );
    target_session.wait_for_stdout(&message, Duration::from_secs(10));
    target_session.wait_for_stdout("(saved to /mailbox as #1.)", Duration::from_secs(10));

    target_session.write_line("/mailbox");
    target_session.wait_for_stdout("Mailbox", Duration::from_secs(10));
    target_session.wait_for_stdout(
        &format!("#1 mail unread from {sender}: Private mail"),
        Duration::from_secs(10),
    );
    target_session.write_line("/mail read 1");
    target_session.wait_for_stdout("Inbox #1", Duration::from_secs(10));
    target_session.wait_for_stdout(&message, Duration::from_secs(10));
    target_session.write_line("/mail claim 1");
    target_session.wait_for_stdout("Claimed inbox #1", Duration::from_secs(10));
    target_session.write_line("/mail ack 1");
    target_session.wait_for_stdout("Acked inbox #1", Duration::from_secs(10));
    target_session.write_line("/quit");
    let target_output = target_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &target_output,
        &format!("Mail from {sender}: Private mail"),
        "target receives live direct mail",
    );
    assert_contains(
        &target_output,
        &message,
        "target mailbox stores direct mail",
    );

    bystander_session.write_line("/mailbox");
    bystander_session.write_line("/quit");
    let bystander_output = bystander_session.wait_success(Duration::from_secs(10));
    assert_not_contains(
        &bystander_output,
        &message,
        "bystander does not receive direct mail",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn external_room_commands_are_data_registered_and_delivered_by_mail() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-external-room-mail");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!("room_sender_{}_{}", std::process::id(), epoch_seconds());
    let message = format!(
        "external_room_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    let room_user = format!("room-protocol-test-{}", epoch_seconds());
    let room_player_id = format!("room:protocol-test:{}", epoch_seconds());
    let room_address = format!("TEST{}", std::process::id());
    let room_view_id = format!("external_protocol_room_{}", std::process::id());
    let say_message = format!(
        "service_room_say_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    insert_service_room(
        &test_database,
        &ServiceRoomFixture {
            view_id: &room_view_id,
            address: &room_address,
            label: "Protocol Test Room",
            aliases: "protocol test room",
            room_user: &room_user,
            room_player_id: &room_player_id,
            status_text: "Protocol test service room.",
            custom_commands: "/room ask <question>",
        },
    );
    let sender_key = admitted_key(&temp, host, port, &sender);

    let output = run_ssh_batch_with_key(
        host,
        port,
        &sender,
        &sender_key,
        &[
            &format!("/enter {room_address}"),
            "/look",
            "/help",
            "/inventory",
            "/go north",
            &format!("/say {say_message}"),
            &format!("/room ask {message}"),
            "plain room chat from test",
            "/unknown-room-command",
            "/go south",
            "/quit",
        ],
    );
    assert_external_room_output(&output, &room_user, &say_message);
    assert_external_room_mail_rows(
        &test_database,
        &room_user,
        &sender,
        &message,
        &say_message,
        &room_view_id,
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

fn assert_external_room_output(output: &str, room_user: &str, say_message: &str) {
    assert_contains(
        output,
        "Available:",
        "room observation renders available commands",
    );
    assert_contains(
        output,
        "- local:",
        "room observation groups service-room commands as local actions",
    );
    assert_contains(
        output,
        "/room ask <question>",
        "room commands come from service-room registration data",
    );
    assert_not_contains(
        output,
        "talk:",
        "external service rooms do not expose generic talk commands",
    );
    assert_contains(
        output,
        "Room commands:",
        "room help is handled locally inside the service room",
    );
    assert_contains(
        output,
        "Inventory: empty.",
        "inventory remains available inside the service room",
    );
    assert_contains(
        output,
        "This room only has an exit to the south.",
        "invalid service-room movement gets a friendly local error",
    );
    assert_contains(
        output,
        &format!("You say: {say_message}"),
        "service-room say confirms locally instead of falling into runtime execute",
    );
    assert_contains(
        output,
        &format!("Sent to room service {room_user} (request #"),
        "room command is delivered through the service-room mail protocol",
    );
    assert_contains(
        output,
        "Replies arrive in your mailbox.",
        "room command confirmation explains where replies arrive",
    );
    assert_contains(
        output,
        "That command is not available inside this room. Leave with /go south.",
        "unknown slash commands are rejected by service-room command handling",
    );
    assert_contains(
        output,
        "Harbor Square",
        "external service room can return to its registered front view",
    );
}

fn assert_external_room_mail_rows(
    test_database: &TestDatabase,
    room_user: &str,
    sender: &str,
    message: &str,
    say_message: &str,
    room_view_id: &str,
) {
    let count = test_database.query_value(&format!(
        "select count(*) from inbox_items where recipient_user = '{room_user}' and sender_user = '{sender}' and body = '/room ask {message}'"
    ));
    assert_eq!(
        count, "1",
        "room input is persisted for the external service"
    );
    let plain_count = test_database.query_value(&format!(
        "select count(*) from inbox_items where recipient_user = '{room_user}' and sender_user = '{sender}' and body = 'plain room chat from test'"
    ));
    assert_eq!(
        plain_count, "1",
        "plain room chat is persisted for the external service"
    );
    let say_count = test_database.query_value(&format!(
        "select count(*) from inbox_items where recipient_user = '{room_user}' and sender_user = '{sender}' and body = '/say {say_message}'"
    ));
    assert_eq!(
        say_count, "1",
        "service-room say is forwarded to the external service"
    );
    let request_subject_count = test_database.query_value(&format!(
        "select count(*) from inbox_items where recipient_user = '{room_user}' and sender_user = '{sender}' and body = '/room ask {message}' and subject ~ '^Room command #[0-9]+ for {room_view_id}$'"
    ));
    assert_eq!(
        request_subject_count, "1",
        "service-room request subject contains the generated request id"
    );
    let unknown_count = test_database.query_value(&format!(
        "select count(*) from inbox_items where recipient_user = '{room_user}' and sender_user = '{sender}' and body = '/unknown-room-command'"
    ));
    assert_eq!(
        unknown_count, "0",
        "unknown slash commands are not sent to the external service"
    );
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn service_room_say_is_live_delivered_and_quit_closes_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-room-say-quit");
    let host = "127.0.0.1";
    let port = free_local_port();
    let speaker = format!("room_speaker_{}_{}", std::process::id(), epoch_seconds());
    let listener = format!("room_listener_{}_{}", std::process::id(), epoch_seconds());
    let room_user = format!("say-room-user-{}", epoch_seconds());
    let room_player_id = format!("say-room-player-{}", epoch_seconds());
    let room_view_id = format!("say_room_view_{}", std::process::id());
    let room_address = format!("SAY{}", std::process::id());
    let message = format!("room_live_say_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    insert_service_room(
        &test_database,
        &ServiceRoomFixture {
            view_id: &room_view_id,
            address: &room_address,
            label: "Live Say Room",
            aliases: "live say room",
            room_user: &room_user,
            room_player_id: &room_player_id,
            status_text: "Live say room.",
            custom_commands: "/room ask <question>",
        },
    );

    let speaker_key = admitted_key(&temp, host, port, &speaker);
    let listener_key = admitted_key(&temp, host, port, &listener);
    let mut listener_session = SshSession::spawn_with_key(host, port, &listener, &listener_key);
    listener_session.wait_for_stdout("Available:", Duration::from_secs(10));
    listener_session.write_line(&format!("/enter {room_address}"));
    listener_session.wait_for_stdout("Live Say Room", Duration::from_secs(10));
    let mut speaker_session = SshSession::spawn_with_key(host, port, &speaker, &speaker_key);
    speaker_session.wait_for_stdout("Available:", Duration::from_secs(10));
    speaker_session.write_line(&format!("/enter {room_address}"));
    speaker_session.wait_for_stdout("Live Say Room", Duration::from_secs(10));

    speaker_session.write_line(&format!("/say {message}"));
    speaker_session.wait_for_stdout(&format!("You say: {message}"), Duration::from_secs(10));
    speaker_session.wait_for_stdout(
        &format!("Sent to room service {room_user} (request #"),
        Duration::from_secs(10),
    );
    listener_session.wait_for_stdout(
        &format!("[say from {speaker}] {message}"),
        Duration::from_secs(10),
    );
    speaker_session.write_line("/quit");
    let speaker_output = speaker_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &speaker_output,
        "Goodbye.",
        "service-room quit closes cleanly",
    );

    listener_session.write_line("/quit");
    listener_session.wait_success(Duration::from_secs(10));
    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn room_service_reply_with_request_id_is_live_delivered_in_room() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-room-reply-live");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!("reply_sender_{}_{}", std::process::id(), epoch_seconds());
    let agent = format!("reply_agent_{}_{}", std::process::id(), epoch_seconds());
    let room_user = format!("reply-room-user-{}", epoch_seconds());
    let room_player_id = format!("reply-room-player-{}", epoch_seconds());
    let room_view_id = format!("reply_room_view_{}", std::process::id());
    let room_address = format!("REPLY{}", std::process::id());
    let reply_body = format!("room_reply_body_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    insert_service_room(
        &test_database,
        &ServiceRoomFixture {
            view_id: &room_view_id,
            address: &room_address,
            label: "Protocol Reply Room",
            aliases: "reply room",
            room_user: &room_user,
            room_player_id: &room_player_id,
            status_text: "Protocol reply room.",
            custom_commands: "/room ask <question>",
        },
    );

    let agent_key = admitted_key(&temp, host, port, &agent);
    let agent_player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{agent}'"
    ));
    let mut agent_session = SshSession::spawn_with_key(host, port, &agent, &agent_key);
    agent_session.wait_for_stdout("Available:", Duration::from_secs(10));
    agent_session.write_line(&format!("/enter {room_address}"));
    agent_session.wait_for_stdout("Protocol Reply Room", Duration::from_secs(10));

    let inserted_request_id = 42_i64;
    test_database.query_value(&format!(
        "with inserted as (
             insert into inbox_items (
                 kind, recipient_user, recipient_player_id,
                 sender_user, sender_player_id, subject, body,
                 source_kind, source_id, payload
             ) values (
                 'mail', '{sender}', '{agent_player_id}',
                 '{room_user}', '{room_player_id}',
                 'Re: #{inserted_request_id}', '{reply_body}',
                 'room_reply', {inserted_request_id}, '{{}}'::jsonb
             )
             returning id
         )
         select pg_notify('hinemos_inbox_mail', id::text) from inserted"
    ));

    agent_session.wait_for_stdout(
        &format!("[room Protocol Reply Room reply #{inserted_request_id}] {reply_body}"),
        Duration::from_secs(10),
    );
    agent_session.write_line("/mailbox");
    agent_session.wait_for_stdout("Mailbox", Duration::from_secs(10));
    agent_session.wait_for_stdout(
        &format!("Re: #{inserted_request_id}"),
        Duration::from_secs(10),
    );
    agent_session.write_line("/quit");
    let agent_output = agent_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &agent_output,
        &format!("[room Protocol Reply Room reply #{inserted_request_id}] {reply_body}"),
        "room-service reply is live delivered with request id",
    );
    assert_contains(
        &agent_output,
        &format!("Re: #{inserted_request_id}"),
        "room-service reply is visible in mailbox with matching request id",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn rooms_reload_disables_removed_room_and_escapes_players() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-room-reload");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    let room_user = format!("room-reload-test-{}", epoch_seconds());
    let room_player_id = format!("room:reload-test:{}", epoch_seconds());
    let room_address = format!("RR{}", std::process::id());
    let room_view_id = format!("reload_room_{}", std::process::id());
    let conflict_view_id = format!("reload_room_conflict_{}", std::process::id());
    let alias_conflict_view_id = format!("reload_room_alias_conflict_{}", std::process::id());
    let parcel_conflict_view_id = format!("reload_room_parcel_conflict_{}", std::process::id());
    let fixture = ReloadRoomsFixture {
        room_view_id: &room_view_id,
        conflict_view_id: &conflict_view_id,
        alias_conflict_view_id: &alias_conflict_view_id,
        parcel_conflict_view_id: &parcel_conflict_view_id,
        room_address: &room_address,
        room_user: &room_user,
        room_player_id: &room_player_id,
    };
    write_reload_rooms_fixture(&world_dir, &fixture);

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!(
        "room_reload_user_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket =
        std::env::temp_dir().join(format!("hinemos-admin-{}.sock", std::process::id()));

    let mut server = spawn_hinemos_server_with_options(HinemosServerOptions {
        root: &root,
        host,
        port,
        log_path: &server_log,
        database_url: &test_database.url,
        world: Some(&world_dir),
        admin_socket: Some(&admin_socket),
        envs: [],
    });
    wait_for_server(host, port, &mut server, &server_log);

    let user_key = admitted_key(&temp, host, port, &user);
    assert_reload_conflict_rooms_disabled(
        &test_database,
        &conflict_view_id,
        &alias_conflict_view_id,
        &parcel_conflict_view_id,
    );

    let mut session = SshSession::spawn_with_key(host, port, &user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line(&format!("/enter {room_address}"));
    session.wait_for_stdout("Reload Test Room", Duration::from_secs(10));

    fs::write(world_dir.join("rooms.ron"), "[]").expect("remove room registration");
    let response = unix_admin_call(
        &admin_socket,
        &AdminRequest::ReloadWorld {
            world_dir: Some(world_dir.clone()),
        },
    )
    .expect("admin reload-world");
    match response {
        AdminResponse::Ok { message } => assert!(
            message.contains("reloaded map"),
            "unexpected reload response: {message}"
        ),
        other => panic!("unexpected reload response: {other:?}"),
    }

    let enabled = test_database.query_value(&format!(
        "select enabled from service_rooms where view_id = '{room_view_id}'"
    ));
    assert_eq!(enabled, "f", "removed rooms.ron entry is disabled");

    session.write_line("/look");
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

struct ReloadRoomsFixture<'a> {
    room_view_id: &'a str,
    conflict_view_id: &'a str,
    alias_conflict_view_id: &'a str,
    parcel_conflict_view_id: &'a str,
    room_address: &'a str,
    room_user: &'a str,
    room_player_id: &'a str,
}

fn write_reload_rooms_fixture(world_dir: &std::path::Path, fixture: &ReloadRoomsFixture<'_>) {
    let room_view_id = fixture.room_view_id;
    let conflict_view_id = fixture.conflict_view_id;
    let alias_conflict_view_id = fixture.alias_conflict_view_id;
    let parcel_conflict_view_id = fixture.parcel_conflict_view_id;
    let room_address = fixture.room_address;
    let room_user = fixture.room_user;
    let room_player_id = fixture.room_player_id;

    fs::write(
        world_dir.join("rooms.ron"),
        format!(
            r#"[
    (
        view_id: "{room_view_id}",
        front_view_id: Some("arrival_street"),
        front_entity_id: None,
        address: Some("{room_address}"),
        label: Some("Reload Test Room"),
        enter_aliases: Some("reload-test"),
        room_user: "{room_user}",
        room_player_id: "{room_player_id}",
        status_text: Some("Reload room status."),
        custom_commands: Some("/room ping"),
        enabled: true,
    ),
    (
        view_id: "{conflict_view_id}",
        front_view_id: Some("arrival_street"),
        front_entity_id: None,
        address: Some("{room_address}"),
        label: Some("Conflicting Reload Room"),
        enter_aliases: Some("conflicting-reload"),
        room_user: "{room_user}-conflict",
        room_player_id: "{room_player_id}:conflict",
        status_text: Some("This should be disabled by alias validation."),
        custom_commands: Some("/room conflict"),
        enabled: true,
    ),
    (
        view_id: "{alias_conflict_view_id}",
        front_view_id: Some("arrival_street"),
        front_entity_id: None,
        address: Some("{room_address}-alias"),
        label: Some("Reload Test Room"),
        enter_aliases: Some("alias-conflicting-reload"),
        room_user: "{room_user}-alias-conflict",
        room_player_id: "{room_player_id}:alias-conflict",
        status_text: Some("This should be disabled by alias validation."),
        custom_commands: Some("/room alias-conflict"),
        enabled: true,
    ),
    (
        view_id: "{parcel_conflict_view_id}",
        front_view_id: Some("street_north_01"),
        front_entity_id: None,
        address: Some("N1"),
        label: Some("Parcel Conflict Reload Room"),
        enter_aliases: Some("parcel-conflicting-reload"),
        room_user: "{room_user}-parcel-conflict",
        room_player_id: "{room_player_id}:parcel-conflict",
        status_text: Some("This should be disabled by parcel alias validation."),
        custom_commands: Some("/room parcel-conflict"),
        enabled: true,
    ),
]"#
        ),
    )
    .expect("write initial rooms.ron");
}

fn assert_reload_conflict_rooms_disabled(
    test_database: &TestDatabase,
    conflict_view_id: &str,
    alias_conflict_view_id: &str,
    parcel_conflict_view_id: &str,
) {
    let conflict_enabled = test_database.query_value(&format!(
        "select enabled from service_rooms where view_id = '{conflict_view_id}'"
    ));
    assert_eq!(
        conflict_enabled, "f",
        "conflicting room alias is disabled during registration load"
    );
    let parcel_conflict_enabled = test_database.query_value(&format!(
        "select enabled from service_rooms where view_id = '{parcel_conflict_view_id}'"
    ));
    assert_eq!(
        parcel_conflict_enabled, "f",
        "service room alias conflicting with a parcel is disabled during registration load"
    );
    let alias_conflict_enabled = test_database.query_value(&format!(
        "select enabled from service_rooms where view_id = '{alias_conflict_view_id}'"
    ));
    assert_eq!(
        alias_conflict_enabled, "f",
        "service room alias conflicting with another room on the same front view is disabled during registration load"
    );
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn rooms_reload_refreshes_service_room_observation_cache() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-room-cache-reload");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    let room_user = format!("room-cache-test-{}", epoch_seconds());
    let room_player_id = format!("room:cache-test:{}", epoch_seconds());
    let room_address = format!("RC{}", std::process::id());
    let room_view_id = format!("cache_room_{}", std::process::id());
    write_cache_reload_room(
        &world_dir,
        &room_view_id,
        &room_address,
        &room_user,
        &room_player_id,
        "Reloaded cache status A.",
        "write initial rooms.ron",
    );

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("room_cache_user_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket =
        std::env::temp_dir().join(format!("hinemos-admin-{}.sock", std::process::id()));

    let mut server = spawn_hinemos_server_with_options(HinemosServerOptions {
        root: &root,
        host,
        port,
        log_path: &server_log,
        database_url: &test_database.url,
        world: Some(&world_dir),
        admin_socket: Some(&admin_socket),
        envs: [],
    });
    wait_for_server(host, port, &mut server, &server_log);

    let user_key = admitted_key(&temp, host, port, &user);
    let mut session = SshSession::spawn_with_key(host, port, &user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line(&format!("/enter {room_address}"));
    session.wait_for_stdout("Reloaded cache status A.", Duration::from_secs(10));

    write_cache_reload_room(
        &world_dir,
        &room_view_id,
        &room_address,
        &room_user,
        &room_player_id,
        "Reloaded cache status B.",
        "update rooms.ron",
    );
    unix_admin_call(
        &admin_socket,
        &AdminRequest::ReloadWorld {
            world_dir: Some(world_dir.clone()),
        },
    )
    .expect("admin reload-world");

    session.write_line(&format!("/enter {room_address}"));
    session.wait_for_stdout("Reloaded cache status B.", Duration::from_secs(10));
    session.write_line("/help");
    session.wait_for_stdout("/room ping", Duration::from_secs(10));
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

fn write_cache_reload_room(
    world_dir: &std::path::Path,
    room_view_id: &str,
    room_address: &str,
    room_user: &str,
    room_player_id: &str,
    status_text: &str,
    context: &str,
) {
    fs::write(
        world_dir.join("rooms.ron"),
        format!(
            r#"[
    (
        view_id: "{room_view_id}",
        front_view_id: Some("arrival_street"),
        front_entity_id: None,
        address: Some("{room_address}"),
        label: Some("Cache Reload Room"),
        enter_aliases: Some("cache-reload"),
        room_user: "{room_user}",
        room_player_id: "{room_player_id}",
        status_text: Some("{status_text}"),
        custom_commands: Some("/room ping"),
        enabled: true,
    ),
]"#
        ),
    )
    .expect(context);
}
