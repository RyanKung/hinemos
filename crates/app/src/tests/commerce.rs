use super::*;

#[test]
fn commercial_parcel_input_routes_through_app() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestCommercialStore {
            parcel: Mutex::new(TestCommercialParcel {
                parcel_id: "P1",
                view_id: "parcel_view",
                front_view_id: "street_view",
                district: "north",
                position: 1,
                owner_user: Some("owner".to_owned()),
                owner_player_id: Some("owner-player".to_owned()),
                room_user: Some("room-user".to_owned()),
                room_player_id: Some("room-player".to_owned()),
                status: PARCEL_STATUS_BUILT,
                title: Some("Parcel".to_owned()),
                description: None,
                style: None,
                operator_prompt: None,
                custom_commands: Some(
                    "/shop request-payment <command_id> <amount> <delivery> preview=hello world"
                        .to_owned(),
                ),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let app = AppService::new(store);
        let identity = AppIdentity::new("visitor", "visitor-player");
        let binding = TestCommercialParcel {
            parcel_id: "P1",
            view_id: "parcel_view",
            front_view_id: "street_view",
            district: "north",
            position: 1,
            owner_user: Some("owner".to_owned()),
            owner_player_id: Some("owner-player".to_owned()),
            room_user: Some("room-user".to_owned()),
            room_player_id: Some("room-player".to_owned()),
            status: PARCEL_STATUS_BUILT,
            title: Some("Parcel".to_owned()),
            description: None,
            style: None,
            operator_prompt: None,
            custom_commands: Some(
                "/shop request-payment <command_id> <amount> <delivery> preview=hello world"
                    .to_owned(),
            ),
        };

        let events = app
            .handle_commercial_parcel_input(
                &identity,
                &binding,
                "/shop request-payment 7 25 hello world",
            )
            .await
            .expect("commercial parcel input")
            .expect("handled");

        assert_eq!(
            events,
            vec![
                UiEvent::Text(
                    "Shop request #1 sent to owner owner for parcel parcel.\r\nStatus: delivered.\r\nPreview: hello\r\n"
                        .to_owned()
                ),
                UiEvent::LiveInboxNotice {
                    target_player_id: "owner-player".to_owned(),
                    notice: LiveInboxNotice {
                        id: 1,
                        kind: "mail".to_owned(),
                        sender_user: "alice".to_owned(),
                        subject: "hello".to_owned(),
                        body: "body".to_owned(),
                    },
                },
            ]
        );
        assert_eq!(
            app.store().calls.lock().unwrap().clone(),
            vec![
                "operator:visitor:visitor-player:P1:/shop request-payment 7 25 hello world:true"
                    .to_owned()
            ]
        );
    });
}

#[test]
fn parcel_list_renders_status_for_humans() {
    let parcels = vec![
        TestParcel {
            parcel_id: "north_01",
            view_id: "parcel_north_01",
            district: "north",
            position: 1,
            owner_user: Some("mainiu"),
            room_user: Some("room_north_01"),
            status: PARCEL_STATUS_BUILT,
            title: Some("Corall牛比站"),
        },
        TestParcel {
            parcel_id: "north_02",
            view_id: "parcel_north_02",
            district: "north",
            position: 2,
            owner_user: None,
            room_user: None,
            status: "vacant",
            title: None,
        },
    ];

    let rendered = render_parcel_list(&parcels);

    assert!(rendered.contains("north_01: Corall牛比站. Owner: mainiu."));
    assert!(rendered.contains("north_02: vacant. Claim: /land claim north_02."));
    assert!(!rendered.contains("view=parcel_north_01"));
}

#[test]
fn app_message_helpers_persist_and_emit_expected_events() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestMessageStore::default();
        let app = AppService::new(store);
        let identity = AppIdentity::new("alice".to_owned(), "player-1".to_owned());

        let say_events = app
            .handle_say(&identity, "arrival_street", "hello world")
            .await
            .expect("say");
        assert_eq!(
            say_events,
            vec![UiEvent::LiveViewMessage {
                view_id: "arrival_street".to_owned(),
                text: "[say from alice] hello world".to_owned(),
            }]
        );

        let mail_events = app
            .handle_mail(&identity, "bob", "hi bob")
            .await
            .expect("mail");
        assert!(mail_events.is_empty());

        let broadcast_events = app
            .handle_broadcast(&identity, "news flash")
            .await
            .expect("broadcast");
        assert!(broadcast_events.is_empty());

        let calls = app.store().calls.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec![
                "say:alice:player-1:arrival_street:hello world".to_owned(),
                "mail:alice:player-1:bob:hi bob".to_owned(),
                "broadcast:alice:player-1:news flash".to_owned(),
            ]
        );
    });
}

#[test]
fn commercial_parcel_actions_emit_cache_invalidation_events() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestCommercialStore {
            parcel: Mutex::new(TestCommercialParcel {
                parcel_id: "parcel-1",
                view_id: "parcel-view",
                front_view_id: "street-a",
                district: "north",
                position: 1,
                owner_user: None,
                owner_player_id: None,
                room_user: Some("room-user".to_owned()),
                room_player_id: Some("room-player".to_owned()),
                status: "vacant",
                title: None,
                description: None,
                style: None,
                operator_prompt: None,
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });

        let claim = app
            .claim_land("parcel-1", "alice", "player-1", "token-1")
            .await
            .expect("claim land");
        assert_eq!(
            claim.invalidate,
            Some(CommercialParcelCacheInvalidation {
                view_id: "parcel-view".to_owned(),
                front_view_id: "street-a".to_owned(),
            })
        );

        let build = app
            .apply_build_sheet(
                "parcel-view",
                "player-1",
                &BuildSheet {
                    title: Some("Shop".to_owned()),
                    description: Some("Desc".to_owned()),
                    style: Some("Style".to_owned()),
                    prompt: Some("Prompt".to_owned()),
                    commands: None,
                },
            )
            .await
            .expect("apply build");
        assert_eq!(
            build.invalidate,
            Some(CommercialParcelCacheInvalidation {
                view_id: "parcel-view".to_owned(),
                front_view_id: "street-a".to_owned(),
            })
        );

        let publish = app
            .publish_build("parcel-view", "player-1")
            .await
            .expect("publish build");
        assert_eq!(
            publish.invalidate,
            Some(CommercialParcelCacheInvalidation {
                view_id: "parcel-view".to_owned(),
                front_view_id: "street-a".to_owned(),
            })
        );
    });
}
