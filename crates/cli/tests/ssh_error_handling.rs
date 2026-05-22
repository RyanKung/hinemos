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
fn command_errors_do_not_close_ssh_session() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-ssh-error-handling");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("typo_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session = SshSession::spawn(host, port, &user);
    session.wait_for_stdout("Available:", Duration::from_secs(10));

    session.write_line("/inspect");
    session.wait_for_stdout("missing command argument", Duration::from_secs(10));

    session.write_line("/inspect taverb_front");
    session.wait_for_any_stdout(
        &[
            "entity is not visible: taverb_front",
            "entity not found: taverb_front",
        ],
        Duration::from_secs(10),
    );

    session.write_line("/inspect tavern_front");
    session.wait_for_stdout("tavern", Duration::from_secs(10));
    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));

    assert_contains(
        &output,
        "missing command argument",
        "missing-argument error stays in session",
    );
    assert_contains(
        &output,
        "taverb_front",
        "mistyped target error stays in session",
    );
    assert_contains(
        &output,
        "tavern",
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

    let temp = TestTempDir::new("xagora-business-error-handling");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("business_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let mut session = SshSession::spawn(host, port, &user);
    session.wait_for_stdout("Available:", Duration::from_secs(10));

    session.write_line("/pay nobody 1 typo-safe");
    session.wait_for_stdout("payment target not found: nobody", Duration::from_secs(10));
    session.write_line("/look");
    session.wait_for_stdout("Town Crossroads", Duration::from_secs(10));

    session.write_line("/pay accept 999999");
    session.wait_for_stdout("payment request not found: 999999", Duration::from_secs(10));
    session.write_line("/look");
    session.wait_for_stdout("Town Crossroads", Duration::from_secs(10));

    session.write_line("/land info missing_parcel");
    session.wait_for_stdout("parcel not found: missing_parcel", Duration::from_secs(10));
    session.write_line("/look");
    session.wait_for_stdout("Town Crossroads", Duration::from_secs(10));

    session.write_line("/build title Should not disconnect");
    session.wait_for_stdout("you do not own this parcel", Duration::from_secs(10));
    session.write_line("/look");
    session.wait_for_stdout("Town Crossroads", Duration::from_secs(10));

    session.write_line("/shop request-payment 999999 1 hello");
    session.wait_for_stdout("shop command not found: 999999", Duration::from_secs(10));
    session.write_line("/look");
    session.wait_for_stdout("Town Crossroads", Duration::from_secs(10));

    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));
    assert_contains(
        &output,
        "payment target not found: nobody",
        "unknown payment target error",
    );
    assert_contains(
        &output,
        "payment request not found: 999999",
        "unknown payment request error",
    );
    assert_contains(
        &output,
        "parcel not found: missing_parcel",
        "unknown parcel error",
    );
    assert_contains(
        &output,
        "you do not own this parcel",
        "build ownership error",
    );
    assert_contains(
        &output,
        "shop command not found: 999999",
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

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn read_and_inspect_return_results_without_repainting_room() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-ssh-compact-actions");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("compact_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        ["/read cyber_scroll_board", "/inspect tavern_front", "/quit"],
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
        "Password login is available for first contact",
        "arrival skill explains password login as a first-contact path",
    );
    assert_contains(
        &output,
        "future cryptographic compatibility",
        "arrival skill explains why ed25519 is recommended",
    );
    assert_contains(
        &output,
        "Blackstone tavern front:",
        "inspect command returns object detail",
    );
    assert_eq!(
        output.matches("Town Crossroads").count(),
        1,
        "non-navigation actions should not repaint the room"
    );
    assert_eq!(
        output.matches("TOWN CROSSROADS").count(),
        0,
        "compact room rendering should not include the banner title"
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

    let temp = TestTempDir::new("xagora-help-formatting");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("help_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
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
fn password_auth_records_first_password_and_reuses_identity() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-password-auth");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("password_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let password = "first-use-password";
    let first_output = run_ssh_password_batch(&temp, host, port, &user, password, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Xagora",
        "first password login shows the welcome",
    );
    assert_contains(
        &first_output,
        "/read board",
        "first password login points the user to the board",
    );
    assert_contains(
        &first_output,
        "First password login recorded",
        "first password login creates the stored password identity",
    );
    assert_contains(
        &first_output,
        "/read board",
        "password onboarding points to the board",
    );
    assert_not_contains(
        &first_output,
        "ssh-keygen -t ed25519",
        "password onboarding should not inline key registration commands",
    );

    let second_output = run_ssh_password_batch(&temp, host, port, &user, password, ["/quit"]);
    assert_not_contains(
        &second_output,
        "First password login recorded",
        "subsequent password login should not repeat first-login onboarding",
    );
    assert_not_contains(
        &second_output,
        "Welcome to Xagora",
        "subsequent password login should not repeat the welcome",
    );

    let failed = run_ssh_password_batch_raw(&temp, host, port, &user, "wrong-password", ["/quit"]);
    assert!(
        !failed.status.success(),
        "wrong remembered password should not authenticate"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres, SSH client, and ssh-keygen"]
fn first_ed25519_key_login_gets_welcome_without_key_warning() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("xagora-ed25519-onboarding");
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
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let first_output = run_ssh_key_batch(host, port, &user, &ed25519_key, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Xagora",
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

    let second_output = run_ssh_key_batch(host, port, &user, &ed25519_key, ["/quit"]);
    assert_not_contains(
        &second_output,
        "Welcome to Xagora",
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
fn first_non_ed25519_key_login_recommends_ed25519() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("xagora-rsa-onboarding");
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
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let first_output = run_ssh_key_batch(host, port, &user, &rsa_key, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Xagora",
        "first RSA login shows the welcome",
    );
    assert_contains(
        &first_output,
        "/read board",
        "first RSA login points the user to the board",
    );
    assert_contains(
        &first_output,
        "You logged in with a",
        "first RSA login should explain the key type",
    );
    assert_contains(
        &first_output,
        "/read board",
        "first RSA login should point to the board",
    );
    assert_not_contains(
        &first_output,
        "ssh-keygen -t ed25519",
        "first RSA login should not inline key registration commands",
    );

    let second_output = run_ssh_key_batch(host, port, &user, &rsa_key, ["/quit"]);
    assert_not_contains(
        &second_output,
        "/read board",
        "existing RSA identity should not repeat onboarding every login",
    );
    assert_not_contains(
        &second_output,
        "Welcome to Xagora",
        "existing RSA identity should not repeat the welcome",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

fn run_ssh_password_batch<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    commands: [&str; N],
) -> String {
    let output = run_ssh_password_batch_raw(temp, host, port, user, password, commands);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "password ssh batch failed for {user}: {}\nstdout:\n{}",
        stderr,
        stdout
    );
    stdout.into_owned()
}

fn run_ssh_password_batch_raw<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    commands: [&str; N],
) -> std::process::Output {
    let askpass = temp.path.join("askpass.sh");
    fs::write(
        &askpass,
        "#!/bin/sh\nprintf '%s\\n' \"$XAGORA_TEST_SSH_PASSWORD\"\n",
    )
    .expect("write askpass helper");
    #[cfg(unix)]
    fs::set_permissions(&askpass, fs::Permissions::from_mode(0o700)).expect("chmod askpass helper");

    let input = format!("{}\n", commands.join("\n"));
    let mut child = Command::new("ssh")
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
        .env("DISPLAY", "xagora-test")
        .env("XAGORA_TEST_SSH_PASSWORD", password)
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
    host: &str,
    port: u16,
    user: &str,
    key_path: &std::path::Path,
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
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "PasswordAuthentication=no",
            "-o",
            "IdentitiesOnly=yes",
            "-i",
        ])
        .arg(key_path)
        .args(["-p", &port.to_string(), &format!("{user}@{host}")])
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
    let output = wait_with_timeout(child, Duration::from_secs(30));
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
