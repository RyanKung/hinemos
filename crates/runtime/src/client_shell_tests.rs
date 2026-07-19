use std::collections::HashMap;

use hinemos_core::{
    ActionKind, BadgeAction, BuildAction, Direction, EntityKind, EntityObservation, EntityRef,
    Gender, GridRoad, InboxAction, JsonObservation, MbtiType, ObservationEvent, ParcelAction,
    ParcelBadgeAction, ParcelDeskAction, ParcelMailingListAction, ParcelRouteAction,
    ParcelShiftAction, ParcelStaffAction, ParcelWorkAction, SemanticCommand, SettingsAction,
};

use super::{Chrome, SlashParseError, render_text_observation};

#[test]
fn text_renderer_highlights_player_marker() {
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Island Harbor Crossing".to_owned(),
        ascii_art: vec!["west --- <Me> --- east".to_owned()],
        description: "A crossing.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: vec![ObservationEvent::Message {
            text: "hello".to_owned(),
        }],
    });

    assert!(rendered.contains(Chrome::ANSI_PLAYER_MARKER));
    assert!(rendered.contains("<Me>"));
    assert!(rendered.contains(Chrome::ANSI_RESET));
}

#[test]
fn text_renderer_distinguishes_place_and_item_markers() {
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Island Harbor Crossing".to_owned(),
        ascii_art: vec!["[Workparcel] -- {bulletin board}".to_owned()],
        description: "A crossing.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    });

    assert!(rendered.contains(Chrome::ANSI_PLACE_MARKER));
    assert!(rendered.contains("[Workparcel]"));
    assert!(rendered.contains(Chrome::ANSI_ITEM_MARKER));
    assert!(rendered.contains("{bulletin board}"));
}

#[test]
fn text_renderer_renders_generated_grid_ascii_without_header_artifacts() {
    let view = GridRoad::new(1, 0).expect("valid generated road").to_view();
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: view.id,
        title: view.title,
        ascii_art: view.ascii_art,
        description: view.description,
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    });

    assert!(rendered.contains("E1-C0-01"));
    assert!(rendered.contains("<Me>"));
    assert!(!rendered.contains("------------------------------------------------------------"));
    assert!(!rendered.contains("EAST 1 RD."));
}

#[test]
fn text_renderer_shows_events_before_room_context() {
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "north_parcel_01".to_owned(),
        title: "North Parcel 01".to_owned(),
        ascii_art: Vec::new(),
        description: "A parcel.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: vec![ObservationEvent::Move {
            from: "arrival_street".to_owned(),
            to: "north_parcel_01".to_owned(),
            direction: Direction::North,
        }],
    });

    let move_index = rendered.find("You go north").expect("move result");
    let title_index = rendered.find("North Parcel 01").expect("room title");
    assert!(move_index < title_index);
}

#[test]
fn text_renderer_lists_executable_entity_commands() {
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Island Harbor Crossing".to_owned(),
        ascii_art: Vec::new(),
        description: "A crossing.".to_owned(),
        exits: Vec::new(),
        entities: vec![EntityObservation {
            id: "cyber_scroll_board".to_owned(),
            kind: EntityKind::Object,
            name: "bulletin board".to_owned(),
            description: "A board.".to_owned(),
            actions: vec![ActionKind::Inspect, ActionKind::Read],
        }],
        online_users: Vec::new(),
        available_commands: vec![
            SemanticCommand::Inspect {
                target: EntityRef::new("cyber_scroll_board"),
            },
            SemanticCommand::Read {
                target: EntityRef::new("cyber_scroll_board"),
            },
        ],
        events: Vec::new(),
    });

    assert!(rendered.contains("/inspect cyber_scroll_board"));
    assert!(rendered.contains("read: /read"));
    assert!(!rendered.contains("/read cyber_scroll_board"));
    assert!(!rendered.contains("interact: bulletin board"));
}

