mod common;

use std::path::Path;

use common::*;

#[derive(Clone, Copy)]
struct SshAgent<'a> {
    user: &'a str,
    key: &'a Path,
}

#[test]
fn two_ssh_agents_can_trade_with_offline_shop_owner() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-two-agent-trade");
    let host = "127.0.0.1";
    let port = free_local_port();
    let owner = format!("owner_{}_{}", std::process::id(), epoch_seconds());
    let customer = format!("customer_{}_{}", std::process::id(), epoch_seconds());
    let peer = format!("peer_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server(&root, host, port, &server_log, &test_database.url);
    wait_for_server(host, port, &mut server, &server_log);
    let owner_key = admitted_key(&temp, host, port, &owner);
    let customer_key = admitted_key(&temp, host, port, &customer);
    let peer_key = admitted_key(&temp, host, port, &peer);

    assert_owner_shop_setup(host, port, &owner, &owner_key);
    assert_owner_mailing_list_setup(host, port, &owner, &owner_key);
    assert_customer_shop_visit(host, port, &customer, &customer_key);
    assert_shop_mailing_list_flow(
        &test_database,
        host,
        port,
        SshAgent {
            user: &owner,
            key: &owner_key,
        },
        SshAgent {
            user: &customer,
            key: &customer_key,
        },
        SshAgent {
            user: &peer,
            key: &peer_key,
        },
    );
    assert_shop_badge_flow(host, port, &owner, &owner_key, &customer, &customer_key);
    assert_shop_mailbox_converged(&test_database);
    let request_id = request_shop_payment(host, port, &owner, &owner_key);
    assert_customer_paid_request(host, port, &customer, &customer_key, request_id);
    assert_owner_received_payment(host, port, &owner, &owner_key);

    terminate(&mut server);
    temp.remove_on_drop();
}

fn assert_owner_mailing_list_setup(host: &str, port: u16, owner: &str, owner_key: &Path) {
    let owner_setup = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &[
            "/shop mailing-list create C0-N1-01 updates Shop Updates",
            "/shop mailing-list list C0-N1-01",
            "/quit",
        ],
    );
    assert_contains(
        &owner_setup,
        "Created shop chat updates for parcel C0-N1-01: Shop Updates.",
        "owner can create a shop chat",
    );
    assert_contains(
        &owner_setup,
        "Post: /chat C0-N1-01 updates -- <message>",
        "create response gives chat command",
    );
    assert_contains(
        &owner_setup,
        "Shop Chats for C0-N1-01",
        "owner can list shop chats",
    );
}

fn assert_shop_mailbox_converged(test_database: &TestDatabase) {
    let shop_command_count = test_database.query_value(
        "select count(*) from inbox_items where recipient_user = 'room-C0-N1-01' and kind = 'shop_command' and source_kind = 'operator_command' and body = '/hello'",
    );
    assert_eq!(
        shop_command_count, "1",
        "visitor shop command should be stored once as the shop actionable item"
    );

    let duplicate_mail_count = test_database.query_value(
        "select count(*) from inbox_items where recipient_user = 'room-C0-N1-01' and kind = 'mail' and body = '/hello'",
    );
    assert_eq!(
        duplicate_mail_count, "0",
        "visitor shop command should not also be stored as generic mail"
    );

    let world_mail_count = test_database.query_value(
        "select count(*) from world_messages where target_user = 'room-C0-N1-01' and kind = 'mail' and body = '/hello'",
    );
    assert_eq!(
        world_mail_count, "0",
        "visitor shop command should not create an extra generic mail history row"
    );
}

fn assert_owner_shop_setup(host: &str, port: u16, owner: &str, owner_key: &Path) {
    let owner_setup = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &[
            "/land claim C0-N1-01",
            "/go north",
            "/enter C0-N1-01",
            "/build {\"title\":\"Offline Tool Broker\",\"description\":\"An operator-run shop that sells a simple greeting string.\",\"style\":\"Ledger-first counter service.\",\"prompt\":\"Parse visitor requests, create payment requests, and deliver content only after payment.\"}",
            "/land info C0-N1-01",
            "/build publish",
            "/hello",
            "/land info C0-N1-01",
            "/quit",
        ],
    );
    assert_contains(
        &owner_setup,
        "Build here with /build",
        "claim response gives the owner a usable build command",
    );
    assert_not_contains(
        &owner_setup,
        "Go to parcel_C0-N1-01",
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
        "Published parcel C0-N1-01",
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
}

