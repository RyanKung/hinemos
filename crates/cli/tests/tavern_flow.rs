mod common;

use common::*;

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_is_closed_when_agent_is_offline() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-blackstone-closed");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("closed_guest_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "0")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/go west",
            "/look",
            "/buy beer",
            "/blame alice failed delivery",
            "/ask is alice reliable",
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "Blackstone is closed. The bartender is not online.",
        "closed status is visible",
    );
    assert_contains(
        &output,
        "Blackstone is closed",
        "closed tavern rejects extension commands",
    );
    assert_not_contains(
        &output,
        "/buy beer, /blame",
        "closed tavern does not advertise buy command",
    );
    assert_not_contains(&output, "buys a beer", "closed tavern does not sell beer");
    assert_eq!(
        test_database.query_value(&format!(
            "select exists(select 1 from blackstone_beer_tabs where username = '{}')",
            sql_literal(&user)
        )),
        "f",
        "closed tavern should not create beer tabs"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select exists(select 1 from blackstone_blame_notes where username = '{}')",
            sql_literal(&user)
        )),
        "f",
        "closed tavern should not create blame notes"
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select exists(select 1 from blackstone_agent_events where username = '{}')",
            sql_literal(&user)
        )),
        "f",
        "closed tavern should not create agent events"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_is_open_when_agent_is_online() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-blackstone-open");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("open_guest_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "1")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(host, port, &user, ["/go west", "/look", "/quit"]);

    assert_contains(
        &output,
        "Blackstone is open. The bartender is online.",
        "open status is visible",
    );
    assert_contains(&output, "/buy beer", "open tavern advertises buy");
    assert_not_contains(
        &output,
        "/blame <complaint>",
        "open tavern does not advertise blame before a drink",
    );
    assert_not_contains(
        &output,
        "/ask <question>",
        "open tavern does not advertise ask before a drink",
    );
    assert_not_contains(
        &output,
        "/grep <query>",
        "open tavern does not advertise grep before a drink",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_advertises_bar_commands_after_a_drink() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-blackstone-drink-window");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("drink_guest_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "1")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        ["/go west", "/buy beer", "/look", "/quit"],
    );

    assert_contains(
        &output,
        "Your drink is active. You can use bar commands or chat with the bartender.",
        "active drink window is explained",
    );
    assert_contains(
        &output,
        "/blame <complaint>",
        "blame is advertised after a drink",
    );
    assert_contains(
        &output,
        "/ask <question>",
        "ask is advertised after a drink",
    );
    assert_contains(&output, "/grep <query>", "grep is advertised after a drink");
    assert_blackstone_event_exists(&test_database, &user, "buy", "");

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_handles_buy_blame_ask_and_grep() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-blackstone-tavern");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("tavern_guest_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "1")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/go west",
            "/blame alice broke a delivery promise",
            "/ask is alice reliable",
            "/buy beer",
            "/blame alice broke a delivery promise",
            "/grep delivery",
            "/broadcast alice denies the delivery complaint",
            "/ask is alice reliable",
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "Blackstone Tavern",
        "guest entered Blackstone Tavern",
    );
    assert_contains(
        &output,
        "You do not consider drinking first?",
        "bartender gates blame and ask behind beer",
    );
    assert_contains(
        &output,
        "buys a beer",
        "beer purchase is recorded by the tavern",
    );
    assert_contains(
        &output,
        "I will remember that story, but I will not call it truth yet",
        "bartender comments on blame without judging truth",
    );
    assert_contains(
        &output,
        "Blackstone records matching 'delivery'",
        "tavern records can be searched",
    );
    assert_contains(
        &output,
        "public broadcasts that may help cross-check",
        "ask uses observed broadcast context",
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_chat_records_complaints_and_answers_within_drink_window() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-blackstone-chat");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("chat_guest_{}_{}", std::process::id(), epoch_seconds());
    let claim = format!("amber_delivery_failed_from_{user}");
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "1")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/go west",
            &format!("{claim} before beer"),
            "/buy beer",
            &format!("I heard {claim}; what should I do next?"),
            &format!("/grep {claim}"),
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "You do not consider drinking first?",
        "free-form chat is gated before a drink",
    );
    assert_contains(
        &output,
        "The bartender listens",
        "free-form chat receives a bartender response after a drink",
    );
    assert_contains(
        &output,
        &format!("Blackstone records matching '{claim}'"),
        "chat content is searchable",
    );
    assert_blackstone_event_exists(&test_database, &user, "chat", &claim);
    assert_eq!(
        test_database.query_value(&format!(
            "select exists(select 1 from blackstone_blame_notes where username = '{}' and position('{}' in body) > 0)",
            sql_literal(&user),
            sql_literal(&claim)
        )),
        "t",
        "complaint-like chat should also create a blame lead"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_chat_window_expires_after_five_minutes() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("psql");

    let temp = TestTempDir::new("hinemos-blackstone-chat-expiry");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("expiry_guest_{}_{}", std::process::id(), epoch_seconds());
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "1")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let first_output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/go west",
            "/buy beer",
            "hello while the drink is fresh",
            "/quit",
        ],
    );
    assert_contains(
        &first_output,
        "The bartender listens",
        "fresh drink enables free-form chat",
    );
    assert_eq!(
        test_database.query_value(&format!(
            "with updated as (update blackstone_beer_tabs set updated_at = now() - interval '6 minutes' where username = '{}' returning 1) select count(*) from updated",
            sql_literal(&user)
        )),
        "1",
        "test should expire the beer tab"
    );

    let second_output = run_ssh_batch(
        host,
        port,
        &user,
        ["hello after the drink expired", "/quit"],
    );
    assert_contains(
        &second_output,
        "You do not consider drinking first?",
        "expired drink blocks free-form chat",
    );
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*) from blackstone_agent_events where username = '{}' and command = 'chat'",
            sql_literal(&user)
        )),
        "1",
        "expired chat should not create another chat event"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_persists_searchable_service_history() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");
    assert_command_exists("psql");

    let temp = TestTempDir::new("hinemos-blackstone-history");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("history_guest_{}_{}", std::process::id(), epoch_seconds());
    let claim = format!("silver_widget_delay_from_{user}");
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [("BLACKSTONE_AGENT_ONLINE", "1")],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/go west",
            "/grep nothing_to_find_here",
            "/buy beer",
            &format!("/blame {claim}"),
            &format!("/ask what do you know about {claim}"),
            &format!("/grep {claim}"),
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "No Blackstone records matched 'nothing_to_find_here'.",
        "empty grep is explicit",
    );
    assert_contains(
        &output,
        &format!("Blackstone records matching '{claim}'"),
        "grep finds persisted complaint",
    );
    assert_eq!(
        test_database
            .query_value("select exists(select 1 from pg_extension where extname = 'pg_trgm')"),
        "t",
        "pg_trgm extension should be installed"
    );
    for index_name in [
        "blackstone_agent_events_body_trgm_idx",
        "blackstone_agent_events_response_trgm_idx",
        "blackstone_agent_events_username_trgm_idx",
    ] {
        assert_eq!(
            test_database.query_value(&format!(
                "select exists(select 1 from pg_indexes where indexname = '{}')",
                sql_literal(index_name)
            )),
            "t",
            "{index_name} should exist"
        );
    }
    assert_blackstone_event_exists(&test_database, &user, "buy", "");
    assert_blackstone_event_exists(&test_database, &user, "blame", &claim);
    assert_blackstone_event_exists(&test_database, &user, "ask", &claim);
    assert_blackstone_event_exists(&test_database, &user, "grep", &claim);
    assert_eq!(
        test_database.query_value(&format!(
            "select count(*) >= 3 from blackstone_agent_events where search_vector @@ plainto_tsquery('simple', '{}')",
            sql_literal(&claim)
        )),
        "t",
        "claim should be found by Blackstone full-text search"
    );

    terminate(&mut server);
    temp.remove_on_drop();
}

