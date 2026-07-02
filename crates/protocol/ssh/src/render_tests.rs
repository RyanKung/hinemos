use hinemos_core::{
    ActionKind, DEFAULT_ADMISSION_VIEW_ID, EntityKind, EntityObservation, JsonObservation,
    PARCEL_STATUS_BUILT, SHOP_MAILING_LIST_STATUS_CLOSED, SHOP_MAILING_LIST_STATUS_OPEN,
    SemanticCommand, SubscriptionAction,
};
use hinemos_runtime::render_text_observation;
use hinemos_storage::{
    INBOX_STATUS_UNREAD, StoredInboxItem, StoredParcel, StoredRoomBinding, StoredRoomBindingKind,
    StoredRoomCommandPolicy, StoredShopMailingList,
};

use super::{
    apply_auto_ascii_map, overlay_parcel_observation, overlay_room_binding_entries,
    overlay_service_room, render_inbox_new_notice, room_reply_live_notice, room_reply_request_id,
};

#[test]
fn fixed_indoor_views_keep_authored_ascii_map() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "service_room".to_owned(),
        title: "Service Room".to_owned(),
        ascii_art: vec![
            "                ╡ SERVICE ROOM ╞".to_owned(),
            "                           {service menu}".to_owned(),
            "            [cedar counter] ════ <Me>".to_owned(),
            "                         room operator".to_owned(),
            "                            south to street".to_owned(),
        ],
        description: "A compact externally hosted service room.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };

    apply_auto_ascii_map(&mut observation, 72, DEFAULT_ADMISSION_VIEW_ID);

    assert!(
        observation
            .ascii_art
            .iter()
            .any(|line| line.contains("SERVICE ROOM"))
    );
    assert!(
        observation
            .ascii_art
            .iter()
            .any(|line| line.contains("[cedar counter] ════ <Me>"))
    );
    assert!(
        !observation
            .ascii_art
            .iter()
            .any(|line| line.contains("MAP: BUILDING"))
    );
    assert!(
        !observation
            .ascii_art
            .iter()
            .any(|line| line.contains("[<Me>]"))
    );
}

#[test]
fn admission_view_map_uses_configured_anchor() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "custom_admission".to_owned(),
        title: "Arrival Street".to_owned(),
        ascii_art: Vec::new(),
        description: "A street entrance.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };

    apply_auto_ascii_map(&mut observation, 72, "custom_admission");

    assert!(!observation.ascii_art.is_empty());
    assert!(
        observation
            .ascii_art
            .iter()
            .any(|line| line.contains("[<Me>]"))
    );
    assert!(
        observation
            .ascii_art
            .iter()
            .any(|line| line.contains("north"))
    );
}

#[test]
fn live_mail_notice_shows_message_body_and_mailbox_persistence() {
    let item = StoredInboxItem {
        id: 42,
        kind: "mail".to_owned(),
        recipient_user: "ryan".to_owned(),
        recipient_player_id: "player".to_owned(),
        sender_user: "room-service".to_owned(),
        sender_player_id: "room:service".to_owned(),
        subject: "Room reply".to_owned(),
        body: "The room operator says hello.".to_owned(),
        source_kind: None,
        source_id: None,
        payload: serde_json::json!({}),
        status: INBOX_STATUS_UNREAD.to_owned(),
        attempts: 0,
        lease_until: None,
        created_at: "2026-06-12 00:00:00 UTC".to_owned(),
    };

    let rendered = render_inbox_new_notice(&item, None);

    assert!(rendered.contains("Mail from room-service: Room reply"));
    assert!(rendered.contains("The room operator says hello."));
    assert!(rendered.contains("saved to /mailbox as #42"));
    assert!(!rendered.contains("Use: /mail read"));
}

#[test]
fn room_reply_subject_extracts_request_id() {
    assert_eq!(room_reply_request_id("Re: #42"), Some(42));
    assert_eq!(room_reply_request_id("  Re: #7  "), Some(7));
    assert_eq!(room_reply_request_id("Room reply"), None);
}

#[test]
fn room_reply_live_notice_includes_request_id_when_available() {
    assert_eq!(
        room_reply_live_notice("Opal Desk", Some(42), "Thanks."),
        "[room Opal Desk reply #42] Thanks."
    );
    assert_eq!(
        room_reply_live_notice("Opal Desk", None, "Thanks."),
        "[room Opal Desk] Thanks."
    );
}

