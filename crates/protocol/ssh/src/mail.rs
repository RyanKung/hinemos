//! SMTP and IMAP sidecar for agent mailbox integrations.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use clap::Args;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use xagora_storage::{PgStorage, StoredInboxItem, StoredMailAuthToken};

use crate::config::{mail_domain_from_env, mask_database_url, normalize_mail_target};

/// SMTP/IMAP sidecar command-line arguments.
#[derive(Debug, Clone, Args)]
pub struct MailArgs {
    /// SMTP bind address. Defaults to loopback because SMTP AUTH is plaintext unless TLS is added by a proxy.
    #[arg(long, default_value = "127.0.0.1:2525")]
    pub(crate) smtp_bind: SocketAddr,

    /// IMAP bind address. Defaults to loopback because IMAP LOGIN is plaintext unless TLS is added by a proxy.
    #[arg(long, default_value = "127.0.0.1:2143")]
    pub(crate) imap_bind: SocketAddr,
}

#[derive(Debug, Clone)]
struct MailState {
    storage: PgStorage,
    mail_domain: Option<String>,
}

#[derive(Debug, Clone)]
struct MailIdentity {
    user: String,
    player_id: String,
}

/// Runs SMTP and IMAP listeners until one listener exits.
pub async fn run_mail_daemon(args: MailArgs) -> Result<()> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set in the environment or .env")?;
    let storage = PgStorage::connect(&database_url).await?;
    storage.migrate().await?;
    let state = Arc::new(MailState {
        storage,
        mail_domain: mail_domain_from_env(),
    });

    println!("Xagora SMTP sidecar listening on {}", args.smtp_bind);
    println!("Xagora IMAP sidecar listening on {}", args.imap_bind);
    println!("Database configured: {}", mask_database_url(&database_url));
    if let Some(domain) = &state.mail_domain {
        println!("Mail domain configured: {domain}");
    }

    let smtp_state = Arc::clone(&state);
    let imap_state = Arc::clone(&state);
    tokio::try_join!(
        run_smtp_listener(args.smtp_bind, smtp_state),
        run_imap_listener(args.imap_bind, imap_state)
    )?;
    Ok(())
}

async fn authenticate_mail_token(
    state: &MailState,
    username: &str,
    token: &str,
) -> Result<Option<MailIdentity>> {
    let identity = state
        .storage
        .verify_mail_auth_token(username, token)
        .await?;
    Ok(identity.map(
        |StoredMailAuthToken {
             username,
             player_id,
             ..
         }| MailIdentity {
            user: username,
            player_id,
        },
    ))
}

async fn run_smtp_listener(bind: SocketAddr, state: Arc<MailState>) -> Result<()> {
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind SMTP listener on {bind}"))?;
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_smtp_connection(stream, state).await {
                eprintln!("smtp connection {peer} failed: {error:#}");
            }
        });
    }
}

