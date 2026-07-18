use super::*;

#[test]
fn service_room_observation_exposes_local_and_registered_commands() {
    let app = AppService::new(TestRoomStore {
        service_room: Some(TestServiceRoom {
            view_id: "external_room",
            label: Some("External Room"),
            address: Some("ER1"),
            front_view_id: Some("arrival_street"),
            room_user: "room-user",
            status_text: None,
            custom_commands: Some("/room ask <question>;/room status"),
        }),
    });
    let observation = app.service_room_observation_for(
        "player",
        &TestServiceRoom {
            view_id: "external_room",
            label: Some("External Room"),
            address: Some("ER1"),
            front_view_id: Some("arrival_street"),
            room_user: "room-user",
            status_text: None,
            custom_commands: Some("/room ask <question>;/room status"),
        },
    );

    assert_eq!(observation.player_id, "player");
    assert_eq!(observation.view_id, "external_room");
    assert_eq!(observation.title, "External Room");
    assert_eq!(observation.exits.len(), 1);
    assert_eq!(observation.exits[0].direction, Direction::South);
    assert!(observation.exits[0].target_known);
    assert_eq!(observation.exits[0].label.as_deref(), Some("Harbor Square"));
    assert!(
        observation
            .ascii_art
            .iter()
            .any(|line| line.contains("south to Harbor Square"))
    );
    assert!(
        observation
            .available_commands
            .contains(&SemanticCommand::Move {
                direction: Direction::South
            })
    );
    assert!(
        observation
            .available_commands
            .contains(&SemanticCommand::Extension {
                name: "room".to_owned(),
                input: "/room ask <question>".to_owned()
            })
    );
    assert!(observation.description.contains("externally hosted room"));
}

#[test]
fn service_room_observation_falls_back_to_view_id_and_unknown_front_view() {
    let app = AppService::new(TestRoomStore {
        service_room: Some(TestServiceRoom {
            view_id: "external_room",
            label: None,
            address: None,
            front_view_id: None,
            room_user: "room-user",
            status_text: None,
            custom_commands: None,
        }),
    });

    let observation = app.service_room_observation_for(
        "player",
        &TestServiceRoom {
            view_id: "external_room",
            label: None,
            address: None,
            front_view_id: None,
            room_user: "room-user",
            status_text: None,
            custom_commands: None,
        },
    );

    assert_eq!(observation.title, "external_room");
    assert_eq!(observation.exits.len(), 1);
    assert!(!observation.exits[0].target_known);
    assert!(observation.exits[0].label.is_none());
    assert!(
        observation
            .available_commands
            .contains(&SemanticCommand::Move {
                direction: Direction::South
            })
    );
    assert!(
        !observation
            .available_commands
            .iter()
            .any(|command| matches!(command, SemanticCommand::Extension { .. }))
    );
}

#[test]
fn service_room_binding_helper_reads_room_binding_view() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let app = AppService::new(TestBindingOnlyRoomStore {
            room_binding: Some(TestRoomBinding {
                view_id: "external_room",
                front_entity_id: Some("front-sign"),
                address: "ER1",
                label: "External Room",
                enter_aliases: vec!["/room".to_owned()],
            }),
        });

        let room = app
            .service_room_binding_by_view("external_room")
            .await
            .expect("service room binding")
            .expect("binding");
        assert_eq!(RoomBindingEntryView::view_id(&room), "external_room");
        assert_eq!(ServiceRoomView::label(&room), Some("External Room"));
        assert_eq!(ServiceRoomView::address(&room), Some("ER1"));
        assert_eq!(
            ServiceRoomView::front_view_id(&room),
            Some("arrival_street")
        );
    });
}

#[test]
fn visible_room_enter_events_returns_none_when_no_visible_binding_matches() {
    let app = AppService::new(TestRoomStore { service_room: None });
    let bindings = vec![TestRoomBinding {
        view_id: "workshop_room",
        front_entity_id: Some("workshop_door"),
        address: "XR4",
        label: "External Workshop",
        enter_aliases: vec!["workshop".to_owned()],
    }];
    let visible_entity_ids = Vec::<String>::new();

    assert!(
        app.visible_room_enter_events("workshop", &visible_entity_ids, &bindings)
            .is_none()
    );
}

#[test]
fn unavailable_room_enter_events_explain_current_visible_entrances() {
    let app = AppService::new(TestRoomStore { service_room: None });
    let bindings = vec![TestRoomBinding {
        view_id: "studio_room",
        front_entity_id: Some("studio_front"),
        address: "XR3",
        label: "External Studio",
        enter_aliases: vec!["studio".to_owned()],
    }];
    let visible_entity_ids = vec!["studio_front".to_owned()];

    let events = app.unavailable_room_enter_events(
        "archive",
        "East Hinemos Blvd",
        &visible_entity_ids,
        &bindings,
    );

    assert_eq!(
        events,
        vec![UiEvent::Text(
            "No entrance named archive is visible from East Hinemos Blvd. Available entrances here: /enter XR3.\r\n"
                .to_owned()
        )]
    );
}

#[test]
fn unavailable_room_enter_events_explain_when_no_entrances_are_visible() {
    let app = AppService::new(TestRoomStore { service_room: None });
    let bindings = Vec::<TestRoomBinding>::new();
    let visible_entity_ids = Vec::<String>::new();

    let events = app.unavailable_room_enter_events(
        "workers",
        "Harbor Square",
        &visible_entity_ids,
        &bindings,
    );

    assert_eq!(
        events,
        vec![UiEvent::Text(
            "No entrance named workers is visible from Harbor Square. Move with /go until the place appears in Available.\r\n"
                .to_owned()
        )]
    );
}

