//! SMTP and IMAP sidecar for agent mailbox integrations.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;
use hinemos_app::{InboxItemView, MailAuthTokenView, MailDaemonStore};
use hinemos_storage::{
    INBOX_FILTER_ALL, INBOX_STATUS_ACKED, INBOX_STATUS_UNREAD, PgStorage, StoredInboxItem,
    StoredMailAuthToken,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use crate::config::{mail_domain_from_env, mask_database_url, normalize_mail_target};
use crate::mail_protocol::*;

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
struct MailState<S> {
    storage: S,
    mail_domain: Option<String>,
}

#[derive(Debug, Clone)]
struct MailIdentity {
    user: String,
    player_id: String,
}

#[derive(Debug, Default)]
struct SmtpSessionState {
    identity: Option<MailIdentity>,
    sender: String,
    recipients: Vec<String>,
}

#[derive(Debug, Default)]
struct ImapSessionState {
    identity: Option<MailIdentity>,
    selected: Vec<StoredInboxItem>,
}

/// Runs SMTP and IMAP listeners until one listener exits.
pub async fn run_mail_daemon(args: MailArgs) -> Result<()> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set in the environment or .env")?;
    let storage = PgStorage::connect(&database_url).await?;
    storage.migrate().await?;
    run_mail_daemon_with_storage(args, storage, mail_domain_from_env(), &database_url).await
}

async fn run_mail_daemon_with_storage<S>(
    args: MailArgs,
    storage: S,
    mail_domain: Option<String>,
    database_url: &str,
) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let state = Arc::new(MailState {
        storage,
        mail_domain,
    });

    println!("Hinemos SMTP sidecar listening on {}", args.smtp_bind);
    println!("Hinemos IMAP sidecar listening on {}", args.imap_bind);
    println!("Database configured: {}", mask_database_url(database_url));
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

async fn authenticate_mail_token<S>(
    state: &MailState<S>,
    username: &str,
    token: &str,
) -> Result<Option<MailIdentity>>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let identity = state
        .storage
        .verify_mail_auth_token(username, token)
        .await?;
    Ok(identity.map(|identity| MailIdentity {
        user: identity.username().to_owned(),
        player_id: identity.player_id().to_owned(),
    }))
}

async fn run_smtp_listener<S>(bind: SocketAddr, state: Arc<MailState<S>>) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
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

async fn handle_smtp_connection<S>(stream: TcpStream, state: Arc<MailState<S>>) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let mut reader = BufReader::new(stream);
    write_line(&mut reader, "220 hinemos ESMTP ready").await?;
    let mut session = SmtpSessionState::default();

    loop {
        let Some(line) = read_protocol_line(&mut reader).await? else {
            break;
        };
        let (command, rest) = split_command(&line);
        if handle_smtp_command(&mut reader, &state, &mut session, &command, rest).await? {
            break;
        }
    }
    Ok(())
}

async fn handle_smtp_command<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut SmtpSessionState,
    command: &str,
    rest: &str,
) -> Result<bool>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    match command {
        "EHLO" | "HELO" => handle_smtp_helo(reader).await?,
        "AUTH" => handle_smtp_auth_command(reader, state, session, rest).await?,
        "MAIL" => handle_smtp_mail_from(reader, session, rest).await?,
        "RCPT" => handle_smtp_recipient(reader, state, session, rest).await?,
        "DATA" => handle_smtp_data(reader, state, session).await?,
        "RSET" => handle_smtp_reset(reader, session).await?,
        "NOOP" => write_line(reader, "250 2.0.0 OK").await?,
        "STARTTLS" => write_line(reader, "454 4.7.0 TLS not available").await?,
        "QUIT" => {
            write_line(reader, "221 2.0.0 Bye").await?;
            return Ok(true);
        }
        _ => write_line(reader, "502 5.5.1 Command not implemented").await?,
    }
    Ok(false)
}

async fn handle_smtp_helo(reader: &mut BufReader<TcpStream>) -> Result<()> {
    write_line(reader, "250-hinemos").await?;
    write_line(reader, "250-AUTH PLAIN LOGIN").await?;
    write_line(reader, "250 SIZE 1048576").await
}

async fn handle_smtp_auth_command<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut SmtpSessionState,
    rest: &str,
) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    if let Some(auth_identity) = handle_smtp_auth(reader, state, rest).await? {
        session.identity = Some(auth_identity);
        write_line(reader, "235 2.7.0 Authentication successful").await
    } else {
        write_line(reader, "535 5.7.8 Authentication failed").await
    }
}

