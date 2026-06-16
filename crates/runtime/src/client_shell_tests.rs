use std::collections::HashMap;

use hinemos_core::{
    ActionKind, BuildAction, Direction, EntityKind, EntityObservation, EntityRef, InboxAction,
    JsonObservation, ObservationEvent, SemanticCommand, SettingsAction,
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
        ascii_art: vec!["[Workshop] -- {bulletin board}".to_owned()],
        description: "A crossing.".to_owned(),
        exits: Vec::new(),
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands: Vec::new(),
        events: Vec::new(),
    });

    assert!(rendered.contains(Chrome::ANSI_PLACE_MARKER));
    assert!(rendered.contains("[Workshop]"));
    assert!(rendered.contains(Chrome::ANSI_ITEM_MARKER));
    assert!(rendered.contains("{bulletin board}"));
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

    assert!(rendered.contains("read: /read"));
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
                "/build {\"title\":\"Tool Broker\",\"description\":\"Simple tools\",\"style\":\"ledger\",\"prompt\":\"reply tersely\"}",
            )
            .expect("build json parses");

    let SemanticCommand::Build {
        action: BuildAction::Apply { sheet },
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
