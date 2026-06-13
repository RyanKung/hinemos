use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::common::*;
use crate::ssh_error_support::*;
use hinemos_admin_protocol::{AdminRequest, AdminResponse, unix_admin_call};

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn pending_admission_blocks_world_until_board_agreement() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");
    assert_command_exists("psql");

    let temp = TestTempDir::new("hinemos-admission-gate");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("pending_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let client_key = temp.path.join("client_ed25519");

    let keygen = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&client_key)
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let input = [
        "/pay nobody 1 before-admission",
        "/agree",
        "/read",
        "/agree",
        "/balance",
        "/quit",
    ]
    .join("\n")
        + "\n";
    let output = Command::new("ssh")
        .args([
            "-T",
            "-i",
            client_key.to_str().expect("key path utf8"),
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-p",
            &port.to_string(),
            &format!("{user}@{host}"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child
                .stdin
                .as_mut()
                .expect("open ssh stdin")
                .write_all(input.as_bytes())?;
            Ok(wait_with_timeout(child, Duration::from_secs(45)))
        })
        .expect("run ssh admission batch");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "admission ssh batch failed\nstderr:\n{stderr}\nstdout:\n{stdout}"
    );

    assert_pending_admission_output(&stdout, &test_database, &user);

    terminate(&mut server);
    temp.remove_on_drop();
}

fn assert_pending_admission_output(stdout: &str, test_database: &TestDatabase, user: &str) {
    assert_contains(stdout, "Admission pending", "new users start pending");
    assert_contains(
        stdout,
        "Read the board agreement first: /read agreement",
        "pending users are told to read agreement",
    );
    assert_not_contains(
        stdout,
        "admission: /agree",
        "agree is not advertised as a room action before reading",
    );
    assert_not_contains(
        stdout,
        "Type /agree to enter. Until then",
        "agree guidance belongs to the read result, not the room description",
    );
    assert_not_contains(
        stdout,
        "payment target not found",
        "pending payment command is blocked before world handling",
    );
    assert_contains(
        stdout,
        "Agreement version: 2026-06-03",
        "agreement version is visible on board",
    );
    assert_contains(
        stdout,
        "Next step: type /agree to enter.",
        "agreement read gives a clear next step",
    );
    assert_contains(
        stdout,
        "Agreement accepted: version 2026-06-03",
        "bare agree admits player after reading agreement",
    );
    assert_contains(
        stdout,
        "Initial grant issued: 1000 MARK",
        "initial MARK is issued after agreement",
    );
    assert_contains(
        stdout,
        "Balance: 1000 MARK",
        "wallet is usable after admission",
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select admission_state || ':' || agreement_version from player_profiles where display_name = '{}'",
            sql_literal(user)
        )),
        "agreed:2026-06-03",
        "profile records accepted agreement version"
    );
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn room_token_request_returns_authenticatable_service_room_token() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("psql");

    let temp = TestTempDir::new("hinemos-room-token");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);

    let host = "127.0.0.1";
    let port = free_local_port();
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket = temp.path.join("admin.sock");

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

    let response = unix_admin_call(
        &admin_socket,
        &AdminRequest::RoomToken {
            view_id: "blackstone_izakaya".to_owned(),
        },
    )
    .expect("admin room-token");

    let (view_id, username, player_id, token) = match response {
        AdminResponse::RoomToken {
            view_id,
            username,
            player_id,
            token,
        } => (view_id, username, player_id, token),
        other => panic!("unexpected room-token response: {other:?}"),
    };
    assert_eq!(view_id, "blackstone_izakaya");
    assert_eq!(username, "room-blackstone_izakaya");
    assert_eq!(player_id, "room:blackstone_izakaya");

    assert_eq!(
        test_database.query_value(&format!(
            "select username || ':' || player_id from mail_auth_tokens where username = '{}'",
            sql_literal(&username)
        )),
        format!("{username}:{player_id}"),
        "room token was persisted for the expected room mailbox"
    );
    assert!(
        !token.is_empty(),
        "room-token response must include one-time plaintext token"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn reload_world_updates_admission_config_from_meta() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("hinemos-world-meta-reload");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    write_admission_meta(&world_dir, "2031-01-01", "write initial world meta");

    let host = "127.0.0.1";
    let port = free_local_port();
    let first_user = format!(
        "meta_reload_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos()
    );
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket = temp.path.join("admin.sock");

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

    admit_user_with_version(&temp, host, port, &first_user, "2031-01-01");

    write_admission_meta(&world_dir, "2031-02-02", "update world meta");
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

    let second_user = format!(
        "meta_reload_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos()
    );
    admit_user_with_version(&temp, host, port, &second_user, "2031-02-02");

    terminate(&mut server);
    temp.remove_on_drop();
}

fn write_admission_meta(world_dir: &std::path::Path, version: &str, context: &str) {
    fs::write(
        world_dir.join("meta.ron"),
        format!(
            r#"(
    admission_view_id: "arrival_street",
    admission_board_entity_id: "cyber_scroll_board",
    agreement_version: "{version}",
)
"#
        ),
    )
    .expect(context);
}