async fn handle_smtp_connection(stream: TcpStream, state: Arc<MailState>) -> Result<()> {
    let mut reader = BufReader::new(stream);
    write_line(&mut reader, "220 xagora ESMTP ready").await?;
    let mut identity: Option<MailIdentity> = None;
    let mut sender = String::new();
    let mut recipients = Vec::<String>::new();

    loop {
        let Some(line) = read_protocol_line(&mut reader).await? else {
            break;
        };
        let (command, rest) = split_command(&line);
        match command.as_str() {
            "EHLO" | "HELO" => {
                write_line(&mut reader, "250-xagora").await?;
                write_line(&mut reader, "250-AUTH PLAIN LOGIN").await?;
                write_line(&mut reader, "250 SIZE 1048576").await?;
            }
            "AUTH" => {
                if let Some(auth_identity) = handle_smtp_auth(&mut reader, &state, rest).await? {
                    identity = Some(auth_identity);
                    write_line(&mut reader, "235 2.7.0 Authentication successful").await?;
                } else {
                    write_line(&mut reader, "535 5.7.8 Authentication failed").await?;
                }
            }
            "MAIL" => {
                if identity.is_none() {
                    write_line(&mut reader, "530 5.7.0 Authentication required").await?;
                    continue;
                }
                let Some(address) = smtp_path_after(rest, "FROM:") else {
                    write_line(&mut reader, "501 5.5.4 Syntax: MAIL FROM:<address>").await?;
                    continue;
                };
                sender = address;
                recipients.clear();
                write_line(&mut reader, "250 2.1.0 Sender OK").await?;
            }
            "RCPT" => {
                if sender.is_empty() {
                    write_line(&mut reader, "503 5.5.1 Need MAIL FROM first").await?;
                    continue;
                }
                let Some(address) = smtp_path_after(rest, "TO:") else {
                    write_line(&mut reader, "501 5.5.4 Syntax: RCPT TO:<address>").await?;
                    continue;
                };
                match normalize_mail_target(&address, state.mail_domain.as_deref()) {
                    Ok(target) => {
                        recipients.push(target);
                        write_line(&mut reader, "250 2.1.5 Recipient OK").await?;
                    }
                    Err(error) => {
                        write_line(&mut reader, &format!("550 5.1.1 {error}")).await?;
                    }
                }
            }
            "DATA" => {
                let Some(identity) = identity.as_ref() else {
                    write_line(&mut reader, "530 5.7.0 Authentication required").await?;
                    continue;
                };
                if recipients.is_empty() {
                    write_line(&mut reader, "503 5.5.1 Need RCPT TO first").await?;
                    continue;
                }
                write_line(&mut reader, "354 End data with <CR><LF>.<CR><LF>").await?;
                let raw_message = read_smtp_data(&mut reader).await?;
                let parsed = parse_email_message(&raw_message);
                for recipient in &recipients {
                    state
                        .storage
                        .save_mail_message_with_subject(
                            &identity.user,
                            &identity.player_id,
                            recipient,
                            &parsed.subject,
                            &parsed.body,
                        )
                        .await?;
                }
                sender.clear();
                recipients.clear();
                write_line(&mut reader, "250 2.0.0 Message accepted").await?;
            }
            "RSET" => {
                sender.clear();
                recipients.clear();
                write_line(&mut reader, "250 2.0.0 Reset OK").await?;
            }
            "NOOP" => write_line(&mut reader, "250 2.0.0 OK").await?,
            "QUIT" => {
                write_line(&mut reader, "221 2.0.0 Bye").await?;
                break;
            }
            _ => write_line(&mut reader, "502 5.5.1 Command not implemented").await?,
        }
    }
    Ok(())
}

async fn handle_smtp_auth(
    reader: &mut BufReader<TcpStream>,
    state: &MailState,
    rest: &str,
) -> Result<Option<MailIdentity>> {
    let (mechanism, initial) = split_command(rest);
    match mechanism.as_str() {
        "PLAIN" => {
            let payload = if initial.is_empty() {
                write_line(reader, "334 ").await?;
                read_protocol_line(reader).await?.unwrap_or_default()
            } else {
                initial.to_owned()
            };
            let Some((username, token)) = decode_auth_plain(&payload) else {
                return Ok(None);
            };
            authenticate_mail_token(state, &username, &token).await
        }
        "LOGIN" => {
            let username = if initial.is_empty() {
                write_line(reader, "334 VXNlcm5hbWU6").await?;
                read_protocol_line(reader).await?.unwrap_or_default()
            } else {
                initial.to_owned()
            };
            write_line(reader, "334 UGFzc3dvcmQ6").await?;
            let password = read_protocol_line(reader).await?.unwrap_or_default();
            let username = decode_base64_text(&username).unwrap_or_default();
            let token = decode_base64_text(&password).unwrap_or_default();
            authenticate_mail_token(state, &username, &token).await
        }
        _ => Ok(None),
    }
}

async fn run_imap_listener(bind: SocketAddr, state: Arc<MailState>) -> Result<()> {
    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind IMAP listener on {bind}"))?;
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_imap_connection(stream, state).await {
                eprintln!("imap connection {peer} failed: {error:#}");
            }
        });
    }
}

