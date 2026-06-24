use hinemos_builtin_rooms::{BuiltinRoomsConfig, run_builtin_rooms};
use hinemos_storage::{INBOX_FILTER_ALL, INBOX_STATUS_ACKED, PgStorage, ServiceRoomUpsert};
use hinemos_test_support::{TestDatabase, assert_contains, load_local_env, workspace_root};

#[tokio::test]
async fn runner_migrates_schema_before_polling() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);

    run_builtin_rooms(
        &test_database.url,
        BuiltinRoomsConfig {
            once: true,
            ..BuiltinRoomsConfig::default()
        },
    )
    .await
    .expect("run built-in room runner once");

    assert_eq!(
        test_database.query_value("select to_regclass('service_rooms') is not null"),
        "t",
        "runner applies storage migrations before polling"
    );
}

#[tokio::test]
async fn runner_processes_built_in_room_mail_without_cli_binary() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");
    storage
        .upsert_service_room(ServiceRoomUpsert {
            view_id: "custom_bank",
            front_view_id: Some("test_street"),
            front_entity_id: None,
            address: Some("T1"),
            label: Some("Custom Bank"),
            enter_aliases: None,
            room_user: "room-custom_bank",
            room_player_id: "room:custom_bank",
            status_text: None,
            custom_commands: Some("/bank balance"),
            builtin_handler: Some("hinemos_bank"),
            enabled: true,
        })
        .await
        .expect("register built-in bank room");

    let request = storage
        .save_mail_message_to_principal(
            "alice",
            "player:alice",
            "room-custom_bank",
            "room:custom_bank",
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
        .find(|item| item.sender_user == "room-custom_bank")
        .expect("bank reply should be delivered to alice");
    assert_contains(
        &reply.body,
        "Cash: 100 MARK. Deposit: 0 MARK. Loan debt: 0 MARK.",
        "bank balance reply",
    );
}

#[tokio::test]
async fn registry_runner_uses_registered_room_view_for_presence() {
    let root = workspace_root();
    let env = load_local_env(&root);
    let test_database = TestDatabase::create(&env);
    let storage = PgStorage::connect(&test_database.url)
        .await
        .expect("connect test database");
    storage.migrate().await.expect("migrate test database");
    storage
        .upsert_service_room(ServiceRoomUpsert {
            view_id: "custom_registry",
            front_view_id: Some("test_street"),
            front_entity_id: None,
            address: Some("T2"),
            label: Some("Custom Registry"),
            enter_aliases: None,
            room_user: "room-custom_registry",
            room_player_id: "room:custom_registry",
            status_text: None,
            custom_commands: Some("/marriage register <user>"),
            builtin_handler: Some("hinemos_registry"),
            enabled: true,
        })
        .await
        .expect("register custom registry room");
    seed_player_profile_and_identity(&test_database, "alice", "player:alice");
    seed_player_profile_and_identity(&test_database, "bob", "player:bob");
    storage
        .ensure_player_wallet("alice", "player:alice")
        .await
        .expect("alice wallet");
    storage
        .ensure_player_wallet("bob", "player:bob")
        .await
        .expect("bob wallet");
    storage
        .record_view_presence("alice", "player:alice", "custom_registry")
        .await
        .expect("alice presence");
    storage
        .record_view_presence("bob", "player:bob", "custom_registry")
        .await
        .expect("bob presence");

    let request = storage
        .save_mail_message_to_principal(
            "alice",
            "player:alice",
            "room-custom_registry",
            "room:custom_registry",
            "Room command",
            "/marriage register bob",
        )
        .await
        .expect("queue registry mail");

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
    assert_eq!(
        test_database
            .query_value("select count(*) from marriage_certificates where status = 'active'"),
        "1",
        "custom registry presence is accepted for marriage registration"
    );
    assert_eq!(
        storage
            .player_balance("player:alice")
            .await
            .expect("alice balance")
            .amount,
        975
    );
    assert_eq!(
        storage
            .player_balance("player:bob")
            .await
            .expect("bob balance")
            .amount,
        975
    );

    let replies = storage
        .list_inbox_items("alice", "player:alice", Some(INBOX_FILTER_ALL), 10)
        .await
        .expect("list alice inbox");
    let reply = replies
        .iter()
        .find(|item| item.sender_user == "room-custom_registry")
        .expect("registry reply should be delivered to alice from custom room");
    assert_contains(
        &reply.body,
        "Marriage registered.",
        "custom registry registers marriage",
    );
}

fn seed_player_profile_and_identity(database: &TestDatabase, username: &str, player_id: &str) {
    database.query_value(&format!(
        "insert into player_profiles (player_id, display_name, admission_state)
         values ('{player_id}', '{username}', 'agreed');
         insert into ssh_identities (username, key_fingerprint, player_id)
         values ('{username}', 'test:{username}', '{player_id}')"
    ));
}
