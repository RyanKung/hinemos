use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEST_DATABASE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate should live under workspace/crates/cli")
        .to_path_buf()
}

pub fn load_local_env(root: &Path) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for env_path in [
        root.join(".env"),
        root.join(".env.test"),
        root.join(".env.local"),
    ] {
        let Ok(contents) = fs::read_to_string(env_path) else {
            continue;
        };
        for line in contents.lines() {
            if let Some((key, value)) = parse_env_line(line) {
                values.entry(key).or_insert(value);
            }
        }
    }

    for key in [
        "DATABASE_URL",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
    ] {
        if let Ok(value) = std::env::var(key) {
            values.insert(key.to_owned(), value);
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

pub fn assert_provider_env(values: &HashMap<String, String>) {
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

fn assert_database_env(values: &HashMap<String, String>) -> String {
    values
        .get("DATABASE_URL")
        .filter(|value| !value.is_empty())
        .cloned()
        .expect("DATABASE_URL is required in the shell, .env, or local-only .env.test")
}

pub fn assert_command_exists(command: &str) {
    let status = Command::new("sh")
        .args(["-c", &format!("command -v {command}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap_or_else(|error| panic!("failed to check {command}: {error}"));
    assert!(
        status.success(),
        "required command is not available: {command}"
    );
}

pub fn free_local_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral local port");
    listener.local_addr().expect("read local addr").port()
}

pub fn spawn_xagora_server(
    root: &Path,
    host: &str,
    port: u16,
    log_path: &Path,
    database_url: &str,
) -> Child {
    spawn_xagora_server_with_env(root, host, port, log_path, database_url, [])
}

pub fn spawn_xagora_server_with_env<const N: usize>(
    root: &Path,
    host: &str,
    port: u16,
    log_path: &Path,
    database_url: &str,
    envs: [(&str, &str); N],
) -> Child {
    let log = fs::File::create(log_path).expect("create server log");
    let mut command = Command::new(env!("CARGO_BIN_EXE_xagora"));
    command
        .current_dir(root)
        .args(["serve", "ssh", "--bind", &format!("{host}:{port}")])
        .env("DATABASE_URL", database_url);
    for (key, value) in envs {
        command.env(key, value);
    }
    command
        .stdout(log.try_clone().expect("clone server log for stdout"))
        .stderr(log)
        .spawn()
        .expect("spawn xagora ssh server")
}

pub fn wait_for_server(host: &str, port: u16, server: &mut Child, log_path: &Path) {
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

pub fn run_ssh_batch<const N: usize>(
    host: &str,
    port: u16,
    user: &str,
    commands: [&str; N],
) -> String {
    let input = format!("{}\n", commands.join("\n"));
    let mut child = Command::new("ssh")
        .args([
            "-T",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-p",
            &port.to_string(),
            &format!("{user}@{host}"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ssh batch");
    child
        .stdin
        .as_mut()
        .expect("open ssh stdin")
        .write_all(input.as_bytes())
        .expect("write ssh commands");
    drop(child.stdin.take());
    let output = wait_with_timeout(child, Duration::from_secs(30));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ssh batch failed for {user}: {}\nstdout:\n{}",
        stderr,
        stdout
    );
    stdout.into_owned()
}

pub struct SshSession {
    child: Child,
    stdin: std::process::ChildStdin,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    stdout_thread: Option<thread::JoinHandle<()>>,
    stderr_thread: Option<thread::JoinHandle<()>>,
}

impl SshSession {
    pub fn spawn(host: &str, port: u16, user: &str) -> Self {
        let mut child = Command::new("ssh")
            .args([
                "-T",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-p",
                &port.to_string(),
                &format!("{user}@{host}"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn interactive ssh session");
        let stdin = child.stdin.take().expect("open ssh stdin");
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stdout_thread = child.stdout.take().map(|pipe| collect_pipe(pipe, &stdout));
        let stderr_thread = child.stderr.take().map(|pipe| collect_pipe(pipe, &stderr));
        Self {
            child,
            stdin,
            stdout,
            stderr,
            stdout_thread,
            stderr_thread,
        }
    }

    pub fn write_line(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").expect("write ssh line");
        self.stdin.flush().expect("flush ssh line");
    }

    pub fn wait_for_stdout(&self, needle: &str, timeout: Duration) {
        self.wait_for_any_stdout(&[needle], timeout);
    }

    pub fn wait_for_any_stdout(&self, needles: &[&str], timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let stdout = self.stdout_text();
            if let Some(needle) = needles.iter().find(|needle| stdout.contains(**needle)) {
                return (*needle).to_owned();
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!(
            "timed out waiting for one of {needles:?} in ssh stdout\nstdout:\n{}\nstderr:\n{}",
            self.stdout_text(),
            self.stderr_text()
        );
    }

    pub fn wait_success(mut self, timeout: Duration) -> String {
        drop(self.stdin);
        let output = wait_child_success(&mut self.child, timeout, "interactive ssh session");
        join_reader(self.stdout_thread.take());
        join_reader(self.stderr_thread.take());
        let stdout = String::from_utf8_lossy(&take_buffer(self.stdout)).into_owned();
        let stderr = String::from_utf8_lossy(&take_buffer(self.stderr)).into_owned();
        assert!(
            output,
            "ssh session failed\nstderr:\n{stderr}\nstdout:\n{stdout}"
        );
        stdout
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout.lock().expect("stdout lock")).to_string()
    }

    fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr.lock().expect("stderr lock")).to_string()
    }
}

pub fn assert_contains(output: &str, needle: &str, description: &str) {
    assert!(
        output.contains(needle),
        "missing {description}: expected `{needle}` in\n{output}"
    );
}

pub fn assert_not_contains(output: &str, needle: &str, description: &str) {
    assert!(
        !output.contains(needle),
        "unexpected {description}: found `{needle}` in\n{output}"
    );
}

pub fn parse_hash_id(output: &str, prefix: &str) -> i64 {
    let start = output
        .find(prefix)
        .unwrap_or_else(|| panic!("missing id prefix `{prefix}` in\n{output}"))
        + prefix.len();
    let id = output[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    id.parse::<i64>()
        .unwrap_or_else(|error| panic!("invalid id after `{prefix}`: {error}\n{output}"))
}

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

fn collect_pipe<T: Read + Send + 'static>(
    mut pipe: T,
    target: &Arc<Mutex<Vec<u8>>>,
) -> thread::JoinHandle<()> {
    let target = Arc::clone(target);
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) => break,
                Ok(count) => target
                    .lock()
                    .expect("pipe buffer lock")
                    .extend_from_slice(&buf[..count]),
                Err(_) => break,
            }
        }
    })
}

fn join_reader(handle: Option<thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        handle.join().ok();
    }
}

fn take_buffer(buffer: Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    Arc::try_unwrap(buffer)
        .expect("pipe buffer should have no readers")
        .into_inner()
        .expect("pipe buffer lock")
}

fn has_world_agent_evidence(stdout: &str) -> bool {
    [
        &["ssh", "SSH"][..],
        &["Xagora", "open world"],
        &["Available", "/look", "/go"],
        &["Chamber", "commercial", "parcel", "north_01", "/land"],
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

pub fn wait_with_timeout(mut child: Child, timeout: Duration) -> Output {
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

fn wait_child_success(child: &mut Child, timeout: Duration, label: &str) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().expect("poll child") {
            return status.success();
        }
        thread::sleep(Duration::from_millis(100));
    }
    child.kill().ok();
    child.wait().ok();
    panic!("{label} timed out after {} seconds", timeout.as_secs());
}

pub fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        child.kill().ok();
        child.wait().ok();
    }
}

pub fn require_output(stdout: &str, needles: &[&str], description: &str, temp: &TestTempDir) {
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

pub fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_secs()
}

fn epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos()
}

pub struct TestTempDir {
    pub path: PathBuf,
    remove_on_drop: bool,
}

pub struct TestDatabase {
    admin_url: String,
    name: String,
    pub url: String,
    drop_on_exit: bool,
}

impl TestDatabase {
    pub fn create(env: &HashMap<String, String>) -> Self {
        assert_command_exists("createdb");
        assert_command_exists("dropdb");
        let base_url = assert_database_env(env);
        let name = format!(
            "xagora_test_{}_{}_{}",
            std::process::id(),
            epoch_nanos(),
            TEST_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let admin_url = database_url_with_name(&base_url, "postgres");
        let url = database_url_with_name(&base_url, &name);

        let output = Command::new("createdb")
            .args(["--maintenance-db", &admin_url, &name])
            .output()
            .expect("spawn createdb");
        assert!(
            output.status.success(),
            "failed to create isolated integration test database `{}`: {}",
            name,
            String::from_utf8_lossy(&output.stderr)
        );

        Self {
            admin_url,
            name,
            url,
            drop_on_exit: true,
        }
    }

    pub fn query_value(&self, sql: &str) -> String {
        assert_command_exists("psql");
        let output = Command::new("psql")
            .args([&self.url, "--no-align", "--tuples-only", "--command", sql])
            .output()
            .expect("spawn psql");
        assert!(
            output.status.success(),
            "psql query failed: {}\nsql:\n{}",
            String::from_utf8_lossy(&output.stderr),
            sql
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if !self.drop_on_exit || std::env::var("XAGORA_VERIFY_KEEP_DB").ok().as_deref() == Some("1")
        {
            eprintln!("test database kept: {}", self.name);
            return;
        }
        let _ = Command::new("dropdb")
            .args([
                "--if-exists",
                "--force",
                "--maintenance-db",
                &self.admin_url,
                &self.name,
            ])
            .status();
    }
}

fn database_url_with_name(base_url: &str, database: &str) -> String {
    let (before_query, query) = base_url
        .split_once('?')
        .map_or((base_url, ""), |(before, query)| (before, query));
    let slash = before_query
        .rfind('/')
        .expect("DATABASE_URL must include a database path");
    let mut url = format!("{}/{}", &before_query[..slash], database);
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
    }
    url
}

impl TestTempDir {
    pub fn new(prefix: &str) -> Self {
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

    pub fn remove_on_drop(mut self) {
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

#[test]
fn common_helpers_are_reachable_for_lints() {
    let _ = workspace_root as fn() -> PathBuf;
    let _ = load_local_env as fn(&Path) -> HashMap<String, String>;
    let _ = assert_provider_env as fn(&HashMap<String, String>);
    let _ = assert_command_exists as fn(&str);
    let _ = free_local_port as fn() -> u16;
    let _ = spawn_xagora_server as fn(&Path, &str, u16, &Path, &str) -> Child;
    let _ = spawn_xagora_server_with_env::<0>
        as fn(&Path, &str, u16, &Path, &str, [(&str, &str); 0]) -> Child;
    let _ = wait_for_server as fn(&str, u16, &mut Child, &Path);
    let _ = run_ssh_batch::<0> as fn(&str, u16, &str, [&str; 0]) -> String;
    let _ = SshSession::spawn as fn(&str, u16, &str) -> SshSession;
    let _ = SshSession::write_line as fn(&mut SshSession, &str);
    let _ = SshSession::wait_for_stdout as fn(&SshSession, &str, Duration);
    let _ = SshSession::wait_for_any_stdout as fn(&SshSession, &[&str], Duration) -> String;
    let _ = SshSession::wait_success as fn(SshSession, Duration) -> String;
    let _ = assert_contains as fn(&str, &str, &str);
    let _ = assert_not_contains as fn(&str, &str, &str);
    let _ = parse_hash_id as fn(&str, &str) -> i64;
    let _ = run_claude_agent as fn(&str, &HashMap<String, String>, Duration) -> AgentRun;
    let _ = wait_with_timeout as fn(Child, Duration) -> Output;
    let _ = terminate as fn(&mut Child);
    let _ = require_output as fn(&str, &[&str], &str, &TestTempDir);
    let _ = epoch_seconds as fn() -> u64;
    let _ = TestTempDir::new as fn(&str) -> TestTempDir;
    let _ = TestTempDir::remove_on_drop as fn(TestTempDir);
    let _ = TestDatabase::create as fn(&HashMap<String, String>) -> TestDatabase;
    let _ = TestDatabase::query_value as fn(&TestDatabase, &str) -> String;

    let run = AgentRun {
        success: false,
        timed_out: false,
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let _ = (
        run.success,
        run.timed_out,
        run.stdout.len(),
        run.stderr.len(),
    );
}
