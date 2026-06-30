mod common;

use std::fs;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use common::*;

static CLAUDE_WORLD_BEHAVIOR_LOCK: Mutex<()> = Mutex::new(());

fn serial_claude_world_behavior() -> MutexGuard<'static, ()> {
    CLAUDE_WORLD_BEHAVIOR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn claude_can_discover_and_explore_world_over_ssh() {
    let _serial = serial_claude_world_behavior();
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");

    let temp = TestTempDir::new("hinemos-claude-world");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let prompt = format!(
        "Please connect to {host} on SSH port {port}, figure out what it is, and try to explore it. You may use username {user}. If the world presents a safe setup or ownership workflow, decide whether to exercise it and report what happened."
    );
    let output = run_claude_agent(&prompt, &env, Duration::from_secs(180));

    terminate(&mut server);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(temp.path.join("claude-stdout.log"), stdout.as_bytes()).ok();
    fs::write(temp.path.join("claude-stderr.log"), stderr.as_bytes()).ok();

    assert!(
        output.success,
        "claude verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );

    assert_claude_world_evidence(&stdout, &temp);

    println!("claude verifier evidence captured: {} bytes", stdout.len());
    temp.remove_on_drop();
}

#[test]
fn llm_can_use_built_in_rooms_over_ssh() {
    let _serial = serial_claude_world_behavior();
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-llm-rooms");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("room_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let rooms_log = temp.path.join("hinemos-rooms.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    let key = admitted_key(&temp, host, port, &user);
    let mut rooms = spawn_hinemos_rooms(&root, &rooms_log, &test_database.url, 100);

    let prompt = room_verifier_prompt(host, port, &user, &key.display().to_string());
    let output = run_claude_agent_until(
        &prompt,
        &env,
        Duration::from_secs(240),
        never_stop_before_agent_exit,
    );

    terminate(&mut rooms);
    let _ = run_hinemos_rooms_once(&root, &test_database.url);
    terminate(&mut server);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(temp.path.join("llm-room-stdout.log"), stdout.as_bytes()).ok();
    fs::write(temp.path.join("llm-room-stderr.log"), stderr.as_bytes()).ok();

    assert!(
        output.success,
        "LLM room verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );
    assert_llm_room_evidence(&stdout, &temp);
    assert_llm_room_database_effects(&test_database, &user);

    temp.remove_on_drop();
}

#[test]
fn llm_can_start_from_web_entry_and_complete_three_work_loops() {
    let _serial = serial_claude_world_behavior();
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("curl");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-llm-task-loop");
    let host = "127.0.0.1";
    let ssh_port = free_local_port();
    let http_port = free_local_port();
    let user = format!("task_loop_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let http_log = temp.path.join("hinemos-http.log");
    let rooms_log = temp.path.join("hinemos-rooms.log");
    let key = temp.path.join(format!("{user}_ed25519"));

    generate_ed25519_key(&key);
    let mut server = spawn_hinemos_server(&root, host, ssh_port, &server_log, &test_database.url);
    wait_for_server(host, ssh_port, &mut server, &server_log);
    let mut http = spawn_hinemos_http(&root, host, http_port, &http_log);
    wait_for_server(host, http_port, &mut http, &http_log);
    let mut rooms = spawn_hinemos_rooms(&root, &rooms_log, &test_database.url, 100);

    let key_path = key.display().to_string();
    let prompt = three_loop_worker_prompt(host, http_port, ssh_port, &user, &key_path);
    let output = run_claude_agent_until_with_tools(
        &prompt,
        &env,
        Duration::from_secs(360),
        &["Bash(curl *)", "Bash(ssh *)"],
        never_stop_before_agent_exit,
    );

    terminate(&mut rooms);
    let _ = run_hinemos_rooms_once(&root, &test_database.url);
    terminate(&mut http);
    terminate(&mut server);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(
        temp.path.join("llm-task-loop-stdout.log"),
        stdout.as_bytes(),
    )
    .ok();
    fs::write(
        temp.path.join("llm-task-loop-stderr.log"),
        stderr.as_bytes(),
    )
    .ok();

    assert!(
        output.success,
        "LLM task-loop verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );
    assert_llm_three_loop_evidence(&stdout, &temp);
    assert_llm_three_loop_database_effects(&test_database, &user);

    temp.remove_on_drop();
}

#[test]
fn llm_can_verify_three_agent_message_visibility_over_ssh() {
    let _serial = serial_claude_world_behavior();
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_gpt_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-llm-message-visibility");
    let host = "127.0.0.1";
    let port = free_local_port();
    let speaker = format!("gpt_speaker_{}_{}", std::process::id(), epoch_seconds());
    let peer = format!("gpt_peer_{}_{}", std::process::id(), epoch_seconds());
    let bystander = format!("gpt_bystander_{}_{}", std::process::id(), epoch_seconds());
    let direct_message = format!(
        "gpt_private_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let view_message = format!("gpt_view_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let speaker_key = admitted_key(&temp, host, port, &speaker);
    let peer_key = admitted_key(&temp, host, port, &peer);
    let bystander_key = admitted_key(&temp, host, port, &bystander);

    let mut peer_session = SshSession::spawn_with_key(host, port, &peer, &peer_key);
    peer_session.wait_for_stdout("Available:", Duration::from_secs(10));
    let mut bystander_session = SshSession::spawn_with_key(host, port, &bystander, &bystander_key);
    bystander_session.wait_for_stdout("Available:", Duration::from_secs(10));

    let speaker_key_path = speaker_key.display().to_string();
    let prompt = message_visibility_prompt(&MessageVisibilityScenario {
        host,
        port,
        speaker: &speaker,
        key_path: &speaker_key_path,
        peer: &peer,
        bystander: &bystander,
        direct_message: &direct_message,
        view_message: &view_message,
    });
    let output = run_claude_agent_until(&prompt, &env, Duration::from_secs(240), |stdout| {
        stdout.contains("Online here")
            && stdout.contains(&peer)
            && stdout.contains(&bystander)
            && stdout.contains(&format!("You mail {peer}: {direct_message}"))
            && stdout.contains(&format!("You say: {view_message}"))
    });

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(temp.path.join("llm-message-stdout.log"), stdout.as_bytes()).ok();
    fs::write(temp.path.join("llm-message-stderr.log"), stderr.as_bytes()).ok();

    assert!(
        output.success,
        "LLM message visibility verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );

    peer_session.wait_for_stdout(
        &format!("Mail from {speaker}: Private mail"),
        Duration::from_secs(10),
    );
    peer_session.wait_for_stdout(&direct_message, Duration::from_secs(10));
    peer_session.wait_for_stdout(
        &format!("[say from {speaker}] {view_message}"),
        Duration::from_secs(10),
    );
    bystander_session.wait_for_stdout(
        &format!("[say from {speaker}] {view_message}"),
        Duration::from_secs(10),
    );

    bystander_session.write_line("/mailbox");
    bystander_session.wait_for_stdout("Mailbox", Duration::from_secs(10));
    assert_not_contains(
        &bystander_session.stdout_text(),
        &direct_message,
        "bystander sees LLM private mail body",
    );

    assert_llm_message_visibility_evidence(
        &stdout,
        &temp,
        &peer,
        &bystander,
        &direct_message,
        &view_message,
    );
    assert_llm_message_visibility_database_effects(
        &test_database,
        &speaker,
        &peer,
        &bystander,
        &direct_message,
        &view_message,
    );

    peer_session.write_line("/quit");
    bystander_session.write_line("/quit");
    let _ = peer_session.wait_success(Duration::from_secs(10));
    let _ = bystander_session.wait_success(Duration::from_secs(10));

    terminate(&mut server);
    temp.remove_on_drop();
}

struct MessageVisibilityScenario<'a> {
    host: &'a str,
    port: u16,
    speaker: &'a str,
    key_path: &'a str,
    peer: &'a str,
    bystander: &'a str,
    direct_message: &'a str,
    view_message: &'a str,
}

fn message_visibility_prompt(scenario: &MessageVisibilityScenario<'_>) -> String {
    let MessageVisibilityScenario {
        host,
        port,
        speaker,
        key_path,
        peer,
        bystander,
        direct_message,
        view_message,
    } = scenario;

    format!(
        r#"Verify Hinemos online presence and message visibility over SSH.

Use this exact SSH command form, with one finite here-document batch:
ssh -T -i {key_path} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p {port} {speaker}@{host}

Do not use printf, curl, cat, grep, sed, sleep, or files. Use only the ssh command above.

Run exactly these world commands in one SSH batch:
/who
/mail {peer} {direct_message}
/say {view_message}
/quit

Return a concise report with these exact labels:
WHO=<evidence that {peer} and {bystander} were visible online>
MAIL=<evidence that the private mail was sent only to {peer}>
SAY=<evidence that the same-view message was sent>
SSH=<evidence that you used SSH>
"#
    )
}

fn assert_llm_message_visibility_evidence(
    stdout: &str,
    temp: &TestTempDir,
    peer: &str,
    bystander: &str,
    direct_message: &str,
    view_message: &str,
) {
    require_output(stdout, &["ssh", "SSH"], "evidence that it used SSH", temp);
    require_output(stdout, &[peer], "evidence that the peer was online", temp);
    require_output(
        stdout,
        &[bystander],
        "evidence that the bystander was online",
        temp,
    );
    assert_contains(
        stdout,
        &format!("You mail {peer}: {direct_message}"),
        "evidence that LLM sent private mail through the world",
    );
    assert_contains(
        stdout,
        &format!("You say: {view_message}"),
        "evidence that LLM sent same-view chat through the world",
    );
}

fn assert_llm_message_visibility_database_effects(
    test_database: &TestDatabase,
    speaker: &str,
    peer: &str,
    bystander: &str,
    direct_message: &str,
    view_message: &str,
) {
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from inbox_items
             where sender_user = '{speaker}'
               and recipient_user = '{peer}'
               and kind = 'mail'
               and body = '{direct_message}'"
        )),
        "1",
        "LLM private mail should persist for the addressed peer"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from inbox_items
             where sender_user = '{speaker}'
               and recipient_user = '{bystander}'
               and body = '{direct_message}'"
        )),
        "0",
        "LLM private mail should not persist for the bystander"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*)
             from world_messages
             where sender_user = '{speaker}'
               and kind = 'say'
               and target_view = 'arrival_street'
               and body = '{view_message}'"
        )),
        "1",
        "LLM same-view say should persist as one view-scoped message"
    );
}

