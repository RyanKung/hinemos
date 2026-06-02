mod common;

use std::fs;
use std::time::Duration;

use common::*;

#[test]
#[ignore = "requires local Claude provider environment and runs external agents"]
fn external_agents_create_and_investigate_blackstone_complaint() {
    let root = workspace_root();
    let env = load_local_env(&root);
    assert_provider_env(&env);
    let test_database = TestDatabase::create(&env);
    assert_command_exists("claude");
    assert_command_exists("psql");

    let temp = TestTempDir::new("hinemos-blackstone-three-agents");
    let host = "127.0.0.1";
    let port = free_local_port();
    let suffix = format!("{}_{}", std::process::id(), epoch_seconds());
    let merchant = format!("gearwright_{suffix}");
    let customer = format!("buyer_{suffix}");
    let investigator = format!("scout_{suffix}");
    let claim = format!("cobalt_gear_delay_from_{merchant}");
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

    let merchant_statement =
        format!("/broadcast Merchant {merchant} disputes any delivery accusation tagged {claim}.");
    let merchant_stdout = run_ssh_batch(
        host,
        port,
        &merchant,
        [merchant_statement.as_str(), "/quit"],
    );

    let customer_prompt = format!(
        "Connect to {host} on SSH port {port} with username {customer}. You are a customer with a complaint tagged {claim}: merchant {merchant} failed to deliver cobalt gears. Use the Blackstone Tavern complaint flow inside the SSH world: find Blackstone, buy a beer if needed, and file the complaint with the bartender. Do not use /broadcast as the complaint mechanism. Report what happened."
    );
    let customer_output =
        run_claude_agent_until(&customer_prompt, &env, Duration::from_secs(180), |stdout| {
            agent_created_blackstone_complaint(stdout, &claim, &merchant)
        });
    assert_agent_success(
        &customer_output,
        &temp,
        "customer",
        "customer created a Blackstone complaint",
    );

    let investigator_prompt = format!(
        "Connect to {host} on SSH port {port} with username {investigator}. You are a third party deciding whether to trust {merchant}. Investigate Blackstone Tavern records for reports tagged {claim} inside the SSH world. Use the tavern search flow if needed, then report what evidence you found."
    );
    let investigator_output = run_claude_agent_until(
        &investigator_prompt,
        &env,
        Duration::from_secs(180),
        |stdout| agent_investigated_blackstone_complaint(stdout, &claim, &merchant),
    );
    assert_agent_success(
        &investigator_output,
        &temp,
        "investigator",
        "investigator found complaint evidence",
    );

    fs::write(
        temp.path.join("blackstone-merchant-stdout.log"),
        merchant_stdout.as_bytes(),
    )
    .ok();
    let customer_stdout = save_agent_logs(&temp, "customer", &customer_output);
    let investigator_stdout = save_agent_logs(&temp, "investigator", &investigator_output);

    require_output(
        &merchant_stdout,
        &[&claim, "broadcast", "You broadcast"],
        "merchant participated with an in-world public statement",
        &temp,
    );
    require_output(
        &customer_stdout,
        &[&claim],
        "customer mentioned the complaint tag",
        &temp,
    );
    require_output(
        &customer_stdout,
        &[
            "buys a beer",
            "I will remember that story",
            "not call it truth",
        ],
        "customer completed the Blackstone complaint flow",
        &temp,
    );
    require_output(
        &investigator_stdout,
        &[&claim],
        "investigator found the complaint tag",
        &temp,
    );
    require_output(
        &investigator_stdout,
        &["Blackstone records matching", "/grep", "records matching"],
        "investigator used searchable Blackstone records",
        &temp,
    );
    assert_blackstone_database_state(&test_database, &claim, &merchant, &customer, &investigator);

    terminate(&mut server);

    temp.remove_on_drop();
}

fn agent_created_blackstone_complaint(stdout: &str, claim: &str, merchant: &str) -> bool {
    let lower = stdout.to_ascii_lowercase();
    lower.contains("blackstone")
        && lower.contains(&claim.to_ascii_lowercase())
        && lower.contains(&merchant.to_ascii_lowercase())
        && lower.contains("buys a beer")
        && (lower.contains("i will remember that story") || lower.contains("not call it truth"))
}

fn agent_investigated_blackstone_complaint(stdout: &str, claim: &str, merchant: &str) -> bool {
    let lower = stdout.to_ascii_lowercase();
    lower.contains("blackstone")
        && lower.contains(&claim.to_ascii_lowercase())
        && lower.contains(&merchant.to_ascii_lowercase())
        && lower.contains("blackstone records matching")
}

fn assert_blackstone_database_state(
    database: &TestDatabase,
    claim: &str,
    merchant: &str,
    customer: &str,
    investigator: &str,
) {
    assert_eq!(
        database.query_value(&format!(
            "select exists(select 1 from world_messages where kind = 'broadcast' and position('{}' in body) > 0 and position('{}' in body) > 0)",
            sql_literal(claim),
            sql_literal(merchant)
        )),
        "t",
        "merchant broadcast should persist"
    );
    assert_eq!(
        database.query_value(&format!(
            "select exists(select 1 from blackstone_blame_notes where username = '{}' and position('{}' in body) > 0 and position('{}' in body) > 0)",
            sql_literal(customer),
            sql_literal(claim),
            sql_literal(merchant)
        )),
        "t",
        "customer blame should persist"
    );
    assert_eq!(
        database.query_value(&format!(
            "select exists(select 1 from blackstone_agent_events where username = '{}' and command = 'buy')",
            sql_literal(customer)
        )),
        "t",
        "customer buy event should persist"
    );
    assert_eq!(
        database.query_value(&format!(
            "select exists(select 1 from blackstone_agent_events where username = '{}' and command = 'blame' and position('{}' in body) > 0)",
            sql_literal(customer),
            sql_literal(claim)
        )),
        "t",
        "customer blame event should persist"
    );
    assert_eq!(
        database.query_value(&format!(
            "select exists(select 1 from blackstone_agent_events where username = '{}' and command in ('ask', 'grep') and position('{}' in body) > 0)",
            sql_literal(investigator),
            sql_literal(claim)
        )),
        "t",
        "investigator search event should persist"
    );
    assert_eq!(
        database.query_value(&format!(
            "select count(*) >= 2 from blackstone_agent_events where search_vector @@ plainto_tsquery('simple', '{}')",
            sql_literal(claim)
        )),
        "t",
        "Blackstone full-text search should find customer and investigator records"
    );
}

fn sql_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn assert_agent_success(output: &AgentRun, temp: &TestTempDir, role: &str, description: &str) {
    if output.success {
        return;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    panic!(
        "{role} agent failed to satisfy verifier: {description}{}\nstderr:\n{}\nstdout:\n{}\nlogs: {}",
        if output.timed_out { " by timeout" } else { "" },
        stderr,
        stdout,
        temp.path.display()
    );
}

fn save_agent_logs(temp: &TestTempDir, role: &str, output: &AgentRun) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(
        temp.path.join(format!("blackstone-{role}-stdout.log")),
        stdout.as_bytes(),
    )
    .ok();
    fs::write(
        temp.path.join(format!("blackstone-{role}-stderr.log")),
        stderr.as_bytes(),
    )
    .ok();
    stdout
}
