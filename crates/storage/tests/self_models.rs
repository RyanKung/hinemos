use hinemos_storage::PgStorage;
use hinemos_test_support::{TestDatabase, load_local_env, workspace_root};
use serde_json::json;

#[tokio::test]
async fn concurrent_self_model_state_writes_preserve_both_versions() {
    let env = load_local_env(&workspace_root());
    if env.get("DATABASE_URL").is_none_or(String::is_empty) {
        eprintln!("skipping self-model storage test because DATABASE_URL is not configured");
        return;
    }
    let db = TestDatabase::create(&env);
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    let agent_id = "agent_concurrent_self_model";
    storage
        .ensure_self_model(
            agent_id,
            &json!({"name": "Concurrent Agent"}),
            &json!({"step": "base"}),
            &json!({"style": "test"}),
        )
        .await
        .expect("ensure self-model");

    let first_state = json!({"step": "first"});
    let second_state = json!({"step": "second"});
    let (first, second) = tokio::join!(
        storage.record_self_model_state(agent_id, &first_state),
        storage.record_self_model_state(agent_id, &second_state),
    );

    first.expect("first concurrent transition");
    second.expect("second concurrent transition");
    assert_eq!(
        db.query_value(
            "select concat_ws(':',
                 count(*),
                 max(version),
                 count(distinct current_state->>'step'))
             from agent_self_models
             where agent_id = 'agent_concurrent_self_model'"
        ),
        "3:3:3",
        "concurrent self-model writes should serialize without dropping either transition"
    );
}

#[tokio::test]
async fn ensure_self_model_cuts_over_identity_and_style_policy() {
    let env = load_local_env(&workspace_root());
    if env.get("DATABASE_URL").is_none_or(String::is_empty) {
        eprintln!("skipping self-model storage test because DATABASE_URL is not configured");
        return;
    }
    let db = TestDatabase::create(&env);
    let storage = PgStorage::connect(&db.url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    let agent_id = "agent_self_model_policy_cutover";
    storage
        .ensure_self_model(
            agent_id,
            &json!({"name": "Resident"}),
            &json!({"step": "already-lived"}),
            &json!({"autonomy": "old policy"}),
        )
        .await
        .expect("ensure original self-model");
    storage
        .record_self_model_state(agent_id, &json!({"step": "legacy-version"}))
        .await
        .expect("record legacy self-model state");

    let cutover = storage
        .ensure_self_model(
            agent_id,
            &json!({
                "name": "Resident",
                "subjectBoundary": "Inside Hinemos, I am the logged-in resident Resident."
            }),
            &json!({"step": "fresh-default-replaces-legacy-state"}),
            &json!({"autonomy": "new policy"}),
        )
        .await
        .expect("cut over self-model policy");

    assert_eq!(cutover.version, 1);
    assert_eq!(
        cutover.current_state,
        json!({"step": "fresh-default-replaces-legacy-state"})
    );
    assert_eq!(
        cutover
            .identity
            .get("subjectBoundary")
            .and_then(|value| value.as_str()),
        Some("Inside Hinemos, I am the logged-in resident Resident.")
    );
    assert_eq!(
        cutover
            .style
            .get("autonomy")
            .and_then(|value| value.as_str()),
        Some("new policy")
    );

    storage
        .ensure_self_model(
            agent_id,
            &cutover.identity,
            &json!({"step": "fresh-default-should-not-replace-current-state"}),
            &cutover.style,
        )
        .await
        .expect("ensure cutover self-model remains idempotent");
    assert_eq!(
        db.query_value(
            "select concat_ws(':', count(*), max(version))
             from agent_self_models
             where agent_id = 'agent_self_model_policy_cutover'"
        ),
        "1:1",
        "cutover should replace legacy self-model versions and remain idempotent"
    );
}
