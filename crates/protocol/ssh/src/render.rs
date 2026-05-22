//! Rendering and channel output helpers for SSH sessions.

use anyhow::Result;
use russh::ChannelId;
use russh::server::Session;
use xagora_core::{JsonObservation, SemanticCommand};
use xagora_runtime::{Chrome, render_text_events, render_text_observation};
use xagora_storage::{
    PgStorage, StoredOperatorCommand, StoredParcel, StoredPaymentRequest, StoredWorldMessage,
    TEST_CURRENCY,
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
        SemanticCommand::Look | SemanticCommand::Map | SemanticCommand::Move { .. }
    )
}

pub(crate) fn overlay_parcel_observation(observation: &mut JsonObservation, parcel: &StoredParcel) {
    let owner = parcel.owner_user.as_deref().unwrap_or("unclaimed");
    match parcel.status.as_str() {
        "built" => {
            if let Some(title) = &parcel.title {
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
                "Commercial parcel {} is claimed by {owner} but not built yet.\nOwner can edit here with /build title <text>, /build description <text>, /build style <text>, /build prompt <text>, /build commands <text>, then /build publish.",
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
     /build title <shop title>\r\n\
     /build description <shop description>\r\n\
     /build style <style note>\r\n\
     /build prompt <operator prompt shown to visitors>\r\n\
     /build commands <custom command help>\r\n\
     /build publish\r\n"
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
    let messages = storage
        .recent_mailbox_messages(&identity.user, &identity.player_id, 10)
        .await?;
    if messages.is_empty() {
        return Ok(());
    }

    session.data(
        channel,
        format!("Mailbox: {} message(s). Use /mailbox.\r\n", messages.len()).into_bytes(),
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
