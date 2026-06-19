use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::assertions::assert_contains;
use super::process::{
    collect_pipe, join_reader, take_buffer, wait_child_success, wait_with_timeout,
};
use super::temp::TestTempDir;

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

pub fn generate_ed25519_key(path: &Path) {
    let output = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        output.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn run_ssh_batch_with_key(
    host: &str,
    port: u16,
    user: &str,
    key_path: &Path,
    commands: &[&str],
) -> String {
    let input = format!("{}\n", commands.join("\n"));
    let mut child = ssh_command(host, port, user, Some(key_path))
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

pub fn admit_ssh_user(host: &str, port: u16, user: &str, key_path: &Path) {
    let output = run_ssh_batch_with_key(
        host,
        port,
        user,
        key_path,
        &["/read agreement", "/agree", "/quit"],
    );
    assert_contains(
        &output,
        "Agreement accepted",
        "test user completes admission",
    );
}

pub fn admitted_key(temp: &TestTempDir, host: &str, port: u16, user: &str) -> PathBuf {
    let key_path = temp.path.join(format!("{user}_ed25519"));
    generate_ed25519_key(&key_path);
    admit_ssh_user(host, port, user, &key_path);
    key_path
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
        Self::spawn_with_args(host, port, user, None, [])
    }

    pub fn spawn_with_key(host: &str, port: u16, user: &str, key_path: &Path) -> Self {
        Self::spawn_with_args(host, port, user, Some(key_path), [])
    }

    #[allow(dead_code)]
    pub fn spawn_exec<const N: usize>(host: &str, port: u16, user: &str, args: [&str; N]) -> Self {
        Self::spawn_with_args(host, port, user, None, args)
    }

    pub fn spawn_exec_with_key<const N: usize>(
        host: &str,
        port: u16,
        user: &str,
        key_path: &Path,
        args: [&str; N],
    ) -> Self {
        Self::spawn_with_args(host, port, user, Some(key_path), args)
    }

    fn spawn_with_args<const N: usize>(
        host: &str,
        port: u16,
        user: &str,
        key_path: Option<&Path>,
        args: [&str; N],
    ) -> Self {
        let mut child = ssh_command(host, port, user, key_path)
            .args(args)
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

fn ssh_command(host: &str, port: u16, user: &str, key_path: Option<&Path>) -> Command {
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
    ]);
    if let Some(key_path) = key_path {
        command
            .arg("-i")
            .arg(key_path)
            .args(["-o", "IdentitiesOnly=yes"]);
    }
    command.args(["-p", &port.to_string(), &format!("{user}@{host}")]);
    command
}
