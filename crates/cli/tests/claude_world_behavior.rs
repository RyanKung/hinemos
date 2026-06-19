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
