use hinemos_core::{JsonObservation, ObservationEvent, SemanticCommand};

use super::Chrome;

/// Renders a structured observation for text clients using the default width.
#[must_use]
pub fn render_text_observation(observation: &JsonObservation) -> String {
    render_text_observation_with_width(observation, None)
}

/// Renders a structured observation for text clients using line-feed separators.
#[must_use]
pub fn render_text_observation_with_width(
    observation: &JsonObservation,
    terminal_cols: Option<usize>,
) -> String {
    let mut output = String::new();
    output.push_str(&render_text_events(observation));
    output.push('\n');
    output.push_str(&styled_block(&observation.title, Chrome::ANSI_TITLE));
    output.push('\n');
    if !observation.ascii_art.is_empty() {
        output.push('\n');
        for line in compact_ascii_art(observation) {
            output.push_str(&highlight_ascii_markers(line));
            output.push('\n');
        }
    }
    output.push('\n');
    output.push_str(&wrap_text(
        &observation.description,
        terminal_cols.unwrap_or(80),
    ));
    output.push('\n');
    output.push_str(&styled_block(
        Chrome::MAP_LEGEND,
        Chrome::ANSI_EVENT_MESSAGE,
    ));
    output.push('\n');

    if !observation.exits.is_empty() {
        let exits = observation
            .exits
            .iter()
            .map(|exit| exit.direction.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&styled_block(
            &format!("{}: {exits}\n", Chrome::LABEL_EXITS),
            Chrome::ANSI_EVENT_MOVE,
        ));
    }

    if !observation.entities.is_empty() {
        let entities = observation
            .entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&styled_block(
            &format!("{}: {entities}\n", Chrome::LABEL_VISIBLE),
            Chrome::ANSI_PLAYER_MARKER,
        ));
    }

    if !observation.online_users.is_empty() {
        output.push_str(&styled_block(
            &format!("Online here: {}\n", observation.online_users.join(", ")),
            Chrome::ANSI_ITEM_MARKER,
        ));
    }

    if !observation.available_commands.is_empty() {
        output.push_str(&styled_block(
            &render_available_summary(observation),
            Chrome::ANSI_AVAILABLE,
        ));
    }

    output
}

/// Renders only command result events, without repeating the current room.
#[must_use]
pub fn render_text_events(observation: &JsonObservation) -> String {
    let mut output = String::new();
    for event in &observation.events {
        match event {
            ObservationEvent::Message { text } => {
                output.push_str(&styled_block(text, Chrome::ANSI_EVENT_MESSAGE));
                output.push('\n');
            }
            ObservationEvent::Move { direction, .. } => {
                output.push_str(&styled_block(
                    &format!("{} {}\n", Chrome::MOVE_VERB, direction.as_str()),
                    Chrome::ANSI_EVENT_MOVE,
                ));
            }
        }
    }
    output
}

fn compact_ascii_art(observation: &JsonObservation) -> Vec<&str> {
    let title = observation.title.trim().to_ascii_uppercase();
    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in &observation.ascii_art {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !lines.is_empty() && !previous_blank {
                lines.push(line.as_str());
                previous_blank = true;
            }
            continue;
        }
        if trimmed.chars().all(|character| character == '=') || trimmed == title {
            continue;
        }
        lines.push(line.as_str());
        previous_blank = false;
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn wrap_text(text: &str, terminal_cols: usize) -> String {
    let width = terminal_cols.max(32);
    let mut wrapped = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if line.is_empty() {
                line.push_str(word);
                continue;
            }

            if line.len() + 1 + word.len() > width {
                wrapped.push(line);
                line = word.to_owned();
            } else {
                line.push(' ');
                line.push_str(word);
            }
        }

        if !line.is_empty() {
            wrapped.push(line);
        }
    }

    wrapped.join("\n")
}