async fn handle_imap_connection(stream: TcpStream, state: Arc<MailState>) -> Result<()> {
    let mut reader = BufReader::new(stream);
    write_line(&mut reader, "* OK Xagora IMAP4rev1 ready").await?;
    let mut identity: Option<MailIdentity> = None;
    let mut selected = Vec::<StoredInboxItem>::new();

    loop {
        let Some(line) = read_protocol_line(&mut reader).await? else {
            break;
        };
        let (tag, after_tag) = split_first_token(&line);
        if tag.is_empty() {
            continue;
        }
        let (mut command, mut rest) = split_command(after_tag);
        if command == "UID" {
            let (uid_command, uid_rest) = split_command(rest);
            command = format!("UID {uid_command}");
            rest = uid_rest;
        }
        match command.as_str() {
            "CAPABILITY" => {
                write_line(
                    &mut reader,
                    "* CAPABILITY IMAP4rev1 AUTH=PLAIN LOGIN-REFERRALS",
                )
                .await?;
                tagged_ok(&mut reader, tag, "CAPABILITY completed").await?;
            }
            "NOOP" => tagged_ok(&mut reader, tag, "NOOP completed").await?,
            "LOGOUT" => {
                write_line(&mut reader, "* BYE Xagora IMAP logging out").await?;
                tagged_ok(&mut reader, tag, "LOGOUT completed").await?;
                break;
            }
            "LOGIN" => {
                let Some((username, token)) = parse_imap_login(rest) else {
                    tagged_no(&mut reader, tag, "LOGIN expects username and password").await?;
                    continue;
                };
                match authenticate_mail_token(&state, &username, &token).await? {
                    Some(authenticated) => {
                        identity = Some(authenticated);
                        tagged_ok(&mut reader, tag, "LOGIN completed").await?;
                    }
                    None => tagged_no(&mut reader, tag, "authentication failed").await?,
                }
            }
            "AUTHENTICATE" => {
                if !rest.eq_ignore_ascii_case("PLAIN") {
                    tagged_no(&mut reader, tag, "unsupported authentication mechanism").await?;
                    continue;
                }
                write_line(&mut reader, "+").await?;
                let payload = read_protocol_line(&mut reader).await?.unwrap_or_default();
                let Some((username, token)) = decode_auth_plain(&payload) else {
                    tagged_no(&mut reader, tag, "authentication failed").await?;
                    continue;
                };
                match authenticate_mail_token(&state, &username, &token).await? {
                    Some(authenticated) => {
                        identity = Some(authenticated);
                        tagged_ok(&mut reader, tag, "AUTHENTICATE completed").await?;
                    }
                    None => tagged_no(&mut reader, tag, "authentication failed").await?,
                }
            }
            _ if identity.is_none() => {
                tagged_no(&mut reader, tag, "authentication required").await?;
            }
            "LIST" | "LSUB" => {
                write_line(&mut reader, r#"* LIST (\HasNoChildren) "/" "INBOX""#).await?;
                tagged_ok(&mut reader, tag, "LIST completed").await?;
            }
            "SELECT" | "EXAMINE" => {
                selected = load_imap_mailbox(&state, identity.as_ref().expect("checked")).await?;
                let unseen = selected
                    .iter()
                    .filter(|item| item.status == "unread")
                    .count();
                write_line(&mut reader, &format!("* {} EXISTS", selected.len())).await?;
                write_line(&mut reader, &format!("* {unseen} RECENT")).await?;
                write_line(&mut reader, r"* FLAGS (\Seen)").await?;
                write_line(&mut reader, r"* OK [PERMANENTFLAGS (\Seen)] Limited flags").await?;
                tagged_ok(&mut reader, tag, "SELECT completed").await?;
            }
            "STATUS" => {
                let items = load_imap_mailbox(&state, identity.as_ref().expect("checked")).await?;
                let unseen = items.iter().filter(|item| item.status == "unread").count();
                write_line(
                    &mut reader,
                    &format!(
                        r#"* STATUS "INBOX" (MESSAGES {} UNSEEN {unseen})"#,
                        items.len()
                    ),
                )
                .await?;
                tagged_ok(&mut reader, tag, "STATUS completed").await?;
            }
            "SEARCH" | "UID SEARCH" => {
                if selected.is_empty() {
                    selected =
                        load_imap_mailbox(&state, identity.as_ref().expect("checked")).await?;
                }
                let ids = imap_search_ids(&selected, rest);
                write_line(&mut reader, &format!("* SEARCH {}", ids.join(" "))).await?;
                tagged_ok(&mut reader, tag, "SEARCH completed").await?;
            }
            "FETCH" | "UID FETCH" => {
                if selected.is_empty() {
                    selected =
                        load_imap_mailbox(&state, identity.as_ref().expect("checked")).await?;
                }
                handle_imap_fetch(&mut reader, &selected, rest).await?;
                tagged_ok(&mut reader, tag, "FETCH completed").await?;
            }
            "STORE" | "UID STORE" => {
                if selected.is_empty() {
                    selected =
                        load_imap_mailbox(&state, identity.as_ref().expect("checked")).await?;
                }
                handle_imap_store(&state, identity.as_ref().expect("checked"), &selected, rest)
                    .await?;
                selected = load_imap_mailbox(&state, identity.as_ref().expect("checked")).await?;
                tagged_ok(&mut reader, tag, "STORE completed").await?;
            }
            "APPEND" => {
                handle_imap_append(&mut reader, rest).await?;
                tagged_ok(&mut reader, tag, "APPEND completed").await?;
            }
            _ => tagged_bad(&mut reader, tag, "command not implemented").await?,
        }
    }
    Ok(())
}

async fn load_imap_mailbox(
    state: &MailState,
    identity: &MailIdentity,
) -> Result<Vec<StoredInboxItem>> {
    let mut items = state
        .storage
        .list_inbox_items(&identity.user, &identity.player_id, Some("all"), 500)
        .await?;
    items.retain(|item| item.kind == "mail");
    items.sort_by_key(|item| item.id);
    Ok(items)
}

fn imap_search_ids(items: &[StoredInboxItem], query: &str) -> Vec<String> {
    let unseen_only = query.to_ascii_uppercase().contains("UNSEEN");
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| !unseen_only || item.status == "unread")
        .map(|(index, _)| (index + 1).to_string())
        .collect()
}

