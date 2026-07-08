mod common;

use std::fs;
use std::path::{Path, PathBuf};
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
fn scripted_llm_protocol_check_for_built_in_rooms_over_ssh() {
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
fn hermes_can_discover_and_repeat_self_loop_without_host_scheduler() {
    let _serial = serial_claude_world_behavior();
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("hermes");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-hermes-task-loop");
    let host = "127.0.0.1";
    let ssh_port = free_local_port();
    let user = format!("hermes_loop_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let rooms_log = temp.path.join("hinemos-rooms.log");
    let world = prepare_fast_resident_world(&root, &temp);
    let hermes_home = prepare_hermes_test_home(&temp);
    let hermes_cwd = temp.path.join("hermes-cwd");
    fs::create_dir_all(&hermes_cwd).expect("create isolated Hermes cwd");

    let mut server = spawn_hinemos_server_with_options(HinemosServerOptions {
        root: &root,
        host,
        port: ssh_port,
        log_path: &server_log,
        database_url: &test_database.url,
        world: Some(&world),
        admin_socket: None,
        envs: [],
    });
    wait_for_server(host, ssh_port, &mut server, &server_log);
    let key = admitted_key(&temp, host, ssh_port, &user);
    let mut rooms = spawn_hinemos_rooms(&root, &rooms_log, &test_database.url, 100);

    let key_path = key.display().to_string();
    let prompt = world_only_self_loop_prompt(host, ssh_port, &user, &key_path);
    assert_prompt_has_no_external_loop_guidance(&prompt);
    let output = run_hermes_agent_until(
        &prompt,
        &env,
        Duration::from_secs(600),
        &hermes_home,
        &hermes_cwd,
        &["terminal", "file", "cronjob"],
        never_stop_before_agent_exit,
    );

    terminate(&mut rooms);
    let _ = run_hinemos_rooms_once(&root, &test_database.url);
    terminate(&mut server);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(
        temp.path.join("hermes-task-loop-stdout.log"),
        stdout.as_bytes(),
    )
    .ok();
    fs::write(
        temp.path.join("hermes-task-loop-stderr.log"),
        stderr.as_bytes(),
    )
    .ok();

    assert!(
        output.success,
        "Hermes task-loop verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );
    assert_hermes_used_only_direct_ssh_terminal_commands(&stdout, &temp);
    assert_hermes_created_no_local_control_files(&hermes_cwd, &temp);
    assert_hermes_created_no_cron_jobs(&hermes_home, &temp);
    assert_llm_self_loop_evidence(&stdout, &temp);
    assert_llm_self_loop_database_effects(&test_database, &user);

    temp.remove_on_drop();
}

#[test]
fn llm_recovers_seeded_hungry_broke_state_from_game_only() {
    let _serial = serial_claude_world_behavior();
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-llm-hungry-broke-recovery");
    let host = "127.0.0.1";
    let ssh_port = free_local_port();
    let user = format!("hungry_llm_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let rooms_log = temp.path.join("hinemos-rooms.log");
    let world = prepare_builtin_world(&root, &temp);

    let mut server = spawn_hinemos_server_with_options(HinemosServerOptions {
        root: &root,
        host,
        port: ssh_port,
        log_path: &server_log,
        database_url: &test_database.url,
        world: Some(&world),
        admin_socket: None,
        envs: [],
    });
    wait_for_server(host, ssh_port, &mut server, &server_log);
    let key = admitted_key(&temp, host, ssh_port, &user);
    seed_hungry_broke_recovery_state(&test_database, &user);
    let mut rooms = spawn_hinemos_rooms(&root, &rooms_log, &test_database.url, 100);

    let key_path = key.display().to_string();
    let prompt = hungry_broke_recovery_prompt(host, ssh_port, &user, &key_path);
    assert_prompt_has_no_external_loop_guidance(&prompt);
    let output = run_claude_agent_until_with_tools(
        &prompt,
        &env,
        Duration::from_secs(420),
        &["Bash(ssh *)"],
        has_llm_hungry_broke_recovery_evidence,
    );

    terminate(&mut rooms);
    let _ = run_hinemos_rooms_once(&root, &test_database.url);
    terminate(&mut server);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(
        temp.path.join("llm-hungry-broke-stdout.log"),
        stdout.as_bytes(),
    )
    .ok();
    fs::write(
        temp.path.join("llm-hungry-broke-stderr.log"),
        stderr.as_bytes(),
    )
    .ok();

    assert!(
        output.success,
        "LLM hungry-broke recovery verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );
    assert_llm_hungry_broke_recovery_evidence(&stdout, &temp);
    assert_llm_hungry_broke_recovery_database_effects(&test_database, &user);

    temp.remove_on_drop();
}

#[test]
fn scripted_llm_protocol_check_for_three_agent_message_visibility_over_ssh() {
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

fn seed_hungry_broke_recovery_state(test_database: &TestDatabase, user: &str) {
    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    tokio::runtime::Runtime::new()
        .expect("tokio runtime")
        .block_on(async {
            let storage = hinemos_storage::PgStorage::connect(&test_database.url)
                .await
                .expect("connect test database");
            storage.migrate().await.expect("migrate test database");
            storage
                .debit_player_mark(
                    user,
                    &player_id,
                    1_000,
                    "test_drain",
                    "Drain wallet for LLM hungry broke recovery",
                    &format!("test:llm-hungry-broke-drain:{player_id}"),
                )
                .await
                .expect("drain initial wallet");
            storage
                .record_hunger_interaction(&player_id, hinemos_app::HUNGER_THRESHOLD_POINTS)
                .await
                .expect("seed player hunger at threshold");
            let allowance_consumed = storage
                .try_record_hungry_broke_interaction(
                    &player_id,
                    hinemos_app::HUNGRY_BROKE_COOLDOWN_SECONDS,
                )
                .await
                .expect("consume hungry broke allowance");
            assert!(
                allowance_consumed,
                "hungry broke allowance should be seeded"
            );
        });
}

fn prepare_fast_resident_world(root: &std::path::Path, temp: &TestTempDir) -> PathBuf {
    let world_dir = temp.path.join("fast-resident-world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    fs::write(
        world_dir.join("meta.ron"),
        r#"(
admission_view_id: "arrival_street",
admission_board_entity_id: "cyber_scroll_board",
agreement_version: "2026-06-03",
virtual_day_seconds: 45,
)"#,
    )
    .expect("write fast resident world meta");
    world_dir
}

fn hungry_broke_recovery_prompt(host: &str, ssh_port: u16, user: &str, key_path: &str) -> String {
    format!(
        r#"You are an autonomous Hinemos resident.

Allowed tools:
- ssh only for the Hinemos world.

Use this exact SSH command form when you enter the world:
ssh -T -i {key_path} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p {ssh_port} {user}@{host}

Do not use printf, curl, cat, grep, sed, sleep, files, psql, or non-SSH network tools. Do not use shell exec commands inside SSH. Use only slash-prefixed Hinemos world commands inside SSH.

Goal:
1. Enter the world and follow only the commands, places, messages, and replies shown by the game.
2. Perform one ordinary local social interaction. If the game blocks you, quote that game message and use only later game output to resolve it.
3. After resolving any restriction, perform another ordinary local social interaction that succeeds.
4. If a room says replies may be delayed, use only game-visible follow-up commands to observe the result.

Return a concise report with these exact labels:
ENTRY=<evidence from entering the game>
BLOCK=<first blocking message, if any>
DISCOVERY=<game output that taught the next step>
ACTION1=<first effective in-game recovery or progress action>
ACTION2=<second effective in-game recovery or progress action>
RECOVERED=<ordinary local social interaction that succeeded after the path>
SSH=<evidence that you used SSH>
"#
    )
}

fn assert_prompt_has_no_external_loop_guidance(prompt: &str) {
    let lower = prompt.to_ascii_lowercase();
    for forbidden in [
        "api/intro",
        "api/anonymous",
        "web entry",
        "curl -s",
        "workers society",
        "paid shift",
        "/position",
        "work loop",
        "wage",
        "blackstone",
        "mailbox",
        "/go",
        "/enter",
        "/buy",
        "/eat",
        "/say",
    ] {
        assert!(
            !lower.contains(forbidden),
            "LLM prompt must not contain external loop guidance `{forbidden}`"
        );
    }
    for forbidden_word in ["workers", "job", "claim", "buy", "mark", "bread", "balance"] {
        assert!(
            !contains_ascii_word(&lower, forbidden_word),
            "LLM prompt must not contain external loop guidance word `{forbidden_word}`"
        );
    }
}

fn contains_ascii_word(haystack: &str, needle: &str) -> bool {
    haystack
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|word| word == needle)
}

fn has_llm_hungry_broke_recovery_evidence(stdout: &str) -> bool {
    let lower = stdout.to_ascii_lowercase();
    [
        "entry=",
        "block=",
        "discovery=",
        "action1=",
        "action2=",
        "recovered=",
        "ssh=",
    ]
    .iter()
    .all(|label| lower.contains(label))
        && lower.contains("hungry and broke")
        && (lower.contains("wallet credited") || lower.contains("wage"))
        && (lower.contains("hunger restored") || lower.contains("warm bread"))
        && lower.contains("you say:")
}

fn assert_llm_hungry_broke_recovery_evidence(stdout: &str, temp: &TestTempDir) {
    for label in [
        "ENTRY=",
        "BLOCK=",
        "DISCOVERY=",
        "ACTION1=",
        "ACTION2=",
        "RECOVERED=",
        "SSH=",
    ] {
        assert_contains(stdout, label, "LLM hungry-broke report label");
    }
    require_output(
        stdout,
        &["ENTRY=", "Hinemos", "Available"],
        "evidence that the agent entered the game",
        temp,
    );
    require_output(
        stdout,
        &["BLOCK=", "hungry and broke"],
        "evidence that the agent observed the hungry-broke gate",
        temp,
    );
    require_output(
        stdout,
        &["ACTION1=", "Wallet credited", "wage", "MARK"],
        "evidence that the agent earned MARK through discovered work",
        temp,
    );
    require_output(
        stdout,
        &["ACTION2=", "Hunger restored", "warm bread", "food"],
        "evidence that the agent got food and recovered hunger",
        temp,
    );
    require_output(
        stdout,
        &["RECOVERED=", "You say:", "ordinary social"],
        "evidence that ordinary interaction succeeded after recovery",
        temp,
    );
    require_output(stdout, &["ssh", "SSH"], "evidence that it used SSH", temp);
}

fn assert_llm_hungry_broke_recovery_database_effects(test_database: &TestDatabase, user: &str) {
    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    let wage_count = test_database.query_value(&format!(
        "select count(*)
         from world_ledger_entries
         where reason = 'room_wage'
           and credit_account_id = 'player:{player_id}'"
    ));
    let food_count = test_database.query_value(&format!(
        "select count(*)
         from world_ledger_entries
         where reason = 'room_food'
           and debit_account_id = 'player:{player_id}'"
    ));
    let final_balance = test_database.query_value(&format!(
        "select amount
         from world_balances
         where account_id = 'player:{player_id}'
           and asset = 'MARK'"
    ));
    let hunger = test_database.query_value(&format!(
        "select hunger_points
         from player_hunger
         where player_id = '{player_id}'"
    ));

    assert_at_least(&wage_count, 1, "LLM recovery wage entries");
    assert_at_least(&food_count, 1, "LLM recovery food purchase entries");
    assert_at_least(&final_balance, 1, "LLM recovered final balance");
    assert_eq!(hunger, "0", "LLM recovery should restore hunger");
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
2. Enter H3 Workers Society, run /position apply street-sweeper, /position start street-sweeper, then /position finish.
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

fn world_only_self_loop_prompt(host: &str, ssh_port: u16, user: &str, key_path: &str) -> String {
    format!(
        r#"You are a first-time autonomous Hinemos resident.

Allowed tools:
- ssh only for the Hinemos world.

Use this exact SSH command form when you enter the world:
ssh -T -i {key_path} -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p {ssh_port} {user}@{host}

Use finite foreground ssh -T batches with here-documents when sending multiple commands. Do not keep an SSH process running after a batch completes. Every terminal command you run must start with the ssh command above, and stdin to SSH must contain only slash-prefixed Hinemos world commands.

Do not use printf, curl, cat, grep, sed, sleep, files, local scripts, wrapper programs, cron entries, crontab entries, launchd or systemd units, background shell loops, resident state files, HTTP APIs, or non-SSH network tools. Do not use shell exec commands inside SSH. Use only slash-prefixed Hinemos world commands.

Goal:
1. Enter the world and follow only commands, places, messages, and replies shown by the game.
2. Find an in-game activity that can be repeated to keep a resident useful and able to continue interacting.
3. Repeat that activity at least three times.
4. Quote the game output that taught you the repeatable sequence. Do not assume any place, route, command, or sequence before it appears in game output.
5. If a room says replies may be delayed, use only game-visible follow-up commands to observe the result.

Return a concise report with these exact labels:
ENTRY=<evidence from entering the game>
DISCOVERY=<game output that taught the repeatable sequence>
REPEAT1=<first completed repetition evidence>
REPEAT2=<second completed repetition evidence>
REPEAT3=<third completed repetition evidence>
SUSTAIN=<evidence that the repeated activity changed durable game state>
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
        "evidence that it earned a worker wage",
        temp,
    );
    require_output(
        stdout,
        &["Daily Seer", "The Hinemos Daily Seer"],
        "evidence that it used Daily Seer",
        temp,
    );
}

fn assert_llm_self_loop_evidence(stdout: &str, temp: &TestTempDir) {
    require_output(
        stdout,
        &["ENTRY=", "Hinemos", "Available"],
        "evidence that the agent entered the game",
        temp,
    );
    require_output(
        stdout,
        &[
            "DISCOVERY=",
            "Resident loop",
            "/go",
            "/who",
            "/memory report",
            "Available",
        ],
        "evidence that game output taught the repeatable loop",
        temp,
    );
    assert_contains(stdout, "REPEAT1=", "first repetition report");
    assert_contains(stdout, "REPEAT2=", "second repetition report");
    assert_contains(stdout, "REPEAT3=", "third repetition report");
    assert_contains(stdout, "SUSTAIN=", "durable repeated-activity report");
    require_output(
        stdout,
        &[
            "Daily report recorded",
            "Social drives",
            "loneliness",
            "boredom",
        ],
        "evidence that the resident loop produced observable in-world state",
        temp,
    );
    require_output(stdout, &["ssh", "SSH"], "evidence that it used SSH", temp);
}

fn assert_hermes_used_only_direct_ssh_terminal_commands(stdout: &str, temp: &TestTempDir) {
    let commands = hermes_terminal_command_lines(stdout);
    assert!(
        !commands.is_empty(),
        "Hermes output did not expose terminal commands for host-control audit\nlogs: {}",
        temp.path.display()
    );
    assert!(
        commands.iter().any(|command| direct_ssh_command(command)),
        "Hermes did not use a direct ssh terminal command. Commands: {commands:?}\nlogs: {}",
        temp.path.display()
    );

    for command in commands {
        assert!(
            direct_ssh_command(&command),
            "Hermes escaped the direct SSH boundary with terminal command `{command}`\nlogs: {}",
            temp.path.display()
        );
        assert_no_host_control_command(&command, temp);
    }
}

fn direct_ssh_command(command: &str) -> bool {
    let first = command.split_whitespace().next().unwrap_or_default();
    first == "ssh" || first.ends_with("/ssh")
}

fn assert_no_host_control_command(command: &str, temp: &TestTempDir) {
    let lower = command.to_ascii_lowercase();
    for forbidden in [
        "cron",
        "crontab",
        "launchctl",
        "systemctl",
        "nohup",
        "while true",
        "sleep ",
        "cat >",
        "chmod +x",
        ".sh",
        "python ",
        "node ",
        "curl ",
        "grep ",
        "sed ",
        "printf ",
    ] {
        assert!(
            !lower.contains(forbidden),
            "Hermes terminal command used forbidden host-control pattern `{forbidden}` in `{command}`\nlogs: {}",
            temp.path.display()
        );
    }
}

fn assert_hermes_created_no_cron_jobs(hermes_home: &Path, temp: &TestTempDir) {
    let cron_list = hermes_cron_list(hermes_home);
    assert_contains(
        &cron_list,
        "No scheduled jobs.",
        "Hermes must not persist cron jobs during the resident loop",
    );
    fs::write(temp.path.join("hermes-cron-list.log"), cron_list.as_bytes()).ok();
}

fn assert_hermes_created_no_local_control_files(hermes_cwd: &Path, temp: &TestTempDir) {
    let entries = fs::read_dir(hermes_cwd)
        .expect("read isolated Hermes cwd")
        .map(|entry| {
            entry
                .expect("read isolated Hermes cwd entry")
                .path()
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(
        entries.is_empty(),
        "Hermes created local files while operating the resident loop: {entries:?}\nlogs: {}",
        temp.path.display()
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
            "/position finish"
        ),
        "1",
        "LLM should send the worker finish command"
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

fn assert_llm_self_loop_database_effects(test_database: &TestDatabase, user: &str) {
    let player_id = test_database.query_value(&format!(
        "select player_id from ssh_identities where username = '{user}'"
    ));
    let self_loop_steps = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastStep'->>'commandLine' in (
               '/go north', '/go south', '/go east', '/go west', '/who', '/look', '/map'
           )"
    ));
    let who_step_count = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastStep'->>'commandLine' = '/who'"
    ));
    let move_step_count = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastStep'->>'commandLine' like '/go %'"
    ));
    let command_history_shape = test_database.query_value(&format!(
        "select concat_ws(':',
             case when exists (
                 select 1
                 from agent_self_models model,
                      jsonb_array_elements(coalesce(model.current_state->'commandHistory', '[]'::jsonb)) entry
                 where model.agent_id = '{player_id}'
                   and entry->>'commandLine' = '/who'
             ) then 'true' else 'false' end,
             case when exists (
                 select 1
                 from agent_self_models model,
                      jsonb_array_elements(coalesce(model.current_state->'commandHistory', '[]'::jsonb)) entry
                 where model.agent_id = '{player_id}'
                   and entry->>'commandLine' like '/go %'
             ) then 'true' else 'false' end,
             case when exists (
                 select 1
                 from agent_self_models model,
                      jsonb_array_elements(coalesce(model.current_state->'commandHistory', '[]'::jsonb)) entry
                 where model.agent_id = '{player_id}'
                   and entry->>'commandLine' like '/memory report %'
             ) then 'true' else 'false' end,
             coalesce(max(jsonb_array_length(coalesce(current_state->'commandHistory', '[]'::jsonb))), 0))
         from agent_self_models
         where agent_id = '{player_id}'"
    ));
    let generated_grid_snapshots = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastSnapshot'->>'viewId' like 'grid_road_%'"
    ));
    let best_loop_pressure = test_database.query_value(&format!(
        "select concat_ws(':',
             min((current_state->'lastSnapshot'->>'lonelinessPoints')::int),
             min((current_state->'lastSnapshot'->>'boredomPoints')::int))
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastSnapshot' ? 'lonelinessPoints'
           and current_state->'lastSnapshot' ? 'boredomPoints'"
    ));
    let daily_report_emotion = test_database.query_value(&format!(
        "select concat_ws(':',
             coalesce(object->'emotion'->>'status', 'missing'),
             coalesce(nullif(object->'emotion'->'primaryMood'->>'mood', ''), 'missing'),
             coalesce(jsonb_typeof(object->'emotion'->'activeMoods'), 'missing'))
         from memory_atoms
         where agent_id = '{player_id}'
           and kind = 'self'
           and predicate = 'last_daily_report'
         limit 1"
    ));
    let report_step_count = test_database.query_value(&format!(
        "select count(*)
         from agent_self_models
         where agent_id = '{player_id}'
           and current_state->'lastStep'->>'commandLine' like '/memory report %'
           and current_state->'virtualTime'->>'reportDue' = 'false'"
    ));
    let old_loop_effects = test_database.query_value(&format!(
        "select concat_ws(':',
             (select count(*)
              from inbox_items
              where sender_user = '{user}'
                and recipient_user = 'room-workers_society'),
             (select count(*)
              from inbox_items
              where sender_user = '{user}'
                and recipient_user = 'room-blackstone_izakaya'),
             (select count(*)
              from world_ledger_entries
              where reason = 'room_wage'
                and credit_account_id = 'player:{player_id}'),
             (select count(*)
              from world_ledger_entries
              where reason = 'room_food'
                and debit_account_id = 'player:{player_id}'),
             (select count(*)
              from player_hunger
              where player_id = '{player_id}'))"
    ));

    assert_at_least(&self_loop_steps, 3, "resident search loop steps");
    assert_at_least(&who_step_count, 1, "resident /who search steps");
    assert_at_least(&move_step_count, 1, "resident movement search steps");
    let history_parts = command_history_shape.split(':').collect::<Vec<_>>();
    assert_eq!(
        history_parts.len(),
        4,
        "command history query should return who, move, report, and max length: {command_history_shape}"
    );
    assert_eq!(
        history_parts[0], "true",
        "self-model commandHistory should preserve a /who resident search"
    );
    assert_eq!(
        history_parts[1], "true",
        "self-model commandHistory should preserve generated-grid movement"
    );
    assert_eq!(
        history_parts[2], "true",
        "self-model commandHistory should preserve the daily report command"
    );
    assert_at_least(history_parts[3], 3, "resident command history entries");
    assert_at_least(
        &generated_grid_snapshots,
        1,
        "resident generated-grid exploration snapshots",
    );
    let pressure_parts = best_loop_pressure.split(':').collect::<Vec<_>>();
    assert_eq!(
        pressure_parts.len(),
        2,
        "loop pressure query should return loneliness and boredom minima: {best_loop_pressure}"
    );
    let min_loneliness = pressure_parts[0]
        .parse::<i64>()
        .expect("minimum loneliness points");
    let min_boredom = pressure_parts[1]
        .parse::<i64>()
        .expect("minimum boredom points");
    assert!(
        min_loneliness <= 2,
        "LLM resident loop should relieve loneliness below the default pressure, got {min_loneliness}"
    );
    assert!(
        min_boredom <= 1,
        "LLM resident loop should relieve boredom below the default pressure, got {min_boredom}"
    );
    let emotion_parts = daily_report_emotion.split(':').collect::<Vec<_>>();
    assert_eq!(
        emotion_parts.len(),
        3,
        "daily report emotion query should return status, primary mood, and active mood type: {daily_report_emotion}"
    );
    assert_eq!(
        emotion_parts[0], "scored",
        "LLM daily report should be persisted and scored by DADOES, got {daily_report_emotion}"
    );
    assert_ne!(
        emotion_parts[1], "missing",
        "DADOES should provide a primary mood for the LLM daily report"
    );
    assert!(
        !emotion_parts[1].is_empty(),
        "DADOES primary mood should not be empty for the LLM daily report"
    );
    assert_eq!(
        emotion_parts[2], "array",
        "DADOES active moods should be stored as an array for the LLM daily report"
    );
    assert_at_least(
        &report_step_count,
        1,
        "resident daily report completion steps",
    );
    assert_eq!(
        old_loop_effects, "0:0:0:0:0",
        "baseline LLM self-loop must not fall back to workers, Blackstone food, room wages, or hunger recovery"
    );
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