fn render_available_summary(observation: &JsonObservation) -> String {
    let mut parts = Vec::new();
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Look))
    {
        parts.push("/look".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Map))
    {
        parts.push("/map".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Inventory))
    {
        parts.push("/inventory".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::History))
    {
        parts.push("/history".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Help))
    {
        parts.push("/help".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Settings { .. }))
    {
        parts.push("/settings".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Who))
    {
        parts.push("/who".to_owned());
    }
    if observation
        .available_commands
        .iter()
        .any(|command| matches!(command, SemanticCommand::Say { .. }))
    {
        parts.push("/say <text>".to_owned());
    }

    let moves = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Move { direction } => Some(format!("/go {}", direction.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !moves.is_empty() {
        parts.push(format!("move: {}", moves.join(", ")));
    }

    let enter_commands = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Enter { target } => Some(format!("/enter {target}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !enter_commands.is_empty() {
        parts.push(format!("enter: {}", enter_commands.join(", ")));
    }

    push_target_commands(
        "inspect",
        observation,
        |command| match command {
            SemanticCommand::Inspect { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );
    push_target_commands(
        "read",
        observation,
        |command| match command {
            SemanticCommand::Read { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );
    push_target_commands(
        "talk",
        observation,
        |command| match command {
            SemanticCommand::Talk { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );
    push_target_commands(
        "take",
        observation,
        |command| match command {
            SemanticCommand::Take { target } => Some(target.id.as_str()),
            _ => None,
        },
        &mut parts,
    );

    let agreement_commands = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Agree { phrase } if phrase.is_empty() => Some("/agree".to_owned()),
            SemanticCommand::Agree { phrase } => Some(format!("/agree {phrase}")),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !agreement_commands.is_empty() {
        parts.push(format!("admission: {}", agreement_commands.join(", ")));
    }

    let extension_commands = observation
        .available_commands
        .iter()
        .filter_map(|command| match command {
            SemanticCommand::Extension { input, .. } => Some(input.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !extension_commands.is_empty() {
        parts.push(format!("local: {}", extension_commands.join(", ")));
    }

    let mut output = format!("{}:\n", Chrome::LABEL_AVAILABLE);
    for part in parts {
        output.push_str("- ");
        output.push_str(&part);
        output.push('\n');
    }
    output
}

fn push_target_commands<'a>(
    verb: &str,
    observation: &'a JsonObservation,
    target_for: impl Fn(&'a SemanticCommand) -> Option<&'a str>,
    parts: &mut Vec<String>,
) {
    let commands = observation
        .available_commands
        .iter()
        .filter_map(|command| target_for(command).map(|target| format!("/{verb} {target}")))
        .collect::<Vec<_>>();
    if !commands.is_empty() {
        if verb == "read" && commands.len() == 1 {
            parts.push(format!("{verb}: /read"));
        } else {
            parts.push(format!("{verb}: {}", commands.join(", ")));
        }
    }
}

fn highlight_ascii_markers(line: &str) -> String {
    highlight_player_marker(&highlight_item_markers(&highlight_place_markers(line)))
}

fn highlight_player_marker(line: &str) -> String {
    style_literal(line, "<Me>", Chrome::ANSI_PLAYER_MARKER)
}

fn highlight_place_markers(line: &str) -> String {
    let mut output = String::new();
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);
        let Some(end) = after_start.find(']') else {
            output.push_str(after_start);
            return output;
        };
        let marker = &after_start[..=end];
        if marker.contains("<Me>") {
            output.push_str(marker);
        } else {
            output.push_str(&styled_marker(marker, Chrome::ANSI_PLACE_MARKER));
        }
        rest = &after_start[end + 1..];
    }
    output.push_str(rest);
    output
}

fn highlight_item_markers(line: &str) -> String {
    line.replace(
        "{bulletin board}",
        &styled_marker("{bulletin board}", Chrome::ANSI_ITEM_MARKER),
    )
}

fn style_literal(line: &str, literal: &str, ansi_style: &str) -> String {
    line.replace(literal, &styled_marker(literal, ansi_style))
}

fn styled_marker(label: &str, ansi_style: &str) -> String {
    format!("{ansi_style}{label}{}", Chrome::ANSI_RESET)
}

pub(super) fn styled_block(text: &str, ansi_style: &str) -> String {
    format!("{ansi_style}{text}{}", Chrome::ANSI_RESET)
}