#[test]
#[ignore = "requires local Postgres and SSH client"]
fn blackstone_tavern_falls_back_when_llm_provider_fails() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("ssh");

    let temp = TestTempDir::new("hinemos-blackstone-llm-fallback");
    let host = "127.0.0.1";
    let port = free_local_port();
    let user = format!("fallback_guest_{}_{}", std::process::id(), epoch_seconds());
    let claim = format!("fallback_claim_from_{user}");
    let server_log = temp.path.join("hinemos-server.log");

    let mut server = spawn_hinemos_server_with_env(
        &root,
        host,
        port,
        &server_log,
        &test_database.url,
        [
            ("BLACKSTONE_AGENT_ONLINE", "1"),
            ("BLACKSTONE_LLM_ENABLED", "1"),
            ("BLACKSTONE_LLM_BASE_URL", "http://127.0.0.1:1"),
            ("BLACKSTONE_LLM_AUTH_TOKEN", "test-token"),
            ("BLACKSTONE_LLM_MODEL", "test-model"),
        ],
    );
    wait_for_server(host, port, &mut server, &server_log);

    let output = run_ssh_batch(
        host,
        port,
        &user,
        [
            "/go west",
            "/buy beer",
            &format!("/blame {claim}"),
            &format!("/ask what do you know about {claim}"),
            "/quit",
        ],
    );

    assert_contains(
        &output,
        "I will remember that story, but I will not call it truth yet",
        "blame falls back when LLM provider is unavailable",
    );
    assert_contains(
        &output,
        "The bartender considers",
        "ask falls back when LLM provider is unavailable",
    );
    assert_blackstone_event_exists(&test_database, &user, "blame", &claim);
    assert_blackstone_event_exists(&test_database, &user, "ask", &claim);

    terminate(&mut server);
    temp.remove_on_drop();
}

fn assert_blackstone_event_exists(
    database: &TestDatabase,
    username: &str,
    command: &str,
    body_contains: &str,
) {
    let body_predicate = if body_contains.is_empty() {
        String::new()
    } else {
        format!(
            " and position('{}' in body) > 0",
            sql_literal(body_contains)
        )
    };
    assert_eq!(
        database.query_value(&format!(
            "select exists(select 1 from blackstone_agent_events where username = '{}' and command = '{}' {})",
            sql_literal(username),
            sql_literal(command),
            body_predicate
        )),
        "t",
        "{command} event should persist"
    );
}

fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}
