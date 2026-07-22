use super::*;

#[test]
fn parcel_input_routes_through_app() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
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
                    "/parcel request-payment <command_id> <amount> <delivery> preview=hello world"
                        .to_owned(),
                ),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let app = AppService::new(store);
        let identity = AppIdentity::new("visitor", "visitor-player");
        let binding = TestParcel {
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
                "/parcel request-payment <command_id> <amount> <delivery> preview=hello world"
                    .to_owned(),
            ),
        };

        assert!(
            app.parcel_consumes_input(
                &binding,
                "/parcel request-payment 7 25 hello world"
            ),
            "parcels should consume matching parcel commands"
        );
        assert!(
            !app.parcel_consumes_input(&binding, "/balance"),
            "parcels should leave global slash commands to the shell"
        );

        let events = app
            .handle_parcel_input(
                &identity,
                &binding,
                "/parcel request-payment 7 25 hello world",
            )
            .await
            .expect("parcel input")
            .expect("handled");

        assert_eq!(
            events,
            vec![
                UiEvent::Text(
                    "Parcel request #1 sent to owner owner for parcel P1.\r\nStatus: delivered. Payment and fulfillment are pending owner reply; check /mailbox and /pay requests.\r\nQueued 1 parcel work item(s). Workers must be inside the parcel with an active shift to list, claim, or complete them.\r\nPreview: hello\r\n"
                        .to_owned()
                ),
                UiEvent::LiveViewMessage {
                    view_id: "parcel_view".to_owned(),
                    text: "[parcel work] 1 new item(s) queued for parcel P1.".to_owned(),
                },
            ]
        );
        assert_eq!(
            app.store().calls.lock().unwrap().clone(),
            vec![
                "operator:visitor:visitor-player:P1:/parcel request-payment 7 25 hello world:true"
                    .to_owned(),
                "dispatch-work:1".to_owned(),
            ]
        );

        let global_command = app
            .handle_parcel_input(&identity, &binding, "/balance")
            .await
            .expect("parcel input");
        assert!(
            global_command.is_none(),
            "parcels should not intercept global slash commands"
        );
    });
}

#[test]
fn parcel_custom_command_preview_uses_longest_literal_command_match() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
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
                    "/paper help preview=\"Show newspaper commands\"; /paper submit <title> -- <body> preview=\"Submit an article for review\""
                        .to_owned(),
                ),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let app = AppService::new(store);
        let identity = AppIdentity::new("visitor", "visitor-player");
        let binding = TestParcel {
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
                "/paper help preview=\"Show newspaper commands\"; /paper submit <title> -- <body> preview=\"Submit an article for review\""
                    .to_owned(),
            ),
        };

        let events = app
            .handle_parcel_input(&identity, &binding, "/paper submit Scoop -- Body")
            .await
            .expect("parcel input")
            .expect("handled");

        assert!(
            matches!(
                events.first(),
                Some(UiEvent::Text(text))
                    if text.contains("Preview: Submit an article for review")
                        && !text.contains("Preview: Show newspaper commands")
            ),
            "submit command should use submit preview: {events:?}"
        );
    });
}

#[test]
fn parcel_list_renders_status_for_humans() {
    let parcels = vec![
        TestListedParcel {
            parcel_id: "north_01",
            view_id: "parcel_north_01",
            district: "north",
            position: 1,
            owner_user: Some("mainiu"),
            room_user: Some("room_north_01"),
            status: PARCEL_STATUS_BUILT,
            title: Some("Corall牛比站"),
        },
        TestListedParcel {
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
    assert!(rendered.contains("north_02: vacant. Claim: /parcel claim north_02."));
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
fn parcel_actions_emit_cache_invalidation_events() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
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
            .claim_parcel("parcel-1", "alice", "player-1", "token-1")
            .await
            .expect("claim parcel");
        assert_eq!(
            claim.invalidate,
            Some(ParcelCacheInvalidation {
                view_id: "parcel-view".to_owned(),
                front_view_id: "street-a".to_owned(),
            })
        );

        let build = app
            .apply_build_sheet(
                "parcel-view",
                "player-1",
                &BuildSheet {
                    title: Some("Parcel".to_owned()),
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
            Some(ParcelCacheInvalidation {
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
            Some(ParcelCacheInvalidation {
                view_id: "parcel-view".to_owned(),
                front_view_id: "street-a".to_owned(),
            })
        );
    });
}

#[test]
fn parcel_mailing_list_send_emits_live_inbox_notice() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
                parcel_id: "P1",
                view_id: "parcel-view",
                front_view_id: "street-a",
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
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });
        let identity = AppIdentity::new("owner", "owner-player");

        let result = app
            .send_parcel_mailing_list_post(ParcelMailingListPostInput {
                current_view: "parcel-view",
                target: "P1",
                slug: "updates",
                sender_user: &identity.user,
                sender_player_id: &identity.player_id,
                subject: "Weekly Deal",
                body: "Body",
            })
            .await
            .expect("send mailing list post");

        assert_eq!(result.post.id(), 7);
        assert_eq!(result.post.recipient_count(), 1);
        assert_eq!(result.deliveries[0].recipient_player_id, "visitor-player");
        assert_eq!(result.deliveries[0].inbox_item.subject(), "Weekly Deal");
    });
}