fn admit_user_with_version(temp: &TestTempDir, host: &str, port: u16, user: &str, version: &str) {
    let user_key = temp.path.join(format!("{user}_ed25519"));
    generate_ed25519_key(&user_key);
    let mut session = SshSession::spawn_with_key(host, port, user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line("/read");
    session.wait_for_stdout("Next step: type /agree to enter.", Duration::from_secs(10));
    session.write_line("/agree");
    session.wait_for_stdout(
        &format!("Agreement accepted: version {version}"),
        Duration::from_secs(10),
    );
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn startup_loads_admission_config_from_meta() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("hinemos-world-meta-startup");
    let world_dir = temp.path.join("world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    fs::write(
        world_dir.join("meta.ron"),
        r#"(
    admission_view_id: "arrival_street",
    admission_board_entity_id: "cyber_scroll_board",
    agreement_version: "2032-03-04",
)
"#,
    )
    .expect("write world meta");

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!(
        "meta_startup_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos()
    );
    let server_log = temp.path.join("hinemos-server.log");
    let admin_socket = temp.path.join("admin.sock");

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

    let user_key = temp.path.join(format!("{user}_ed25519"));
    generate_ed25519_key(&user_key);
    let mut session = SshSession::spawn_with_key(host, port, &user, &user_key);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    session.write_line("/read");
    session.wait_for_stdout("Next step: type /agree to enter.", Duration::from_secs(10));
    session.write_line("/agree");
    session.wait_for_stdout(
        "Agreement accepted: version 2032-03-04",
        Duration::from_secs(10),
    );
    session.write_line("/quit");
    session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn command_errors_do_not_close_ssh_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-ssh-error-handling");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("typo_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session = SshSession::spawn(host, port, &user);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    admit_session(&mut session);

    session.write_line("/inspect");
    session.wait_for_stdout("What do you want to inspect?", Duration::from_secs(10));

    session.write_line("/inspect missing_board");
    session.wait_for_any_stdout(
        &[
            "You do not see missing_board here.",
            "The world has no visible record named missing_board.",
        ],
        Duration::from_secs(10),
    );

    session.write_line("/inspect cyber_scroll_board");
    session.wait_for_stdout("bulletin board", Duration::from_secs(10));
    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));

    assert_contains(
        &output,
        "What do you want to inspect?",
        "friendly missing-argument error stays in session",
    );
    assert_contains(
        &output,
        "missing_board",
        "mistyped target error stays in session",
    );
    assert_contains(
        &output,
        "bulletin board",
        "valid command after mistyped target still runs",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn natural_language_commands_execute_over_ssh_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-ssh-natural-language");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("natural_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/read cyber_scroll_board",
            "/agree",
            "往北",
            "持ち物を見る",
            "言う：hello world",
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "You go north",
        "Chinese natural-language movement executes",
    );
    assert_contains(
        &output,
        "Agentopia Blvd North 01",
        "natural movement reaches the north street",
    );
    assert_contains(
        &output,
        "Inventory: empty.",
        "Japanese natural-language inventory command executes",
    );
    assert_contains(
        &output,
        "You say: hello world",
        "Japanese natural-language say command executes",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn slash_prefixed_natural_language_does_not_trigger_ssh_nlp() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-ssh-slash-natural-language");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!(
        "slash_natural_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/read cyber_scroll_board",
            "/agree",
            "/往北",
            "/go north",
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "That command is not on the town board.",
        "slash-prefixed natural language stays in slash parser",
    );
    assert_contains(
        &output,
        "Agentopia Blvd North 01",
        "subsequent slash movement starts from the original room",
    );
    assert_not_contains(
        &output,
        "Agentopia Blvd North 02",
        "slash-prefixed natural language did not move before /go north",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn business_command_errors_do_not_close_ssh_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-business-error-handling");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("business_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session = SshSession::spawn(host, port, &user);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    admit_session(&mut session);

    session.write_line("/pay nobody 1 typo-safe");
    session.wait_for_stdout(
        "No player named nobody can be found for payment.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));

    session.write_line("/pay accept 999999");
    session.wait_for_stdout(
        "No payment request #999999 is open on the ledger.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));

    session.write_line("/land info missing_parcel");
    session.wait_for_stdout(
        "The Guild has no parcel record named missing_parcel.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));

    session.write_line("/build title Should not disconnect");
    session.wait_for_stdout(
        "The Guild will not accept that build sheet here; you do not own this parcel.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));

    session.write_line("/shop request-payment 999999 1 hello");
    session.wait_for_stdout(
        "No shop notice #999999 is waiting here.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Harbor Square", Duration::from_secs(10));

    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));
    assert_contains(
        &output,
        "No player named nobody can be found for payment.",
        "unknown payment target error",
    );
    assert_contains(
        &output,
        "No payment request #999999 is open on the ledger.",
        "unknown payment request error",
    );
    assert_contains(
        &output,
        "The Guild has no parcel record named missing_parcel.",
        "unknown parcel error",
    );
    assert_contains(
        &output,
        "The Guild will not accept that build sheet here; you do not own this parcel.",
        "build ownership error",
    );
    assert_contains(
        &output,
        "No shop notice #999999 is waiting here.",
        "unknown shop command error",
    );
    assert_contains(
        &output,
        "Goodbye.",
        "session remains usable through quit after business errors",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}