#[test]
fn built_parcel_replaces_static_ascii_title_with_shop_title() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "north_parcel_01".to_owned(),
        title: "North Commercial Parcel 01".to_owned(),
        ascii_art: vec![
            "               NORTH COMMERCIAL PARCEL 01".to_owned(),
            "                       |".to_owned(),
            "                    <Me>".to_owned(),
        ],
        description: "Static parcel description.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };
    let parcel = StoredParcel {
        parcel_id: "north_01".to_owned(),
        view_id: "north_parcel_01".to_owned(),
        front_view_id: "street_north_01".to_owned(),
        district: "north".to_owned(),
        position: 1,
        owner_user: Some("mainiu".to_owned()),
        owner_player_id: Some("player".to_owned()),
        room_user: Some("room-north_01".to_owned()),
        room_player_id: Some("room:north_01".to_owned()),
        status: "built".to_owned(),
        title: Some("Offline Tool Broker".to_owned()),
        description: Some("Simple tools.".to_owned()),
        style: Some("ledger".to_owned()),
        operator_prompt: Some("reply tersely".to_owned()),
        custom_commands: Some("/hello preview=hello price=25".to_owned()),
    };
    let binding = StoredRoomBinding::from_parcel(parcel);

    overlay_parcel_observation(&mut observation, &binding);
    let rendered = render_text_observation(&observation);

    assert!(rendered.contains("Offline Tool Broker"));
    assert!(rendered.contains("[Offline Tool Broker]"));
    assert!(rendered.contains("Shop commands: /hello - hello, price 25"));
    assert!(!rendered.contains("Custom commands: /hello preview=hello price=25"));
    assert!(!rendered.contains("NORTH COMMERCIAL PARCEL 01"));
}

#[test]
fn built_parcel_advertises_only_open_mailing_lists() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "north_parcel_01".to_owned(),
        title: "North Commercial Parcel 01".to_owned(),
        ascii_art: Vec::new(),
        description: "Static parcel description.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };
    let parcel = StoredParcel {
        parcel_id: "north_01".to_owned(),
        view_id: "north_parcel_01".to_owned(),
        front_view_id: "street_north_01".to_owned(),
        district: "north".to_owned(),
        position: 1,
        owner_user: Some("mainiu".to_owned()),
        owner_player_id: Some("player".to_owned()),
        room_user: Some("room-north_01".to_owned()),
        room_player_id: Some("room:north_01".to_owned()),
        status: PARCEL_STATUS_BUILT.to_owned(),
        title: Some("Offline Tool Broker".to_owned()),
        description: Some("Simple tools.".to_owned()),
        style: Some("ledger".to_owned()),
        operator_prompt: Some("reply tersely".to_owned()),
        custom_commands: None,
    };
    let mut binding = StoredRoomBinding::from_parcel(parcel);
    binding.parcel_mailing_lists = vec![
        StoredShopMailingList {
            id: 1,
            parcel_id: "north_01".to_owned(),
            owner_player_id: "player".to_owned(),
            slug: "updates".to_owned(),
            title: "Shop Updates".to_owned(),
            status: SHOP_MAILING_LIST_STATUS_OPEN.to_owned(),
            subscriber_count: 0,
            created_at: "2026-06-27 00:00:00 UTC".to_owned(),
        },
        StoredShopMailingList {
            id: 2,
            parcel_id: "north_01".to_owned(),
            owner_player_id: "player".to_owned(),
            slug: "archive".to_owned(),
            title: "Old News".to_owned(),
            status: SHOP_MAILING_LIST_STATUS_CLOSED.to_owned(),
            subscriber_count: 0,
            created_at: "2026-06-27 00:00:00 UTC".to_owned(),
        },
    ];

    overlay_parcel_observation(&mut observation, &binding);
    let rendered = render_text_observation(&observation);

    assert!(observation.description.contains(
        "Shop Updates (updates) join: /subscribe north_01 updates; chat after joining: /chat north_01 updates -- <message>"
    ));
    assert!(!rendered.contains("Old News"));
    assert!(
        observation
            .available_commands
            .contains(&SemanticCommand::Subscription {
                action: SubscriptionAction::Subscribe {
                    target: "north_01".to_owned(),
                    slug: "updates".to_owned(),
                }
            })
    );
    assert!(
        !observation
            .available_commands
            .contains(&SemanticCommand::Subscription {
                action: SubscriptionAction::Subscribe {
                    target: "north_01".to_owned(),
                    slug: "archive".to_owned(),
                }
            })
    );
}

