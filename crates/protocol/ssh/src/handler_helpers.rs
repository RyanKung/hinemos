use super::*;

pub(super) fn visible_street_parcels<'a>(
    view_id: &str,
    parcels: &'a [StoredParcel],
) -> Vec<&'a StoredParcel> {
    let parcel_ids = street_parcel_ids(view_id);
    parcels
        .iter()
        .filter(|parcel| parcel_ids.contains(&parcel.parcel_id.as_str()))
        .collect()
}

pub(super) fn street_parcel_ids(view_id: &str) -> Vec<&'static str> {
    match view_id {
        "street_north_01" => vec!["N1", "N2"],
        "street_north_02" => vec!["N3", "N4"],
        "street_north_03" => vec!["N5", "N6"],
        "street_north_04" => vec!["N7", "N8"],
        "street_north_05" => vec!["N9", "N10"],
        "street_south_01" => vec!["S1", "S2"],
        "street_south_02" => vec!["S3", "S4"],
        "street_south_03" => vec!["S5", "S6"],
        "street_south_04" => vec!["S7", "S8"],
        "street_south_05" => vec!["S9", "S10"],
        _ => Vec::new(),
    }
}

pub(super) fn slash_parse_feedback(line: &str, error: &SlashParseError) -> String {
    let command = line
        .trim()
        .strip_prefix('/')
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("")
        .to_ascii_lowercase();
    match error {
        SlashParseError::MissingArgument => match command.as_str() {
            "read" => "What do you want to read? Try /read <name>.".to_owned(),
            "inspect" | "x" | "examine" => {
                "What do you want to inspect? Try /inspect <name>.".to_owned()
            }
            "go" | "move" => "Which street direction do you want? Try /go north or /go west.".to_owned(),
            "enter" | "visit" => "Where do you want to enter? Try /enter <place>.".to_owned(),
            "talk" => "Who do you want to talk to? Try /talk <name>.".to_owned(),
            "take" | "get" | "pick" => "What do you want to take? Try /take <name>.".to_owned(),
            "pay" => {
                "Who do you want to pay, and how much? Try /pay <user> <amount> <memo>.".to_owned()
            }
            "mail" | "inbox" => "Which mail item do you mean? Try /mail read <id>.".to_owned(),
            "land" => {
                "Which land command do you need? Try /land list or /land info <parcel>.".to_owned()
            }
            "settings" => "Which setting do you want to change? Try /settings.".to_owned(),
            "build" => {
                "What do you want to build? Use one JSON build sheet after /build.".to_owned()
            }
            "shop" => "Which shop notice do you want to handle? Try /shop request-payment <cmd_id> <amount> <delivery>."
                .to_owned(),
            _ => "That command needs a little more detail. Choose one Available command and include its target."
                .to_owned(),
        },
        SlashParseError::UnexpectedArgument => {
            "That command does not need anything after it. Send it by itself.".to_owned()
        }
        SlashParseError::InvalidAmount => "The amount must be a plain number of MARK.".to_owned(),
        SlashParseError::InvalidInboxFilter => {
            "That mailbox shelf is unknown. Try open, unread, claimed, done, or all.".to_owned()
        }
        SlashParseError::InvalidJson => {
            "The build sheet could not be read as JSON. Check the braces and quotes.".to_owned()
        }
        SlashParseError::UnknownCommand => {
            "That command is not on the town board. Choose one Available command.".to_owned()
        }
    }
}

pub(super) fn resolve_enter_target<'a>(
    visible: &[&'a StoredParcel],
    target: &str,
) -> Result<&'a StoredParcel> {
    let normalized = normalize_enter_target(target);
    if normalized.is_empty() {
        anyhow::bail!("Where do you want to enter? Try /enter <place>.");
    }

    visible
        .iter()
        .copied()
        .find(|parcel| {
            normalize_enter_target(&parcel.parcel_id) == normalized
                || parcel
                    .title
                    .as_deref()
                    .is_some_and(|title| normalize_enter_target(title) == normalized)
        })
        .ok_or_else(|| {
            let available = visible
                .iter()
                .map(|parcel| parcel.parcel_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if available.is_empty() {
                anyhow::anyhow!(
                    "no adjacent parcel here; move along the street with /go, then use /enter <parcel>"
                )
            } else {
                anyhow::anyhow!("parcel not adjacent here: {target}. Available: {available}")
            }
        })
}

