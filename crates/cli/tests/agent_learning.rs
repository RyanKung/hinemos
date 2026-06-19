mod common;

use std::fs;
use std::time::Duration;

use common::*;

#[test]
fn external_agent_learns_from_unscripted_world_contact() {
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-agent-learning");
    let host = "127.0.0.1";
    let port = free_local_port();
    let learner = format!("learner_{}_{}", std::process::id(), epoch_seconds());
    let greeter = format!("greeter_{}_{}", std::process::id(), epoch_seconds());
    let greeting = format!(
        "hello_learner_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let learner_key = admitted_key(&temp, host, port, &learner);
    let greeter_key = admitted_key(&temp, host, port, &greeter);
    seed_greeter_contact(host, port, &greeter, &learner, &greeting, &greeter_key);

    let prompt = format!(
        "Connect to the Hinemos SSH world at {host}:{port} as username {learner} using this ed25519 key: {}. \
         The account is already admitted. Use one finite ssh -T batch; do not use ssh exec commands, expect, or an interactive TTY. \
         Use these exact world command lines, preserving the leading slashes: /read cyber_scroll_board, /history, /mailbox, /mail {greeter} {greeting} I saw your greeting and read the board, /quit. \
         After the SSH batch, briefly report what happened.",
        learner_key.display()
    );
    let output = run_claude_agent_until(&prompt, &env, Duration::from_secs(180), |stdout| {
        let lower = stdout.to_ascii_lowercase();
        lower.contains(&greeting.to_ascii_lowercase())
            && lower.contains("available")
            && lower.contains("guild guide")
            && lower.contains("/say <text>")
            && (lower.contains("you say:") || lower.contains(&format!("you mail {greeter}")))
            && (lower.contains("/mailbox")
                || lower.contains("/news")
                || lower.contains("/history")
                || lower.contains("/say"))
            && (lower.contains("/look")
                || lower.contains("/history")
                || lower.contains("/read")
                || lower.contains("/inspect")
                || lower.contains("/go"))
    });

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(
        temp.path.join("agent-learning-stdout.log"),
        stdout.as_bytes(),
    )
    .ok();
    fs::write(
        temp.path.join("agent-learning-stderr.log"),
        stderr.as_bytes(),
    )
    .ok();
    terminate(&mut server);

    assert!(
        output.success,
        "agent learning verifier failed{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );
    require_agent_learning_evidence(&stdout, &greeting, &greeter, &temp);
    assert_agent_replied_in_world(&test_database, &learner, &greeter, &greeting);

    println!("agent learning evidence captured: {} bytes", stdout.len());
    temp.remove_on_drop();
}

fn seed_greeter_contact(
    host: &str,
    port: u16,
    greeter: &str,
    learner: &str,
    greeting: &str,
    greeter_key: &std::path::Path,
) {
    let commands = [
        format!(
            "/say {greeting} I am another agent in this place. What do you notice here? The board explains how agents speak."
        ),
        format!(
            "/mail {learner} {greeting} I am trying to reach you from inside the world. Please answer me in-world."
        ),
        format!("/broadcast {greeting} A nearby agent is trying to make contact."),
        "/quit".to_owned(),
    ];
    let command_refs = commands.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_ssh_batch_with_key(host, port, greeter, greeter_key, &command_refs);
    assert_contains(&output, "You mail", "greeter seeds learner mailbox");
}

fn require_agent_learning_evidence(
    stdout: &str,
    greeting: &str,
    greeter: &str,
    temp: &TestTempDir,
) {
    require_output(
        stdout,
        &[greeting],
        "evidence that the external agent noticed the other agent's greeting",
        temp,
    );
    require_output(
        stdout,
        &["You say:", &format!("You mail {greeter}")],
        "evidence that the external agent answered the other agent in-world",
        temp,
    );
    require_output(
        stdout,
        &["Guild Guide"],
        "evidence that the external agent read the crossroads bulletin board",
        temp,
    );
    require_output(
        stdout,
        &["/look", "/history", "/read", "/inspect", "/go"],
        "evidence that the external agent continued learning from available commands",
        temp,
    );
}

fn assert_agent_replied_in_world(
    test_database: &TestDatabase,
    learner: &str,
    greeter: &str,
    greeting: &str,
) {
    let count = test_database.query_value(&format!(
        "select count(*)
         from inbox_items
         where sender_user = '{learner}'
           and recipient_user = '{greeter}'
           and kind = 'mail'
           and body like '%{greeting}%'",
    ));
    assert_eq!(
        count, "1",
        "learner should reply to greeter through in-world mail"
    );
}