#[test]
fn built_street_parcel_replaces_static_ascii_label_with_shop_title() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "street_north_01".to_owned(),
        title: "North Commercial Street 01".to_owned(),
        ascii_art: vec![
            "               north to street 02".to_owned(),
            "                       |".to_owned(),
            "       [north_01] --- <Me>".to_owned(),
            "                       |".to_owned(),
        ],
        description: "Static street description.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };
    let parcel = StoredParcel {
        parcel_id: "north_01".to_owned(),
        view_id: "parcel_north_01".to_owned(),
        front_view_id: "street_north_01".to_owned(),
        district: "north".to_owned(),
        position: 1,
        owner_user: Some("mainiu".to_owned()),
        owner_player_id: Some("player".to_owned()),
        room_user: Some("room-north_01".to_owned()),
        room_player_id: Some("room:north_01".to_owned()),
        status: "built".to_owned(),
        title: Some("Corall牛比站".to_owned()),
        description: Some("Simple tools.".to_owned()),
        style: Some("ledger".to_owned()),
        operator_prompt: Some("reply tersely".to_owned()),
        custom_commands: Some("/hello preview=hello price=25".to_owned()),
    };

    overlay_room_binding_entries(&mut observation, &[StoredRoomBinding::from_parcel(parcel)]);
    let rendered = render_text_observation(&observation);

    assert!(rendered.contains("[Corall牛比站]"));
    assert!(!rendered.contains("[north_01]"));
    assert!(rendered.contains("Enter: /enter north_01."));
}

#[test]
fn service_room_binding_respects_front_entity_before_overlay() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "official_street".to_owned(),
        title: "Official Street".to_owned(),
        ascii_art: Vec::new(),
        description: "Static street description.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };
    let binding = StoredRoomBinding {
        kind: StoredRoomBindingKind::ServiceRoom,
        view_id: "hinemos_bank".to_owned(),
        front_view_id: "official_street".to_owned(),
        front_entity_id: Some("bank_kiosk".to_owned()),
        address: "bank".to_owned(),
        label: "Hinemos Bank".to_owned(),
        status_text: Some("Open weekdays".to_owned()),
        custom_commands: Some("/balance".to_owned()),
        recovery_commands: None,
        entry_text: "- bank Hinemos Bank. Enter: /enter bank.".to_owned(),
        ascii_label: None,
        owner_user: None,
        parcel_status: None,
        parcel_title: None,
        parcel_description: None,
        parcel_style: None,
        parcel_operator_prompt: None,
        parcel_custom_commands: None,
        parcel_mailing_lists: Vec::new(),
        enter_aliases: vec!["bank".to_owned()],
        room_user: Some("room-bank".to_owned()),
        room_player_id: Some("room:bank".to_owned()),
        owner_player_id: None,
        command_policy: StoredRoomCommandPolicy::ForwardListed(vec!["/balance".to_owned()]),
    };

    overlay_room_binding_entries(&mut observation, std::slice::from_ref(&binding));
    assert!(!observation.description.contains("Hinemos Bank"));

    observation.entities.push(EntityObservation {
        id: "bank_kiosk".to_owned(),
        kind: EntityKind::Npc,
        name: "Bank Kiosk".to_owned(),
        description: String::new(),
        actions: vec![ActionKind::Talk],
    });
    overlay_room_binding_entries(&mut observation, &[binding]);

    assert!(observation.description.contains("Hinemos Bank"));
    assert!(render_text_observation(&observation).contains("/enter bank"));
}

#[test]
fn service_room_binding_overlays_status_without_duplicating_commands() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "external_room".to_owned(),
        title: "External Room".to_owned(),
        ascii_art: Vec::new(),
        description: "Static room description.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: vec![hinemos_core::SemanticCommand::Extension {
            name: "room".to_owned(),
            input: "/room ask <question>".to_owned(),
        }],
        events: Vec::new(),
    };
    let binding = StoredRoomBinding {
        kind: StoredRoomBindingKind::ServiceRoom,
        view_id: "external_room".to_owned(),
        front_view_id: "arrival_street".to_owned(),
        front_entity_id: None,
        address: "ER1".to_owned(),
        label: "External Room".to_owned(),
        status_text: Some("Status line".to_owned()),
        custom_commands: Some("/room ask <question>;/room status".to_owned()),
        recovery_commands: None,
        entry_text: "- ER1 External Room. Enter: /enter ER1.".to_owned(),
        ascii_label: None,
        owner_user: None,
        parcel_status: None,
        parcel_title: None,
        parcel_description: None,
        parcel_style: None,
        parcel_operator_prompt: None,
        parcel_custom_commands: None,
        parcel_mailing_lists: Vec::new(),
        enter_aliases: Vec::new(),
        room_user: Some("room-external".to_owned()),
        room_player_id: Some("room:external".to_owned()),
        owner_player_id: None,
        command_policy: StoredRoomCommandPolicy::ForwardListed(vec![
            "/room ask <question>".to_owned(),
        ]),
    };

    overlay_service_room(&mut observation, &binding);

    assert!(observation.description.contains("Status line"));
    let command_count = observation
        .available_commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                hinemos_core::SemanticCommand::Extension { input, .. }
                    if input == "/room ask <question>"
            )
        })
        .count();
    assert_eq!(command_count, 1);
}

