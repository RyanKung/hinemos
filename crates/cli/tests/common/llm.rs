use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use hinemos_test_support::{TestTempDir, collect_pipe, join_reader, take_buffer};
use serde_json::Value;

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
    fs::write(hermes_home.join("config.yaml"), hermes_test_config())
        .expect("write isolated Hermes config");
    hermes_home
}

fn hermes_test_config() -> String {
    let provider = env_or("HERMES_TEST_PROVIDER", "rotom");
    let model = env_or("HERMES_TEST_MODEL", "gpt-5.5");
    let base_url = env_or("HERMES_TEST_BASE_URL", "http://127.0.0.1:14550/v1");
    let api_mode = env_or("HERMES_TEST_API_MODE", "codex_responses");
    format!(
        r#"model:
  default: {model}
  provider: {provider}
  base_url: {base_url}
  api_mode: {api_mode}
providers:
  {provider}:
    name: isolated Hermes test provider
    base_url: {base_url}
    api_mode: {api_mode}
    default_model: {model}
agent:
  max_turns: 90
  tool_use_enforcement: auto
terminal:
  backend: local
  cwd: .
  timeout: 180
  persistent_shell: true
  lifetime_seconds: 300
  env_passthrough: []
  shell_init_files: []
  auto_source_bashrc: false
display:
  personality: concise
  show_reasoning: false
  tool_preview_length: 0
  tool_progress: all
"#
    )
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
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

#[derive(Debug, Clone)]
pub struct HermesToolCall {
    pub name: String,
    pub arguments: Value,
}

pub fn hermes_latest_session_json(hermes_home: &Path) -> Value {
    let sessions = hermes_home.join("sessions");
    let mut entries = fs::read_dir(&sessions)
        .unwrap_or_else(|error| panic!("read Hermes session dir {}: {error}", sessions.display()))
        .map(|entry| entry.expect("read Hermes session entry"))
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    let path = entries
        .last()
        .unwrap_or_else(|| panic!("no Hermes session JSON under {}", sessions.display()))
        .path();
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read Hermes session JSON {}: {error}", path.display()));
    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("parse Hermes session JSON {}: {error}", path.display()))
}

pub fn hermes_session_tool_names(session: &Value) -> Vec<String> {
    let Some(tools) = session.get("tools").and_then(Value::as_array) else {
        return Vec::new();
    };
    tools
        .iter()
        .filter_map(|tool| {
            tool.get("function")?
                .get("name")?
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

pub fn hermes_session_tool_calls(session: &Value) -> Vec<HermesToolCall> {
    let mut calls = Vec::new();
    let Some(messages) = session.get("messages").and_then(Value::as_array) else {
        return calls;
    };
    for message in messages {
        let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) else {
            continue;
        };
        for tool_call in tool_calls {
            let Some(function) = tool_call.get("function") else {
                continue;
            };
            let Some(name) = function.get("name").and_then(Value::as_str) else {
                continue;
            };
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|arguments| serde_json::from_str(arguments).ok())
                .unwrap_or_else(|| function.get("arguments").cloned().unwrap_or(Value::Null));
            calls.push(HermesToolCall {
                name: name.to_owned(),
                arguments,
            });
        }
    }
    calls
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
