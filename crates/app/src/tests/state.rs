use super::*;

#[test]
fn app_player_state_helpers_forward_to_store() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestPlayerStateStore::default();
        let app = AppService::new(store);

        let loaded = app.load_player_state("player-1").await.expect("load");
        assert!(loaded.is_some());

        let player = PlayerState {
            id: "player-1".to_owned(),
            current_view: "arrival_street".to_owned(),
            inventory: Vec::new(),
        };
        app.save_player_state(&player).await.expect("save");

        let calls = app.store().calls.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec![
                "load:player-1".to_owned(),
                "save:player-1:arrival_street".to_owned(),
            ]
        );
    });
}

#[test]
fn app_presence_helper_forwards_to_store() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestPresenceStore::default();
        let app = AppService::new(store);

        app.record_view_presence("alice", "player-1", "arrival_street")
            .await
            .expect("presence");

        let calls = app.store().calls.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec!["presence:alice:player-1:arrival_street".to_owned()]
        );
    });
}

#[test]
fn login_banner_helpers_render_expected_text() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let message_store = TestMessageStore::default();
        let app = AppService::new(message_store);
        let balance = app.balance_summary("player-1").await.expect("balance");
        assert!(balance.contains("Balance: 1000 MARK (account)"));

        let inbox_store = TestInboxStore;
        let inbox_app = AppService::new(inbox_store);
        let inbox = inbox_app
            .open_inbox_summary("alice", "player-1")
            .await
            .expect("inbox");
        assert_eq!(inbox.as_deref(), Some("Inbox: 2 open item(s).\r\n"));
    });
}