fn assert_customer_shop_visit(host: &str, port: u16, customer: &str, customer_key: &Path) {
    let customer_visit = run_ssh_batch_with_key(
        host,
        port,
        customer,
        customer_key,
        &["/go north", "/enter C0-N1-01", "/hello", "/quit"],
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
        "Style: Ledger-first counter",
        "customer sees the edited shop style",
    );
    assert_contains(
        &customer_visit,
        "service.",
        "customer sees the edited shop style terminator",
    );
    assert_contains(
        &customer_visit,
        "Shop commands: /hello - hello, price 25; /status",
        "customer sees readable shop commands",
    );
    assert_contains(
        &customer_visit,
        "Mailing lists: Shop Updates (updates) join: /subscribe C0-N1-01 updates",
        "customer sees the shop chat join command",
    );
    assert_contains(
        &customer_visit,
        "joining: /chat C0-N1-01 updates -- <message>",
        "customer sees the shop chat post command",
    );
    assert_contains(
        &customer_visit,
        "subscriptions: /subscribe C0-N1-01 updates",
        "customer sees subscription command in Available",
    );
    assert_contains(
        &customer_visit,
        "local: /hello preview=hello price=25, /status",
        "customer sees shop commands in Available",
    );
    assert_contains(
        &customer_visit,
        "badges: /badges",
        "customer sees badge lookup command in Available",
    );
    assert_contains(
        &customer_visit,
        "Operator prompt: Parse visitor requests",
        "customer sees the edited operator prompt",
    );
    assert_contains(
        &customer_visit,
        "content only after payment.",
        "customer sees the full edited operator prompt",
    );
    assert_contains(
        &customer_visit,
        "Shop request",
        "customer raw command forwarded to offline owner",
    );
    assert_contains(
        &customer_visit,
        "Status: delivered.",
        "offline owner command was delivered to the shop inbox",
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
}

fn assert_shop_mailing_list_flow(
    test_database: &TestDatabase,
    host: &str,
    port: u16,
    owner: SshAgent<'_>,
    customer: SshAgent<'_>,
    peer: SshAgent<'_>,
) {
    let customer_subscribe = run_ssh_batch_with_key(
        host,
        port,
        customer.user,
        customer.key,
        &["/subscribe C0-N1-01 updates", "/subscriptions", "/quit"],
    );
    assert_contains(
        &customer_subscribe,
        "Joined shop chat Shop Updates (updates) at C0-N1-01.",
        "customer can subscribe to the shop mailing list",
    );
    assert_contains(
        &customer_subscribe,
        "Post: /chat C0-N1-01 updates -- <message>",
        "subscription response gives chat command",
    );

    let peer_subscribe = run_ssh_batch_with_key(
        host,
        port,
        peer.user,
        peer.key,
        &["/subscribe C0-N1-01 updates", "/subscriptions", "/quit"],
    );
    assert_contains(
        &peer_subscribe,
        "Joined shop chat Shop Updates (updates) at C0-N1-01.",
        "peer can join the same shop chat",
    );
    assert_contains(
        &peer_subscribe,
        "Shop Chat Memberships",
        "subscription list uses group-chat language",
    );

    let owner_send = run_ssh_batch_with_key(
        host,
        port,
        owner.user,
        owner.key,
        &[
            "/shop mailing-list subscribers C0-N1-01 updates",
            "/shop mailing-list send C0-N1-01 updates Weekly Deal -- Subscribers get first notice.",
            "/quit",
        ],
    );
    assert_contains(
        &owner_send,
        "Shop Chat Members for C0-N1-01 updates: 2 active",
        "owner can inspect active subscriber count",
    );
    assert_contains(
        &owner_send,
        "Sent shop chat post",
        "owner can send a mailing-list post",
    );
    assert_contains(
        &owner_send,
        "to 2 member(s): Weekly Deal",
        "send response reports recipient count",
    );

    let customer_mailbox = run_ssh_batch_with_key(
        host,
        port,
        customer.user,
        customer.key,
        &["/mailbox", "/quit"],
    );
    assert_contains(
        &customer_mailbox,
        "Weekly Deal",
        "subscriber receives mailing-list post through mailbox",
    );
    assert_contains(
        &customer_mailbox,
        "shop_mailing_list_post",
        "mailbox item keeps mailing-list source metadata",
    );
    assert_contains(
        &test_database.query_value(&format!(
            "select count(*)
             from inbox_items
             where recipient_user = '{}'
               and subject = 'Weekly Deal'
               and body like '%Reply: /chat C0-N1-01 updates -- <message>%'",
            customer.user
        )),
        "1",
        "mailing-list delivery carries group-chat reply command",
    );

    let customer_chat = run_ssh_batch_with_key(
        host,
        port,
        customer.user,
        customer.key,
        &[
            "/chat C0-N1-01 updates -- I can vouch for this shop.",
            "/quit",
        ],
    );
    assert_contains(
        &customer_chat,
        "Posted shop chat message",
        "subscriber can post to the shop chat",
    );

    let peer_mailbox =
        run_ssh_batch_with_key(host, port, peer.user, peer.key, &["/mailbox", "/quit"]);
    assert_contains(
        &peer_mailbox,
        "Shop chat: updates",
        "peer sees another member's shop chat subject",
    );
    assert_contains(
        &peer_mailbox,
        "from customer_",
        "shop chat message preserves the member sender",
    );
    assert_contains(
        &test_database.query_value(&format!(
            "select count(*)
             from inbox_items
             where recipient_user = '{}'
               and sender_user = '{}'
               and subject = 'Shop chat: updates'
               and body like '%I can vouch for this shop.%'",
            peer.user, customer.user
        )),
        "1",
        "peer receives another member's shop chat message",
    );

    let customer_unsubscribe = run_ssh_batch_with_key(
        host,
        port,
        customer.user,
        customer.key,
        &["/unsubscribe C0-N1-01 updates", "/subscriptions", "/quit"],
    );
    assert_contains(
        &customer_unsubscribe,
        "Left shop chat Shop Updates (updates) at C0-N1-01.",
        "customer can unsubscribe",
    );
    assert_contains(
        &customer_unsubscribe,
        "No active shop chats.",
        "customer subscriptions list reflects unsubscribe",
    );

    let peer_unsubscribe = run_ssh_batch_with_key(
        host,
        port,
        peer.user,
        peer.key,
        &["/unsubscribe C0-N1-01 updates", "/subscriptions", "/quit"],
    );
    assert_contains(
        &peer_unsubscribe,
        "Left shop chat Shop Updates (updates) at C0-N1-01.",
        "peer can leave the shop chat",
    );

    let owner_send_after_unsubscribe = run_ssh_batch_with_key(
        host,
        port,
        owner.user,
        owner.key,
        &[
            "/shop mailing-list send C0-N1-01 updates Hidden Deal -- No one should get this.",
            "/quit",
        ],
    );
    assert_contains(
        &owner_send_after_unsubscribe,
        "shop chat has no active members",
        "send is blocked after all members leave",
    );
}

fn assert_shop_badge_flow(
    host: &str,
    port: u16,
    owner: &str,
    owner_key: &Path,
    customer: &str,
    customer_key: &Path,
) {
    let owner_create = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &[
            "/shop badge create C0-N1-01 patron Good Patron -- Paid and polite",
            "/shop badge list C0-N1-01",
            "/quit",
        ],
    );
    assert_contains(
        &owner_create,
        "Saved badge patron for parcel C0-N1-01: Good Patron.",
        "owner can create a shop badge",
    );
    assert_contains(
        &owner_create,
        "/shop badge award C0-N1-01 patron <user> [note]",
        "create response gives award command",
    );
    assert_contains(
        &owner_create,
        "Shop Badges for C0-N1-01",
        "owner can list shop badges",
    );

    let customer_award = run_ssh_batch_with_key(
        host,
        port,
        customer,
        customer_key,
        &[
            &format!("/shop badge award C0-N1-01 patron {customer} not allowed"),
            "/quit",
        ],
    );
    assert_contains(
        &customer_award,
        "The Guild will not accept that parcel action; you do not own this parcel.",
        "non-owner cannot award shop badges",
    );

    let owner_award = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &[
            &format!("/shop badge award C0-N1-01 patron {customer} first visit"),
            &format!("/shop badge award C0-N1-01 patron {customer} duplicate"),
            "/quit",
        ],
    );
    assert_contains(
        &owner_award,
        "Awarded badge Good Patron (patron) from C0-N1-01",
        "owner can award a shop badge",
    );

    let customer_badges =
        run_ssh_batch_with_key(host, port, customer, customer_key, &["/badges", "/quit"]);
    assert_contains(
        &customer_badges,
        "Badges for",
        "customer can inspect own badges",
    );
    assert_contains(
        &customer_badges,
        "Good Patron (patron) from Offline Tool Broker [C0-N1-01]",
        "badge output includes shop identity and badge title",
    );
    assert_contains(
        &customer_badges,
        "issued by",
        "badge output includes issuer and issue time",
    );

    let public_badges = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &[&format!("/badges {customer}"), "/quit"],
    );
    assert_contains(
        &public_badges,
        "Good Patron (patron) from Offline Tool Broker [C0-N1-01]",
        "other players can inspect public badges",
    );
}

fn request_shop_payment(host: &str, port: u16, owner: &str, owner_key: &Path) -> i64 {
    let owner_request = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &[
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
    parse_hash_id(&owner_request, "Created payment request #")
}

fn assert_customer_paid_request(
    host: &str,
    port: u16,
    customer: &str,
    customer_key: &Path,
    request_id: i64,
) {
    let accept_request = format!("/pay accept {request_id}");

    let customer_payment = run_ssh_batch_with_key(
        host,
        port,
        customer,
        customer_key,
        &["/pay requests", &accept_request, "/balance", "/quit"],
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
}

fn assert_owner_received_payment(host: &str, port: u16, owner: &str, owner_key: &Path) {
    let owner_reconnect = run_ssh_batch_with_key(
        host,
        port,
        owner,
        owner_key,
        &["/shop inbox", "/balance", "/quit"],
    );
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
}