async fn handle_imap_fetch(
    reader: &mut BufReader<TcpStream>,
    selected: &[StoredInboxItem],
    rest: &str,
) -> Result<()> {
    let (set, _attributes) = split_first_token(rest);
    for sequence in expand_imap_set(set, selected.len()) {
        let Some(item) = selected.get(sequence - 1) else {
            continue;
        };
        let message = render_rfc822_message(item);
        let flags = if item.status == "unread" {
            ""
        } else {
            r"\Seen"
        };
        write_line(
            reader,
            &format!(
                "* {sequence} FETCH (UID {} FLAGS ({flags}) INTERNALDATE \"{}\" ENVELOPE {} BODYSTRUCTURE {} RFC822.SIZE {} BODY[] {{{}}}",
                item.id,
                imap_internal_date(item),
                render_imap_envelope(item),
                render_bodystructure(item),
                message.len(),
                message.len()
            ),
        )
        .await?;
        reader.get_mut().write_all(message.as_bytes()).await?;
        write_line(reader, ")").await?;
    }
    Ok(())
}

async fn handle_imap_store(
    state: &MailState,
    identity: &MailIdentity,
    selected: &[StoredInboxItem],
    rest: &str,
) -> Result<()> {
    let upper = rest.to_ascii_uppercase();
    if !upper.contains("\\SEEN") {
        return Ok(());
    }
    let (set, _rest) = split_first_token(rest);
    for sequence in expand_imap_set(set, selected.len()) {
        let Some(item) = selected.get(sequence - 1) else {
            continue;
        };
        let _ = state
            .storage
            .finish_inbox_item(&identity.user, &identity.player_id, item.id, "acked")
            .await;
    }
    Ok(())
}

async fn handle_imap_append(reader: &mut BufReader<TcpStream>, rest: &str) -> Result<()> {
    let Some(size) = imap_literal_size(rest) else {
        return Ok(());
    };
    write_line(reader, "+ Ready for literal data").await?;
    let mut literal = vec![0_u8; size];
    reader.read_exact(&mut literal).await?;
    let mut trailing = [0_u8; 2];
    let _ = reader.read_exact(&mut trailing).await;
    Ok(())
}

