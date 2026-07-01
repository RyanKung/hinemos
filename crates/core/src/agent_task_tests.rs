use crate::{Direction, ExitObservation, ObservationEvent, ViewId};

use super::*;

#[test]
fn resident_task_has_default_goal_and_hunger_constraint() {
    let task = TaskMode::resident("alice");

    assert!(task.objective.starts_with("As alice, earn MARK"));
    assert_eq!(
        task.constraints.hunger,
        HungerPolicy::RequireRecoveryWhenGated
    );
    assert!(task.command_history.is_empty());
}

#[test]
fn task_mode_accepts_existing_extension_command_without_protocol_leak() {
    let task = TaskMode::new("earn MARK and build standing").expect("task");
    let observation = observation(vec![
        SemanticCommand::Say {
            text: String::new(),
        },
        SemanticCommand::Extension {
            name: "position".to_owned(),
            input: "/position start <position>".to_owned(),
        },
    ]);
    let snapshot = task.snapshot(
        &observation,
        ObservedTaskState {
            usable_mark: Some(1_000),
            bank_mark: Some(0),
            hunger: HungerSignal::Clear,
            progress_units: 0,
            ..ObservedTaskState::default()
        },
    );

    let command = task
        .validate_command(
            &snapshot,
            SemanticCommand::Extension {
                name: "position".to_owned(),
                input: "/position start greeter".to_owned(),
            },
        )
        .expect("position command is available");

    assert_eq!(command.line(), "/position start greeter");
    assert!(!command.line().contains("earn MARK"));
    assert!(!command.line().to_ascii_lowercase().contains("/plan"));
    assert!(!command.line().to_ascii_lowercase().contains("/act"));
}

#[test]
fn memory_template_authorizes_memory_subcommands() {
    let task = TaskMode::new("remember context").expect("task");
    let observation = observation(vec![SemanticCommand::Memory {
        rest: "<command>".to_owned(),
    }]);
    let snapshot = task.snapshot(&observation, ObservedTaskState::default());

    let command = task
        .validate_command(
            &snapshot,
            SemanticCommand::Memory {
                rest: "self".to_owned(),
            },
        )
        .expect("memory subcommand is available");

    assert_eq!(command.line(), "/memory self");
}

#[test]
fn hunger_gate_allows_memory_context_reads() {
    let task = TaskMode::new("remember context").expect("task");
    let observation = observation(vec![SemanticCommand::Memory {
        rest: "<command>".to_owned(),
    }]);
    let snapshot = task.snapshot(
        &observation,
        ObservedTaskState {
            hunger: HungerSignal::GatedNeedsWork,
            ..ObservedTaskState::default()
        },
    );

    let command = task
        .validate_command(
            &snapshot,
            SemanticCommand::Memory {
                rest: "self".to_owned(),
            },
        )
        .expect("memory remains available while hungry");

    assert_eq!(command.line(), "/memory self");
}

#[test]
fn hunger_gate_allows_available_room_extensions_and_rejects_ordinary_action() {
    let task = TaskMode::new("make money").expect("task");
    let observation = observation(vec![
        SemanticCommand::Say {
            text: String::new(),
        },
        SemanticCommand::Extension {
            name: "room".to_owned(),
            input: "/room recover <action>".to_owned(),
        },
    ]);
    let snapshot = task.snapshot(
        &observation,
        ObservedTaskState {
            usable_mark: Some(100),
            bank_mark: None,
            hunger: HungerSignal::GatedCanBuyFood,
            progress_units: 0,
            ..ObservedTaskState::default()
        },
    );

    let blocked = task.validate_command(
        &snapshot,
        SemanticCommand::Say {
            text: "I will keep chatting".to_owned(),
        },
    );
    let recovery = task.validate_command(
        &snapshot,
        SemanticCommand::Extension {
            name: "room".to_owned(),
            input: "/room recover food".to_owned(),
        },
    );

    assert_eq!(blocked, Err(TaskCommandError::HungerRequiresRecovery));
    assert_eq!(
        recovery.expect("available room command").line(),
        "/room recover food"
    );
}

