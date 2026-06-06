mod common;

use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use common::*;

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn pending_admission_blocks_world_until_board_agreement() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");
    assert_command_exists("psql");

    let temp = TestTempDir::new("hinemos-admission-gate");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("pending_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");
    let client_key = temp.path.join("client_ed25519");

    let keygen = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&client_key)
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let input = [
        "/pay nobody 1 before-admission",
        "/agree",
        "/read",
        "/agree",
        "/balance",
        "/quit",
    ]
    .join("\n")
        + "\n";
    let output = Command::new("ssh")
        .args([
            "-T",
            "-i",
            client_key.to_str().expect("key path utf8"),
            "-o",
            "IdentitiesOnly=yes",
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
        .and_then(|mut child| {
            child
                .stdin
                .as_mut()
                .expect("open ssh stdin")
                .write_all(input.as_bytes())?;
            Ok(wait_with_timeout(child, Duration::from_secs(45)))
        })
        .expect("run ssh admission batch");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "admission ssh batch failed\nstderr:\n{stderr}\nstdout:\n{stdout}"
    );

    assert_contains(&stdout, "Admission pending", "new users start pending");
    assert_contains(
        &stdout,
        "Read the board agreement first: /read agreement",
        "pending users are told to read agreement",
    );
    assert_not_contains(
        &stdout,
        "admission: /agree",
        "agree is not advertised as a room action before reading",
    );
    assert_not_contains(
        &stdout,
        "Type /agree to enter. Until then",
        "agree guidance belongs to the read result, not the room description",
    );
    assert_not_contains(
        &stdout,
        "payment target not found",
        "pending payment command is blocked before world handling",
    );
    assert_contains(
        &stdout,
        "Agreement version: 2026-06-03",
        "agreement version is visible on board",
    );
    assert_contains(
        &stdout,
        "Next step: type /agree to enter.",
        "agreement read gives a clear next step",
    );
    assert_contains(
        &stdout,
        "Agreement accepted: version 2026-06-03",
        "bare agree admits player after reading agreement",
    );
    assert_contains(
        &stdout,
        "Initial grant issued: 1000 MARK",
        "initial MARK is issued after agreement",
    );
    assert_contains(
        &stdout,
        "Balance: 1000 MARK",
        "wallet is usable after admission",
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select admission_state || ':' || agreement_version from player_profiles where display_name = '{}'",
            sql_literal(&user)
        )),
        "agreed:2026-06-03",
        "profile records accepted agreement version"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn command_errors_do_not_close_ssh_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-ssh-error-handling");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("typo_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session = SshSession::spawn(host, port, &user);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    admit_session(&mut session);

    session.write_line("/go north");
    session.wait_for_stdout("North Island Market Path 01", Duration::from_secs(10));
    session.write_line("/read");
    session.wait_for_stdout("What do you want to read?", Duration::from_secs(10));

    session.write_line("/inspect");
    session.wait_for_stdout("What do you want to inspect?", Duration::from_secs(10));

    session.write_line("/inspect taverb_front");
    session.wait_for_any_stdout(
        &[
            "You do not see taverb_front here.",
            "The world has no visible record named taverb_front.",
        ],
        Duration::from_secs(10),
    );

    session.write_line("/inspect tavern_front");
    session.wait_for_stdout("izakaya", Duration::from_secs(10));
    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));

    assert_contains(
        &output,
        "What do you want to inspect?",
        "friendly missing-argument error stays in session",
    );
    assert_contains(
        &output,
        "taverb_front",
        "mistyped target error stays in session",
    );
    assert_contains(
        &output,
        "izakaya",
        "valid command after mistyped target still runs",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn business_command_errors_do_not_close_ssh_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-business-error-handling");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("business_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session = SshSession::spawn(host, port, &user);
    session.wait_for_stdout("Available:", Duration::from_secs(10));
    admit_session(&mut session);

    session.write_line("/pay nobody 1 typo-safe");
    session.wait_for_stdout(
        "No player named nobody can be found for payment.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Island Harbor Crossing", Duration::from_secs(10));

    session.write_line("/pay accept 999999");
    session.wait_for_stdout(
        "No payment request #999999 is open on the ledger.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Island Harbor Crossing", Duration::from_secs(10));

    session.write_line("/land info missing_parcel");
    session.wait_for_stdout(
        "The Guild has no parcel record named missing_parcel.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Island Harbor Crossing", Duration::from_secs(10));

    session.write_line("/build title Should not disconnect");
    session.wait_for_stdout(
        "The Guild will not accept that build sheet here; you do not own this parcel.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Island Harbor Crossing", Duration::from_secs(10));

    session.write_line("/shop request-payment 999999 1 hello");
    session.wait_for_stdout(
        "No shop notice #999999 is waiting here.",
        Duration::from_secs(10),
    );
    session.write_line("/look");
    session.wait_for_stdout("Island Harbor Crossing", Duration::from_secs(10));

    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));
    assert_contains(
        &output,
        "No player named nobody can be found for payment.",
        "unknown payment target error",
    );
    assert_contains(
        &output,
        "No payment request #999999 is open on the ledger.",
        "unknown payment request error",
    );
    assert_contains(
        &output,
        "The Guild has no parcel record named missing_parcel.",
        "unknown parcel error",
    );
    assert_contains(
        &output,
        "The Guild will not accept that build sheet here; you do not own this parcel.",
        "build ownership error",
    );
    assert_contains(
        &output,
        "No shop notice #999999 is waiting here.",
        "unknown shop command error",
    );
    assert_contains(
        &output,
        "Goodbye.",
        "session remains usable through quit after business errors",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

fn admit_session(session: &mut SshSession) {
    session.write_line("/read");
    session.wait_for_stdout("Agreement version: 2026-06-03", Duration::from_secs(10));
    session.write_line("/agree");
    session.wait_for_stdout(
        "Agreement accepted: version 2026-06-03",
        Duration::from_secs(10),
    );
    session.wait_for_stdout("Available:", Duration::from_secs(10));
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn read_and_inspect_return_results_without_repainting_room() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-ssh-compact-actions");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("compact_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/read cyber_scroll_board",
            "/agree",
            "/inspect tavern_front",
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "Arrival Skill",
        "read command returns board contents",
    );
    assert_contains(
        &output,
        "ssh-keygen -t ed25519",
        "arrival skill carries the recommended key registration command",
    );
    assert_contains(
        &output,
        "Recommended setup: run /settings mail-token",
        "arrival skill explains the recommended key setup path",
    );
    assert_contains(
        &output,
        "Agent integration: connect to IMAP as this username",
        "arrival skill explains the agent mail path",
    );
    assert_contains(
        &output,
        "Blackstone izakaya front:",
        "inspect command returns object detail",
    );
    let inspect_output = output
        .rsplit_once("> /inspect tavern_front")
        .map(|(_, tail)| tail)
        .unwrap_or(&output);
    assert_not_contains(
        inspect_output,
        "Island Harbor Crossing",
        "inspect output should not repaint the room",
    );
    assert_not_contains(
        inspect_output,
        "ISLAND HARBOR CROSSING",
        "inspect output should not render the banner title",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn help_output_is_grouped_across_lines() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-help-formatting");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("help_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(host, port, &user, ["/help", "/quit"]);

    assert_contains(&output, "Commands:", "help has a heading");
    assert_contains(&output, "Movement:", "help groups movement commands");
    assert_contains(&output, "Mail and news:", "help groups message commands");
    assert_contains(&output, "Wallet:", "help groups wallet commands");
    assert_contains(
        &output,
        "Local extensions",
        "help explains extension commands",
    );
    assert_not_contains(
        &output,
        "Commands: /look /map",
        "help should not render as one long command line",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn password_auth_is_rejected_for_new_users() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-no-local-key-password-auth");
    let isolated_home = temp.path.join("empty-home");
    fs::create_dir_all(isolated_home.join(".ssh")).expect("create isolated ssh home");

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("no_key_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_password_batch_raw_with_home(
        &temp,
        host,
        port,
        &user,
        "first-use-password",
        Some(&isolated_home),
        ["/quit"],
    );
    assert!(
        !output.status.success(),
        "password login should be rejected for new users"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}\n{stdout}");
    assert_contains(
        &combined,
        "Welcome to hinemos.ai.",
        "password rejection should still show the banner",
    );
    assert_contains(
        &combined,
        "Only ed25519 SSH keys are accepted.",
        "password rejection should explain the new login policy",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn authentication_banner_explains_ed25519_only_login() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-auth-banner");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("banner_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output =
        run_ssh_password_batch_raw(&temp, host, port, &user, "first-use-password", ["/quit"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}\n{stdout}");
    assert_contains(
        &combined,
        "Welcome to hinemos.ai.",
        "authentication banner should be shown before login completes",
    );
    assert_contains(
        &combined,
        "Hinemos is a persistent SSH world for humans and agents.",
        "banner should include a short project description",
    );
    assert_contains(
        &combined,
        "Only ed25519 SSH keys are accepted.",
        "banner should clearly state the login policy",
    );
    assert_contains(
        &combined,
        "ssh-keygen -t ed25519 -C \"<user>@hinemos\"",
        "banner should explain how to create an ed25519 key",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn ed25519_auth_only_login_works() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("hinemos-ed25519-auth");
    let host = "127.0.0.1";
    let port = free_local_port();
    let key_path = temp.path.join("id_ed25519");
    let keygen = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key_path)
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );
    let user = format!("ed25519_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let first_output = run_ssh_key_batch(&temp, host, port, &user, &key_path, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Hinemos",
        "first ed25519 login shows the welcome",
    );
    assert_contains(
        &first_output,
        "/read board",
        "first ed25519 login points the user to the board",
    );
    assert_contains(
        &first_output,
        "shared by agents and humans",
        "ed25519 login should reach the world",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres, SSH client, and ssh-keygen"]
fn first_login_with_only_ed25519_key_gets_welcome_without_key_warning() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("hinemos-ed25519-onboarding");
    let ed25519_key = temp.path.join("id_ed25519");
    let keygen = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&ed25519_key)
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("ed25519_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let first_output = run_ssh_key_batch(&temp, host, port, &user, &ed25519_key, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Hinemos",
        "first ed25519 login shows the welcome",
    );
    assert_contains(
        &first_output,
        "shared by agents and humans",
        "welcome describes the mixed world",
    );
    assert_contains(
        &first_output,
        "/read board",
        "first ed25519 login points the user to the board",
    );
    assert_not_contains(
        &first_output,
        "You logged in with a",
        "ed25519 login should not warn about the key type",
    );
    assert_not_contains(
        &first_output,
        "ssh-keygen -t ed25519",
        "ed25519 login should not inline key registration commands",
    );

    let second_output = run_ssh_key_batch(&temp, host, port, &user, &ed25519_key, ["/quit"]);
    assert_not_contains(
        &second_output,
        "Welcome to Hinemos",
        "existing ed25519 identity should not repeat the welcome",
    );
    assert_not_contains(
        &second_output,
        "/read board",
        "existing ed25519 identity should not repeat board onboarding",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres, SSH client, and ssh-keygen"]
fn rsa_key_is_rejected_before_login() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("hinemos-rsa-onboarding");
    let rsa_key = temp.path.join("id_rsa");
    let keygen = Command::new("ssh-keygen")
        .args(["-q", "-t", "rsa", "-N", "", "-f"])
        .arg(&rsa_key)
        .output()
        .expect("spawn ssh-keygen");
    assert!(
        keygen.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&keygen.stderr)
    );

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("rsa_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_key_batch_raw(&temp, host, port, &user, &rsa_key, ["/quit"]);
    assert!(
        !output.status.success(),
        "RSA login should be rejected before a shell opens:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_contains(
        &stderr,
        "Welcome to hinemos.ai.",
        "RSA rejection should still show the login banner",
    );
    assert_contains(
        &stderr,
        "RSA keys are not accepted.",
        "RSA rejection should explain the reason",
    );
    assert_not_contains(
        &String::from_utf8_lossy(&output.stdout),
        "Welcome to Hinemos",
        "RSA rejection should not reach the world shell",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

fn run_ssh_password_batch_raw<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    commands: [&str; N],
) -> std::process::Output {
    run_ssh_password_batch_raw_with_home(temp, host, port, user, password, None, commands)
}

fn run_ssh_password_batch_raw_with_home<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    home: Option<&std::path::Path>,
    commands: [&str; N],
) -> std::process::Output {
    let askpass = temp.path.join("askpass.sh");
    fs::write(
        &askpass,
        "#!/bin/sh\nprintf '%s\\n' \"$HINEMOS_TEST_SSH_PASSWORD\"\n",
    )
    .expect("write askpass helper");
    #[cfg(unix)]
    fs::set_permissions(&askpass, fs::Permissions::from_mode(0o700)).expect("chmod askpass helper");

    let input = format!("{}\n", commands.join("\n"));
    let mut command = Command::new("ssh");
    command
        .args([
            "-T",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=/dev/null",
            "-o",
            "PreferredAuthentications=password",
            "-o",
            "PubkeyAuthentication=no",
            "-o",
            "NumberOfPasswordPrompts=1",
            "-p",
            &port.to_string(),
            &format!("{user}@{host}"),
        ])
        .env("SSH_ASKPASS", &askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("DISPLAY", "hinemos-test")
        .env("HINEMOS_TEST_SSH_PASSWORD", password);
    if let Some(home) = home {
        command.env("HOME", home);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn password ssh batch");
    child
        .stdin
        .as_mut()
        .expect("open ssh stdin")
        .write_all(input.as_bytes())
        .expect("write ssh commands");
    drop(child.stdin.take());
    wait_with_timeout(child, Duration::from_secs(30))
}

fn run_ssh_key_batch<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    key_path: &std::path::Path,
    commands: [&str; N],
) -> String {
    let output = run_ssh_key_batch_raw(temp, host, port, user, key_path, commands);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "key ssh batch failed for {user}: {}\nstdout:\n{}",
        stderr,
        stdout
    );
    stdout.into_owned()
}

fn run_ssh_key_batch_raw<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    key_path: &std::path::Path,
    commands: [&str; N],
) -> std::process::Output {
    let isolated_home = temp.path.join(format!("key-home-{user}"));
    fs::create_dir_all(isolated_home.join(".ssh")).expect("create isolated key ssh home");
    let known_hosts = isolated_home.join(".ssh/known_hosts");
    let known_hosts_option = format!(
        "UserKnownHostsFile={}",
        known_hosts
            .to_str()
            .expect("known_hosts path is valid utf-8")
    );
    let input = format!("{}\n", commands.join("\n"));
    let mut child = Command::new("ssh")
        .args([
            "-T",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            &known_hosts_option,
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "PasswordAuthentication=no",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "IdentityAgent=none",
            "-i",
        ])
        .arg(key_path)
        .args(["-p", &port.to_string(), &format!("{user}@{host}")])
        .env("HOME", &isolated_home)
        .env_remove("SSH_AUTH_SOCK")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn key ssh batch");
    child
        .stdin
        .as_mut()
        .expect("open ssh stdin")
        .write_all(input.as_bytes())
        .expect("write ssh commands");
    drop(child.stdin.take());
    wait_with_timeout(child, Duration::from_secs(30))
}

fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}
