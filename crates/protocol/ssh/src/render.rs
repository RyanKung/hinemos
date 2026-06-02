//! Rendering and channel output helpers for SSH sessions.

use anyhow::Result;
use russh::ChannelId;
use russh::server::Session;
use xagora_core::{JsonObservation, SemanticCommand};
use xagora_runtime::{Chrome, render_text_events, render_text_observation};
use xagora_storage::{
    PgStorage, StoredInboxItem, StoredOperatorCommand, StoredParcel, StoredPaymentRequest,
    StoredWorldMessage, TEST_CURRENCY,
};

use crate::auth::AuthIdentity;
use crate::presence::{PresenceDelivery, PresenceViewUser};

pub(crate) fn send_text_observation(
    session: &mut Session,
    channel: ChannelId,
    observation: &xagora_core::JsonObservation,
) -> Result<()> {
    session.data(
        channel,
        render_text_observation(observation)
            .replace('\n', "\r\n")
            .into_bytes(),
    )?;
    Ok(())
}

pub(crate) fn send_text_events(
    session: &mut Session,
    channel: ChannelId,
    observation: &xagora_core::JsonObservation,
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
                observation.description = format!(
                    "{description}\nOwner: {owner}. Parcel: {}. Style: {}.\nCustom commands: {}.\nOperator prompt: {}",
                    parcel.parcel_id,
                    parcel.style.as_deref().unwrap_or("unspecified"),
                    parcel.custom_commands.as_deref().unwrap_or("not specified"),
                    parcel.operator_prompt.as_deref().unwrap_or("not specified")
                );
            }
        }
        "claimed" => {
            observation.description = format!(
                "Commercial parcel {} is claimed by {owner} but not built yet.\nOwner can edit here with one JSON build sheet: /build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /build publish. Custom commands are auto-filled if omitted.",
                parcel.parcel_id
            );
        }
        _ => {
            observation.description = format!(
                "Vacant commercial parcel {}. Claim it for free from the Chamber of Commerce with /land claim {}.",
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
        lines.push(format!("- {label}. Enter: /enter {}.", parcel.parcel_id));
    }

    observation.description = format!("{}\n{}", observation.description, lines.join("\n"));
    observation
        .available_commands
        .extend(parcels.iter().map(|parcel| SemanticCommand::Enter {
            target: parcel.parcel_id.clone(),
        }));
}

fn overlay_ascii_title(observation: &mut JsonObservation, title: &str) {
    let old_title = observation.title.trim().to_ascii_uppercase();
    if old_title.is_empty() {
        return;
    }

    for line in &mut observation.ascii_art {
        if line.trim() == old_title {
            let indent = line.len() - line.trim_start().len();
            *line = format!("{}[{title}]", " ".repeat(indent));
        }
    }
}

pub(crate) fn render_parcel_list(parcels: &[StoredParcel]) -> String {
    let mut lines = vec!["Commercial Parcels".to_owned()];
    for parcel in parcels {
        let owner = parcel.owner_user.as_deref().unwrap_or("-");
        let title = parcel.title.as_deref().unwrap_or("-");
        lines.push(format!(
            "- {} view={} district={} position={} status={} owner={} title={}",
            parcel.parcel_id,
            parcel.view_id,
            parcel.district,
            parcel.position,
            parcel.status,
            owner,
            title
        ));
    }
    lines.push(
        "Use /land claim <parcel>, /land info <parcel>, or /land transfer <parcel> <user>."
            .to_owned(),
    );
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn render_inbox_items(title: &str, items: &[StoredInboxItem]) -> String {
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
                compact_inbox_field(&item.sender_user),
                compact_inbox_field(&item.subject),
                item.attempts,
                lease
            ));
        }
    }
    lines.push(
        "Use /mail read <id>, /mail claim <id>, /mail ack <id>, or /mail archive <id>.".to_owned(),
    );
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn render_inbox_new_notice(item: &StoredInboxItem) -> String {
    format!(
        "Inbox: new {} #{} from {}\nUse: /mail read {}\n",
        item.kind,
        item.id,
        compact_inbox_field(&item.sender_user),
        item.id
    )
}