fn imap_literal_size(input: &str) -> Option<usize> {
    let input = input.trim_end();
    let start = input.rfind('{')?;
    let end = input[start..].find('}')? + start;
    input[start + 1..end]
        .trim_end_matches('+')
        .parse::<usize>()
        .ok()
}

fn render_rfc822_message(item: &StoredInboxItem) -> String {
    format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nDate: {}\r\nMessage-ID: <xagora-{}@local>\r\n\r\n{}\r\n",
        item.sender_user,
        item.recipient_user,
        sanitize_header(&item.subject),
        item.created_at,
        item.id,
        item.body
    )
}

fn imap_internal_date(_item: &StoredInboxItem) -> &'static str {
    "02-Jun-2026 00:00:00 +0000"
}

fn render_imap_envelope(item: &StoredInboxItem) -> String {
    let from = render_imap_address(&item.sender_user);
    let to = render_imap_address(&item.recipient_user);
    format!(
        "(\"{}\" \"{}\" ({from}) ({from}) ({from}) ({to}) NIL NIL NIL \"<xagora-{}@local>\")",
        imap_internal_date(item),
        imap_quote(&item.subject),
        item.id
    )
}

fn render_bodystructure(item: &StoredInboxItem) -> String {
    let body_lines = item.body.lines().count().max(1);
    format!(
        "(\"TEXT\" \"PLAIN\" NIL NIL NIL \"7BIT\" {} {body_lines} NIL NIL NIL)",
        item.body.len()
    )
}

fn render_imap_address(user: &str) -> String {
    let local = imap_quote(user);
    format!("(NIL NIL \"{local}\" \"xagora.local\")")
}

fn imap_quote(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\\' => ['\\', '\\'],
            '"' => ['\\', '"'],
            _ => ['\0', character],
        })
        .filter(|character| *character != '\0')
        .collect()
}

fn expand_imap_set(input: &str, len: usize) -> Vec<usize> {
    let mut values = Vec::new();
    for part in input.split(',') {
        if let Some((start, end)) = part.split_once(':') {
            let start = parse_imap_index(start, len).unwrap_or(1);
            let end = parse_imap_index(end, len).unwrap_or(len);
            for value in start.min(end)..=start.max(end) {
                if value > 0 && value <= len {
                    values.push(value);
                }
            }
        } else if let Some(value) = parse_imap_index(part, len)
            && value > 0
            && value <= len
        {
            values.push(value);
        }
    }
    values.sort_unstable();
    values.dedup();
    values
}

