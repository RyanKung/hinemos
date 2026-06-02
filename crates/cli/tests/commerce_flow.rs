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
            "/enter north_01",
            "/build {\"title\":\"Offline Tool Broker\",\"description\":\"An operator-run shop that sells a simple greeting string.\",\"style\":\"Ledger-first counter service.\",\"prompt\":\"Parse visitor requests, create payment requests, and deliver content only after payment.\"}",
            "/land info north_01",
            "/build publish",
            "/hello",
            "/land info north_01",
            "/quit",
        ],
    );
    assert_contains(
        &owner_setup,
        "You can build here with /build",
        "claim response gives the owner a usable build command",
    );
    assert_not_contains(
        &owner_setup,
        "Go to parcel_north_01",
        "claim response does not expose internal view ids",
    );
    assert_contains(
        &owner_setup,
        "Title: Offline Tool Broker",
        "owner build title was persisted",
    );
    assert_contains(
        &owner_setup,
        "Description: An operator-run shop that sells a simple greeting string.",
        "owner build description was persisted",
    );
    assert_contains(
        &owner_setup,
        "Style: Ledger-first counter service.",
        "owner build style was persisted",
    );
    assert_contains(
        &owner_setup,
        "Prompt: Parse visitor requests, create payment requests, and deliver content only after payment.",
        "owner build prompt was persisted",
    );
    assert_contains(
        &owner_setup,
        "Commands: /hello preview=hello price=25; /status",
        "owner build custom commands were persisted",
    );
    assert_contains(
        &owner_setup,
        "Published parcel north_01",
        "owner published shop",
    );
    assert_contains(
        &owner_setup,
        "You own this shop. Visitors use /hello here",
        "owner custom command usage explains visitor flow",
    );
    assert_contains(
        &owner_setup,
        "Status: built",
        "published build status is visible in parcel detail",
    );

    let customer_visit = run_ssh_batch(
        host,
        port,
        &customer,
        ["/go north", "/enter north_01", "/hello", "/quit"],
    );
    assert_contains(
        &customer_visit,
        "Offline Tool Broker",
        "customer sees the edited shop title",
    );
    assert_contains(
        &customer_visit,
        "[Offline Tool Broker]",
        "customer sees the shop title on the street sign",
    );
    assert_contains(
        &customer_visit,
        "An operator-run shop that sells a simple greeting string.",
        "customer sees the edited shop description",
    );
    assert_contains(
        &customer_visit,
        "Style: Ledger-first counter service.",
        "customer sees the edited shop style",
    );
    assert_contains(
        &customer_visit,
        "Shop commands: /hello - hello, price 25; /status",
        "customer sees readable shop commands",
    );
    assert_contains(
        &customer_visit,
        "local: /hello, /status",
        "customer sees shop commands in Available",
    );
    assert_contains(
        &customer_visit,
        "Operator prompt: Parse visitor requests, create payment requests, and deliver content only after payment.",
        "customer sees the edited operator prompt",
    );
    assert_contains(
        &customer_visit,
        "Shop request",
        "customer raw command forwarded to offline owner",
    );
    assert_contains(
        &customer_visit,
        "queued",
        "offline owner command was queued",
    );
    assert_contains(
        &customer_visit,
        "Preview: hello",
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