async fn handle_smtp_mail_from(
    reader: &mut BufReader<TcpStream>,
    session: &mut SmtpSessionState,
    rest: &str,
) -> Result<()> {
    if session.identity.is_none() {
        write_line(reader, "530 5.7.0 Authentication required").await?;
        return Ok(());
    }
    let Some(address) = smtp_path_after(rest, "FROM:") else {
        write_line(reader, "501 5.5.4 Syntax: MAIL FROM:<address>").await?;
        return Ok(());
    };
    session.sender = address;
    session.recipients.clear();
    write_line(reader, "250 2.1.0 Sender OK").await
}

async fn handle_smtp_recipient<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut SmtpSessionState,
    rest: &str,
) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken>,
{
    if session.sender.is_empty() {
        write_line(reader, "503 5.5.1 Need MAIL FROM first").await?;
        return Ok(());
    }
    let Some(address) = smtp_path_after(rest, "TO:") else {
        write_line(reader, "501 5.5.4 Syntax: RCPT TO:<address>").await?;
        return Ok(());
    };
    match normalize_mail_target(&address, state.mail_domain.as_deref()) {
        Ok(target) => {
            session.recipients.push(target);
            write_line(reader, "250 2.1.5 Recipient OK").await
        }
        Err(error) => write_line(reader, &format!("550 5.1.1 {error}")).await,
    }
}

async fn handle_smtp_data<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut SmtpSessionState,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let Some(identity) = session.identity.as_ref() else {
        write_line(reader, "530 5.7.0 Authentication required").await?;
        return Ok(());
    };
    if session.recipients.is_empty() {
        write_line(reader, "503 5.5.1 Need RCPT TO first").await?;
        return Ok(());
    }
    write_line(reader, "354 End data with <CR><LF>.<CR><LF>").await?;
    let raw_message = read_smtp_data(reader).await?;
    let parsed = parse_email_message(&raw_message);
    for recipient in &session.recipients {
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
    session.sender.clear();
    session.recipients.clear();
    write_line(reader, "250 2.0.0 Message accepted").await
}

async fn handle_smtp_reset(
    reader: &mut BufReader<TcpStream>,
    session: &mut SmtpSessionState,
) -> Result<()> {
    session.sender.clear();
    session.recipients.clear();
    write_line(reader, "250 2.0.0 Reset OK").await
}

async fn handle_smtp_auth<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    rest: &str,
) -> Result<Option<MailIdentity>>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
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

async fn run_imap_listener<S>(bind: SocketAddr, state: Arc<MailState<S>>) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
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

async fn handle_imap_connection<S>(stream: TcpStream, state: Arc<MailState<S>>) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>
        + Send
        + Sync
        + 'static,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let mut reader = BufReader::new(stream);
    write_line(&mut reader, "* OK Hinemos IMAP4rev1 ready").await?;
    let mut session = ImapSessionState::default();

    loop {
        let Some(line) = read_protocol_line(&mut reader).await? else {
            break;
        };
        let (tag, after_tag) = split_first_token(&line);
        if tag.is_empty() {
            continue;
        }
        let (command, rest) = normalize_imap_command(after_tag);
        if handle_imap_command(&mut reader, &state, &mut session, tag, &command, rest).await? {
            break;
        }
    }
    Ok(())
}

fn normalize_imap_command(input: &str) -> (String, &str) {
    let (command, rest) = split_command(input);
    if command == "UID" {
        let (uid_command, uid_rest) = split_command(rest);
        (format!("UID {uid_command}"), uid_rest)
    } else {
        (command, rest)
    }
}

