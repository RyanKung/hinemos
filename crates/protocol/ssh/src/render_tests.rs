use hinemos_core::JsonObservation;
use hinemos_runtime::render_text_observation;
use hinemos_storage::{StoredInboxItem, StoredParcel};

use super::{
    apply_auto_ascii_map, overlay_parcel_observation, overlay_street_parcels,
    render_inbox_new_notice, render_parcel_list,
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

    apply_auto_ascii_map(&mut observation, 72);

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
        status: "unread".to_owned(),
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

    overlay_parcel_observation(&mut observation, &parcel);
    let rendered = render_text_observation(&observation);

    assert!(rendered.contains("Offline Tool Broker"));
    assert!(rendered.contains("[Offline Tool Broker]"));
    assert!(rendered.contains("Shop commands: /hello - hello, price 25"));
    assert!(!rendered.contains("Custom commands: /hello preview=hello price=25"));
    assert!(!rendered.contains("NORTH COMMERCIAL PARCEL 01"));
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

    overlay_street_parcels(&mut observation, &[&parcel]);
    let rendered = render_text_observation(&observation);

    assert!(rendered.contains("[Corall牛比站]"));
    assert!(!rendered.contains("[north_01]"));
    assert!(rendered.contains("Enter: /enter north_01."));
}

#[test]
fn parcel_list_renders_status_for_humans() {
    let parcels = vec![
        StoredParcel {
            parcel_id: "north_01".to_owned(),
            view_id: "parcel_north_01".to_owned(),
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
        },
        StoredParcel {
            parcel_id: "north_02".to_owned(),
            view_id: "parcel_north_02".to_owned(),
            district: "north".to_owned(),
            position: 2,
            owner_user: None,
            owner_player_id: None,
            room_user: None,
            room_player_id: None,
            status: "vacant".to_owned(),
            title: None,
            description: None,
            style: None,
            operator_prompt: None,
            custom_commands: None,
        },
    ];

    let rendered = render_parcel_list(&parcels);

    assert!(rendered.contains("north_01: Corall牛比站. Owner: mainiu."));
    assert!(rendered.contains("north_02: vacant. Claim: /land claim north_02."));
    assert!(!rendered.contains("view=parcel_north_01"));
}
