use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use hinemos_test_support::{
    TestTempDir, copy_dir_recursive, epoch_nanos, read_lossy, wait_with_timeout,
};

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

pub fn spawn_hinemos_http(root: &Path, host: &str, port: u16, log_path: &Path) -> Child {
    let log = fs::File::create(log_path).expect("create http log");
    Command::new(env!("CARGO_BIN_EXE_hinemos"))
        .current_dir(root)
        .args([
            "serve",
            "http",
            "--bind",
            &format!("{host}:{port}"),
            "--world",
            "worlds/sample",
            "--static-dir",
            "web/landing/dist",
        ])
        .stdout(log.try_clone().expect("clone http log for stdout"))
        .stderr(log)
        .spawn()
        .expect("spawn hinemos http server")
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

pub fn prepare_builtin_world(root: &Path, temp: &TestTempDir) -> PathBuf {
    let world_dir = temp.path.join("builtin-world");
    copy_dir_recursive(&root.join("worlds/sample"), &world_dir);
    restore_builtin_shopfronts(&world_dir);
    fs::write(
        world_dir.join("meta.ron"),
        r#"(
builtin_service_rooms_enabled: true,
hunger_loop_enabled: true,
)"#,
    )
    .expect("write builtin world meta");
    world_dir
}

fn restore_builtin_shopfronts(world_dir: &Path) {
    let path = world_dir.join("views.ron");
    let mut views = fs::read_to_string(&path).expect("read views.ron");
    replace_in_view(
        &mut views,
        "arrival_street",
        r#"(direction: north, target: "grid_road_x0_yp1", label: Some("North 1 Rd."), requirements: [])"#,
        r#"(direction: north, target: "street_north_01", label: Some("Agentopia Blvd North"), requirements: [])"#,
    );
    replace_in_view(
        &mut views,
        "arrival_street",
        r#"(direction: south, target: "grid_road_x0_ym1", label: Some("South 1 Rd."), requirements: [])"#,
        r#"(direction: south, target: "street_south_01", label: Some("Agentopia Blvd South"), requirements: [])"#,
    );
    replace_in_view(
        &mut views,
        "arrival_street",
        r#"(direction: west, target: "grid_road_xm1_y0", label: Some("West 1 Rd."), requirements: [])"#,
        r#"(direction: west, target: "west_main_street", label: Some("West Hinemos Blvd"), requirements: [])"#,
    );
    replace_in_view(
        &mut views,
        "arrival_street",
        r#"(direction: east, target: "grid_road_xp1_y0", label: Some("East 1 Rd."), requirements: [])"#,
        r#"(direction: east, target: "official_street", label: Some("East Hinemos Blvd"), requirements: [])"#,
    );
    replace_in_view(
        &mut views,
        "west_main_street",
        "West Hinemos Blvd is a quiet grid street. The old H1 and H2 lots are shuttered in this baseline world; farther west the boulevard thins into wilderness, beach, and sea.",
        "West Hinemos Blvd is a hand-kept official street. H1 stands on the north side and H2 on the south side; farther west the boulevard thins into wilderness, beach, and sea.",
    );
    replace_in_view(
        &mut views,
        "west_main_street",
        "H1 [shuttered]",
        "H1 [Blackstone]",
    );
    replace_in_view(
        &mut views,
        "west_main_street",
        "H2 [shuttered]",
        "H2 [Hinemos School]",
    );
    replace_in_view(
        &mut views,
        "west_main_street",
        "entities: []",
        r#"entities: ["tavern_front", "school_front"]"#,
    );
    replace_in_view(
        &mut views,
        "official_street",
        "East Hinemos Blvd is a quiet grid street. The old H3 through H6 lots are shuttered in this baseline world; farther east the boulevard fades into wilderness, beach, and sea.",
        "East Hinemos Blvd is a hand-kept official street. H3, H4, H5, and H6 stand along the civic side; farther east the boulevard fades into wilderness, beach, and sea.",
    );
    replace_in_view(
        &mut views,
        "official_street",
        "H3 [shuttered]",
        "H3 [Workers Society]",
    );
    replace_in_view(
        &mut views,
        "official_street",
        "H4 [shuttered]         H5 [shuttered]",
        "H4 [Hinemos Bank]      H5 [Daily Seer]",
    );
    replace_in_view(
        &mut views,
        "official_street",
        "H6 [shuttered]",
        "H6 [Registry Office]",
    );
    replace_in_view(
        &mut views,
        "official_street",
        "entities: []",
        r#"entities: ["workers_society_front", "bank_front", "daily_seer_front", "registry_front"]"#,
    );
    fs::write(path, views).expect("write views.ron");
}

fn replace_in_view(views: &mut String, view_id: &str, from: &str, to: &str) {
    let marker = format!("id: \"{view_id}\"");
    let view_start = views
        .find(&marker)
        .unwrap_or_else(|| panic!("missing view marker `{marker}`"));
    let relative_start = views[view_start..]
        .find(from)
        .unwrap_or_else(|| panic!("missing `{from}` in view `{view_id}`"));
    let start = view_start + relative_start;
    views.replace_range(start..start + from.len(), to);
}