async fn handle_imap_command<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
    command: &str,
    rest: &str,
) -> Result<bool>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken, InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    match command {
        "CAPABILITY" => handle_imap_capability(reader, tag).await?,
        "ID" => handle_imap_id(reader, tag).await?,
        "NOOP" => tagged_ok(reader, tag, "NOOP completed").await?,
        "STARTTLS" => tagged_no(reader, tag, "STARTTLS unavailable").await?,
        "LOGOUT" => {
            write_line(reader, "* BYE Hinemos IMAP logging out").await?;
            tagged_ok(reader, tag, "LOGOUT completed").await?;
            return Ok(true);
        }
        "LOGIN" => handle_imap_login(reader, state, session, tag, rest).await?,
        "AUTHENTICATE" => handle_imap_authenticate(reader, state, session, tag, rest).await?,
        _ if session.identity.is_none() => {
            tagged_no(reader, tag, "authentication required").await?;
        }
        "ENABLE" => tagged_ok(reader, tag, "ENABLE completed").await?,
        "NAMESPACE" => handle_imap_namespace(reader, tag).await?,
        "LIST" | "LSUB" | "XLIST" => handle_imap_list(reader, tag, command).await?,
        "SUBSCRIBE" | "UNSUBSCRIBE" => tagged_ok(reader, tag, "subscription updated").await?,
        "CHECK" => tagged_ok(reader, tag, "CHECK completed").await?,
        "CLOSE" => {
            session.selected.clear();
            tagged_ok(reader, tag, "CLOSE completed").await?;
        }
        "EXPUNGE" => tagged_ok(reader, tag, "EXPUNGE completed").await?,
        "SELECT" | "EXAMINE" => handle_imap_select(reader, state, session, tag).await?,
        "STATUS" => handle_imap_status(reader, state, session, tag).await?,
        "SEARCH" => handle_imap_search(reader, state, session, tag, rest, false).await?,
        "UID SEARCH" => handle_imap_search(reader, state, session, tag, rest, true).await?,
        "FETCH" | "UID FETCH" => {
            handle_imap_fetch_command(reader, state, session, tag, rest, command == "UID FETCH")
                .await?
        }
        "STORE" | "UID STORE" => {
            handle_imap_store_command(reader, state, session, tag, rest, command == "UID STORE")
                .await?
        }
        "IDLE" => handle_imap_idle(reader, state, session, tag).await?,
        "APPEND" => {
            handle_imap_append(reader, rest).await?;
            tagged_ok(reader, tag, "APPEND completed").await?;
        }
        _ => tagged_bad(reader, tag, "command not implemented").await?,
    }
    Ok(false)
}

async fn handle_imap_capability(reader: &mut BufReader<TcpStream>, tag: &str) -> Result<()> {
    write_line(
        reader,
        "* CAPABILITY IMAP4rev1 ID IDLE NAMESPACE AUTH=PLAIN LOGIN-REFERRALS",
    )
    .await?;
    tagged_ok(reader, tag, "CAPABILITY completed").await
}

async fn handle_imap_id(reader: &mut BufReader<TcpStream>, tag: &str) -> Result<()> {
    write_line(reader, r#"* ID ("name" "Hinemos" "vendor" "Hinemos")"#).await?;
    tagged_ok(reader, tag, "ID completed").await
}

async fn handle_imap_namespace(reader: &mut BufReader<TcpStream>, tag: &str) -> Result<()> {
    write_line(reader, r#"* NAMESPACE (("" "/")) NIL NIL"#).await?;
    tagged_ok(reader, tag, "NAMESPACE completed").await
}

async fn handle_imap_list(
    reader: &mut BufReader<TcpStream>,
    tag: &str,
    command: &str,
) -> Result<()> {
    let response = if command == "LSUB" { "LSUB" } else { "LIST" };
    write_line(
        reader,
        &format!(r#"* {response} (\HasNoChildren) "/" "INBOX""#),
    )
    .await?;
    tagged_ok(reader, tag, &format!("{command} completed")).await
}

async fn handle_imap_login<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
    rest: &str,
) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let Some((username, token)) = parse_imap_login(rest) else {
        tagged_no(reader, tag, "LOGIN expects username and password").await?;
        return Ok(());
    };
    match authenticate_mail_token(state, &username, &token).await? {
        Some(authenticated) => {
            session.identity = Some(authenticated);
            tagged_ok(reader, tag, "LOGIN completed").await
        }
        None => tagged_no(reader, tag, "authentication failed").await,
    }
}

async fn handle_imap_authenticate<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
    rest: &str,
) -> Result<()>
where
    S: MailDaemonStore<MailAuthToken = StoredMailAuthToken>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    if !rest.eq_ignore_ascii_case("PLAIN") {
        tagged_no(reader, tag, "unsupported authentication mechanism").await?;
        return Ok(());
    }
    write_line(reader, "+").await?;
    let payload = read_protocol_line(reader).await?.unwrap_or_default();
    let Some((username, token)) = decode_auth_plain(&payload) else {
        tagged_no(reader, tag, "authentication failed").await?;
        return Ok(());
    };
    match authenticate_mail_token(state, &username, &token).await? {
        Some(authenticated) => {
            session.identity = Some(authenticated);
            tagged_ok(reader, tag, "AUTHENTICATE completed").await
        }
        None => tagged_no(reader, tag, "authentication failed").await,
    }
}

async fn handle_imap_select<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let identity = session.identity.as_ref().expect("checked");
    session.selected = load_imap_mailbox(state, identity).await?;
    let unseen = session
        .selected
        .iter()
        .filter(|item| item.status() == INBOX_STATUS_UNREAD)
        .count();
    write_line(reader, &format!("* {} EXISTS", session.selected.len())).await?;
    write_line(reader, &format!("* {unseen} RECENT")).await?;
    write_line(reader, r"* FLAGS (\Seen)").await?;
    write_line(reader, r"* OK [PERMANENTFLAGS (\Seen)] Limited flags").await?;
    tagged_ok(reader, tag, "SELECT completed").await
}