#[test]
fn parcel_mailing_list_chat_posts_as_member_message() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
                parcel_id: "P1",
                view_id: "parcel-view",
                front_view_id: "street-a",
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
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });
        let identity = AppIdentity::new("visitor", "visitor-player");

        let result = app
            .post_parcel_mailing_list_chat(
                "parcel-view",
                "Offline Tool Broker",
                "updates",
                &identity.user,
                &identity.player_id,
                "Hello members",
            )
            .await
            .expect("post parcel chat");

        assert_eq!(result.post.recipient_count(), 1);
        assert_eq!(
            app.store().calls.lock().unwrap().clone(),
            vec![
                "mailing-list-send:Offline Tool Broker:updates:visitor:visitor-player:Parcel chat: updates:Hello members"
                    .to_owned()
            ]
        );
    });
}

#[test]
fn parcel_command_route_service_commands_render_expected_text() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
                parcel_id: "P1",
                view_id: "parcel-view",
                front_view_id: "street-a",
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
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });

        let added = app
            .add_parcel_command_route(
                "parcel-view",
                "P1",
                "owner-player",
                "updates",
                "/paper submit",
            )
            .await
            .expect("add route");
        assert!(
            added
                .text
                .contains("Routed parcel commands matching /paper submit")
        );
        assert!(added.text.contains("work desk"));
        assert!(added.text.contains("start a shift"));

        let listed = app
            .list_parcel_command_routes("parcel-view", "P1", "owner-player")
            .await
            .expect("list routes");
        assert!(listed.text.contains("Parcel Command Routes for P1"));
        assert!(listed.text.contains("/hello -> updates"));

        let removed = app
            .remove_parcel_command_route(
                "parcel-view",
                "P1",
                "owner-player",
                "updates",
                "/paper submit",
            )
            .await
            .expect("remove route");
        assert!(
            removed
                .text
                .contains("Removed parcel command route /paper submit")
        );

        assert_eq!(
            app.store().calls.lock().unwrap().clone(),
            vec![
                "route-add:P1:owner-player:updates:/paper submit".to_owned(),
                "route-list:P1:owner-player".to_owned(),
                "route-remove:P1:owner-player:updates:/paper submit".to_owned(),
            ]
        );
    });
}

