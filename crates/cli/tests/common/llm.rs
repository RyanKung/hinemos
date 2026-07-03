use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hinemos_test_support::{collect_pipe, join_reader, take_buffer};

pub struct AgentRun {
    pub success: bool,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn run_claude_agent(
    prompt: &str,
    provider_env: &HashMap<String, String>,
    timeout: Duration,
) -> AgentRun {
    run_claude_agent_until(prompt, provider_env, timeout, has_world_agent_evidence)
}

pub fn run_claude_agent_until(
    prompt: &str,
    provider_env: &HashMap<String, String>,
    timeout: Duration,
    evidence: impl Fn(&str) -> bool,
) -> AgentRun {
    run_claude_agent_until_with_tools(prompt, provider_env, timeout, &["Bash(ssh *)"], evidence)
}

pub fn run_claude_agent_until_with_tools(
    prompt: &str,
    provider_env: &HashMap<String, String>,
    timeout: Duration,
    allowed_tools: &[&str],
    evidence: impl Fn(&str) -> bool,
) -> AgentRun {
    let allowed_tools = allowed_tools.join(",");
    let mut child = Command::new("claude")
        .args([
            "--print",
            "--verbose",
            "--output-format",
            "stream-json",
            "--include-partial-messages",
            "--permission-mode",
            "bypassPermissions",
            "--allowedTools",
            &allowed_tools,
        ])
        .envs(provider_env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn claude verifier");

    child
        .stdin
        .as_mut()
        .expect("open claude stdin")
        .write_all(prompt.as_bytes())
        .expect("write claude prompt");
    drop(child.stdin.take());

    wait_for_agent_evidence(child, timeout, evidence)
}

fn wait_for_agent_evidence(
    mut child: Child,
    timeout: Duration,
    evidence: impl Fn(&str) -> bool,
) -> AgentRun {
    let stdout = Arc::new(Mutex::new(Vec::new()));
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let mut stdout_thread = child.stdout.take().map(|pipe| collect_pipe(pipe, &stdout));
    let mut stderr_thread = child.stderr.take().map(|pipe| collect_pipe(pipe, &stderr));
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("poll child") {
            join_reader(stdout_thread.take());
            join_reader(stderr_thread.take());
            return AgentRun {
                success: status.success(),
                timed_out: false,
                stdout: take_buffer(stdout),
                stderr: take_buffer(stderr),
            };
        }

        let snapshot = String::from_utf8_lossy(&stdout.lock().expect("stdout lock")).to_string();
        if evidence(&snapshot) {
            child.kill().ok();
            child.wait().ok();
            join_reader(stdout_thread.take());
            join_reader(stderr_thread.take());
            return AgentRun {
                success: true,
                timed_out: false,
                stdout: take_buffer(stdout),
                stderr: take_buffer(stderr),
            };
        }

        thread::sleep(Duration::from_millis(250));
    }

    child.kill().ok();
    child.wait().ok();
    join_reader(stdout_thread.take());
    join_reader(stderr_thread.take());
    AgentRun {
        success: false,
        timed_out: true,
        stdout: take_buffer(stdout),
        stderr: take_buffer(stderr),
    }
}

fn has_world_agent_evidence(stdout: &str) -> bool {
    [
        &["ssh", "SSH"][..],
        &["Hinemos", "open world"],
        &["Available", "/look", "/go"],
        &["Guild", "market", "parcel", "north_01", "/land"],
        &["claim", "build", "publish", "/build", "owned", "shop"],
    ]
    .iter()
    .all(|needles| {
        needles.iter().any(|needle| {
            stdout
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
        })
    })
}