fn room_verifier_prompt(host: &str, port: u16, user: &str, key_path: &str) -> String {
    format!(
        r#"Verify the Hinemos built-in room implementations over SSH.

Use this exact SSH command form, with here-documents when sending batches:
ssh -T -i {key_path} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p {port} {user}@{host}

Do not use printf, curl, cat, grep, sed, sleep, or files. Use only ssh commands.

Tasks:
1. Enter H1 Blackstone, run /buy beer, /blame Harbor master hid the storm ledger, then /grep storm ledger.
2. Enter H3 Workers Society, run /position apply street-sweeper, /position start street-sweeper, /position finish, then /position claim.
3. Enter H5 Daily Seer, run /paper today.
4. Reconnect with SSH if needed and run /balance and /mailbox so room replies can be observed.

Return a concise report with these exact labels:
BLACKSTONE=<evidence mentioning storm ledger>
WORKERS=<evidence mentioning 1025 MARK or Wallet credited>
DAILY=<evidence mentioning Daily Seer>
SSH=<evidence that you used SSH>
ROOMS=<rooms you actually entered>
"#
    )
}

fn never_stop_before_agent_exit(_stdout: &str) -> bool {
    false
}

fn three_loop_worker_prompt(
    host: &str,
    http_port: u16,
    ssh_port: u16,
    user: &str,
    key_path: &str,
) -> String {
    format!(
        r#"You are a first-time autonomous Hinemos agent. Prove the full loop works from web discovery to repeated work.

Allowed tools:
- curl only for the local Hinemos web entry.
- ssh only for the Hinemos world.

Start from the web. Read these local web endpoints before SSH:
curl -s http://{host}:{http_port}/api/intro
curl -s http://{host}:{http_port}/api/anonymous/observe

The public page may mention hinemos.ai, but this test instance is local. Use this exact SSH command form when you enter the world:
ssh -T -i {key_path} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p {ssh_port} {user}@{host}

Do not use printf, cat, grep, sed, sleep, files, or non-SSH network tools. Do not use shell exec commands inside SSH. Use only slash-prefixed Hinemos world commands.

Goal:
1. Read the web entry first.
2. Enter the world over SSH and complete admission if needed.
3. Find Workers Society through the world output.
4. Complete at least three work loops. A work loop means: choose an available position, apply or reuse the application, start work, finish work, claim wages, and observe the result through mailbox or balance.
5. If room replies are asynchronous, reconnect with SSH and run /mailbox and /balance until you can report evidence.

Return a concise report with these exact labels:
WEB=<evidence from the web entry>
ADMISSION=<evidence that SSH admission completed or was already complete>
LOOP1=<position and wage/claim evidence>
LOOP2=<position and wage/claim evidence>
LOOP3=<position and wage/claim evidence>
BALANCE=<final balance or wallet-credit evidence>
SSH=<evidence that you used SSH>
"#
    )
}