#[test]
fn text_renderer_splits_move_and_enter_commands() {
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "street_north_01".to_owned(),
        title: "North Island Market Path 01".to_owned(),
        ascii_art: Vec::new(),
        description: "A street.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: vec![
            SemanticCommand::Move {
                direction: Direction::North,
            },
            SemanticCommand::Enter {
                target: "north_01".to_owned(),
            },
        ],
        events: Vec::new(),
    });

    assert!(rendered.contains("Available:\n- move: /go north\n- enter: /enter north_01\n"));
    assert!(!rendered.contains("move: /go north, /enter north_01"));
    assert!(!rendered.contains("Available: "));
    assert!(!rendered.contains("; "));
}

#[test]
fn slash_parser_accepts_bare_agreement() {
    let command = Chrome::with_aliases(HashMap::new())
        .parse_command("/agree")
        .expect("agree parses");

    assert_eq!(
        command,
        SemanticCommand::Agree {
            phrase: String::new()
        }
    );
}

#[test]
fn text_renderer_shows_agreement_command_when_available() {
    let rendered = render_text_observation(&JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Arrival Hall Crossroads".to_owned(),
        ascii_art: Vec::new(),
        description: "Admission pending.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: vec![
            SemanticCommand::Look,
            SemanticCommand::Read {
                target: EntityRef::new("cyber_scroll_board"),
            },
            SemanticCommand::Agree {
                phrase: String::new(),
            },
        ],
        events: Vec::new(),
    });

    assert!(rendered.contains("read: /read agreement"));
    assert!(rendered.contains("/agree"));
    assert!(!rendered.contains("/map"));
    assert!(!rendered.contains("/history"));
}

#[test]
fn slash_parser_accepts_read_without_argument_when_single_target_available() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "arrival_street".to_owned(),
        title: "Island Harbor Crossing".to_owned(),
        ascii_art: Vec::new(),
        description: "A crossing.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: vec![SemanticCommand::Read {
            target: EntityRef::new("cyber_scroll_board"),
        }],
        events: Vec::new(),
    };

    let command = chrome
        .parse_command_with_observation("/read", Some(&observation))
        .expect("read parses with implicit target");

    assert_eq!(
        command,
        SemanticCommand::Read {
            target: EntityRef::new("cyber_scroll_board"),
        }
    );
}

#[test]
fn slash_parser_accepts_build_json() {
    let command = Chrome::with_aliases(HashMap::new())
            .parse_command(
                "/parcel build {\"title\":\"Tool Broker\",\"description\":\"Simple tools\",\"style\":\"ledger\",\"prompt\":\"reply tersely\"}",
            )
            .expect("build json parses");

    let SemanticCommand::Parcel {
        action: ParcelAction::Build {
            action: BuildAction::Apply { sheet },
        },
    } = command
    else {
        panic!("expected build sheet");
    };
    assert_eq!(sheet.title.as_deref(), Some("Tool Broker"));
    assert_eq!(sheet.description.as_deref(), Some("Simple tools"));
    assert_eq!(sheet.style.as_deref(), Some("ledger"));
    assert_eq!(sheet.prompt.as_deref(), Some("reply tersely"));
    assert_eq!(sheet.commands, None);
}

#[test]
fn slash_parser_accepts_enter_target_with_spaces() {
    let command = Chrome::with_aliases(HashMap::new())
        .parse_command("/enter Offline Tool Broker")
        .expect("enter parses");

    assert_eq!(
        command,
        SemanticCommand::Enter {
            target: "Offline Tool Broker".to_owned()
        }
    );
}

#[test]
fn slash_parser_accepts_inbox_actions() {
    let command = Chrome::with_aliases(HashMap::new())
        .parse_command("/mail claim 42")
        .expect("mail claim parses");

    assert_eq!(
        command,
        SemanticCommand::Inbox {
            action: InboxAction::Claim { item_id: 42 }
        }
    );
}

