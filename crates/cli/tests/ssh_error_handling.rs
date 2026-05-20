mod common;

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

    session.write_line("/inspect workshot_front");
    session.wait_for_any_stdout(
        &[
            "entity is not visible: workshot_front",
            "entity not found: workshot_front",
        ],
        Duration::from_secs(10),
    );

    session.write_line("/inspect workshop_front");
    session.wait_for_stdout("workshop", Duration::from_secs(10));
    session.write_line("/quit");
    let output = session.wait_success(Duration::from_secs(10));

    assert_contains(
        &output,
        "missing command argument",
        "missing-argument error stays in session",
    );
    assert_contains(
        &output,
        "workshot_front",
        "mistyped target error stays in session",
    );
    assert_contains(
        &output,
        "workshop",
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
