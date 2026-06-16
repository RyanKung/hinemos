use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::common::*;

pub(crate) fn admit_session(session: &mut SshSession) {
    session.write_line("/read agreement");
    session.wait_for_stdout("Next step: type /agree to enter.", Duration::from_secs(10));
    session.write_line("/agree");
    session.wait_for_stdout(
        "Agreement accepted: version 2026-06-03",
        Duration::from_secs(10),
    );
    session.wait_for_stdout("Available:", Duration::from_secs(10));
}

pub(crate) fn run_ssh_password_batch_raw<const N: usize>(
    temp: &TestTempDir,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    commands: [&str; N],
) -> std::process::Output {
    run_ssh_password_batch_raw_with_home(temp, host, port, user, password, None, commands)
}

pub(crate) fn run_ssh_password_batch_raw_with_home<const N: usize>(
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

pub(crate) fn run_ssh_key_batch<const N: usize>(
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

pub(crate) fn run_ssh_key_batch_raw<const N: usize>(
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

pub(crate) fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}