#[test]
fn slash_parser_accepts_parcel_mailing_list_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_command("/parcel mailing-list create N1 updates Parcel Updates")
            .expect("mailing-list create parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::MailingList {
                action: ParcelMailingListAction::Create {
                    parcel_id: "N1".to_owned(),
                    slug: "updates".to_owned(),
                    title: "Parcel Updates".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel mailing-list send N1 updates Weekly Deal -- Body keeps -- text")
            .expect("mailing-list send parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::MailingList {
                action: ParcelMailingListAction::Send {
                    parcel_id: "N1".to_owned(),
                    slug: "updates".to_owned(),
                    subject: "Weekly Deal".to_owned(),
                    body: "Body keeps -- text".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel mailing-list create N1 n Tiny List")
            .expect("mailing-list create with short slug parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::MailingList {
                action: ParcelMailingListAction::Create {
                    parcel_id: "N1".to_owned(),
                    slug: "n".to_owned(),
                    title: "Tiny List".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel mailing-list send N1 n Hello -- Body")
            .expect("mailing-list send with short slug parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::MailingList {
                action: ParcelMailingListAction::Send {
                    parcel_id: "N1".to_owned(),
                    slug: "n".to_owned(),
                    subject: "Hello".to_owned(),
                    body: "Body".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel route add N1 submissions /paper submit")
            .expect("parcel route add parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Route {
                action: ParcelRouteAction::Add {
                    parcel_id: "N1".to_owned(),
                    slug: "submissions".to_owned(),
                    command_prefix: "/paper submit".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel route remove N1 submissions /paper submit")
            .expect("parcel route remove parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Route {
                action: ParcelRouteAction::Remove {
                    parcel_id: "N1".to_owned(),
                    slug: "submissions".to_owned(),
                    command_prefix: "/paper submit".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel desk create N1 submissions Submissions Desk")
            .expect("parcel desk create parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Desk {
                action: ParcelDeskAction::Create {
                    parcel_id: "N1".to_owned(),
                    slug: "submissions".to_owned(),
                    title: "Submissions Desk".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel staff add N1 submissions hermes-reporter")
            .expect("parcel staff add parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Staff {
                action: ParcelStaffAction::Add {
                    parcel_id: "N1".to_owned(),
                    slug: "submissions".to_owned(),
                    username: "hermes-reporter".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel shift start N1 submissions")
            .expect("parcel shift start parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Shift {
                action: ParcelShiftAction::Start {
                    parcel_id: "N1".to_owned(),
                    slug: "submissions".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel work list N1 submissions")
            .expect("parcel work list parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Work {
                action: ParcelWorkAction::List {
                    parcel_id: "N1".to_owned(),
                    slug: Some("submissions".to_owned())
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel work done N1 42 -- accepted for daily issue")
            .expect("parcel work done parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Work {
                action: ParcelWorkAction::Done {
                    parcel_id: "N1".to_owned(),
                    work_id: 42,
                    result: "accepted for daily issue".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel chat Offline Tool Broker updates -- Hello members")
            .expect("parcel chat parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Chat {
                target: "Offline Tool Broker".to_owned(),
                slug: "updates".to_owned(),
                body: "Hello members".to_owned()
            }
        }
    );
}

#[test]
fn slash_parser_accepts_parcel_badge_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_command("/parcel badge list N1")
            .expect("parcel badge list parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Badge {
                action: ParcelBadgeAction::List {
                    parcel_id: "N1".to_owned()
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command(
                "/parcel badge create N1 reliable-worker Reliable Worker -- Finished cleanly"
            )
            .expect("parcel badge create parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Badge {
                action: ParcelBadgeAction::Create {
                    parcel_id: "N1".to_owned(),
                    slug: "reliable-worker".to_owned(),
                    title: "Reliable Worker".to_owned(),
                    description: Some("Finished cleanly".to_owned())
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel badge award N1 reliable-worker ada paid invoice 42")
            .expect("parcel badge award parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Badge {
                action: ParcelBadgeAction::Award {
                    parcel_id: "N1".to_owned(),
                    slug: "reliable-worker".to_owned(),
                    target: "ada".to_owned(),
                    note: Some("paid invoice 42".to_owned())
                }
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel badge revoke N1 reliable-worker ada")
            .expect("parcel badge revoke parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Badge {
                action: ParcelBadgeAction::Revoke {
                    parcel_id: "N1".to_owned(),
                    slug: "reliable-worker".to_owned(),
                    target: "ada".to_owned()
                }
            }
        }
    );
}

#[test]
fn slash_parser_accepts_badge_lookup_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome.parse_command("/badges").expect("badges parses"),
        SemanticCommand::Badges {
            action: BadgeAction::ListMine
        }
    );
    assert_eq!(
        chrome
            .parse_command("/badges Ada Lovelace")
            .expect("badge lookup parses"),
        SemanticCommand::Badges {
            action: BadgeAction::ListUser {
                target: "Ada Lovelace".to_owned()
            }
        }
    );
}

#[test]
fn slash_parser_accepts_parcel_subscription_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_command("/parcel subscribe N1 updates")
            .expect("subscribe parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Subscribe {
                target: "N1".to_owned(),
                slug: "updates".to_owned()
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel unsubscribe N1 updates")
            .expect("unsubscribe parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Unsubscribe {
                target: "N1".to_owned(),
                slug: "updates".to_owned()
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel subscriptions")
            .expect("subscriptions parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Subscriptions
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel subscribe Offline Tool Broker updates")
            .expect("subscribe with parcel title parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Subscribe {
                target: "Offline Tool Broker".to_owned(),
                slug: "updates".to_owned()
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/parcel unsubscribe Offline Tool Broker updates")
            .expect("unsubscribe with parcel title parses"),
        SemanticCommand::Parcel {
            action: ParcelAction::Unsubscribe {
                target: "Offline Tool Broker".to_owned(),
                slug: "updates".to_owned()
            }
        }
    );
}

#[test]
fn slash_parser_accepts_settings_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome.parse_command("/settings").expect("settings parses"),
        SemanticCommand::Settings {
            action: SettingsAction::Show
        }
    );
    assert_eq!(
        chrome
            .parse_command("/settings mail-token")
            .expect("mail token setting parses"),
        SemanticCommand::Settings {
            action: SettingsAction::MailToken
        }
    );
    assert_eq!(
        chrome.parse_command("/settings mail-token extra"),
        Err(SlashParseError::UnexpectedArgument)
    );
    assert_eq!(
        chrome
            .parse_command("/settings name Ada Lovelace")
            .expect("role-card name setting parses"),
        SemanticCommand::Settings {
            action: SettingsAction::Name {
                name: "Ada Lovelace".to_owned()
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/settings gender Female")
            .expect("role-card gender setting parses"),
        SemanticCommand::Settings {
            action: SettingsAction::Gender {
                gender: Gender::Female
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/settings mbti infp")
            .expect("role-card MBTI setting parses"),
        SemanticCommand::Settings {
            action: SettingsAction::Mbti {
                mbti: MbtiType::Infp
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/settings intro Building quiet tools")
            .expect("role-card intro setting parses"),
        SemanticCommand::Settings {
            action: SettingsAction::Intro {
                intro: Some("Building quiet tools".to_owned())
            }
        }
    );
    assert_eq!(
        chrome
            .parse_command("/settings intro clear")
            .expect("role-card intro clear parses"),
        SemanticCommand::Settings {
            action: SettingsAction::Intro { intro: None }
        }
    );
    assert_eq!(
        chrome.parse_command("/settings gender robot"),
        Err(SlashParseError::InvalidGender)
    );
    assert_eq!(
        chrome.parse_command("/settings mbti ABCD"),
        Err(SlashParseError::InvalidMbti)
    );
}

#[test]
fn slash_parser_rejects_unknown_inbox_filter() {
    let error = Chrome::with_aliases(HashMap::new())
        .parse_command("/mail list stale")
        .expect_err("unknown inbox filter is rejected");

    assert_eq!(error, SlashParseError::InvalidInboxFilter);
}

#[test]
fn player_input_preserves_slash_command_behavior() {
    let command = Chrome::with_aliases(HashMap::new())
        .parse_player_input_with_observation("/go north", None)
        .expect("slash command still parses");

    assert_eq!(
        command,
        SemanticCommand::Move {
            direction: Direction::North
        }
    );
}

#[test]
fn natural_parser_accepts_chinese_and_english_movement() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_player_input_with_observation("往北", None)
            .expect("Chinese direction parses"),
        SemanticCommand::Move {
            direction: Direction::North
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("go east", None)
            .expect("English direction parses"),
        SemanticCommand::Move {
            direction: Direction::East
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("go to north", None)
            .expect("English go-to direction parses"),
        SemanticCommand::Move {
            direction: Direction::North
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("move to west", None)
            .expect("English move-to direction parses"),
        SemanticCommand::Move {
            direction: Direction::West
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("walk south", None)
            .expect("English walk direction parses"),
        SemanticCommand::Move {
            direction: Direction::South
        }
    );
}

#[test]
fn natural_parser_accepts_japanese_movement() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_player_input_with_observation("北へ", None)
            .expect("Japanese north parses"),
        SemanticCommand::Move {
            direction: Direction::North
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("下へ", None)
            .expect("Japanese down parses"),
        SemanticCommand::Move {
            direction: Direction::Down
        }
    );
}

#[test]
fn natural_parser_accepts_simple_global_commands() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_player_input_with_observation("查看背包", None)
            .expect("inventory parses"),
        SemanticCommand::Inventory
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("打开地图", None)
            .expect("map parses"),
        SemanticCommand::Map
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("可以做什么", None)
            .expect("help parses"),
        SemanticCommand::Help
    );
}

#[test]
fn natural_parser_accepts_japanese_global_commands() {
    let chrome = Chrome::with_aliases(HashMap::new());

    assert_eq!(
        chrome
            .parse_player_input_with_observation("持ち物を見る", None)
            .expect("Japanese inventory parses"),
        SemanticCommand::Inventory
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("地図を開く", None)
            .expect("Japanese map parses"),
        SemanticCommand::Map
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("ヘルプ", None)
            .expect("Japanese help parses"),
        SemanticCommand::Help
    );
}

#[test]
fn natural_parser_maps_visible_entity_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = natural_parser_observation();

    assert_eq!(
        chrome
            .parse_player_input_with_observation("拿起 red key", Some(&observation))
            .expect("take parses"),
        SemanticCommand::Take {
            target: EntityRef::new("red_key")
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("阅读 bulletin board", Some(&observation))
            .expect("read parses"),
        SemanticCommand::Read {
            target: EntityRef::new("bulletin_board")
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("和 keeper 聊天", Some(&observation))
            .expect("talk parses"),
        SemanticCommand::Talk {
            target: EntityRef::new("keeper")
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("检查 wooden door", Some(&observation))
            .expect("inspect parses"),
        SemanticCommand::Inspect {
            target: EntityRef::new("wooden_door")
        }
    );
}

#[test]
fn natural_parser_maps_japanese_visible_entity_actions() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = natural_parser_observation();

    assert_eq!(
        chrome
            .parse_player_input_with_observation("red key を拾う", Some(&observation))
            .expect("Japanese take parses"),
        SemanticCommand::Take {
            target: EntityRef::new("red_key")
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("bulletin board を読む", Some(&observation))
            .expect("Japanese read parses"),
        SemanticCommand::Read {
            target: EntityRef::new("bulletin_board")
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("keeper に話しかける", Some(&observation))
            .expect("Japanese talk parses"),
        SemanticCommand::Talk {
            target: EntityRef::new("keeper")
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("wooden door を調べる", Some(&observation))
            .expect("Japanese inspect parses"),
        SemanticCommand::Inspect {
            target: EntityRef::new("wooden_door")
        }
    );
}

#[test]
fn natural_parser_maps_enter_to_available_enter_command() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = natural_parser_observation();

    assert_eq!(
        chrome
            .parse_player_input_with_observation("进入 north_01", Some(&observation))
            .expect("enter parses"),
        SemanticCommand::Enter {
            target: "north_01".to_owned()
        }
    );
}

#[test]
fn natural_parser_maps_japanese_enter_to_available_enter_command() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = natural_parser_observation();

    assert_eq!(
        chrome
            .parse_player_input_with_observation("north_01 に入る", Some(&observation))
            .expect("Japanese enter parses"),
        SemanticCommand::Enter {
            target: "north_01".to_owned()
        }
    );
}

#[test]
fn natural_parser_rejects_unavailable_entity_action() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = natural_parser_observation();

    let error = chrome
        .parse_player_input_with_observation("拿起 wooden door", Some(&observation))
        .expect_err("take is not available for the door");

    assert_eq!(error, SlashParseError::UnknownCommand);
}

#[test]
fn natural_parser_rejects_ambiguous_entity_target() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let observation = JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "test_room".to_owned(),
        title: "Test Room".to_owned(),
        ascii_art: Vec::new(),
        description: "A room.".to_owned(),
        exits: Vec::new(),
        entities: vec![
            EntityObservation {
                id: "red_key".to_owned(),
                kind: EntityKind::Item,
                name: "red key".to_owned(),
                description: "A red key.".to_owned(),
                actions: vec![ActionKind::Take],
            },
            EntityObservation {
                id: "blue_key".to_owned(),
                kind: EntityKind::Item,
                name: "blue key".to_owned(),
                description: "A blue key.".to_owned(),
                actions: vec![ActionKind::Take],
            },
        ],
        online_users: Vec::new(),
        available_commands: vec![
            SemanticCommand::Take {
                target: EntityRef::new("red_key"),
            },
            SemanticCommand::Take {
                target: EntityRef::new("blue_key"),
            },
        ],
        events: Vec::new(),
    };

    let error = chrome
        .parse_player_input_with_observation("拿 key", Some(&observation))
        .expect_err("ambiguous target is rejected");

    assert_eq!(error, SlashParseError::UnknownCommand);
}

#[test]
fn natural_parser_accepts_explicit_say_prefix() {
    let chrome = Chrome::with_aliases(HashMap::new());
    let command = chrome
        .parse_player_input_with_observation("说 hello world", None)
        .expect("say parses");

    assert_eq!(
        command,
        SemanticCommand::Say {
            text: "hello world".to_owned()
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("说：hello world", None)
            .expect("colon say parses"),
        SemanticCommand::Say {
            text: "hello world".to_owned()
        }
    );
    assert_eq!(
        chrome
            .parse_player_input_with_observation("言う：hello world", None)
            .expect("Japanese say parses"),
        SemanticCommand::Say {
            text: "hello world".to_owned()
        }
    );
}

#[test]
fn slash_prefixed_japanese_input_does_not_trigger_natural_parser() {
    let observation = natural_parser_observation();
    let error = Chrome::with_aliases(HashMap::new())
        .parse_player_input_with_observation("/red key を拾う", Some(&observation))
        .expect_err("slash input stays in slash parser");

    assert_eq!(error, SlashParseError::UnknownCommand);
}

fn natural_parser_observation() -> JsonObservation {
    JsonObservation {
        player_id: "local_player".to_owned(),
        view_id: "test_room".to_owned(),
        title: "Test Room".to_owned(),
        ascii_art: Vec::new(),
        description: "A room.".to_owned(),
        exits: Vec::new(),
        entities: vec![
            EntityObservation {
                id: "red_key".to_owned(),
                kind: EntityKind::Item,
                name: "red key".to_owned(),
                description: "A red key.".to_owned(),
                actions: vec![ActionKind::Take, ActionKind::Inspect],
            },
            EntityObservation {
                id: "bulletin_board".to_owned(),
                kind: EntityKind::Object,
                name: "bulletin board".to_owned(),
                description: "A board.".to_owned(),
                actions: vec![ActionKind::Read, ActionKind::Inspect],
            },
            EntityObservation {
                id: "keeper".to_owned(),
                kind: EntityKind::Npc,
                name: "keeper".to_owned(),
                description: "A keeper.".to_owned(),
                actions: vec![ActionKind::Talk, ActionKind::Inspect],
            },
            EntityObservation {
                id: "wooden_door".to_owned(),
                kind: EntityKind::Object,
                name: "wooden door".to_owned(),
                description: "A door.".to_owned(),
                actions: vec![ActionKind::Inspect],
            },
        ],
        online_users: Vec::new(),
        available_commands: vec![
            SemanticCommand::Take {
                target: EntityRef::new("red_key"),
            },
            SemanticCommand::Read {
                target: EntityRef::new("bulletin_board"),
            },
            SemanticCommand::Talk {
                target: EntityRef::new("keeper"),
            },
            SemanticCommand::Inspect {
                target: EntityRef::new("wooden_door"),
            },
            SemanticCommand::Enter {
                target: "north_01".to_owned(),
            },
        ],
        events: Vec::new(),
    }
}
