use std::fs;
use std::process::Command;

use crate::common::*;
use crate::ssh_error_support::*;

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
            "/inspect cyber_scroll_board",
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
    assert_contains(&output, "front:", "inspect command returns object detail");
    let inspect_output = output
        .rsplit_once("> /inspect cyber_scroll_board")
        .map(|(_, tail)| tail)
        .unwrap_or(&output);
    assert_not_contains(
        inspect_output,
        "Harbor Square",
        "inspect output should not repaint the room",
    );
    assert_not_contains(
        inspect_output,
        "HARBOR SQUARE",
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
