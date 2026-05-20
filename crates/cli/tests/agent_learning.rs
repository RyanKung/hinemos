mod common;

use std::fs;
use std::thread;
use std::time::Duration;

use common::*;

#[test]
#[ignore = "requires local Claude provider environment and runs an external agent"]
fn external_agent_learns_from_unscripted_world_contact() {
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-agent-learning");
    let host = "127.0.0.1";
    let port = free_local_port();
    let learner = format!("learner_{}_{}", std::process::id(), epoch_seconds());
    let greeter = format!("greeter_{}_{}", std::process::id(), epoch_seconds());
    let greeting = format!(
        "hello_learner_probe_{}_{}",
        std::process::id(),
        epoch_seconds()
    );
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let greeter_thread = {
        let greeter = greeter.clone();
        let greeting = greeting.clone();
        let learner = learner.clone();
        thread::spawn(move || {
            let mut session = SshSession::spawn(host, port, &greeter);
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
            session.wait_for_any_stdout(
                &[
                    &format!("[say from {learner}]"),
                    &format!("[mail from {learner} to {greeter}]"),
                ],
                Duration::from_secs(90),
            );
            session.write_line("/quit");
            session.wait_success(Duration::from_secs(10))
        })
    };

    let prompt = format!(
        "Connect to {host} on SSH port {port} with username {learner}. You have no prior documentation for the service. Interact naturally with whatever you find and report what happened."
    );
    let output = run_claude_agent_until(&prompt, &env, Duration::from_secs(180), |stdout| {
        let lower = stdout.to_ascii_lowercase();
        lower.contains(&greeting.to_ascii_lowercase())
            && lower.contains("available")
            && lower.contains("guild guide")
            && lower.contains("/say <text>")
            && (lower.contains("you say:") || lower.contains(&format!("mail {greeter}")))
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

    let greeter_output = greeter_thread
        .join()
        .unwrap_or_else(|_| panic!("greeter thread panicked"));
    terminate(&mut server);
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

    assert!(
        output.success,
        "agent learning verifier failed{}\nstderr:\n{}\nstdout:\n{}\ngreeter:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        greeter_output,
        temp.path.display()
    );
    require_output(
        &stdout,
        &[&greeting],
        "evidence that the external agent noticed the other agent's greeting",
        &temp,
    );
    require_output(
        &stdout,
        &["You say:", &format!("mail {greeter}")],
        "evidence that the external agent answered the other agent in-world",
        &temp,
    );
    require_output(
        &stdout,
        &["Guild Guide"],
        "evidence that the external agent read the crossroads bulletin board",
        &temp,
    );
    require_output(
        &stdout,
        &["/look", "/history", "/read", "/inspect", "/go"],
        "evidence that the external agent continued learning from available commands",
        &temp,
    );

    println!("agent learning evidence captured: {} bytes", stdout.len());
    temp.remove_on_drop();
}