#[test]
fn reward_uses_observed_mark_and_progress_deltas() {
    let task = TaskMode::new("save money")
        .expect("task")
        .with_reward(RewardSpec {
            mark_delta_weight: 2,
            progress_delta_weight: 5,
            social_contact_delta_weight: 7,
            standing_delta_weight: 11,
            commitment_satisfaction_delta_weight: 13,
            loneliness_relief_delta_weight: 17,
            boredom_relief_delta_weight: 19,
        });
    let before = task.snapshot(
        &observation(vec![SemanticCommand::Balance]),
        ObservedTaskState {
            usable_mark: Some(100),
            bank_mark: Some(50),
            hunger: HungerSignal::Clear,
            progress_units: 1,
            social_contact_units: Some(0),
            standing_units: Some(1),
            commitment_satisfaction_units: Some(0),
            loneliness_points: Some(7),
            boredom_points: Some(5),
        },
    );
    let after = task.snapshot(
        &observation(vec![SemanticCommand::Balance]),
        ObservedTaskState {
            usable_mark: Some(140),
            bank_mark: Some(70),
            hunger: HungerSignal::Clear,
            progress_units: 3,
            social_contact_units: Some(2),
            standing_units: Some(3),
            commitment_satisfaction_units: Some(1),
            loneliness_points: Some(4),
            boredom_points: Some(1),
        },
    );
    let command = task
        .validate_command(&before, SemanticCommand::Balance)
        .expect("balance available");

    let evaluation = task.evaluate_step(&before, command, after);

    assert_eq!(evaluation.mark_delta, 60);
    assert_eq!(evaluation.progress_delta, 2);
    assert_eq!(evaluation.social_contact_delta, 2);
    assert_eq!(evaluation.standing_delta, 2);
    assert_eq!(evaluation.commitment_satisfaction_delta, 1);
    assert_eq!(evaluation.loneliness_relief_delta, 3);
    assert_eq!(evaluation.boredom_relief_delta, 4);
    assert_eq!(evaluation.reward, 306);
}

#[test]
fn reward_prefers_social_progress_over_isolated_survival() {
    let task = TaskMode::new("build relationships, standing, and wealth").expect("task");
    let before = task.snapshot(
        &observation(vec![SemanticCommand::Say {
            text: "<text>".to_owned(),
        }]),
        ObservedTaskState {
            usable_mark: Some(100),
            hunger: HungerSignal::Clear,
            social_contact_units: Some(0),
            standing_units: Some(0),
            commitment_satisfaction_units: Some(0),
            loneliness_points: Some(4),
            boredom_points: Some(4),
            ..ObservedTaskState::default()
        },
    );
    let isolated_after = TaskSnapshot {
        usable_mark: Some(125),
        ..before.clone()
    };
    let social_after = TaskSnapshot {
        social_contact_units: Some(2),
        standing_units: Some(1),
        commitment_satisfaction_units: Some(1),
        loneliness_points: Some(2),
        boredom_points: Some(2),
        ..before.clone()
    };
    let command = task
        .validate_command(
            &before,
            SemanticCommand::Say {
                text: "hello neighbor".to_owned(),
            },
        )
        .expect("social command available");

    let isolated = task.evaluate_step(&before, command.clone(), isolated_after);
    let social = task.evaluate_step(&before, command, social_after);

    assert_eq!(isolated.reward, 25);
    assert_eq!(social.reward, 29);
    assert!(social.reward > isolated.reward);
}

#[test]
fn task_history_transcript_contains_only_world_commands() {
    let mut task = TaskMode::new("own a shop").expect("task");
    let before = task.snapshot(
        &observation(vec![SemanticCommand::Move {
            direction: Direction::North,
        }]),
        ObservedTaskState::default(),
    );
    let command = task
        .validate_command(
            &before,
            SemanticCommand::Move {
                direction: Direction::North,
            },
        )
        .expect("move available");
    let after = TaskSnapshot {
        progress_units: 1,
        ..before.clone()
    };

    let evaluation = task.evaluate_step(&before, command, after);
    task.record_step(evaluation);

    assert_eq!(task.command_transcript(), vec!["/go north"]);
    assert!(
        task.command_transcript()
            .iter()
            .all(|line| !line.contains("own a shop"))
    );
}

#[test]
fn plan_act_and_goal_json_are_rejected_as_protocol_leaks() {
    let task = TaskMode::new("become a shopkeeper").expect("task");
    let observation = observation(vec![SemanticCommand::Extension {
        name: "plan".to_owned(),
        input: "/plan <json>".to_owned(),
    }]);
    let snapshot = task.snapshot(&observation, ObservedTaskState::default());

    let plan = task.validate_command(
        &snapshot,
        SemanticCommand::Extension {
            name: "plan".to_owned(),
            input: "/plan {\"objective\":\"become a shopkeeper\"}".to_owned(),
        },
    );

    assert_eq!(plan, Err(TaskCommandError::TaskProtocolLeak));
}

#[test]
fn say_placeholder_template_authorizes_single_line_text() {
    let task = TaskMode::new("talk in the world").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![SemanticCommand::Say {
            text: "<text>".to_owned(),
        }]),
        ObservedTaskState::default(),
    );

    let command = task
        .validate_command(
            &snapshot,
            SemanticCommand::Say {
                text: "hello world".to_owned(),
            },
        )
        .expect("say placeholder accepts text");

    assert_eq!(command.line(), "/say hello world");
}

