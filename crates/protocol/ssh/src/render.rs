//! Rendering and channel output helpers for SSH sessions.

#[path = "render_map.rs"]
mod render_map;
#[cfg(test)]
#[path = "render_tests.rs"]
mod render_tests;

use anyhow::Result;
use hinemos_core::{JsonObservation, SemanticCommand};
use hinemos_runtime::{Chrome, render_text_events, render_text_observation_with_width};
use hinemos_storage::{
    PgStorage, StoredInboxItem, StoredOperatorCommand, StoredParcel, StoredPaymentRequest,
    StoredServiceRoom, StoredWorldMessage, TEST_CURRENCY,
};
use russh::ChannelId;
use russh::server::Session;

use crate::auth::AuthIdentity;
use crate::config::format_mail_user;
use crate::presence::{PresenceDelivery, PresenceDeliveryMode, PresenceViewUser};
use render_map::{apply_auto_ascii_map, overlay_ascii_parcel_label, overlay_ascii_title};

pub(crate) fn send_text_observation(
    session: &mut Session,
    channel: ChannelId,
    observation: &hinemos_core::JsonObservation,
    terminal_cols: Option<usize>,
) -> Result<()> {
    let width = frame_width(terminal_cols);
    let content_width = width.saturating_sub(4);
    let map_width = map_width_for_content(content_width);
    let mut observation = observation.clone();
    apply_auto_ascii_map(&mut observation, map_width);
    center_ascii_map(&mut observation, content_width);
    let rendered = render_text_observation_with_width(&observation, Some(content_width));
    let framed = frame_text(&rendered, width);
    session.data(channel, format!("\x1b[2J\x1b[H{framed}").into_bytes())?;
    Ok(())
}

pub(crate) fn clear_terminal(session: &mut Session, channel: ChannelId) -> Result<()> {
    session.data(channel, b"\x1b[2J\x1b[H".to_vec())?;
    Ok(())
}

fn frame_width(terminal_cols: Option<usize>) -> usize {
    terminal_cols.unwrap_or(88).clamp(48, 120)
}

fn map_width_for_content(content_width: usize) -> usize {
    content_width.saturating_sub(4).clamp(44, 96)
}

fn frame_text(text: &str, width: usize) -> String {
    let inner = width.saturating_sub(2);
    let horizontal = "─".repeat(inner);
    let mut output = String::new();
    output.push_str(&format!("╭{horizontal}╮\r\n"));
    for line in text.lines() {
        for segment in visual_segments(line, inner) {
            output.push('│');
            output.push_str(&segment);
            output.push_str(&" ".repeat(inner.saturating_sub(visual_width(&segment))));
            output.push_str("│\r\n");
        }
    }
    output.push_str(&format!("╰{horizontal}╯\r\n"));
    output
}

