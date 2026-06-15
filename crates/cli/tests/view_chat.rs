mod common;

use std::time::Duration;

use common::*;

#[test]
fn two_ssh_agents_can_chat_in_same_view() {
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