async fn handle_imap_status<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &ImapSessionState,
    tag: &str,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let items = load_imap_mailbox(state, session.identity.as_ref().expect("checked")).await?;
    let unseen = items
        .iter()
        .filter(|item| item.status() == INBOX_STATUS_UNREAD)
        .count();
    write_line(
        reader,
        &format!(
            r#"* STATUS "INBOX" (MESSAGES {} UNSEEN {unseen})"#,
            items.len()
        ),
    )
    .await?;
    tagged_ok(reader, tag, "STATUS completed").await
}

async fn ensure_imap_selected<S>(state: &MailState<S>, session: &mut ImapSessionState) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    if session.selected.is_empty() {
        session.selected =
            load_imap_mailbox(state, session.identity.as_ref().expect("checked")).await?;
    }
    Ok(())
}

async fn handle_imap_search<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
    rest: &str,
    use_uid: bool,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    ensure_imap_selected(state, session).await?;
    let ids = imap_search_ids(&session.selected, rest, use_uid);
    write_line(reader, &format!("* SEARCH {}", ids.join(" "))).await?;
    tagged_ok(reader, tag, "SEARCH completed").await
}

async fn handle_imap_fetch_command<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
    rest: &str,
    use_uid: bool,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    ensure_imap_selected(state, session).await?;
    handle_imap_fetch(reader, &session.selected, rest, use_uid).await?;
    tagged_ok(reader, tag, "FETCH completed").await
}

async fn handle_imap_store_command<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
    rest: &str,
    use_uid: bool,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    ensure_imap_selected(state, session).await?;
    let identity = session.identity.as_ref().expect("checked");
    handle_imap_store(state, identity, &session.selected, rest, use_uid).await?;
    session.selected = load_imap_mailbox(state, identity).await?;
    tagged_ok(reader, tag, "STORE completed").await
}

async fn handle_imap_idle<S>(
    reader: &mut BufReader<TcpStream>,
    state: &MailState<S>,
    session: &mut ImapSessionState,
    tag: &str,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    ensure_imap_selected(state, session).await?;
    let mut known_exists = session.selected.len();
    write_line(reader, "+ idling").await?;
    let mut client_closed = false;

    loop {
        match timeout(Duration::from_secs(1), read_protocol_line(reader)).await {
            Ok(Ok(Some(line))) => {
                if line.eq_ignore_ascii_case("DONE") {
                    break;
                }
            }
            Ok(Ok(None)) => {
                client_closed = true;
                break;
            }
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                let identity = session.identity.as_ref().expect("checked").clone();
                let latest = load_imap_mailbox(state, &identity).await?;
                if latest.len() != known_exists {
                    known_exists = latest.len();
                    write_line(reader, &format!("* {known_exists} EXISTS")).await?;
                }
                session.selected = latest;
            }
        }
    }

    if client_closed {
        Ok(())
    } else {
        tagged_ok(reader, tag, "IDLE completed").await
    }
}

async fn load_imap_mailbox<S>(
    state: &MailState<S>,
    identity: &MailIdentity,
) -> Result<Vec<S::InboxItem>>
where
    S: MailDaemonStore<InboxItem = StoredInboxItem>,
    S::InboxItem: InboxItemView,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let mut items = state
        .storage
        .list_inbox_items(
            &identity.user,
            &identity.player_id,
            Some(INBOX_FILTER_ALL),
            500,
        )
        .await?;
    items.retain(|item| imap_visible_inbox_kind(item.kind()));
    items.sort_by_key(|item| item.id());
    Ok(items)
}

fn imap_visible_inbox_kind(kind: &str) -> bool {
    matches!(kind, "mail" | "shop_command")
}

fn imap_search_ids<I: InboxItemView>(items: &[I], query: &str, use_uid: bool) -> Vec<String> {
    let unseen_only = query.to_ascii_uppercase().contains("UNSEEN");
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| !unseen_only || item.status() == INBOX_STATUS_UNREAD)
        .map(|(index, item)| {
            if use_uid {
                item.id().to_string()
            } else {
                (index + 1).to_string()
            }
        })
        .collect()
}

