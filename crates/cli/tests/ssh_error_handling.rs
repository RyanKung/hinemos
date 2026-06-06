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
        "Blackstone izakaya front:",
        "inspect command returns object detail",
    );
    assert_eq!(
        output.matches("Island Harbor Crossing").count(),
        1,
        "non-navigation actions should not repaint the room"
    );
    assert_eq!(
        output.matches("ISLAND HARBOR CROSSING").count(),
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
fn password_auth_works_without_local_ssh_keys() {
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

    let output = run_ssh_password_batch_with_home(
        &temp,
        host,
        port,
        &user,
        "first-use-password",
        &isolated_home,
        ["/quit"],
    );
    assert_contains(
        &output,
        "First password login recorded",
        "password login works when the client has no local SSH keys",
    );
    assert_contains(
        &output,
        "Welcome to Hinemos",
        "no-local-key first contact still reaches the world",
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

    let temp = TestTempDir::new("hinemos-password-auth");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("password_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let password = "first-use-password";
    let first_output = run_ssh_password_batch(&temp, host, port, &user, password, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Hinemos",
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
        "Welcome to Hinemos",
        "subsequent password login should not repeat the welcome",
    );
    assert_contains(
        &second_output,
        "You entered by password",
        "password login should remind the user every time",
    );
    assert_contains(
        &second_output,
        "/addkey <openssh-public-key>",
        "password login should point to ed25519 key binding",
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
fn password_user_can_add_ed25519_key_and_blocks_unbound_keys() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("ssh-keygen");

    let temp = TestTempDir::new("hinemos-addkey");
    let bound_key = temp.path.join("bound_ed25519");
    let unbound_key = temp.path.join("unbound_ed25519");
    for key in [&bound_key, &unbound_key] {
        let keygen = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(key)
            .output()
            .expect("spawn ssh-keygen");
        assert!(
            keygen.status.success(),
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&keygen.stderr)
        );
    }
    let public_key = fs::read_to_string(bound_key.with_extension("pub")).expect("read public key");

    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("addkey_probe_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let password = "first-use-password";
    let addkey_command = format!("/addkey {}", public_key.trim());
    let addkey_output = run_ssh_password_batch(
        &temp,
        host,
        port,
        &user,
        password,
        [&addkey_command, "/quit"],
    );
    assert_contains(
        &addkey_output,
        "SSH ed25519 public key bound",
        "/addkey should bind the ed25519 key",
    );

    let key_output = run_ssh_key_batch(&temp, host, port, &user, &bound_key, ["/quit"]);
    assert_contains(
        &key_output,
        &format!("Authenticated as {user}"),
        "bound key should authenticate as the same username",
    );
    assert_not_contains(
        &key_output,
        "First password login recorded",
        "bound key login should not be treated as password auth",
    );

    let unbound = run_ssh_key_batch_raw(&temp, host, port, &user, &unbound_key, ["/quit"]);
    assert!(
        !unbound.status.success(),
        "unbound key must not authenticate as an existing username"
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
fn first_login_with_only_rsa_key_recommends_ed25519() {
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

    let first_output = run_ssh_key_batch(&temp, host, port, &user, &rsa_key, ["/quit"]);
    assert_contains(
        &first_output,
        "Welcome to Hinemos",
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

    let second_output = run_ssh_key_batch(&temp, host, port, &user, &rsa_key, ["/quit"]);
    assert_not_contains(
        &second_output,
        "/read board",
        "existing RSA identity should not repeat onboarding every login",
    );
    assert_not_contains(
        &second_output,
        "Welcome to Hinemos",
        "existing RSA identity should not repeat the welcome",
    );
    assert_contains(
        &second_output,
        "You entered with a",
        "existing RSA identity should repeat the key-type reminder",
    );
    assert_contains(
        &second_output,
        "/addkey <openssh-public-key>",
        "existing RSA identity should point to ed25519 key binding",
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
    ssh_output_stdout(output, user, "password ssh batch")
}

fn run_ssh_password_batch_with_home<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    home: &std::path::Path,
    commands: [&str; N],
) -> String {
    let output = run_ssh_password_batch_raw_with_home(
        temp,
        host,
        port,
        user,
        password,
        Some(home),
        commands,
    );
    ssh_output_stdout(output, user, "password ssh batch")
}

fn ssh_output_stdout(output: std::process::Output, user: &str, label: &str) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{label} failed for {user}: {}\nstdout:\n{}",
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
