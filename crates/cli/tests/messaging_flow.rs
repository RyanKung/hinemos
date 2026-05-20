mod common;

use std::time::Duration;

use common::*;

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn direct_mail_reaches_only_target_and_persists_in_mailbox() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-direct-mail");
    let host = "127.0.0.1";
    let port = free_local_port();
    let sender = format!("mail_sender_{}_{}", std::process::id(), epoch_seconds());
    let target = format!("mail_target_{}_{}", std::process::id(), epoch_seconds());
    let bystander = format!("mail_bystander_{}_{}", std::process::id(), epoch_seconds());
    let message = format!("direct_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut target_session = SshSession::spawn(host, port, &target);
    target_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut bystander_session = SshSession::spawn(host, port, &bystander);
    bystander_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let sender_output = run_ssh_batch(
        host,
        port,
        &sender,
        [&format!("/mail {target} {message}"), "/quit"],
    );
    assert_contains(
        &sender_output,
        &format!("You mail {target}: {message}"),
        "sender sees mail confirmation",
    );
    target_session.wait_for_stdout(
        &format!("[mail from {sender} to {target}] {message}"),
        Duration::from_secs(10),
    );

    target_session.write_line("/mailbox");
    target_session.wait_for_stdout("Mailbox", Duration::from_secs(10));
    target_session.wait_for_stdout(&message, Duration::from_secs(10));
    target_session.write_line("/quit");
    let target_output = target_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &target_output,
        &format!("[mail from {sender} to {target}] {message}"),
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
fn same_view_say_reaches_only_players_in_that_view() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-view-multicast");
    let host = "127.0.0.1";
    let port = free_local_port();
    let speaker = format!("say_speaker_{}_{}", std::process::id(), epoch_seconds());
    let listener = format!("say_listener_{}_{}", std::process::id(), epoch_seconds());
    let outsider = format!("say_outsider_{}_{}", std::process::id(), epoch_seconds());
    let message = format!("view_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut listener_session = SshSession::spawn(host, port, &listener);
    listener_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut outsider_session = SshSession::spawn(host, port, &outsider);
    outsider_session.wait_for_stdout("Available:", Duration::from_secs(10));
    outsider_session.write_line("/go east");
    outsider_session.wait_for_stdout("Workshop Repair Bay", Duration::from_secs(10));

    let speaker_output = run_ssh_batch(
        host,
        port,
        &speaker,
        [&format!("/say {message}"), "/history", "/quit"],
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
fn broadcast_reaches_all_online_players_and_persists_in_news() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-broadcast");
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
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session_a = SshSession::spawn(host, port, &listener_a);
    session_a.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut session_b = SshSession::spawn(host, port, &listener_b);
    session_b.wait_for_stdout("Available:", Duration::from_secs(10));
    session_b.write_line("/go east");
    session_b.wait_for_stdout("Workshop Repair Bay", Duration::from_secs(10));

    let sender_output = run_ssh_batch(
        host,
        port,
        &sender,
        [&format!("/broadcast {message}"), "/news", "/quit"],
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