fn parse_imap_index(input: &str, len: usize) -> Option<usize> {
    let input = input.trim();
    if input == "*" {
        Some(len)
    } else {
        input.parse::<usize>().ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedEmail {
    subject: String,
    body: String,
}

fn parse_email_message(raw: &str) -> ParsedEmail {
    let normalized = raw.replace("\r\n", "\n");
    let (headers, body) = normalized.split_once("\n\n").unwrap_or(("", &normalized));
    let headers = parse_headers(headers);
    ParsedEmail {
        subject: headers
            .get("subject")
            .cloned()
            .unwrap_or_else(|| "Private mail".to_owned()),
        body: body.trim_end_matches('\n').to_owned(),
    }
}

fn parse_headers(input: &str) -> HashMap<String, String> {
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut current_key = String::new();
    for line in input.lines() {
        if line.starts_with([' ', '\t']) && !current_key.is_empty() {
            if let Some(value) = headers.get_mut(&current_key) {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        current_key = key.trim().to_ascii_lowercase();
        headers.insert(current_key.clone(), value.trim().to_owned());
    }
    headers
}

async fn read_smtp_data(reader: &mut BufReader<TcpStream>) -> Result<String> {
    let mut data = String::new();
    loop {
        let Some(line) = read_protocol_line(reader).await? else {
            break;
        };
        if line == "." {
            break;
        }
        let line = line.strip_prefix("..").unwrap_or(&line);
        data.push_str(line);
        data.push_str("\r\n");
    }
    Ok(data)
}

async fn read_protocol_line(reader: &mut BufReader<TcpStream>) -> Result<Option<String>> {
    let mut line = String::new();
    let read = reader.read_line(&mut line).await?;
    if read == 0 {
        return Ok(None);
    }
    Ok(Some(
        line.trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_owned(),
    ))
}

async fn write_line(reader: &mut BufReader<TcpStream>, line: &str) -> Result<()> {
    let stream = reader.get_mut();
    stream.write_all(line.as_bytes()).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await?;
    Ok(())
}

async fn tagged_ok(reader: &mut BufReader<TcpStream>, tag: &str, message: &str) -> Result<()> {
    write_line(reader, &format!("{tag} OK {message}")).await
}

async fn tagged_no(reader: &mut BufReader<TcpStream>, tag: &str, message: &str) -> Result<()> {
    write_line(reader, &format!("{tag} NO {message}")).await
}

async fn tagged_bad(reader: &mut BufReader<TcpStream>, tag: &str, message: &str) -> Result<()> {
    write_line(reader, &format!("{tag} BAD {message}")).await
}

fn split_command(input: &str) -> (String, &str) {
    let (command, rest) = split_first_token(input);
    (command.to_ascii_uppercase(), rest)
}

fn split_first_token(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    input
        .split_once(char::is_whitespace)
        .map_or((input, ""), |(head, rest)| (head, rest.trim_start()))
}

fn smtp_path_after(rest: &str, marker: &str) -> Option<String> {
    let rest = rest.trim_start();
    if !rest
        .get(..marker.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(marker))
    {
        return None;
    }
    let value = rest[marker.len()..].trim();
    Some(
        value
            .trim_start_matches('<')
            .trim_end_matches('>')
            .trim()
            .to_owned(),
    )
}

fn decode_auth_plain(input: &str) -> Option<(String, String)> {
    let decoded = decode_base64_bytes(input)?;
    let mut parts = decoded.split(|byte| *byte == 0);
    let _authorization_identity = parts.next()?;
    let username = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let password = String::from_utf8(parts.next()?.to_vec()).ok()?;
    Some((username, password))
}

fn decode_base64_text(input: &str) -> Option<String> {
    String::from_utf8(decode_base64_bytes(input)?).ok()
}

fn decode_base64_bytes(input: &str) -> Option<Vec<u8>> {
    BASE64.decode(input.trim()).ok()
}

fn parse_imap_login(input: &str) -> Option<(String, String)> {
    let (username, rest) = parse_imap_atom_or_string(input)?;
    let (password, _) = parse_imap_atom_or_string(rest)?;
    Some((username, password))
}

fn parse_imap_atom_or_string(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('"') {
        let mut escaped = false;
        let mut value = String::new();
        for (index, character) in rest.char_indices() {
            if escaped {
                value.push(character);
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => return Some((value, &rest[index + 1..])),
                _ => value.push(character),
            }
        }
        None
    } else {
        let (value, rest) = split_first_token(input);
        (!value.is_empty()).then(|| (value.to_owned(), rest))
    }
}

fn sanitize_header(input: &str) -> String {
    input.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::{
        decode_auth_plain, expand_imap_set, parse_email_message, parse_imap_login, smtp_path_after,
    };
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;

    #[test]
    fn decodes_auth_plain() {
        let encoded = BASE64.encode(b"\0alice\0secret");

        assert_eq!(
            decode_auth_plain(&encoded),
            Some(("alice".to_owned(), "secret".to_owned()))
        );
    }

    #[test]
    fn parses_quoted_imap_login() {
        assert_eq!(
            parse_imap_login("\"alice\" \"s e c r e t\""),
            Some(("alice".to_owned(), "s e c r e t".to_owned()))
        );
    }

    #[test]
    fn extracts_smtp_paths_case_insensitively() {
        assert_eq!(
            smtp_path_after("to:<bob@xagora.local>", "TO:"),
            Some("bob@xagora.local".to_owned())
        );
    }

    #[test]
    fn parses_email_subject_and_body() {
        let parsed = parse_email_message("Subject: Hello\r\nFrom: alice\r\n\r\nBody\r\n");

        assert_eq!(parsed.subject, "Hello");
        assert_eq!(parsed.body, "Body");
    }

    #[test]
    fn expands_imap_ranges() {
        assert_eq!(expand_imap_set("1:3,5,*", 8), vec![1, 2, 3, 5, 8]);
    }
}
