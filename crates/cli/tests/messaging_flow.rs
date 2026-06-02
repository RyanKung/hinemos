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
        &format!("Inbox: new mail #1 from {sender}"),
        Duration::from_secs(10),
    );
    target_session.wait_for_stdout("Use: /mail read 1", Duration::from_secs(10));

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
        &format!("Inbox: new mail #1 from {sender}"),
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

    let mut target_session = SshSession::spawn(host, port, &target);
    target_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let sender_output = run_ssh_batch(
        host,
        port,
        &sender,
        [
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
        &format!("Inbox: new mail #1 from {sender}@hinemos.local"),
        Duration::from_secs(10),
    );
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

    let mut mailbox_session = SshSession::spawn_exec(host, port, &target, ["mailbox"]);
    mailbox_session.wait_for_stdout(
        &format!("OK HINEMOS-MAIL ready user {target}@hinemos.local"),
        Duration::from_secs(10),
    );
    mailbox_session.write_line("IDLE");
    mailbox_session.wait_for_stdout("IDLE active", Duration::from_secs(10));

    let sender_output = run_ssh_batch(
        host,
        port,
        &sender,
        [&format!("/mail {target} {message}"), "/quit"],
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

    let mut agent_session = SshSession::spawn(host, port, &agent);
    agent_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let sender_output = run_ssh_batch(
        host,
        port,
        &sender,
        [&format!("/mail {agent} {message}"), "/quit"],
    );
    assert_contains(
        &sender_output,
        &format!("You mail {agent}: {message}"),
        "sender sees mail confirmation",
    );

    let notice_prefix = "Inbox: new mail #";
    agent_session.wait_for_stdout(notice_prefix, Duration::from_secs(10));
    agent_session.wait_for_stdout(&format!(" from {sender}"), Duration::from_secs(10));
    let inbox_id = parse_hash_id(&agent_session.stdout_text(), notice_prefix);

    agent_session.write_line(&format!("/mail read {inbox_id}"));
    agent_session.wait_for_stdout(&format!("Inbox #{inbox_id}"), Duration::from_secs(10));
    agent_session.wait_for_stdout(&message, Duration::from_secs(10));
    agent_session.write_line(&format!("/mail ack {inbox_id}"));
    agent_session.wait_for_stdout(&format!("Acked inbox #{inbox_id}"), Duration::from_secs(10));
    agent_session.write_line("/quit");
    let agent_output = agent_session.wait_success(Duration::from_secs(10));
    assert_contains(
        &agent_output,
        &format!("Inbox: new mail #{inbox_id} from {sender}"),
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

    let mut listener_session = SshSession::spawn(host, port, &listener);
    listener_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut outsider_session = SshSession::spawn(host, port, &outsider);
    outsider_session.wait_for_stdout("Available:", Duration::from_secs(10));
    outsider_session.write_line("/go west");
    outsider_session.wait_for_stdout("Blackstone Tavern", Duration::from_secs(10));

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

    let mut observer_session = SshSession::spawn(host, port, &observer);
    observer_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let mut peer_sessions = Vec::new();
    let mut peers = Vec::new();
    for index in 0..11 {
        let peer = format!(
            "who_peer_{index}_{}_{}",
            std::process::id(),
            epoch_seconds()
        );
        let session = SshSession::spawn(host, port, &peer);
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

    let mut session_a = SshSession::spawn(host, port, &listener_a);
    session_a.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut session_b = SshSession::spawn(host, port, &listener_b);
    session_b.wait_for_stdout("Available:", Duration::from_secs(10));
    session_b.write_line("/go west");
    session_b.wait_for_stdout("Blackstone Tavern", Duration::from_secs(10));

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
