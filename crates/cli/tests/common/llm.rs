use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hinemos_test_support::{TestTempDir, collect_pipe, join_reader, take_buffer};

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

pub fn prepare_hermes_test_home(temp: &TestTempDir) -> PathBuf {
    let hermes_home = temp.path.join("hermes-home");
    fs::create_dir_all(&hermes_home).expect("create isolated Hermes home");
    let source_home = source_hermes_home();
    let source_config = source_home.join("config.yaml");
    assert!(
        source_config.exists(),
        "Hermes config not found at {}. Set HERMES_TEST_SOURCE_HOME or HERMES_HOME to a configured Hermes home.",
        source_config.display()
    );
    fs::copy(&source_config, hermes_home.join("config.yaml")).expect("copy Hermes config");
    copy_optional_file(&source_home.join(".env"), &hermes_home.join(".env"));
    hermes_home
}

fn source_hermes_home() -> PathBuf {
    std::env::var_os("HERMES_TEST_SOURCE_HOME")
        .or_else(|| std::env::var_os("HERMES_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".hermes")))
        .expect("HOME or HERMES_TEST_SOURCE_HOME must be set for Hermes tests")
}

fn copy_optional_file(source: &Path, destination: &Path) {
    if source.exists() {
        fs::copy(source, destination).expect("copy optional Hermes file");
    }
}

pub fn run_hermes_agent_until(
    prompt: &str,
    provider_env: &HashMap<String, String>,
    timeout: Duration,
    hermes_home: &Path,
    working_dir: &Path,
    toolsets: &[&str],
    evidence: impl Fn(&str) -> bool,
) -> AgentRun {
    let toolsets = toolsets.join(",");
    let child = Command::new("hermes")
        .args([
            "chat",
            "--ignore-rules",
            "--source",
            "hinemos-test",
            "--max-turns",
            "90",
            "-t",
            &toolsets,
            "-q",
            prompt,
        ])
        .current_dir(working_dir)
        .envs(provider_env)
        .env("HERMES_HOME", hermes_home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn Hermes verifier");

    wait_for_agent_evidence(child, timeout, evidence)
}

pub fn hermes_cron_list(hermes_home: &Path) -> String {
    let output = Command::new("hermes")
        .args(["cron", "list"])
        .env("HERMES_HOME", hermes_home)
        .output()
        .expect("run Hermes cron list");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "Hermes cron list failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    format!("{stdout}{stderr}")
}

pub fn hermes_terminal_command_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| {
            let (_, command) = line.split_once(" $")?;
            let command = command.trim();
            (!command.is_empty()).then(|| command.to_owned())
        })
        .collect()
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