pub(crate) fn render_inbox_item(item: &StoredInboxItem) -> String {
    format!(
        "Inbox #{}\nKind: {}\nStatus: {}\nFrom: {}\nSubject: {}\nCreated: {}\nAttempts: {}\nBody: {}\n\n",
        item.id,
        item.kind,
        item.status,
        item.sender_user,
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
        "Parcel {}\nView: {}\nDistrict: {} {}\nStatus: {}\nOwner: {}\nTitle: {}\nDescription: {}\nStyle: {}\nPrompt: {}\nCommands: {}\n\n",
        parcel.parcel_id,
        parcel.view_id,
        parcel.district,
        parcel.position,
        parcel.status,
        parcel.owner_user.as_deref().unwrap_or("-"),
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
        let Some((_, after_preview)) = entry.split_once("preview=") else {
            continue;
        };
        let preview = after_preview
            .split_whitespace()
            .next()
            .unwrap_or(after_preview)
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if !preview.is_empty() {
            return Some(preview.to_owned());
        }
    }
    None
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

pub(crate) fn non_empty(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn send_prompt(session: &mut Session, channel: ChannelId) -> Result<()> {
    session.data(channel, Chrome::PROMPT.as_bytes().to_vec())?;
    Ok(())
}

pub(crate) fn send_command_error(
    session: &mut Session,
    channel: ChannelId,
    error: anyhow::Error,
    prompt: bool,
) -> Result<()> {
    session.data(channel, format!("{error}\r\n").into_bytes())?;
    if prompt {
        send_prompt(session, channel)?;
    }
    Ok(())
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
        format!(
            "\r\nConnection note: {status}\r\n\
             This SSH channel cannot receive more commands after client stdin is closed, so Xagora is closing it cleanly.\r\n\
             Your player state is saved by SSH identity. Reconnect to continue from the latest observation.\r\n\
             Non-TTY agent skill:\r\n\
             1. Connect with ssh -T -p <port> <user>@<host>.\r\n\
             2. Read the observation and the Available command list.\r\n\
             3. Send exactly one chosen command, or a short finite batch ending with /quit.\r\n\
             4. When this channel closes, do not wait on it. Reconnect and repeat from step 1.\r\n\
             Example batch:\r\n\
               printf '/look\\n/go east\\n/history\\n/quit\\n' | ssh -T -p <port> <user>@<host>\r\n"
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
        session.data(channel, format!("{empty}\r\n").into_bytes())?;
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
            format!(
                "- [{}] {} from {}{}: {}\r\n",
                message.created_at, message.kind, message.sender_user, expiry, message.body
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
    let payload = format!("\r\n{message}\r\n{}", Chrome::PROMPT);
    for recipient in recipients {
        let _ = recipient
            .handle
            .data(recipient.channel_id, payload.clone())
            .await;
    }
}

pub(crate) fn exec_help() -> &'static str {
    "Xagora is an open world served over SSH, not a general-purpose Unix shell.\n\
     Open an SSH shell: ssh -p <port> <user>@<host>\n\
     Keep the SSH connection open, read each observation, choose one Available command, send it, and continue.\n\
     Common commands inside the session: /look, /go east, /go west, /inspect board, /read board, /help.\n\
     Wallet commands: /balance, /pay <user> <amount> [memo], /pay requests, /pay accept <id>."
}

#[cfg(test)]
mod tests {
    use xagora_core::JsonObservation;
    use xagora_runtime::render_text_observation;
    use xagora_storage::StoredParcel;

    use super::overlay_parcel_observation;

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
        assert!(!rendered.contains("NORTH COMMERCIAL PARCEL 01"));
    }
}