#[test]
fn room_binding_accepts_input_honors_forward_all_and_prefix_matching() {
    struct ForwardAllBinding;

    impl RoomCommandPolicyView for ForwardAllBinding {
        fn forwards_all_input(&self) -> bool {
            true
        }

        fn listed_commands(&self) -> &[String] {
            &[]
        }
    }

    let app = AppService::new(TestRoomStore { service_room: None });
    let policy_binding = ForwardAllBinding;
    assert!(app.room_binding_accepts_input(&policy_binding, "/anything"));

    let binding = TestRoomBinding {
        view_id: "workshop_room",
        front_entity_id: Some("workshop_door"),
        address: "XR4",
        label: "External Workshop",
        enter_aliases: vec!["/Room Status".to_owned(), "/room ask".to_owned()],
    };
    assert!(app.room_binding_accepts_input(&binding, "/ROOM STATUS"));
    assert!(app.room_binding_accepts_input(&binding, "/room ask question"));
    assert!(app.room_binding_accepts_input(&binding, "   /ROOM ASK question"));
    assert!(app.room_binding_accepts_input(&binding, "/ROOM ASK question"));
    assert!(!app.room_binding_accepts_input(&binding, "/room look"));

    struct TemplateBinding {
        commands: Vec<String>,
    }

    impl RoomCommandPolicyView for TemplateBinding {
        fn forwards_all_input(&self) -> bool {
            false
        }

        fn listed_commands(&self) -> &[String] {
            &self.commands
        }
    }

    let template_binding = TemplateBinding {
        commands: vec!["/room ask <question>".to_owned(), "/room status".to_owned()],
    };
    assert!(app.room_binding_accepts_input(&template_binding, "/room ask hello"));
    assert!(app.room_binding_accepts_input(&template_binding, "/ROOM ASK hello"));
    assert!(app.room_binding_accepts_input(&template_binding, "/room status"));
    assert!(!app.room_binding_accepts_input(&template_binding, "/help"));
}

#[test]
fn service_room_command_for_binding_say_routes_through_app() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestServiceRoomCommandStore {
            service_room: Some(TestServiceRoom {
                view_id: "external_room",
                label: Some("External Room"),
                address: Some("ER1"),
                front_view_id: Some("arrival_street"),
                room_user: "room-user",
                status_text: None,
                custom_commands: Some("/room ask <question>;/room status"),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let app = AppService::new(store);
        let identity = AppIdentity::new("alice", "player-1");
        let binding = TestServiceRoom {
            view_id: "external_room",
            label: Some("External Room"),
            address: Some("ER1"),
            front_view_id: Some("arrival_street"),
            room_user: "room-user",
            status_text: None,
            custom_commands: Some("/room ask <question>;/room status"),
        };

        let events = app
            .handle_service_room_command_for_binding(
                &identity,
                "external_room",
                &binding,
                &SemanticCommand::Say {
                    text: "hello".to_owned(),
                },
            )
            .await
            .expect("service room say");

        assert_eq!(
            events,
            vec![
                UiEvent::Text(
                    "You say: hello\r\nSent to room service room-user (request #17). Replies arrive in your mailbox with subject Re: #17; use /mailbox, then /mail read <inbox-id> for that reply.\r\n"
                        .to_owned()
                ),
                UiEvent::LiveViewMessage {
                    view_id: "external_room".to_owned(),
                    text: "[say from alice] hello".to_owned(),
                },
            ]
        );
        assert_eq!(
            app.store().calls.lock().unwrap().clone(),
            vec!["mailbox:external_room:alice:player-1:/say hello".to_owned()]
        );
    });
}

#[test]
fn service_room_command_for_binding_quit_closes_session() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let store = TestServiceRoomCommandStore {
            service_room: Some(TestServiceRoom {
                view_id: "external_room",
                label: Some("External Room"),
                address: Some("ER1"),
                front_view_id: Some("arrival_street"),
                room_user: "room-user",
                status_text: None,
                custom_commands: Some("/room ask <question>;/room status"),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let app = AppService::new(store);
        let identity = AppIdentity::new("alice", "player-1");
        let binding = TestServiceRoom {
            view_id: "external_room",
            label: Some("External Room"),
            address: Some("ER1"),
            front_view_id: Some("arrival_street"),
            room_user: "room-user",
            status_text: None,
            custom_commands: Some("/room ask <question>;/room status"),
        };

        let events = app
            .handle_service_room_command_for_binding(
                &identity,
                "external_room",
                &binding,
                &SemanticCommand::Quit,
            )
            .await
            .expect("service room quit");

        assert_eq!(
            events,
            vec![
                UiEvent::Text(format!("{}\r\n", hinemos_core::FEEDBACK_QUIT)),
                UiEvent::CloseSession(0),
            ]
        );
    });
}

#[test]
fn room_binding_enter_matching_uses_explicit_tokens_and_visibility() {
    let app = AppService::new(TestRoomStore { service_room: None });
    let binding = TestRoomBinding {
        view_id: "workshop_room",
        front_entity_id: Some("workshop_door"),
        address: "XR4",
        label: "External Workshop",
        enter_aliases: vec!["workshop".to_owned()],
    };

    assert!(app.room_binding_enter_matches(&binding, &app.normalize_enter_target("xr4")));
    assert!(
        app.room_binding_enter_matches(&binding, &app.normalize_enter_target("External Workshop"))
    );
    assert!(app.room_binding_enter_matches(&binding, &app.normalize_enter_target("workshop")));
    assert!(!app.room_binding_enter_matches(&binding, &app.normalize_enter_target("room")));
    assert!(app.room_binding_is_visible(&binding, &["workshop_door".to_owned()]));
    assert!(!app.room_binding_is_visible(&binding, &["other_door".to_owned()]));
}