fn visual_segments(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            current.push(ch);
            for next in chars.by_ref() {
                current.push(next);
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        let ch_width = 1;
        if current_width + ch_width > width && !current.is_empty() {
            segments.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }
    segments.push(current);
    segments
}

fn visual_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

fn center_ascii_map(observation: &mut JsonObservation, content_width: usize) {
    if observation.ascii_art.is_empty() {
        return;
    }
    let map_width = observation
        .ascii_art
        .iter()
        .map(|line| visual_width(line))
        .max()
        .unwrap_or(0);
    if map_width >= content_width {
        return;
    }
    let padding = " ".repeat((content_width - map_width) / 2);
    for line in &mut observation.ascii_art {
        if !line.trim().is_empty() {
            *line = format!("{padding}{line}");
        }
    }
}

pub(crate) fn send_text_events(
    session: &mut Session,
    channel: ChannelId,
    observation: &hinemos_core::JsonObservation,
) -> Result<()> {
    let rendered = render_text_events(observation).replace('\n', "\r\n");
    if !rendered.is_empty() {
        session.data(channel, rendered.into_bytes())?;
    }
    Ok(())
}

pub(crate) fn should_render_full_observation(command: &SemanticCommand) -> bool {
    matches!(
        command,
        SemanticCommand::Look
            | SemanticCommand::Map
            | SemanticCommand::Move { .. }
            | SemanticCommand::Enter { .. }
    )
}

pub(crate) fn overlay_parcel_observation(observation: &mut JsonObservation, parcel: &StoredParcel) {
    let owner = parcel.owner_user.as_deref().unwrap_or("unclaimed");
    match parcel.status.as_str() {
        "built" => {
            if let Some(title) = &parcel.title {
                overlay_ascii_title(observation, title);
                observation.title = title.clone();
            }
            if let Some(description) = &parcel.description {
                let shop_commands = format_shop_commands(parcel);
                observation.description = format!(
                    "{description}\nOwner: {owner}. Parcel: {}. Style: {}.\nShop commands: {}.\nOperator prompt: {}",
                    parcel.parcel_id,
                    parcel.style.as_deref().unwrap_or("unspecified"),
                    shop_commands.as_deref().unwrap_or("not specified"),
                    parcel.operator_prompt.as_deref().unwrap_or("not specified")
                );
            }
            observation
                .available_commands
                .extend(custom_command_inputs(parcel).map(|input| {
                    SemanticCommand::Extension {
                        name: input
                            .trim_start_matches('/')
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_owned(),
                        input,
                    }
                }));
        }
        "claimed" => {
            observation.description = format!(
                "Commercial parcel {} is claimed by {owner} but not built yet.\nOwner can edit here with one JSON build sheet: /build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /build publish. Custom commands are auto-filled if omitted.",
                parcel.parcel_id
            );
        }
        _ => {
            observation.description = format!(
                "Vacant commercial parcel {}. Claim it from the land registry with /land claim {}.",
                parcel.parcel_id, parcel.parcel_id
            );
        }
    }
}

pub(crate) fn overlay_street_parcels(observation: &mut JsonObservation, parcels: &[&StoredParcel]) {
    if parcels.is_empty() {
        return;
    }

    let mut lines = vec!["Street parcels:".to_owned()];
    for parcel in parcels {
        let label = match parcel.status.as_str() {
            "built" => parcel
                .title
                .as_deref()
                .unwrap_or(&parcel.parcel_id)
                .to_owned(),
            "claimed" => format!(
                "{} claimed by {}",
                parcel.parcel_id,
                parcel.owner_user.as_deref().unwrap_or("unknown")
            ),
            _ => format!("{} vacant", parcel.parcel_id),
        };
        if parcel.status == "built"
            && let Some(title) = parcel.title.as_deref()
        {
            overlay_ascii_parcel_label(observation, &parcel.parcel_id, title);
        }
        lines.push(format!("- {label}. Enter: /enter {}.", parcel.parcel_id));
    }

    observation.description = format!("{}\n{}", observation.description, lines.join("\n"));
    observation
        .available_commands
        .extend(parcels.iter().map(|parcel| SemanticCommand::Enter {
            target: parcel.parcel_id.clone(),
        }));
}

pub(crate) fn overlay_service_room(observation: &mut JsonObservation, room: &StoredServiceRoom) {
    if let Some(status) = room
        .status_text
        .as_deref()
        .filter(|status| !status.is_empty())
    {
        observation.description = format!("{}\n{status}", observation.description);
    }
    observation
        .available_commands
        .extend(
            command_inputs(room.custom_commands.as_deref()).map(|input| {
                let name = input
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                SemanticCommand::Extension { name, input }
            }),
        );
}

pub(crate) fn overlay_service_room_entries(
    observation: &mut JsonObservation,
    rooms: &[StoredServiceRoom],
) {
    let mut lines = Vec::new();
    for room in rooms {
        let Some(front_entity_id) = room.front_entity_id.as_deref() else {
            continue;
        };
        if !observation
            .entities
            .iter()
            .any(|entity| entity.id == front_entity_id)
        {
            continue;
        }
        let address = room.address.as_deref().unwrap_or(&room.view_id);
        let label = room.label.as_deref().unwrap_or(&room.view_id);
        lines.push(format!("- {address} {label}. Enter: /enter {address}."));
        observation.available_commands.push(SemanticCommand::Enter {
            target: address.to_owned(),
        });
    }
    if !lines.is_empty() {
        observation.description = format!("{}\n{}", observation.description, lines.join("\n"));
    }
}

pub(crate) fn service_room_accepts_input(commands: Option<&str>, raw_input: &str) -> bool {
    let Some(input_command) = raw_input.split_whitespace().next() else {
        return false;
    };
    command_inputs(commands).any(|command| command.split_whitespace().next() == Some(input_command))
}

pub(crate) fn render_parcel_list(parcels: &[StoredParcel]) -> String {
    let mut lines = vec!["Commercial Parcels".to_owned()];
    let mut vacant_count = 0_u32;
    for parcel in parcels {
        match parcel.status.as_str() {
            "built" => lines.push(format!(
                "- {}: {}. Owner: {}. Enter from street: /enter {}.",
                parcel.parcel_id,
                parcel.title.as_deref().unwrap_or("built shop"),
                parcel.owner_user.as_deref().unwrap_or("unknown"),
                parcel.parcel_id
            )),
            "claimed" => lines.push(format!(
                "- {}: claimed by {}; not built yet.",
                parcel.parcel_id,
                parcel.owner_user.as_deref().unwrap_or("unknown")
            )),
            _ => {
                vacant_count += 1;
                lines.push(format!(
                    "- {}: vacant. Claim: /land claim {}.",
                    parcel.parcel_id, parcel.parcel_id
                ));
            }
        }
    }
    if vacant_count == 0 {
        lines.push("No vacant parcels right now. Use /land info <parcel> for details.".to_owned());
    } else {
        lines.push(format!(
            "{vacant_count} vacant parcel(s). Use /land claim <parcel>, /land token <parcel>, /land info <parcel>, or /land transfer <parcel> <user>."
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn render_inbox_items(
    title: &str,
    items: &[StoredInboxItem],
    mail_domain: Option<&str>,
) -> String {
    let mut lines = vec![title.to_owned()];
    if items.is_empty() {
        lines.push("No inbox items.".to_owned());
    } else {
        for item in items {
            let lease = item
                .lease_until
                .as_deref()
                .map(|value| format!(" lease until {value}"))
                .unwrap_or_default();
            lines.push(format!(
                "#{} {} {} from {}: {} (attempts {}){}",
                item.id,
                item.kind,
                item.status,
                compact_inbox_field(&format_mail_user(&item.sender_user, mail_domain)),
                compact_inbox_field(&item.subject),
                item.attempts,
                lease
            ));
        }
        lines.push(
            "Use /mail read <id>, /mail claim <id>, /mail ack <id>, or /mail archive <id>."
                .to_owned(),
        );
    }
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn render_inbox_new_notice(item: &StoredInboxItem, mail_domain: Option<&str>) -> String {
    let sender = compact_inbox_field(&format_mail_user(&item.sender_user, mail_domain));
    if item.kind == "mail" {
        return format!(
            "Mail from {sender}: {}\n{}\n(saved to /mailbox as #{}.)\n",
            compact_inbox_field(&item.subject),
            item.body.trim(),
            item.id
        );
    }
    format!(
        "Inbox: new {} #{} from {}\nUse: /mail read {}\n",
        item.kind, item.id, sender, item.id
    )
}

pub(crate) fn render_mailbox_new_notice(
    item: &StoredInboxItem,
    mail_domain: Option<&str>,
) -> String {
    format!(
        "* NEWMAIL {} KIND {} FROM {} SUBJECT {}\n",
        item.id,
        item.kind,
        compact_inbox_field(&format_mail_user(&item.sender_user, mail_domain)),
        compact_inbox_field(&item.subject)
    )
}

pub(crate) fn render_inbox_item(item: &StoredInboxItem, mail_domain: Option<&str>) -> String {
    format!(
        "Inbox #{}\nKind: {}\nStatus: {}\nFrom: {}\nSubject: {}\nCreated: {}\nAttempts: {}\nBody: {}\n\n",
        item.id,
        item.kind,
        item.status,
        format_mail_user(&item.sender_user, mail_domain),
        item.subject,
        item.created_at,
        item.attempts,
        item.body
    )
}

fn compact_inbox_field(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn render_parcel_detail(parcel: &StoredParcel) -> String {
    format!(
        "Parcel {}\nView: {}\nDistrict: {} {}\nStatus: {}\nOwner: {}\nRoom mail: {}\nTitle: {}\nDescription: {}\nStyle: {}\nPrompt: {}\nCommands: {}\n\n",
        parcel.parcel_id,
        parcel.view_id,
        parcel.district,
        parcel.position,
        parcel.status,
        parcel.owner_user.as_deref().unwrap_or("-"),
        parcel.room_user.as_deref().unwrap_or("-"),
        parcel.title.as_deref().unwrap_or("-"),
        parcel.description.as_deref().unwrap_or("-"),
        parcel.style.as_deref().unwrap_or("-"),
        parcel.operator_prompt.as_deref().unwrap_or("-"),
        parcel.custom_commands.as_deref().unwrap_or("-")
    )
}

pub(crate) fn custom_command_preview(parcel: &StoredParcel, raw_input: &str) -> Option<String> {
    let command = raw_input.split_whitespace().next()?;
    let commands = parcel.custom_commands.as_deref()?;
    for entry in commands.split(['\n', ';']) {
        let entry = entry.trim();
        if !entry.starts_with(command) {
            continue;
        }
        let Some(preview) = command_field_value(entry, "preview=") else {
            continue;
        };
        let preview = preview.trim();
        if !preview.is_empty() {
            return Some(preview.to_owned());
        }
    }
    None
}

pub(crate) fn is_custom_command_input(parcel: &StoredParcel, raw_input: &str) -> bool {
    let Some(input_command) = raw_input.split_whitespace().next() else {
        return false;
    };
    custom_command_inputs(parcel)
        .any(|command| command.split_whitespace().next() == Some(input_command))
}

fn custom_command_inputs(parcel: &StoredParcel) -> impl Iterator<Item = String> + '_ {
    command_inputs(parcel.custom_commands.as_deref())
}

pub(crate) fn command_inputs(commands: Option<&str>) -> impl Iterator<Item = String> + '_ {
    commands
        .unwrap_or_default()
        .split(['\n', ';'])
        .filter_map(|entry| {
            let entry = entry.trim();
            let command = entry.split_whitespace().next()?;
            command.starts_with('/').then(|| entry.to_owned())
        })
}

fn format_shop_commands(parcel: &StoredParcel) -> Option<String> {
    let rendered = parcel
        .custom_commands
        .as_deref()?
        .split(['\n', ';'])
        .filter_map(format_shop_command_entry)
        .collect::<Vec<_>>();
    (!rendered.is_empty()).then(|| rendered.join("; "))
}

fn format_shop_command_entry(entry: &str) -> Option<String> {
    let entry = entry.trim();
    let command = entry.split_whitespace().next()?;
    if !command.starts_with('/') {
        return None;
    }

    let preview = command_field_value(entry, "preview=");
    let price = command_field_value(entry, "price=");
    let mut details = Vec::new();
    if let Some(preview) = preview.as_deref().filter(|preview| !preview.is_empty()) {
        details.push(preview.to_owned());
    }
    if let Some(price) = price.as_deref().filter(|price| !price.is_empty()) {
        details.push(format!("price {price}"));
    }

    if details.is_empty() {
        Some(command.to_owned())
    } else {
        Some(format!("{command} - {}", details.join(", ")))
    }
}

fn command_field_value(entry: &str, marker: &str) -> Option<String> {
    let (_, after_marker) = entry.split_once(marker)?;
    let value = after_marker.trim_start();
    if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'').unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    Some(
        value
            .split_whitespace()
            .next()
            .unwrap_or(value)
            .trim()
            .to_owned(),
    )
}

pub(crate) fn build_help() -> &'static str {
    "Build commands for the current owned parcel:\r\n\
     /build {\"title\":\"shop title\",\"description\":\"shop description\",\"style\":\"style note\",\"prompt\":\"operator prompt\"}\r\n\
     Optional JSON field: \"commands\". If omitted, commands are auto-filled.\r\n\
     Legacy field commands still work for manual correction: /build title <text>, /build description <text>, /build style <text>, /build prompt <text>, /build commands <text>\r\n\
     /build publish\r\n\
     After publishing, visitor slash commands inside the shop become inbox items for the owner.\r\n"
}

pub(crate) const fn default_build_commands() -> &'static str {
    "/hello preview=hello price=25; /status"
}

pub(crate) const ANSI_RESET: &str = "\x1b[0m";
pub(crate) const ANSI_RED: &str = "\x1b[1;31m";
pub(crate) const ANSI_GREEN: &str = "\x1b[1;32m";
pub(crate) const ANSI_YELLOW: &str = "\x1b[1;33m";
pub(crate) const ANSI_MAGENTA: &str = "\x1b[1;35m";
pub(crate) const ANSI_CYAN: &str = "\x1b[1;36m";
pub(crate) const ANSI_DIM: &str = "\x1b[2m";

pub(crate) fn styled_block(text: &str, ansi_style: &str) -> String {
    format!("{ansi_style}{text}{ANSI_RESET}")
}

pub(crate) fn non_empty(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn send_prompt(session: &mut Session, channel: ChannelId) -> Result<()> {
    session.data(
        channel,
        styled_block(Chrome::PROMPT, ANSI_GREEN).into_bytes(),
    )?;
    Ok(())
}

pub(crate) fn send_command_error(
    session: &mut Session,
    channel: ChannelId,
    error: anyhow::Error,
    prompt: bool,
) -> Result<()> {
    session.data(
        channel,
        format!(
            "{}\r\n",
            styled_block(&world_error_feedback(&error.to_string()), ANSI_RED)
        )
        .into_bytes(),
    )?;
    if prompt {
        send_prompt(session, channel)?;
    }
    Ok(())
}

pub(crate) fn world_error_feedback(message: &str) -> String {
    if let Some(name) = message.strip_prefix("payment target not found: ") {
        return format!("No player named {name} can be found for payment.");
    }
    if let Some(id) = message.strip_prefix("payment request not found: ") {
        return format!("No payment request #{id} is open on the ledger.");
    }
    if let Some(parcel) = message.strip_prefix("parcel not found: ") {
        return format!("The Guild has no parcel record named {parcel}.");
    }
    if message == "you do not own this parcel"
        || message.starts_with("you do not own this parcel: ")
    {
        return "The Guild will not accept that build sheet here; you do not own this parcel."
            .to_owned();
    }
    if let Some(id) = message.strip_prefix("shop command not found: ") {
        return format!("No shop notice #{id} is waiting here.");
    }
    if let Some(target) = message.strip_prefix("entity is not visible: ") {
        return format!("You do not see {target} here.");
    }
    if let Some(target) = message.strip_prefix("entity not found: ") {
        return format!("The world has no visible record named {target}.");
    }
    if let Some(target) = message.strip_prefix("item is not visible: ") {
        return format!("You do not see {target} here.");
    }
    if let Some(target) = message.strip_prefix("item not found: ") {
        return format!("The world has no item record named {target}.");
    }
    if let Some(rest) = message.strip_prefix("parcel not adjacent here: ") {
        return format!("That parcel is not beside this path. {rest}");
    }
    if message.starts_with("no adjacent parcel here") {
        return "Nothing opens from this side. Move along the street with /go, or use /enter <place> when a shopfront or parcel is visible."
            .to_owned();
    }
    message.to_owned()
}

pub(crate) fn send_stdin_closed_guidance(
    session: &mut Session,
    channel: ChannelId,
    commands_seen: u64,
    had_buffered_input: bool,
) -> Result<()> {
    let status = if commands_seen == 0 {
        "No world command was received before stdin closed."
    } else if had_buffered_input {
        "The final buffered world command was processed before stdin closed."
    } else {
        "The submitted world command batch is complete."
    };
    session.data(
        channel,
        styled_block(
            &format!(
            "\r\nConnection note: {status}\r\n\
             This SSH channel cannot receive more commands after client stdin is closed, so Hinemos is closing it cleanly.\r\n\
             Your player state is saved by SSH identity. Reconnect to continue from the latest observation.\r\n\
             Non-TTY agent skill:\r\n\
             1. Connect with ssh -T -p <port> <user>@<host>.\r\n\
             2. Read the observation and the Available command list.\r\n\
             3. Send exactly one chosen command, or a short finite batch ending with /quit.\r\n\
             4. When this channel closes, do not wait on it. Reconnect and repeat from step 1.\r\n\
             Example batch:\r\n\
               printf '/look\\n/go east\\n/history\\n/quit\\n' | ssh -T -p <port> <user>@<host>\r\n"
        ),
            ANSI_DIM,
        )
        .into_bytes(),
    )?;
    Ok(())
}

pub(crate) fn send_message_list(
    session: &mut Session,
    channel: ChannelId,
    title: &str,
    messages: &[StoredWorldMessage],
    empty: &str,
) -> Result<()> {
    session.data(channel, format!("\r\n{title}\r\n").into_bytes())?;
    if messages.is_empty() {
        session.data(
            channel,
            styled_block(&format!("{empty}\r\n"), ANSI_DIM).into_bytes(),
        )?;
        return Ok(());
    }

    for message in messages.iter().rev() {
        let expiry = message
            .expires_at
            .as_ref()
            .map(|expires_at| format!(" expires={expires_at}"))
            .unwrap_or_default();
        session.data(
            channel,
            styled_block(
                &format!(
                    "- [{}] {} from {}{}: {}\r\n",
                    message.created_at, message.kind, message.sender_user, expiry, message.body
                ),
                ANSI_DIM,
            )
            .into_bytes(),
        )?;
    }
    Ok(())
}

pub(crate) fn render_online_summary(users: &[PresenceViewUser], limit: usize) -> Vec<String> {
    let mut rendered = users
        .iter()
        .take(limit)
        .map(|user| user.user.clone())
        .collect::<Vec<_>>();
    let remaining = users.len().saturating_sub(limit);
    if remaining > 0 {
        rendered.push(format!("+{remaining} more (use /who)"));
    }
    rendered
}

pub(crate) fn render_who(view_id: &str, users: &[PresenceViewUser]) -> String {
    if users.is_empty() {
        return format!("Online here in {view_id}: nobody else.\r\n");
    }
    let names = users
        .iter()
        .map(|user| user.user.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!("Online here in {view_id} ({}): {names}\r\n", users.len())
}

pub(crate) fn send_operator_command_list(
    session: &mut Session,
    channel: ChannelId,
    commands: &[StoredOperatorCommand],
) -> Result<()> {
    session.data(channel, b"\r\nShop Inbox\r\n".to_vec())?;
    if commands.is_empty() {
        session.data(channel, b"No shop commands.\r\n".to_vec())?;
        return Ok(());
    }

    for command in commands.iter().rev() {
        session.data(
            channel,
            format!(
                "- #{} [{}] {} from {} in {}: {}\r\n",
                command.id,
                command.created_at,
                command.status,
                command.sender_user,
                command.parcel_id,
                command.raw_input
            )
            .into_bytes(),
        )?;
    }
    Ok(())
}

pub(crate) fn send_payment_request_list(
    session: &mut Session,
    channel: ChannelId,
    requests: &[StoredPaymentRequest],
) -> Result<()> {
    session.data(channel, b"\r\nPayment Requests\r\n".to_vec())?;
    if requests.is_empty() {
        session.data(channel, b"No pending payment requests.\r\n".to_vec())?;
        return Ok(());
    }

    for request in requests.iter().rev() {
        session.data(
            channel,
            render_payment_popup(request)
                .replace('\n', "\r\n")
                .into_bytes(),
        )?;
    }
    Ok(())
}

pub(crate) fn render_payment_popup(request: &StoredPaymentRequest) -> String {
    format!(
        "\n=== Payment Request #{} ===\nShop: {} ({})\nAmount: {} {}\nFor: shop command #{}\nDelivery: locked until payment\nAccept: /pay accept {}\nReject: ignore this request\n==========================\n",
        request.id,
        request.parcel_id,
        request.payee_user,
        request.amount,
        request.asset,
        request.operator_command_id,
        request.id
    )
}

pub(crate) fn render_paid_request(request: &StoredPaymentRequest, sender_balance: i64) -> String {
    format!(
        "Paid payment request #{}: {} {} to {}. Balance: {} {}.\r\nUnlocked content: {}\r\n",
        request.id,
        request.amount,
        request.asset,
        request.payee_user,
        sender_balance,
        request.asset,
        request.delivery
    )
}

pub(crate) async fn send_mailbox_summary(
    session: &mut Session,
    channel: ChannelId,
    storage: &PgStorage,
    identity: &AuthIdentity,
) -> Result<()> {
    let items = storage
        .list_inbox_items(&identity.user, &identity.player_id, Some("open"), 10)
        .await?;
    if items.is_empty() {
        return Ok(());
    }

    session.data(
        channel,
        format!("Inbox: {} open item(s).\r\n", items.len()).into_bytes(),
    )?;
    Ok(())
}

pub(crate) async fn send_balance_summary(
    session: &mut Session,
    channel: ChannelId,
    storage: &PgStorage,
    identity: &AuthIdentity,
) -> Result<()> {
    let balance = storage.player_balance(&identity.player_id).await?;
    session.data(
        channel,
        format!(
            "Wallet: {} {}. Use /balance, /pay <user> <amount> [memo], /pay requests, or /pay accept <id>.\r\n",
            balance.amount, TEST_CURRENCY
        )
        .into_bytes(),
    )?;
    Ok(())
}

pub(crate) async fn deliver_live_message(recipients: Vec<PresenceDelivery>, message: &str) {
    let message = message.replace('\n', "\r\n");
    for recipient in recipients {
        let payload = match recipient.mode {
            PresenceDeliveryMode::Shell => format!("\r\n{message}\r\n{}", Chrome::PROMPT),
            PresenceDeliveryMode::Mailbox => format!("{message}\r\n"),
        };
        let _ = recipient
            .handle
            .data(recipient.channel_id, payload.into_bytes())
            .await;
    }
}

pub(crate) async fn deliver_live_inbox_notice(
    recipients: Vec<PresenceDelivery>,
    item: &StoredInboxItem,
    mail_domain: Option<&str>,
) {
    for recipient in recipients {
        let message = match recipient.mode {
            PresenceDeliveryMode::Shell => render_inbox_new_notice(item, mail_domain),
            PresenceDeliveryMode::Mailbox => render_mailbox_new_notice(item, mail_domain),
        }
        .replace('\n', "\r\n");
        let payload = match recipient.mode {
            PresenceDeliveryMode::Shell => format!("\r\n{message}\r\n{}", Chrome::PROMPT),
            PresenceDeliveryMode::Mailbox => message,
        };
        let _ = recipient
            .handle
            .data(recipient.channel_id, payload.into_bytes())
            .await;
    }
}

pub(crate) fn exec_help() -> &'static str {
    "Hinemos is an open world served over SSH, not a general-purpose Unix shell.\n\
     Open an SSH shell: ssh -p <port> <user>@<host>\n\
     Open the SSH-authenticated mailbox protocol: ssh -T -p <port> <user>@<host> mailbox\n\
     Autonomous agent mail: log in with an ed25519 SSH key, run /settings mail-token, then use SMTP/IMAP with username <user> and that token. Keep IMAP IDLE open to receive no-prompt EXISTS notifications, then FETCH and STORE +FLAGS (\\Seen).\n\
     Keep the SSH connection open, read each observation, choose one Available command, send it, and continue.\n\
     Common commands inside the session: /look, /go east, /go west, /inspect board, /read board, /help.\n\
     Wallet commands: /balance, /pay <user> <amount> [memo], /pay requests, /pay accept <id>."
}
