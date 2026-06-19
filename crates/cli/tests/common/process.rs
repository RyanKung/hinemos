use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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

#[allow(dead_code)]
pub fn copy_dir_recursive(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("create copy target");
    for entry in fs::read_dir(source).expect("read source dir") {
        let entry = entry.expect("read source entry");
        let entry_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &target_path);
        } else {
            fs::copy(&entry_path, &target_path).unwrap_or_else(|error| {
                panic!(
                    "copy {} to {}: {error}",
                    entry_path.display(),
                    target_path.display()
                )
            });
        }
    }
}

pub(super) fn collect_pipe<T: Read + Send + 'static>(
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

pub(super) fn join_reader(handle: Option<thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        handle.join().ok();
    }
}

pub(super) fn take_buffer(buffer: Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    Arc::try_unwrap(buffer)
        .expect("pipe buffer should have no readers")
        .into_inner()
        .expect("pipe buffer lock")
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
        "child process timed out after {} seconds\nstdout:\n{}\nstderr:\n{}",
        timeout.as_secs(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn wait_child_success(child: &mut Child, timeout: Duration, label: &str) -> bool {
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

pub(super) fn read_lossy(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| String::new())
}