#[test]
fn service_room_binding_without_status_keeps_description_and_no_extensions() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "external_room".to_owned(),
        title: "External Room".to_owned(),
        ascii_art: Vec::new(),
        description: "Static room description.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };
    let binding = StoredRoomBinding {
        kind: StoredRoomBindingKind::ServiceRoom,
        view_id: "external_room".to_owned(),
        front_view_id: "arrival_street".to_owned(),
        front_entity_id: None,
        address: "ER1".to_owned(),
        label: "External Room".to_owned(),
        status_text: None,
        custom_commands: None,
        recovery_commands: None,
        entry_text: "- ER1 External Room. Enter: /enter ER1.".to_owned(),
        ascii_label: None,
        owner_user: None,
        parcel_status: None,
        parcel_title: None,
        parcel_description: None,
        parcel_style: None,
        parcel_operator_prompt: None,
        parcel_custom_commands: None,
        parcel_mailing_lists: Vec::new(),
        enter_aliases: Vec::new(),
        room_user: Some("room-external".to_owned()),
        room_player_id: Some("room:external".to_owned()),
        owner_player_id: None,
        command_policy: StoredRoomCommandPolicy::ForwardListed(Vec::new()),
    };

    overlay_service_room(&mut observation, &binding);

    assert_eq!(observation.description, "Static room description.");
    assert!(observation.available_commands.is_empty());
}

#[test]
fn room_binding_entries_keep_visible_order_and_skip_hidden_front_entity() {
    let mut observation = JsonObservation {
        player_id: "player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Arrival Street".to_owned(),
        ascii_art: Vec::new(),
        description: "Street description.".to_owned(),
        exits: Vec::new(),
        entities: vec![EntityObservation {
            id: "visible_kiosk".to_owned(),
            kind: EntityKind::Npc,
            name: "Visible Kiosk".to_owned(),
            description: String::new(),
            actions: vec![ActionKind::Talk],
        }],
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    };
    let hidden_binding = StoredRoomBinding {
        kind: StoredRoomBindingKind::CommercialParcel,
        view_id: "hidden_parcel".to_owned(),
        front_view_id: "arrival_street".to_owned(),
        front_entity_id: Some("hidden_kiosk".to_owned()),
        address: "HIDDEN".to_owned(),
        label: "Hidden".to_owned(),
        status_text: None,
        custom_commands: None,
        recovery_commands: None,
        entry_text: "- hidden Hidden. Enter: /enter hidden.".to_owned(),
        ascii_label: Some("hidden".to_owned()),
        owner_user: None,
        parcel_status: None,
        parcel_title: None,
        parcel_description: None,
        parcel_style: None,
        parcel_operator_prompt: None,
        parcel_custom_commands: None,
        parcel_mailing_lists: Vec::new(),
        enter_aliases: Vec::new(),
        room_user: None,
        room_player_id: None,
        owner_player_id: None,
        command_policy: StoredRoomCommandPolicy::ForwardListed(Vec::new()),
    };
    let visible_binding = StoredRoomBinding {
        kind: StoredRoomBindingKind::CommercialParcel,
        view_id: "visible_parcel".to_owned(),
        front_view_id: "arrival_street".to_owned(),
        front_entity_id: Some("visible_kiosk".to_owned()),
        address: "VISIBLE".to_owned(),
        label: "Visible".to_owned(),
        status_text: None,
        custom_commands: None,
        recovery_commands: None,
        entry_text: "- visible Visible. Enter: /enter visible.".to_owned(),
        ascii_label: Some("visible".to_owned()),
        owner_user: None,
        parcel_status: None,
        parcel_title: None,
        parcel_description: None,
        parcel_style: None,
        parcel_operator_prompt: None,
        parcel_custom_commands: None,
        parcel_mailing_lists: Vec::new(),
        enter_aliases: Vec::new(),
        room_user: None,
        room_player_id: None,
        owner_player_id: None,
        command_policy: StoredRoomCommandPolicy::ForwardListed(Vec::new()),
    };

    overlay_room_binding_entries(&mut observation, &[hidden_binding, visible_binding]);

    assert!(observation.description.contains("Enter: /enter visible."));
    assert!(!observation.description.contains("Enter: /enter hidden."));
    assert!(
        observation
            .available_commands
            .contains(&hinemos_core::SemanticCommand::Enter {
                target: "VISIBLE".to_owned()
            })
    );
    assert!(
        !observation
            .available_commands
            .contains(&hinemos_core::SemanticCommand::Enter {
                target: "HIDDEN".to_owned()
            })
    );
}