#[test]
fn parcel_actions_require_inside_parcel_before_mutating_state() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
                parcel_id: "P1",
                view_id: "parcel-view",
                front_view_id: "street-a",
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
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });
        let expected = TestCommerceError::ParcelWork(
            "parcel actions can only happen while inside that parcel".to_owned(),
        );

        let desk = app
            .create_parcel_work_desk("street-a", "P1", "owner-player", "updates", "Updates")
            .await
            .expect_err("desk create outside parcel");
        assert_eq!(desk, expected);

        let staff = app
            .add_parcel_staff("street-a", "P1", "updates", "owner-player", "reporter")
            .await
            .expect_err("staff add outside parcel");
        assert_eq!(staff, expected);

        let route = app
            .add_parcel_command_route("street-a", "P1", "owner-player", "updates", "/paper")
            .await
            .expect_err("route add outside parcel");
        assert_eq!(route, expected);

        let inbox = app
            .parcel_inbox("street-a", "owner-player")
            .await
            .expect_err("parcel inbox outside parcel");
        assert_eq!(
            inbox,
            TestCommerceError::ParcelWork(
                "parcel inbox can only be read while inside that parcel".to_owned()
            )
        );

        let mailing_create = app
            .create_parcel_mailing_list("street-a", "P1", "owner-player", "updates", "Updates")
            .await
            .expect_err("mailing-list create outside parcel");
        assert_eq!(mailing_create, expected);

        let mailing_send = app
            .send_parcel_mailing_list_post(ParcelMailingListPostInput {
                current_view: "street-a",
                target: "P1",
                slug: "updates",
                sender_user: "owner",
                sender_player_id: "owner-player",
                subject: "Weekly Deal",
                body: "Body",
            })
            .await
            .err()
            .expect("mailing-list send outside parcel");
        assert_eq!(mailing_send, expected);

        let mailing_chat = app
            .post_parcel_mailing_list_chat(
                "street-a",
                "P1",
                "updates",
                "visitor",
                "visitor-player",
                "Hello members",
            )
            .await
            .err()
            .expect("parcel chat outside parcel");
        assert_eq!(mailing_chat, expected);

        let mailing_subscribe = app
            .subscribe_parcel_mailing_list("street-a", "P1", "updates", "visitor", "visitor-player")
            .await
            .expect_err("mailing-list subscribe outside parcel");
        assert_eq!(mailing_subscribe, expected);

        let payment = app
            .request_parcel_payment("street-a", 1, "owner-player", 25, "delivery")
            .await
            .err()
            .expect("payment request outside parcel");
        assert_eq!(payment, expected);

        let badge = app
            .create_parcel_badge(
                "street-a",
                "P1",
                "owner-player",
                "patron",
                "Good Patron",
                None,
            )
            .await
            .expect_err("badge create outside parcel");
        assert_eq!(badge, expected);

        assert!(
            app.store().calls.lock().unwrap().is_empty(),
            "outside-parcel actions must not reach storage mutation calls"
        );
    });
}

#[test]
fn parcel_inbox_only_lists_current_owned_parcel() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
                parcel_id: "P1",
                view_id: "parcel-view",
                front_view_id: "street-a",
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
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });

        let inbox = app
            .parcel_inbox("parcel-view", "owner-player")
            .await
            .expect("parcel inbox inside owned parcel");

        assert!(inbox.text.contains("Parcel Inbox for P1"));
        assert_eq!(
            app.store().calls.lock().unwrap().clone(),
            vec!["operator-list:P1:owner-player".to_owned()]
        );

        let denied = app
            .parcel_inbox("parcel-view", "visitor-player")
            .await
            .expect_err("non-owner cannot read parcel inbox");
        assert_eq!(
            denied,
            TestCommerceError::ParcelWork(
                "parcel inbox can only be read by the parcel owner inside that parcel".to_owned()
            )
        );
    });
}

#[test]
fn parcel_badge_service_commands_render_expected_text() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestParcelFixtureStore {
            parcel: Mutex::new(TestParcel {
                parcel_id: "P1",
                view_id: "parcel-view",
                front_view_id: "street-a",
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
                custom_commands: None,
            }),
            calls: Mutex::new(Vec::new()),
        });

        let created = app
            .create_parcel_badge(
                "parcel-view",
                "P1",
                "owner-player",
                "patron",
                "Good Patron",
                Some("Paid and polite"),
            )
            .await
            .expect("create badge");
        assert!(created.text.contains("Saved badge patron for parcel P1"));
        assert!(
            created
                .text
                .contains("/parcel badge award P1 patron <user> [note]")
        );

        let list = app
            .list_parcel_badges("parcel-view", "P1", "owner-player")
            .await
            .expect("list badges");
        assert!(list.text.contains("Parcel Badges for P1"));
        assert!(list.text.contains("Good Patron"));

        let award = app
            .award_parcel_badge(ParcelBadgeAwardInput {
                current_view: "parcel-view",
                parcel_id: "P1",
                slug: "patron",
                issuer_user: "owner",
                issuer_player_id: "owner-player",
                target: "visitor",
                note: Some("great work"),
            })
            .await
            .expect("award badge");
        assert!(award.text.contains("Awarded badge Good Patron (patron)"));
        assert!(award.text.contains("Issued: awarded by owner"));

        let badges = app
            .player_badges("visitor", "visitor-player")
            .await
            .expect("player badges");
        assert!(badges.text.contains("Badges for visitor"));
        assert!(
            badges
                .text
                .contains("Good Patron (patron) from Parcel [P1]")
        );
        assert!(badges.text.contains("Note: great work"));

        let revoke = app
            .revoke_parcel_badge("parcel-view", "P1", "patron", "owner-player", "visitor")
            .await
            .expect("revoke badge");
        assert!(revoke.text.contains("Revoked badge Good Patron (patron)"));
    });
}
