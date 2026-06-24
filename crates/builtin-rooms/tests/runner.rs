use hinemos_builtin_rooms::{BuiltinRoomsConfig, run_builtin_rooms};
use hinemos_storage::{INBOX_FILTER_ALL, INBOX_STATUS_ACKED, PgStorage};
use hinemos_test_support::{TestDatabase, assert_contains, load_local_env, workspace_root};

#[tokio::test]
async fn runner_processes_built_in_room_mail_without_cli_binary() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");

    let request = storage
        .save_mail_message_to_principal(
            "alice",
            "player:alice",
            "room-hinemos_bank",
            "room:hinemos_bank",
            "Room command",
            "/bank balance",
        )
        .await
        .expect("queue room mail");

    run_builtin_rooms(
        &test_database.url,
        BuiltinRoomsConfig {
            once: true,
            ..BuiltinRoomsConfig::default()
        },
    )
    .await
    .expect("run built-in room runner once");

    let processed_request = storage
        .inbox_item(request.id)
        .await
        .expect("read processed request");
    assert_eq!(processed_request.status, INBOX_STATUS_ACKED);

    let replies = storage
        .list_inbox_items("alice", "player:alice", Some(INBOX_FILTER_ALL), 10)
        .await
        .expect("list alice inbox");
    let reply = replies
        .iter()
        .find(|item| item.sender_user == "room-hinemos_bank")
        .expect("bank reply should be delivered to alice");
    assert_contains(
        &reply.body,
        "Cash: 100 MARK. Deposit: 0 MARK. Loan debt: 0 MARK.",
        "bank balance reply",
    );
}
