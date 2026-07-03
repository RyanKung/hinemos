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
