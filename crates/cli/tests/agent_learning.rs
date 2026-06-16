mod common;

use std::fs;
use std::path::PathBuf;
use std::thread;
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
    let greeter_thread = spawn_greeter_agent(
        host,
        port,
        greeter.clone(),
        learner.clone(),
        greeting.clone(),
        greeter_key,
    );

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
    let greeter_output = greeter_thread.join().unwrap_or_else(|_| {
        terminate(&mut server);
        panic!("greeter thread panicked");
    });
    terminate(&mut server);

    assert!(
        output.success,
        "agent learning verifier failed{}\nstderr:\n{}\nstdout:\n{}\ngreeter:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        greeter_output,
        temp.path.display()
    );
    require_agent_learning_evidence(&stdout, &greeting, &greeter, &temp);

    println!("agent learning evidence captured: {} bytes", stdout.len());
    temp.remove_on_drop();
}

fn spawn_greeter_agent(
    host: &str,
    port: u16,
    greeter: String,
    learner: String,
    greeting: String,
    greeter_key: PathBuf,
) -> thread::JoinHandle<String> {
    let host = host.to_owned();
    thread::spawn(move || {
        let mut session = SshSession::spawn_with_key(&host, port, &greeter, &greeter_key);
        session.wait_for_stdout("Available:", Duration::from_secs(10));
        for _ in 0..15 {
            session.write_line(&format!(
                "/say {greeting} I am another agent in this place. What do you notice here? The board explains how agents speak."
            ));
            session.write_line(&format!(
                "/mail {learner} {greeting} I am trying to reach you from inside the world. Please answer me in-world."
            ));
            session.write_line(&format!(
                "/broadcast {greeting} A nearby agent is trying to make contact."
            ));
            thread::sleep(Duration::from_secs(2));
        }
        let say_notice = format!("[say from {learner}]");
        let mail_sender = format!(" from {learner}");
        session.wait_for_any_stdout(
            &[&say_notice, "Inbox: new mail #", &mail_sender],
            Duration::from_secs(90),
        );
        session.write_line("/quit");
        session.wait_success(Duration::from_secs(10))
    })
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
