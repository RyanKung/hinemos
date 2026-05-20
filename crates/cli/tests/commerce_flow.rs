mod common;

use common::*;

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn two_ssh_agents_can_trade_with_offline_shop_owner() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("xagora-two-agent-trade");
    let host = "127.0.0.1";
    let port = free_local_port();
    let owner = format!("owner_{}_{}", std::process::id(), epoch_seconds());
    let customer = format!("customer_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("xagora-server.log");

    let mut server = spawn_xagora_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);

    let owner_setup = run_ssh_batch(
        host,
        port,
        &owner,
        [
            "/land claim north_01",
            "/go north",
            "/build title Offline Tool Broker",
            "/build description An operator-run shop that sells a simple greeting string.",
            "/build style Ledger-first counter service.",
            "/build prompt Parse visitor requests, create payment requests, and deliver content only after payment.",
            "/build commands /hello preview=hello price=25; /status",
            "/build publish",
            "/quit",
        ],
    );
    assert_contains(
        &owner_setup,
        "Published parcel north_01",
        "owner published shop",
    );

    let customer_visit = run_ssh_batch(host, port, &customer, ["/go north", "/hello", "/quit"]);
    assert_contains(
        &customer_visit,
        "Sent shop command",
        "customer raw command forwarded to offline owner",
    );
    assert_contains(
        &customer_visit,
        "queued",
        "offline owner command was queued",
    );
    assert_contains(
        &customer_visit,
        "Trial: hello",
        "customer received trial content",
    );
    assert_not_contains(
        &customer_visit,
        "hello world",
        "customer did not receive paid content before payment",
    );

    let owner_request = run_ssh_batch(
        host,
        port,
        &owner,
        [
            "/shop inbox",
            "/shop request-payment 1 25 hello world",
            "/quit",
        ],
    );
    assert_contains(
        &owner_request,
        "/hello",
        "owner sees visitor custom command",
    );
    assert_contains(
        &owner_request,
        "Created payment request",
        "owner created a payment request",
    );
    let request_id = parse_hash_id(&owner_request, "Created payment request #");
    let accept_request = format!("/pay accept {request_id}");

    let customer_payment = run_ssh_batch(
        host,
        port,
        &customer,
        ["/pay requests", &accept_request, "/balance", "/quit"],
    );
    assert_contains(
        &customer_payment,
        "Payment Request",
        "customer sees explicit payment request popup",
    );
    assert_contains(
        &customer_payment,
        "Delivery: locked until payment",
        "payment request states delivery is locked",
    );
    assert_contains(
        &customer_payment,
        "Accept: /pay accept",
        "payment request states the confirmation command",
    );
    assert_contains(
        &customer_payment,
        "Paid payment request",
        "customer accepted the payment request",
    );
    assert_contains(
        &customer_payment,
        "Unlocked content: hello world",
        "customer received paid content after payment",
    );
    assert_contains(
        &customer_payment,
        "Balance: 975 MARK",
        "customer balance updated",
    );

    let owner_reconnect = run_ssh_batch(host, port, &owner, ["/shop inbox", "/balance", "/quit"]);
    assert_contains(
        &owner_reconnect,
        "/hello",
        "owner sees offline shop command",
    );
    assert_contains(
        &owner_reconnect,
        "handled",
        "shop command is marked handled after payment request creation",
    );
    assert_contains(
        &owner_reconnect,
        "Balance: 1025 MARK",
        "owner received payment",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}