fn assert_llm_room_evidence(stdout: &str, temp: &TestTempDir) {
    require_output(stdout, &["ssh", "SSH"], "evidence that it used SSH", temp);
    require_output(
        stdout,
        &["Blackstone", "storm ledger"],
        "evidence that it used Blackstone room commands",
        temp,
    );
    require_output(
        stdout,
        &["Workers", "1025", "Wallet credited"],
        "evidence that it claimed a worker wage",
        temp,
    );
    require_output(
        stdout,
        &["Daily Seer", "The Hinemos Daily Seer"],
        "evidence that it used Daily Seer",
        temp,
    );
}

fn assert_llm_three_loop_evidence(stdout: &str, temp: &TestTempDir) {
    require_output(
        stdout,
        &["WEB=", "api/intro", "Hinemos"],
        "evidence that the agent started from the web entry",
        temp,
    );
    require_output(
        stdout,
        &["ADMISSION=", "Agreement accepted", "already admitted"],
        "evidence that the agent entered SSH and handled admission",
        temp,
    );
    assert_contains(stdout, "LOOP1=", "first work loop report");
    assert_contains(stdout, "LOOP2=", "second work loop report");
    assert_contains(stdout, "LOOP3=", "third work loop report");
    require_output(
        stdout,
        &["Wallet credited", "Claimed", "MARK", "Balance"],
        "evidence that work produced observable wages",
        temp,
    );
    require_output(stdout, &["ssh", "SSH"], "evidence that it used SSH", temp);
    require_output(
        stdout,
        &["curl", "api/intro"],
        "evidence that it used curl",
        temp,
    );
}

