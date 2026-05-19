use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "requires local Claude provider environment and runs an external agent"]
fn claude_can_discover_and_explore_world_over_ssh() {
    let root = workspace_root();
    let provider_env = load_local_provider_env(&root);
    assert_provider_env(&provider_env);
    assert_command_exists("claude");

    let temp = TestTempDir::new("xagora-claude-world");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log);
    wait_for_server(host, port, &mut server, &server_log);

    let prompt = format!(
        "Please connect to {host} on SSH port {port}, figure out what it is, and try to explore it. You may use username {user}."
    );
    let output = run_claude_agent(&prompt, &provider_env, Duration::from_secs(180));

    terminate(&mut server);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(temp.path.join("claude-stdout.log"), stdout.as_bytes()).ok();
    fs::write(temp.path.join("claude-stderr.log"), stderr.as_bytes()).ok();

    assert!(
        output.status.success(),
        "claude verifier failed with status {:?}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        output.status.code(),
        stderr,
        stdout,
        temp.path.display()
    );

    require_output(&stdout, &["ssh", "SSH"], "evidence that it used SSH", &temp);
    require_output(
        &stdout,
        &["Xagora", "open world", "MUD-like"],
        "evidence that it identified the world",
        &temp,
    );
    require_output(
        &stdout,
        &["Available", "/look", "/go", "/mailbox", "/history", "/news"],
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

    println!("{stdout}");
    temp.remove_on_drop();
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live under workspace/crates/cli")
        .to_path_buf()
}

fn load_local_provider_env(root: &Path) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for key in [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
    ] {
        if let Ok(value) = std::env::var(key) {
            values.insert(key.to_owned(), value);
        }
    }

    let env_path = root.join(".env.local");
    let Ok(contents) = fs::read_to_string(env_path) else {
        return values;
    };

    for line in contents.lines() {
        if let Some((key, value)) = parse_env_line(line) {
            values.entry(key).or_insert(value);
        }
    }
    values
}

fn parse_env_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value);
    Some((key.to_owned(), value.to_owned()))
}

fn assert_provider_env(values: &HashMap<String, String>) {
    let missing = [
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
    ]
    .into_iter()
    .filter(|key| values.get(*key).is_none_or(String::is_empty))
    .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "Claude provider environment is incomplete. Missing: {}. Set them in your shell or local-only .env.local.",
        missing.join(", ")
    );
}

fn assert_command_exists(command: &str) {
    let status = Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {command}: {error}"));
    assert!(
        status.success(),
        "required command is not available: {command}"
    );
}

fn free_local_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral local port");
    listener.local_addr().expect("read local addr").port()
}

fn spawn_xagora_server(root: &Path, host: &str, port: u16, log_path: &Path) -> Child {
    let log = fs::File::create(log_path).expect("create server log");
    Command::new(env!("CARGO_BIN_EXE_xagora"))
        .current_dir(root)
        .args(["serve", "ssh", "--bind", &format!("{host}:{port}")])
        .stdout(log.try_clone().expect("clone server log for stdout"))
        .stderr(log)
        .spawn()
        .expect("spawn xagora ssh server")
}

fn wait_for_server(host: &str, port: u16, server: &mut Child, log_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if TcpStream::connect((host, port)).is_ok() {
            return;
        }
        if let Some(status) = server.try_wait().expect("poll server") {
            panic!(
                "xagora server exited before accepting SSH connections: {status}\n{}",
                read_lossy(log_path)
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
    panic!(
        "xagora server did not listen on {host}:{port}\n{}",
        read_lossy(log_path)
    );
}

fn run_claude_agent(
    prompt: &str,
    provider_env: &HashMap<String, String>,
    timeout: Duration,
) -> Output {
    let mut child = Command::new("claude")
        .args([
            "--print",
            "--permission-mode",
            "bypassPermissions",
            "--allowedTools",
            "Bash(ssh *)",
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

    wait_with_timeout(child, timeout)
}

fn wait_with_timeout(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child.try_wait().expect("poll child").is_some() {
            return child.wait_with_output().expect("collect child output");
        }
        thread::sleep(Duration::from_millis(250));
    }
    child.kill().ok();
    let output = child
        .wait_with_output()
        .expect("collect timed-out child output");
    panic!(
        "claude verifier timed out after {} seconds\nstdout:\n{}\nstderr:\n{}",
        timeout.as_secs(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        child.kill().ok();
        child.wait().ok();
    }
}

fn require_output(stdout: &str, needles: &[&str], description: &str, temp: &TestTempDir) {
    let found = needles.iter().any(|needle| {
        stdout
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    });
    assert!(
        found,
        "Claude verifier output is missing: {description}\nlogs: {}\nstdout:\n{stdout}",
        temp.path.display()
    );
}

fn read_lossy(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| String::new())
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
}

struct TestTempDir {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            epoch_seconds()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self {
            path,
            remove_on_drop: false,
        }
    }

    fn remove_on_drop(mut self) {
        self.remove_on_drop = true;
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        if self.remove_on_drop
            && std::env::var("XAGORA_VERIFY_KEEP_LOGS").ok().as_deref() != Some("1")
        {
            fs::remove_dir_all(&self.path).ok();
        } else {
            eprintln!("verifier logs kept at {}", self.path.display());
        }
    }
}