async fn handle_imap_fetch<I>(
    reader: &mut BufReader<TcpStream>,
    selected: &[I],
    rest: &str,
    use_uid: bool,
) -> Result<()>
where
    I: InboxItemView,
{
    let (set, _attributes) = split_first_token(rest);
    let sequences = if use_uid {
        expand_imap_uid_set(set, selected)
    } else {
        expand_imap_set(set, selected.len())
    };
    for sequence in sequences {
        let Some(item) = selected.get(sequence - 1) else {
            continue;
        };
        let message = render_rfc822_message(item);
        let flags = if item.status() == INBOX_STATUS_UNREAD {
            ""
        } else {
            r"\Seen"
        };
        write_line(
            reader,
            &format!(
                "* {sequence} FETCH (UID {} FLAGS ({flags}) INTERNALDATE \"{}\" ENVELOPE {} BODYSTRUCTURE {} RFC822.SIZE {} BODY[] {{{}}}",
                item.id(),
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

async fn handle_imap_store<S, I>(
    state: &MailState<S>,
    identity: &MailIdentity,
    selected: &[I],
    rest: &str,
    use_uid: bool,
) -> Result<()>
where
    S: MailDaemonStore<InboxItem = I>,
    I: InboxItemView,
{
    let upper = rest.to_ascii_uppercase();
    if !upper.contains("\\SEEN") {
        return Ok(());
    }
    let (set, _rest) = split_first_token(rest);
    let sequences = if use_uid {
        expand_imap_uid_set(set, selected)
    } else {
        expand_imap_set(set, selected.len())
    };
    for sequence in sequences {
        let Some(item) = selected.get(sequence - 1) else {
            continue;
        };
        let _ = state
            .storage
            .finish_inbox_item(
                &identity.user,
                &identity.player_id,
                item.id(),
                INBOX_STATUS_ACKED,
            )
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

fn render_rfc822_message(item: &impl InboxItemView) -> String {
    format!(
        "From: {}\r\nTo: {}\r\nSubject: {}\r\nDate: {}\r\nMessage-ID: <hinemos-{}@local>\r\n\r\n{}\r\n",
        item.sender_user(),
        item.subject(),
        sanitize_header(item.subject()),
        item.created_at(),
        item.id(),
        item.body()
    )
}

fn imap_internal_date(_item: &impl InboxItemView) -> &'static str {
    "02-Jun-2026 00:00:00 +0000"
}

fn render_imap_envelope(item: &impl InboxItemView) -> String {
    let from = render_imap_address(item.sender_user());
    let to = render_imap_address(item.subject());
    format!(
        "(\"{}\" \"{}\" ({from}) ({from}) ({from}) ({to}) NIL NIL NIL \"<hinemos-{}@local>\")",
        imap_internal_date(item),
        imap_quote(item.subject()),
        item.id()
    )
}

fn render_bodystructure(item: &impl InboxItemView) -> String {
    let body_lines = item.body().lines().count().max(1);
    format!(
        "(\"TEXT\" \"PLAIN\" NIL NIL NIL \"7BIT\" {} {body_lines} NIL NIL NIL)",
        item.body().len()
    )
}

fn render_imap_address(user: &str) -> String {
    let local = imap_quote(user);
    format!("(NIL NIL \"{local}\" \"hinemos.local\")")
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

fn expand_imap_uid_set<I: InboxItemView>(input: &str, selected: &[I]) -> Vec<usize> {
    let max_uid = selected.iter().map(InboxItemView::id).max().unwrap_or(0);
    let mut sequences = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start, end)) = part.split_once(':') {
            let Some(start) = parse_imap_uid(start, max_uid) else {
                continue;
            };
            let Some(end) = parse_imap_uid(end, max_uid) else {
                continue;
            };
            let lower = start.min(end);
            let upper = start.max(end);
            sequences.extend(
                selected
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.id() >= lower && item.id() <= upper)
                    .map(|(index, _)| index + 1),
            );
        } else if let Some(uid) = parse_imap_uid(part, max_uid) {
            sequences.extend(
                selected
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.id() == uid)
                    .map(|(index, _)| index + 1),
            );
        }
    }
    sequences.sort_unstable();
    sequences.dedup();
    sequences
}

fn parse_imap_uid(input: &str, max_uid: i64) -> Option<i64> {
    let input = input.trim();
    if input == "*" {
        (max_uid > 0).then_some(max_uid)
    } else {
        input.parse::<i64>().ok()
    }
}

#[cfg(test)]
mod mail_tests {
    use super::imap_visible_inbox_kind;

    #[test]
    fn imap_exposes_mail_and_shop_commands_but_not_player_action_items() {
        assert!(imap_visible_inbox_kind("mail"));
        assert!(imap_visible_inbox_kind("shop_command"));
        assert!(!imap_visible_inbox_kind("payment_request"));
    }
}