pub(super) fn normalize_enter_target(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

pub(super) fn service_room_enter_matches(room: &StoredServiceRoom, normalized: &str) -> bool {
    if room
        .address
        .as_deref()
        .is_some_and(|address| address.eq_ignore_ascii_case(normalized))
        || room
            .view_id
            .split('_')
            .any(|part| part.eq_ignore_ascii_case(normalized))
    {
        return true;
    }
    room.enter_aliases
        .as_deref()
        .unwrap_or_default()
        .split([',', ';', '\n', ' '])
        .map(str::trim)
        .filter(|alias| !alias.is_empty())
        .any(|alias| alias.eq_ignore_ascii_case(normalized))
}

pub(super) fn admission_guidance(_admission: &StoredAdmission) -> String {
    let next_step = "Read the board agreement first: /read agreement";
    format!(
        "Admission pending. SSH authentication is complete, but this account is not admitted into the world yet.\n{next_step}. Until then, other commands are blocked."
    )
}

pub(super) fn send_pending_admission_rejection(
    session: &mut Session,
    channel: ChannelId,
    admission: &StoredAdmission,
    prompt: bool,
) -> Result<()> {
    session.data(
        channel,
        format!(
            "{}\r\n",
            admission_guidance(admission).replace('\n', "\r\n")
        )
        .into_bytes(),
    )?;
    if prompt {
        send_prompt(session, channel)?;
    }
    Ok(())
}

pub(super) fn restrict_pending_admission_observation(
    observation: &mut JsonObservation,
    admission: &StoredAdmission,
) {
    observation.description = format!(
        "{}\n\n{}",
        observation.description,
        admission_guidance(admission)
    );
    observation.exits.clear();
    observation.available_commands = vec![
        SemanticCommand::Look,
        SemanticCommand::Read {
            target: hinemos_core::EntityRef::new(ADMISSION_BOARD_ENTITY_ID),
        },
        SemanticCommand::Help,
        SemanticCommand::Quit,
    ];
}

pub(super) fn mailbox_help() -> &'static str {
    "Commands: HELP, IDLE, LIST [open|unread|claimed|done|all], READ <id>, SEND <user-or-address> <body>, ACK <id>, NOOP, QUIT"
}

pub(super) fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "set" } else { "not set" }
}

pub(super) fn generate_mail_auth_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn append_memory_atom_lines(
    lines: &mut Vec<String>,
    label: &str,
    memories: &[StoredMemoryAtom],
) {
    if memories.is_empty() {
        return;
    }
    lines.push(format!("{label}:"));
    for memory in memories {
        lines.push(format!(
            "- [{}:{}] {}",
            memory.subject, memory.predicate, memory.summary
        ));
    }
}

pub(super) fn memory_help() -> &'static str {
    "Memory commands:\n\
     /memory self - show self-model and self memories\n\
     /memory commitments - show open obligations\n\
     /memory recall <person> - show relationship memory\n\
     /memory search <query> - search remembered events and memories"
}

pub(super) fn model_text(model: Option<&StoredAgentSelfModel>) -> Option<String> {
    let model = model?;
    let mut lines = Vec::new();
    lines.push(format!(
        "Self model v{} from {}",
        model.version, model.created_at
    ));
    if !model.identity.is_object()
        || model
            .identity
            .as_object()
            .is_some_and(|value| !value.is_empty())
    {
        lines.push(format!("Identity: {}", compact_json(&model.identity)));
    }
    if !model.current_state.is_object()
        || model
            .current_state
            .as_object()
            .is_some_and(|value| !value.is_empty())
    {
        lines.push(format!(
            "Current state: {}",
            compact_json(&model.current_state)
        ));
    }
    Some(lines.join("\n"))
}

pub(super) fn render_memory_view(
    title: &str,
    preface: Option<String>,
    memories: &[StoredMemoryAtom],
) -> String {
    let mut lines = vec![title.to_owned()];
    if let Some(preface) = preface {
        lines.push(preface);
    }
    if memories.is_empty() {
        lines.push("(none)".to_owned());
    } else {
        append_memory_atom_lines(&mut lines, "Memories", memories);
    }
    lines.join("\n")
}

pub(super) fn render_person_memory(
    person: &str,
    edge: Option<&StoredSocialEdge>,
    memories: &[StoredMemoryAtom],
) -> String {
    let mut lines = vec![format!("Memory for {person}")];
    if let Some(edge) = edge {
        lines.push(format!(
            "Relationship: trust={:.2} affinity={:.2} obligation={:.2} rivalry={:.2} familiarity={:.2} tags={}",
            edge.trust,
            edge.affinity,
            edge.obligation,
            edge.rivalry,
            edge.familiarity,
            edge.tags.join(",")
        ));
    }
    if memories.is_empty() {
        lines.push("(none)".to_owned());
    } else {
        append_memory_atom_lines(&mut lines, "Memories", memories);
    }
    lines.join("\n")
}

pub(super) fn render_memory_search(
    query: &str,
    events: &[StoredMemoryEvent],
    memories: &[StoredMemoryAtom],
) -> String {
    let mut lines = vec![format!("Memory search: {query}")];
    if memories.is_empty() {
        lines.push("Memories: (none)".to_owned());
    } else {
        append_memory_atom_lines(&mut lines, "Memories", memories);
    }
    if events.is_empty() {
        lines.push("Events: (none)".to_owned());
    } else {
        lines.push("Events:".to_owned());
        for event in events {
            lines.push(format!(
                "- [{}:{}] {}",
                event.source, event.event_type, event.content
            ));
        }
    }
    lines.join("\n")
}

pub(super) fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_owned())
}

pub(super) fn parse_mailbox_item_id(input: &str) -> Result<i64> {
    input
        .trim()
        .parse::<i64>()
        .context("mail item id must be an integer")
}
