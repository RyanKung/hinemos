use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use hinemos_test_support::{epoch_nanos, read_lossy, wait_with_timeout};

pub fn spawn_hinemos_server(
    root: &Path,
    host: &str,
    port: u16,
    log_path: &Path,
    database_url: &str,
) -> Child {
    spawn_hinemos_server_with_env(root, host, port, log_path, database_url, [])
}

pub fn spawn_hinemos_server_with_env<const N: usize>(
    root: &Path,
    host: &str,
    port: u16,
    log_path: &Path,
    database_url: &str,
    envs: [(&str, &str); N],
) -> Child {
    spawn_hinemos_server_with_options(HinemosServerOptions {
        root,
        host,
        port,
        log_path,
        database_url,
        world: None,
        admin_socket: None,
        envs,
    })
}

pub struct HinemosServerOptions<'a, const N: usize> {
    pub root: &'a Path,
    pub host: &'a str,
    pub port: u16,
    pub log_path: &'a Path,
    pub database_url: &'a str,
    pub world: Option<&'a Path>,
    pub admin_socket: Option<&'a Path>,
    pub envs: [(&'a str, &'a str); N],
}

#[allow(dead_code)]
pub fn spawn_hinemos_server_with_options<const N: usize>(
    options: HinemosServerOptions<'_, N>,
) -> Child {
    let log = fs::File::create(options.log_path).expect("create server log");
    let default_admin_socket = options.admin_socket.is_none().then(default_admin_socket);
    let mut command = Command::new(env!("CARGO_BIN_EXE_hinemos"));
    command
        .current_dir(options.root)
        .args([
            "serve",
            "ssh",
            "--bind",
            &format!("{}:{}", options.host, options.port),
        ])
        .env("DATABASE_URL", options.database_url);
    if let Some(world) = options.world {
        command.arg("--world").arg(world);
    }
    if let Some(admin_socket) = options.admin_socket {
        command.arg("--admin-socket").arg(admin_socket);
    } else if let Some(admin_socket) = &default_admin_socket {
        command.arg("--admin-socket").arg(admin_socket);
    }
    for (key, value) in options.envs {
        command.env(key, value);
    }
    command
        .stdout(log.try_clone().expect("clone server log for stdout"))
        .stderr(log)
        .spawn()
        .expect("spawn hinemos ssh server")
}

fn default_admin_socket() -> PathBuf {
    std::env::temp_dir().join(format!(
        "hinemos-admin-{}-{}.sock",
        std::process::id(),
        epoch_nanos()
    ))
}

#[allow(dead_code)]
pub fn spawn_hinemos_rooms(
    root: &Path,
    log_path: &Path,
    database_url: &str,
    poll_interval_ms: u64,
) -> Child {
    let log = fs::File::create(log_path).expect("create rooms log");
    Command::new(env!("CARGO_BIN_EXE_hinemos"))
        .current_dir(root)
        .args([
            "serve",
            "rooms",
            "--database-url",
            database_url,
            "--poll-interval-ms",
            &poll_interval_ms.to_string(),
        ])
        .stdout(log.try_clone().expect("clone rooms log for stdout"))
        .stderr(log)
        .spawn()
        .expect("spawn hinemos room runner")
}

pub fn run_hinemos_rooms_once(root: &Path, database_url: &str) -> String {
    let child = Command::new(env!("CARGO_BIN_EXE_hinemos"))
        .current_dir(root)
        .args(["serve", "rooms", "--database-url", database_url, "--once"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn one-shot room runner");
    let output = wait_with_timeout(child, Duration::from_secs(30));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "one-shot room runner failed: {stderr}\nstdout:\n{stdout}"
    );
    stdout.into_owned()
}

pub fn wait_for_server(host: &str, port: u16, server: &mut Child, log_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if TcpStream::connect((host, port)).is_ok() {
            return;
        }
        if let Some(status) = server.try_wait().expect("poll server") {
            panic!(
                "hinemos server exited before accepting SSH connections: {status}\n{}",
                read_lossy(log_path)
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
    panic!(
        "hinemos server did not listen on {host}:{port}\n{}",
        read_lossy(log_path)
    );
}