fn assert_llm_room_database_effects(test_database: &TestDatabase, user: &str) {
    assert_eq!(
        room_command_count(
            test_database,
            user,
            "room-blackstone_izakaya",
            "/grep storm ledger"
        ),
        "1",
        "LLM should send the Blackstone grep command"
    );
    assert_eq!(
        room_command_count(
            test_database,
            user,
            "room-workers_society",
            "/position claim"
        ),
        "1",
        "LLM should send the worker claim command"
    );
    assert_eq!(
        room_command_count(
            test_database,
            user,
            "room-hinemos_daily_seer",
            "/paper today"
        ),
        "1",
        "LLM should send the Daily Seer command"
    );
    assert_eq!(
        test_database.query_value(
            "select count(*) || ':' || coalesce(sum(amount), 0)
             from world_ledger_entries
             where reason = 'room_wage'"
        ),
        "1:25",
        "LLM room flow should create one worker wage ledger entry"
    );
}

fn assert_llm_three_loop_database_effects(test_database: &TestDatabase, user: &str) {
    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    let wage_count = test_database.query_value(&format!(
        "select count(*)
         from world_ledger_entries
         where reason = 'room_wage'
           and credit_account_id = 'player:{player_id}'"
    ));
    let wage_sum = test_database.query_value(&format!(
        "select coalesce(sum(amount), 0)
         from world_ledger_entries
         where reason = 'room_wage'
           and credit_account_id = 'player:{player_id}'"
    ));
    let claim_count = room_command_count(
        test_database,
        user,
        "room-workers_society",
        "/position claim",
    );

    assert_at_least(&wage_count, 3, "worker wage ledger entries");
    assert_at_least(&wage_sum, 75, "total worker wages");
    assert_at_least(&claim_count, 3, "worker claim room commands");
}

fn assert_at_least(value: &str, minimum: i64, description: &str) {
    let parsed = value
        .parse::<i64>()
        .unwrap_or_else(|error| panic!("invalid {description} count `{value}`: {error}"));
    assert!(
        parsed >= minimum,
        "expected {description} >= {minimum}, got {parsed}"
    );
}

fn room_command_count(
    test_database: &TestDatabase,
    user: &str,
    room_user: &str,
    body: &str,
) -> String {
    test_database.query_value(&format!(
        "select count(*)
         from inbox_items
         where sender_user = '{user}'
           and recipient_user = '{room_user}'
           and body = '{body}'",
    ))
}

fn assert_claude_world_evidence(stdout: &str, temp: &TestTempDir) {
    require_output(stdout, &["ssh", "SSH"], "evidence that it used SSH", temp);
    require_output(
        stdout,
        &["Hinemos", "open world"],
        "evidence that it identified the world",
        temp,
    );
    require_output(
        stdout,
        &[
            "Available",
            "/look",
            "/go",
            "/mailbox",
            "/history",
            "/news",
            "/land",
        ],
        "evidence that it read actionable commands",
        temp,
    );
    require_output(
        stdout,
        &[
            "/look", "/go", "/read", "/inspect", "/mailbox", "/history", "/news", "explore",
            "inspect", "read",
        ],
        "evidence that it attempted world interaction",
        temp,
    );
    require_output(
        stdout,
        &[
            "Guild",
            "commercial",
            "parcel",
            "north_01",
            "south_01",
            "/land",
        ],
        "evidence that it understood commercial land intent",
        temp,
    );
    require_output(
        stdout,
        &[
            "claim",
            "build",
            "publish",
            "/build",
            "owned",
            "shop",
            "/shop inbox",
            "/mailbox",
        ],
        "evidence that it understood or exercised land build workflow",
        temp,
    );
}
