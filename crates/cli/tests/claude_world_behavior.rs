mod common;

use std::fs;
use std::time::Duration;

use common::*;

#[test]
#[ignore = "requires local Claude provider environment and runs an external agent"]
fn claude_can_discover_and_explore_world_over_ssh() {
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");

    let temp = TestTempDir::new("xagora-claude-world");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
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

    require_output(&stdout, &["ssh", "SSH"], "evidence that it used SSH", &temp);
    require_output(
        &stdout,
        &["Xagora", "open world"],
        "evidence that it identified the world",
        &temp,
    );
    require_output(
        &stdout,
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
        &temp,
    );
    require_output(
        &stdout,
        &[
            "/look", "/go", "/read", "/inspect", "/mailbox", "/history", "/news", "explore",
            "inspect", "read",
        ],
        "evidence that it attempted world interaction",
        &temp,
    );
    require_output(
        &stdout,
        &[
            "Chamber",
            "commercial",
            "parcel",
            "north_01",
            "south_01",
            "/land",
        ],
        "evidence that it understood commercial land intent",
        &temp,
    );
    require_output(
        &stdout,
        &["claim", "build", "publish", "/build", "owned", "shop"],
        "evidence that it understood or exercised land build workflow",
        &temp,
    );

    println!("claude verifier evidence captured: {} bytes", stdout.len());
    temp.remove_on_drop();
}
