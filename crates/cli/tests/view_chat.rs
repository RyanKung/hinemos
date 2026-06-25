mod common;

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use common::*;

static VIEW_CHAT_LOCK: Mutex<()> = Mutex::new(());

fn serial_view_chat() -> MutexGuard<'static, ()> {
    VIEW_CHAT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn two_ssh_agents_can_chat_in_same_view() {
    let _serial = serial_view_chat();
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-two-agent-chat");
    let host = "127.0.0.1";
    let port = free_local_port();
    let listener = format!("listener_{}_{}", std::process::id(), epoch_seconds());
    let speaker = format!("speaker_{}_{}", std::process::id(), epoch_seconds());
    let message = format!("chat_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let listener_key = admitted_key(&temp, host, port, &listener);
    let speaker_key = admitted_key(&temp, host, port, &speaker);

    let mut listener_session = SshSession::spawn_with_key(host, port, &listener, &listener_key);
    listener_session.wait_for_stdout("Available:", Duration::from_secs(10));

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
        "speaker sees local say confirmation",
    );
    assert_contains(
        &speaker_output,
        "Online here:",
        "speaker sees same-view presence before speaking",
    );
    assert_contains(
        &speaker_output,
        &listener,
        "speaker can identify the listener as an online user in the same view",
    );
    assert_contains(
        &speaker_output,
        &message,
        "speaker can read room history after speaking",
    );

    listener_session.wait_for_stdout(
        &format!("[say from {speaker}] {message}"),
        Duration::from_secs(10),
    );
    listener_session.write_line("/history");
    listener_session.wait_for_stdout(&message, Duration::from_secs(10));
    listener_session.write_line("/quit");
    let listener_output = listener_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &listener_output,
        &format!("[say from {speaker}] {message}"),
        "listener receives live room chat",
    );
    assert_contains(
        &listener_output,
        "Room History",
        "listener can inspect persisted room history",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
fn three_ssh_agents_share_presence_and_message_visibility() {
    let _serial = serial_view_chat();
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-three-agent-visibility");
    let host = "127.0.0.1";
    let port = free_local_port();
    let alpha = format!("alpha_{}_{}", std::process::id(), epoch_seconds());
    let bravo = format!("bravo_{}_{}", std::process::id(), epoch_seconds());
    let charlie = format!("charlie_{}_{}", std::process::id(), epoch_seconds());
    let direct_message = format!("private_probe_{}_{}", std::process::id(), epoch_seconds());
    let view_message = format!("view_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let alpha_key = admitted_key(&temp, host, port, &alpha);
    let bravo_key = admitted_key(&temp, host, port, &bravo);
    let charlie_key = admitted_key(&temp, host, port, &charlie);

    let mut alpha_session = SshSession::spawn_with_key(host, port, &alpha, &alpha_key);
    alpha_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut bravo_session = SshSession::spawn_with_key(host, port, &bravo, &bravo_key);
    bravo_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut charlie_session = SshSession::spawn_with_key(host, port, &charlie, &charlie_key);
    charlie_session.wait_for_stdout("Available:", Duration::from_secs(10));

    assert_same_view_presence(&mut alpha_session, &[&bravo, &charlie]);
    assert_same_view_presence(&mut bravo_session, &[&alpha, &charlie]);
    assert_same_view_presence(&mut charlie_session, &[&alpha, &bravo]);

    alpha_session.write_line(&format!("/mail {bravo} {direct_message}"));
    alpha_session.wait_for_stdout(
        &format!("You mail {bravo}: {direct_message}"),
        Duration::from_secs(10),
    );
    bravo_session.wait_for_stdout(
        &format!("Mail from {alpha}: Private mail"),
        Duration::from_secs(10),
    );
    bravo_session.wait_for_stdout(&direct_message, Duration::from_secs(10));

    alpha_session.write_line(&format!("/say {view_message}"));
    alpha_session.wait_for_stdout(&format!("You say: {view_message}"), Duration::from_secs(10));
    bravo_session.wait_for_stdout(
        &format!("[say from {alpha}] {view_message}"),
        Duration::from_secs(10),
    );
    charlie_session.wait_for_stdout(
        &format!("[say from {alpha}] {view_message}"),
        Duration::from_secs(10),
    );

    charlie_session.write_line("/mailbox");
    charlie_session.wait_for_stdout("Mailbox", Duration::from_secs(10));
    assert_not_contains(
        &charlie_session.stdout_text(),
        &direct_message,
        "bystander sees private mail body",
    );

    assert_message_visibility_database_effects(
        &test_database,
        &alpha,
        &bravo,
        &charlie,
        &direct_message,
        &view_message,
    );

    alpha_session.write_line("/quit");
    bravo_session.write_line("/quit");
    charlie_session.write_line("/quit");
    let _ = alpha_session.wait_success(Duration::from_secs(10));
    let _ = bravo_session.wait_success(Duration::from_secs(10));
    let _ = charlie_session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

fn assert_same_view_presence(session: &mut SshSession, expected_users: &[&str]) {
    session.write_line("/who");
    session.wait_for_stdout("Online here", Duration::from_secs(10));
    for expected_user in expected_users {
        session.wait_for_stdout(expected_user, Duration::from_secs(10));
    }
}

fn assert_message_visibility_database_effects(
    test_database: &TestDatabase,
    sender: &str,
    target: &str,
    bystander: &str,
    direct_message: &str,
    view_message: &str,
) {
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from inbox_items
             where sender_user = '{sender}'
               and recipient_user = '{target}'
               and kind = 'mail'
               and body = '{direct_message}'"
        )),
        "1",
        "direct mail should persist for the addressed agent"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from inbox_items
             where sender_user = '{sender}'
               and recipient_user = '{bystander}'
               and body = '{direct_message}'"
        )),
        "0",
        "direct mail should not persist for bystanders"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from world_messages
             where sender_user = '{sender}'
               and kind = 'say'
               and target_view = 'arrival_street'
               and body = '{view_message}'"
        )),
        "1",
        "same-view say should persist as a view-scoped message"
    );
}
