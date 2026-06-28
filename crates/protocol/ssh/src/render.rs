//! Rendering and channel output helpers for SSH sessions.

#[path = "render_map.rs"]
mod render_map;
#[cfg(test)]
#[path = "render_tests.rs"]
mod render_tests;

use anyhow::Result;
use hinemos_core::{
    JsonObservation, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED, SHOP_MAILING_LIST_STATUS_OPEN,
    SemanticCommand, SubscriptionAction,
};
use hinemos_runtime::{Chrome, render_text_events, render_text_observation_with_width};
use hinemos_storage::{StoredInboxItem, StoredRoomBinding};
use russh::ChannelId;
use russh::server::Session;

use crate::config::format_mail_user;
use crate::presence::{PresenceDelivery, PresenceDeliveryMode};
use hinemos_app::{RecentPresenceUser, ServiceRoomView};
use render_map::{apply_auto_ascii_map, overlay_ascii_parcel_label, overlay_ascii_title};

pub(crate) fn send_text_observation(
    session: &mut Session,
    channel: ChannelId,
    observation: &hinemos_core::JsonObservation,
    terminal_cols: Option<usize>,
    admission_view_id: &str,
) -> Result<()> {
    let width = frame_width(terminal_cols);
    let content_width = width.saturating_sub(4);
    let map_width = map_width_for_content(content_width);
    let mut observation = observation.clone();
    apply_auto_ascii_map(&mut observation, map_width, admission_view_id);
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

pub(crate) fn overlay_parcel_observation(
    observation: &mut JsonObservation,
    binding: &StoredRoomBinding,
) {
    let owner = binding.owner_user.as_deref().unwrap_or("unclaimed");
    match binding.parcel_status.as_deref().unwrap_or_default() {
        PARCEL_STATUS_BUILT => {
            if let Some(title) = binding.parcel_title.as_deref() {
                overlay_ascii_title(observation, title);
                observation.title = title.to_owned();
            }
            if let Some(description) = binding.parcel_description.as_deref() {
                let shop_commands = format_shop_commands(binding);
                observation.description = format!(
                    "{description}\nOwner: {owner}. Parcel: {}. Style: {}.\nShop commands: {}.\nMailing lists: {}.\nOperator prompt: {}",
                    binding.address,
                    binding.parcel_style.as_deref().unwrap_or("unspecified"),
                    shop_commands.as_deref().unwrap_or("not specified"),
                    format_shop_mailing_lists(binding)
                        .as_deref()
                        .unwrap_or("none"),
                    binding
                        .parcel_operator_prompt
                        .as_deref()
                        .unwrap_or("not specified")
                );
            }
            observation
                .available_commands
                .extend(custom_command_inputs(binding).map(|input| {
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
            observation
                .available_commands
                .extend(open_shop_mailing_lists(binding).map(|list| {
                    SemanticCommand::Subscription {
                        action: SubscriptionAction::Subscribe {
                            target: binding.address.clone(),
                            slug: list.slug.clone(),
                        },
                    }
                }));
        }
        PARCEL_STATUS_CLAIMED => {
            observation.description = format!(
                "Commercial parcel {} is claimed by {owner} but not built yet.\nOwner can edit here with one JSON build sheet: /build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /build publish. Custom commands are auto-filled if omitted.",
                binding.address
            );
        }
        _ => {
            observation.description = format!(
                "Vacant commercial parcel {}. Claim it from the land registry with /land claim {}.",
                binding.address, binding.address
            );
        }
    }
}

pub(crate) fn overlay_service_room(observation: &mut JsonObservation, room: &impl ServiceRoomView) {
    if let Some(status) = room.status_text().filter(|status| !status.is_empty()) {
        observation.description = format!("{}\n{status}", observation.description);
    }
}

pub(crate) fn overlay_room_binding_entries(
    observation: &mut JsonObservation,
    bindings: &[StoredRoomBinding],
) {
    let mut lines = Vec::new();
    for binding in bindings {
        if binding
            .front_entity_id
            .as_deref()
            .is_some_and(|front_entity_id| {
                !observation
                    .entities
                    .iter()
                    .any(|entity| entity.id == front_entity_id)
            })
        {
            continue;
        }
        if let Some(ascii_label) = binding.ascii_label.as_deref() {
            overlay_ascii_parcel_label(observation, &binding.address, ascii_label);
        }
        lines.push(binding.entry_text.clone());
        observation.available_commands.push(SemanticCommand::Enter {
            target: binding.address.clone(),
        });
    }
    if !lines.is_empty() {
        observation.description = format!("{}\n{}", observation.description, lines.join("\n"));
    }
}

pub(crate) fn render_inbox_new_notice(item: &StoredInboxItem, mail_domain: Option<&str>) -> String {
    render_inbox_notice_fields(
        item.id,
        &item.kind,
        &item.sender_user,
        &item.subject,
        &item.body,
        mail_domain,
    )
}

pub(crate) fn render_inbox_notice_fields(
    id: i64,
    kind: &str,
    sender_user: &str,
    subject: &str,
    body: &str,
    mail_domain: Option<&str>,
) -> String {
    let sender = compact_inbox_field(&format_mail_user(sender_user, mail_domain));
    if kind == "mail" {
        return format!(
            "Mail from {sender}: {}\n{}\n(saved to /mailbox as #{}.)\n",
            compact_inbox_field(subject),
            body.trim(),
            id
        );
    }
    format!("Inbox: new {kind} #{id} from {sender}\nUse: /mail read {id}\n")
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

pub(crate) fn room_reply_request_id(subject: &str) -> Option<i64> {
    let subject = subject.trim();
    let request_id = subject.strip_prefix("Re: #")?.trim();
    request_id.parse::<i64>().ok()
}

pub(crate) fn room_reply_live_notice(label: &str, request_id: Option<i64>, body: &str) -> String {
    match request_id {
        Some(request_id) => format!("[room {label} reply #{request_id}] {body}"),
        None => format!("[room {label}] {body}"),
    }
}

fn compact_inbox_field(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn custom_command_inputs(binding: &StoredRoomBinding) -> impl Iterator<Item = String> + '_ {
    command_inputs(binding.parcel_custom_commands.as_deref())
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

fn format_shop_commands(binding: &StoredRoomBinding) -> Option<String> {
    let rendered = binding
        .parcel_custom_commands
        .as_deref()?
        .split(['\n', ';'])
        .filter_map(format_shop_command_entry)
        .collect::<Vec<_>>();
    (!rendered.is_empty()).then(|| rendered.join("; "))
}

fn format_shop_mailing_lists(binding: &StoredRoomBinding) -> Option<String> {
    let rendered = open_shop_mailing_lists(binding)
        .map(|list| {
            format!(
                "{} ({}) join: /subscribe {} {}; chat after joining: /chat {} {} -- <message>",
                list.title, list.slug, binding.address, list.slug, binding.address, list.slug
            )
        })
        .collect::<Vec<_>>();
    (!rendered.is_empty()).then(|| rendered.join("; "))
}

fn open_shop_mailing_lists(
    binding: &StoredRoomBinding,
) -> impl Iterator<Item = &hinemos_storage::StoredShopMailingList> {
    binding
        .parcel_mailing_lists
        .iter()
        .filter(|list| list.status == SHOP_MAILING_LIST_STATUS_OPEN)
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
        return "The Guild will not accept that parcel action; you do not own this parcel."
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

pub(crate) fn render_online_summary(users: &[RecentPresenceUser], limit: usize) -> Vec<String> {
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
     Room service agents: requests arrive with subject `Room command #<id> for <view_id>`; reply with subject `Re: #<id>` so players can associate the answer with the request.\n\
     Keep the SSH connection open, read each observation, choose one Available command, send it, and continue.\n\
     Common commands inside the session: /look, /go east, /go west, /inspect board, /read board, /help.\n\
     Wallet commands: /balance, /pay <user> <amount> [memo], /pay requests, /pay accept <id>."
}