#[test]
fn multiline_commands_are_rejected_before_execution() {
    let task = TaskMode::new("avoid injected commands").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![
            SemanticCommand::Say {
                text: String::new(),
            },
            SemanticCommand::Mail {
                target: String::new(),
                text: String::new(),
            },
            SemanticCommand::Extension {
                name: "position".to_owned(),
                input: "/position start <position>".to_owned(),
            },
        ]),
        ObservedTaskState::default(),
    );

    let say = task.validate_command(
        &snapshot,
        SemanticCommand::Say {
            text: "hello\n/pay bob 10".to_owned(),
        },
    );
    let mail = task.validate_command(
        &snapshot,
        SemanticCommand::Mail {
            target: "bob".to_owned(),
            text: "hello\r\n/pay bob 10".to_owned(),
        },
    );
    let extension = task.validate_command(
        &snapshot,
        SemanticCommand::Extension {
            name: "position".to_owned(),
            input: "/position start greeter\n/pay bob 10".to_owned(),
        },
    );

    assert_eq!(say, Err(TaskCommandError::MultilineCommand));
    assert_eq!(mail, Err(TaskCommandError::MultilineCommand));
    assert_eq!(extension, Err(TaskCommandError::MultilineCommand));
}

#[test]
fn pay_requests_template_does_not_authorize_direct_payment() {
    let task = TaskMode::new("inspect payment requests").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![SemanticCommand::Pay {
            action: PayAction::Requests,
        }]),
        ObservedTaskState::default(),
    );

    let direct = task.validate_command(
        &snapshot,
        SemanticCommand::Pay {
            action: PayAction::Direct {
                target: "bob".to_owned(),
                amount: 10,
                memo: String::new(),
            },
        },
    );

    assert_eq!(direct, Err(TaskCommandError::CommandNotAvailable));
}

#[test]
fn pay_direct_template_authorizes_direct_payment_placeholders() {
    let task = TaskMode::new("pay a worker").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![SemanticCommand::Pay {
            action: PayAction::Direct {
                target: String::new(),
                amount: 0,
                memo: String::new(),
            },
        }]),
        ObservedTaskState::default(),
    );

    let command = task
        .validate_command(
            &snapshot,
            SemanticCommand::Pay {
                action: PayAction::Direct {
                    target: "bob".to_owned(),
                    amount: 10,
                    memo: String::new(),
                },
            },
        )
        .expect("direct payment template accepts placeholders");

    assert_eq!(command.line(), "/pay bob 10");
}

#[test]
fn shop_inbox_template_does_not_authorize_payment_request() {
    let task = TaskMode::new("check shop inbox").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![SemanticCommand::Shop {
            action: ShopAction::Inbox,
        }]),
        ObservedTaskState::default(),
    );

    let request = task.validate_command(
        &snapshot,
        SemanticCommand::Shop {
            action: ShopAction::RequestPayment {
                command_id: 1,
                amount: 10,
                delivery: "done".to_owned(),
            },
        },
    );

    assert_eq!(request, Err(TaskCommandError::CommandNotAvailable));
}

#[test]
fn subscription_list_template_does_not_authorize_chat() {
    let task = TaskMode::new("read subscriptions").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![SemanticCommand::Subscription {
            action: SubscriptionAction::List,
        }]),
        ObservedTaskState::default(),
    );

    let chat = task.validate_command(
        &snapshot,
        SemanticCommand::Subscription {
            action: SubscriptionAction::Chat {
                target: "P1".to_owned(),
                slug: "news".to_owned(),
                body: "hello".to_owned(),
            },
        },
    );

    assert_eq!(chat, Err(TaskCommandError::CommandNotAvailable));
}

#[test]
fn unavailable_command_is_rejected() {
    let task = TaskMode::new("deposit money").expect("task");
    let snapshot = task.snapshot(
        &observation(vec![SemanticCommand::Balance]),
        ObservedTaskState::default(),
    );

    let result = task.validate_command(
        &snapshot,
        SemanticCommand::Extension {
            name: "bank".to_owned(),
            input: "/bank deposit 10".to_owned(),
        },
    );

    assert_eq!(result, Err(TaskCommandError::CommandNotAvailable));
}

#[test]
fn hunger_signal_is_inferred_from_existing_event_text() {
    let mut observation = observation(Vec::new());
    observation.events.push(ObservationEvent::Message {
        text: "You are hungry and broke. Recovery commands still work.".to_owned(),
    });

    assert_eq!(
        HungerSignal::from_observation(&observation),
        HungerSignal::GatedNeedsWork
    );
}

fn observation(available_commands: Vec<SemanticCommand>) -> JsonObservation {
    JsonObservation {
        player_id: "player:alice".to_owned(),
        view_id: ViewId::from("harbor_square"),
        title: "Harbor Square".to_owned(),
        ascii_art: Vec::new(),
        description: "A place where agents can act.".to_owned(),
        exits: vec![ExitObservation {
            direction: Direction::North,
            target_known: true,
            label: Some("North".to_owned()),
        }],
        entities: Vec::new(),
        online_users: Vec::new(),
        available_commands,
        events: Vec::new(),
    }
}
