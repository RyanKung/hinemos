#![deny(missing_docs)]

//! SSH adapter for the Xagora open-world runtime.

use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[cfg(unix)]
mod admin;
mod auth;
mod config;
mod presence;
mod runtime_state;

use anyhow::{Context, Result};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, ssh_key};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, server};
use tokio::sync::Mutex;
use xagora_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use xagora_core::{
    BuildAction, JsonObservation, LandAction, PayAction, SemanticCommand, ShopAction,
};
use xagora_runtime::{Chrome, SlashParseError, render_text_observation};
use xagora_storage::{
    PgStorage, PlayerStateStore, StoredOperatorCommand, StoredParcel, StoredPaymentRequest,
    StoredWorldMessage, TEST_CURRENCY,
};

use auth::{AuthIdentity, PublicKeyAuthPolicy};
pub use config::SshArgs;
use config::{DaemonConfig, mask_database_url};
use presence::{PresenceDelivery, PresenceRegistry};
use runtime_state::RuntimeHandle;

/// Runs the SSH server and (on Unix) the admin socket listener until shutdown.
pub async fn run_daemon(args: SshArgs) -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = DaemonConfig::from_args(args);
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set in the environment or .env")?;
    let storage = PgStorage::connect(&database_url).await?;
    storage.migrate().await?;
    let world = load_world_from_dir(&cli.world)
        .with_context(|| format!("failed to load world from {}", cli.world.display()))?;
    let runtime = RuntimeHandle::new(world);

    let host_key = load_or_create_host_key(&cli.host_key)
        .with_context(|| format!("failed to load host key from {}", cli.host_key.display()))?;
    let config = Arc::new(russh::server::Config {
        inactivity_timeout: cli.idle_timeout(),
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: Some(Duration::from_millis(250)),
        keys: vec![host_key],
        ..Default::default()
    });
    let shared = Arc::new(SharedState {
        runtime,
        presence: Mutex::new(PresenceRegistry::default()),
        next_connection_id: AtomicU64::new(1),
        auth_policy: PublicKeyAuthPolicy,
        storage,
    });

    #[cfg(unix)]
    {
        let shared_admin = Arc::clone(&shared);
        let admin_socket = cli.admin_socket.clone();
        let world_path = cli.world.clone();
        tokio::spawn(async move {
            if let Err(error) =
                admin::run_admin_listener(admin_socket, shared_admin, world_path).await
            {
                eprintln!("admin listener exited: {error:#}");
            }
        });
    }

    let mut server = SshServer { shared };
    println!("Xagora SSH adapter listening on {}", cli.bind);
    println!("Database configured: {}", mask_database_url(&database_url));
    server.run_on_address(config, cli.bind).await?;
    Ok(())
}

fn load_or_create_host_key(path: &Path) -> Result<PrivateKey> {
    if path.exists() {
        return Ok(PrivateKey::read_openssh_file(path)?);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let private_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)?;
    private_key.write_openssh_file(path, LineEnding::LF)?;
    Ok(private_key)
}

pub(crate) struct SharedState {
    runtime: RuntimeHandle,
    presence: Mutex<PresenceRegistry>,
    next_connection_id: AtomicU64,
    auth_policy: PublicKeyAuthPolicy,
    storage: PgStorage,
}

#[derive(Clone)]
struct SshServer {
    shared: Arc<SharedState>,
}

impl server::Server for SshServer {
    type Handler = ConnectionHandler;

    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        let connection_id = self
            .shared
            .next_connection_id
            .fetch_add(1, Ordering::Relaxed);
        ConnectionHandler {
            shared: Arc::clone(&self.shared),
            connection_id,
            peer_addr,
            identity: None,
            input_buffer: String::new(),
            commands_seen: 0,
            channel: None,
            chrome: None,
        }
    }

    fn handle_session_error(&mut self, error: <Self::Handler as server::Handler>::Error) {
        eprintln!("SSH session error: {error:#}");
    }
}

struct ConnectionHandler {
    shared: Arc<SharedState>,
    connection_id: u64,
    peer_addr: Option<SocketAddr>,
    identity: Option<AuthIdentity>,
    input_buffer: String,
    commands_seen: u64,
    channel: Option<ChannelId>,
    chrome: Option<Chrome>,
}

impl ConnectionHandler {
    async fn send_initial_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        prompt: bool,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"Authentication required.\r\n".to_vec())?;
            return Ok(());
        };

        session.data(
            channel,
            format!(
                "Authenticated as {} with {}.\r\nPlayer session: {}\r\n",
                identity.user, identity.fingerprint, identity.player_id
            )
            .into_bytes(),
        )?;
        send_balance_summary(session, channel, &self.shared.storage, identity).await?;
        send_mailbox_summary(session, channel, &self.shared.storage, identity).await?;
        let observation = self
            .shared
            .runtime
            .observe_json(&identity.player_id)
            .await?;
        self.send_text_observation(channel, session, observation)
            .await?;
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(())
    }

    async fn handle_command_line(
        &mut self,
        channel: ChannelId,
        line: &str,
        session: &mut Session,
        prompt: bool,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"Authentication required.\r\n".to_vec())?;
            return Ok(());
        };

        self.shared.presence.lock().await.touch(self.connection_id);
        self.commands_seen += 1;

        let Some(chrome) = &self.chrome else {
            session.data(channel, b"Session is not ready.\r\n".to_vec())?;
            return Ok(());
        };

        let command = match runtime_state::parse_command(chrome, line) {
            Ok(command) => command,
            Err(error) => {
                if matches!(error, SlashParseError::UnknownCommand)
                    && line.trim_start().starts_with('/')
                    && self
                        .handle_operator_input(channel, session, line, identity, prompt)
                        .await?
                {
                    return Ok(());
                }
                session.data(channel, format!("{error}\r\n").into_bytes())?;
                if prompt {
                    send_prompt(session, channel)?;
                }
                return Ok(());
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        if self
            .handle_message_view_command(channel, session, &command, identity, prompt)
            .await?
        {
            return Ok(());
        }
        self.dispatch_live_message(&command, identity).await?;
        let (observation, player_state) = match self
            .shared
            .runtime
            .execute(&identity.player_id, &command)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                session.data(channel, format!("{error}\r\n").into_bytes())?;
                if prompt {
                    send_prompt(session, channel)?;
                }
                return Ok(());
            }
        };
        PlayerStateStore::save_player_state(&self.shared.storage, &player_state).await?;
        self.shared
            .presence
            .lock()
            .await
            .update_view(self.connection_id, player_state.current_view.clone());

        self.send_text_observation(channel, session, observation)
            .await?;
        if should_quit {
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
        } else if prompt {
            send_prompt(session, channel)?;
        }

        Ok(())
    }

    async fn handle_message_view_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        command: &SemanticCommand,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        match command {
            SemanticCommand::Mailbox => {
                let messages = self
                    .shared
                    .storage
                    .recent_mailbox_messages(&identity.user, &identity.player_id, 20)
                    .await?;
                send_message_list(session, channel, "Mailbox", &messages, "No mail.")?;
            }
            SemanticCommand::History => {
                let player = self
                    .shared
                    .runtime
                    .player_state(&identity.player_id)
                    .await?;
                let messages = self
                    .shared
                    .storage
                    .recent_view_messages(&player.current_view, 20)
                    .await?;
                send_message_list(
                    session,
                    channel,
                    "Room History",
                    &messages,
                    "No room history.",
                )?;
                session.data(
                    channel,
                    format!("You are still in: {}\r\n", player.current_view).into_bytes(),
                )?;
            }
            SemanticCommand::News => {
                let messages = self.shared.storage.recent_news_messages(20).await?;
                send_message_list(session, channel, "News", &messages, "No news.")?;
            }
            SemanticCommand::Balance => {
                let balance = self
                    .shared
                    .storage
                    .player_balance(&identity.player_id)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Balance: {} {} ({})\r\n",
                        balance.amount, balance.asset, balance.account_id
                    )
                    .into_bytes(),
                )?;
            }
            SemanticCommand::Pay { action } => {
                self.handle_pay_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Land { action } => {
                self.handle_land_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Build { action } => {
                self.handle_build_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Shop { action } => {
                self.handle_shop_command(channel, session, action, identity)
                    .await?;
            }
            _ => return Ok(false),
        }

        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn handle_operator_input(
        &self,
        channel: ChannelId,
        session: &mut Session,
        line: &str,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let Some(parcel) = self
            .shared
            .storage
            .commercial_parcel_by_view(&player.current_view)
            .await?
        else {
            return Ok(false);
        };
        if parcel.status != "built" {
            return Ok(false);
        }
        let Some(owner_player_id) = parcel.owner_player_id.as_deref() else {
            return Ok(false);
        };
        if owner_player_id == identity.player_id {
            return Ok(false);
        }

        let recipients = self
            .shared
            .presence
            .lock()
            .await
            .direct_recipients(self.connection_id, owner_player_id);
        let delivered = !recipients.is_empty();
        let command = self
            .shared
            .storage
            .save_operator_command(
                &parcel,
                &identity.user,
                &identity.player_id,
                line,
                delivered,
            )
            .await?;
        if delivered {
            deliver_live_message(
                recipients,
                &format!(
                    "[shop command #{} in {} from {}] {}",
                    command.id, command.parcel_id, command.sender_user, command.raw_input
                ),
            )
            .await;
        }
        session.data(
            channel,
            format!(
                "Sent shop command #{} to {} ({}) for parcel {}.\r\n{}",
                command.id,
                command.owner_user,
                if delivered { "delivered" } else { "queued" },
                command.parcel_id,
                custom_command_preview(&parcel, line)
                    .map(|preview| format!("Trial: {preview}\r\n"))
                    .unwrap_or_default()
            )
            .into_bytes(),
        )?;
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn handle_pay_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &PayAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            PayAction::Direct {
                target,
                amount,
                memo,
            } => {
                let transfer = self
                    .shared
                    .storage
                    .transfer_mark(&identity.user, &identity.player_id, target, *amount, memo)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Paid {} {} to {}. Ledger #{}. Balance: {} {}.\r\n",
                        transfer.amount,
                        transfer.asset,
                        transfer.target_user,
                        transfer.ledger_id,
                        transfer.sender_balance,
                        transfer.asset
                    )
                    .into_bytes(),
                )?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, target);
                if !recipients.is_empty() {
                    let memo_text = if transfer.memo.is_empty() {
                        String::new()
                    } else {
                        format!(" memo={}", transfer.memo)
                    };
                    deliver_live_message(
                        recipients,
                        &format!(
                            "[payment from {}] {} {}{}",
                            identity.user, transfer.amount, transfer.asset, memo_text
                        ),
                    )
                    .await;
                }
            }
            PayAction::Requests => {
                let requests = self
                    .shared
                    .storage
                    .pending_payment_requests(&identity.player_id, 20)
                    .await?;
                send_payment_request_list(session, channel, &requests)?;
            }
            PayAction::Accept { request_id } => {
                let (request, sender_balance) = self
                    .shared
                    .storage
                    .accept_payment_request(&identity.user, &identity.player_id, *request_id)
                    .await?;
                session.data(
                    channel,
                    render_paid_request(&request, sender_balance).into_bytes(),
                )?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, &request.payee_player_id);
                if !recipients.is_empty() {
                    deliver_live_message(
                        recipients,
                        &format!(
                            "[payment request #{} paid by {}] {} {}",
                            request.id, identity.user, request.amount, request.asset
                        ),
                    )
                    .await;
                }
            }
        }
        Ok(())
    }

    async fn handle_land_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &LandAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            LandAction::List => {
                let parcels = self.shared.storage.list_commercial_parcels().await?;
                session.data(
                    channel,
                    render_parcel_list(&parcels)
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
            }
            LandAction::Info { parcel_id } => {
                let parcel = self.shared.storage.commercial_parcel(parcel_id).await?;
                session.data(
                    channel,
                    render_parcel_detail(&parcel)
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
            }
            LandAction::Claim { parcel_id } => {
                let parcel = self
                    .shared
                    .storage
                    .claim_commercial_parcel(parcel_id, &identity.user, &identity.player_id)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Claimed parcel {}. Go to {} and use /build title, /build description, /build prompt, /build commands, then /build publish.\r\n",
                        parcel.parcel_id, parcel.view_id
                    )
                    .into_bytes(),
                )?;
            }
            LandAction::Transfer { parcel_id, target } => {
                let parcel = self
                    .shared
                    .storage
                    .transfer_commercial_parcel(parcel_id, &identity.player_id, target)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Transferred parcel {} to {}.\r\n",
                        parcel.parcel_id,
                        parcel.owner_user.as_deref().unwrap_or("unknown")
                    )
                    .into_bytes(),
                )?;
            }
        }
        Ok(())
    }

    async fn handle_build_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &BuildAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        match action {
            BuildAction::Help => {
                session.data(channel, build_help().as_bytes().to_vec())?;
            }
            BuildAction::Set { field, value } => {
                let parcel = self
                    .shared
                    .storage
                    .update_parcel_build_field(
                        &player.current_view,
                        &identity.player_id,
                        field,
                        value,
                    )
                    .await?;
                session.data(
                    channel,
                    format!("Updated {} for parcel {}.\r\n", field, parcel.parcel_id).into_bytes(),
                )?;
            }
            BuildAction::Publish => {
                let parcel = self
                    .shared
                    .storage
                    .publish_parcel_build(&player.current_view, &identity.player_id)
                    .await?;
                session.data(
                    channel,
                    format!("Published parcel {} as a built shop.\r\n", parcel.parcel_id)
                        .into_bytes(),
                )?;
            }
        }
        Ok(())
    }

    async fn handle_shop_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &ShopAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            ShopAction::Inbox => {
                let commands = self
                    .shared
                    .storage
                    .recent_operator_commands(&identity.player_id, 20)
                    .await?;
                send_operator_command_list(session, channel, &commands)?;
            }
            ShopAction::RequestPayment {
                command_id,
                amount,
                delivery,
            } => {
                let request = self
                    .shared
                    .storage
                    .create_payment_request(*command_id, &identity.player_id, *amount, delivery)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Created payment request #{} for {}: {} {}. Delivery is locked until payment.\r\n",
                        request.id, request.payer_user, request.amount, request.asset
                    )
                    .into_bytes(),
                )?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, &request.payer_player_id);
                if !recipients.is_empty() {
                    deliver_live_message(recipients, &render_payment_popup(&request)).await;
                }
            }
        }
        Ok(())
    }

    async fn send_text_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        mut observation: JsonObservation,
    ) -> Result<()> {
        if let Some(parcel) = self
            .shared
            .storage
            .commercial_parcel_by_view(&observation.view_id)
            .await?
        {
            overlay_parcel_observation(&mut observation, &parcel);
        }
        send_text_observation(session, channel, &observation)?;
        Ok(())
    }

    async fn dispatch_live_message(
        &self,
        command: &SemanticCommand,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let (recipients, message) = match command {
            SemanticCommand::Say { text } => {
                let player = self
                    .shared
                    .runtime
                    .player_state(&identity.player_id)
                    .await?;
                self.shared
                    .storage
                    .save_say_message(
                        &identity.user,
                        &identity.player_id,
                        &player.current_view,
                        text,
                    )
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .view_recipients(self.connection_id, &player.current_view);
                (recipients, format!("[say from {}] {text}", identity.user))
            }
            SemanticCommand::Mail { target, text } => {
                self.shared
                    .storage
                    .save_mail_message(&identity.user, &identity.player_id, target, text)
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, target);
                (
                    recipients,
                    format!("[mail from {} to {target}] {text}", identity.user),
                )
            }
            SemanticCommand::Broadcast { text } => {
                self.shared
                    .storage
                    .save_broadcast_message(&identity.user, &identity.player_id, text)
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .broadcast_recipients(self.connection_id);
                (
                    recipients,
                    format!("[broadcast from {}] {text}", identity.user),
                )
            }
            _ => return Ok(()),
        };

        deliver_live_message(recipients, &message).await;
        Ok(())
    }

    async fn handle_exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        let command = String::from_utf8_lossy(data).trim().to_owned();
        session.channel_success(channel)?;
        if command.is_empty() {
            session.data(channel, exec_help().replace('\n', "\r\n").into_bytes())?;
        } else {
            session.data(
                channel,
                format!(
                    "{}\r\nSSH exec is not a world command interface. Rejected exec command: {command}\r\n",
                    exec_help()
                )
                .into_bytes(),
            )?;
        }
        session.exit_status_request(channel, 1)?;
        session.close(channel)?;
        Ok(())
    }

    async fn handle_terminal_input(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        if data == [3] {
            session.close(channel)?;
            return Ok(());
        }

        if self
            .shared
            .presence
            .lock()
            .await
            .poll_kick(self.connection_id)
        {
            session.close(channel)?;
            return Ok(());
        }

        let incoming = String::from_utf8_lossy(data);
        for character in incoming.chars() {
            match character {
                '\r' | '\n' => {
                    if character == '\n' && self.input_buffer.is_empty() {
                        continue;
                    }
                    session.data(channel, b"\r\n".to_vec())?;
                    let line = std::mem::take(&mut self.input_buffer);
                    self.handle_command_line(channel, line.trim(), session, true)
                        .await?;
                }
                '\u{8}' | '\u{7f}' => {
                    if self.input_buffer.pop().is_some() {
                        session.data(channel, b"\x08 \x08".to_vec())?;
                    }
                }
                _ if character.is_control() => {}
                _ => {
                    self.input_buffer.push(character);
                    session.data(channel, character.to_string().into_bytes())?;
                }
            }
        }

        Ok(())
    }
}

impl server::Handler for ConnectionHandler {
    type Error = anyhow::Error;

    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        if self
            .shared
            .auth_policy
            .accepts_public_key_offer(user, public_key)
        {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            })
        }
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        let authorized = self.shared.auth_policy.authorize(user, public_key);
        let identity = self
            .shared
            .storage
            .upsert_ssh_identity(user, &authorized.fingerprint, &authorized.player_id)
            .await?;
        self.shared
            .storage
            .ensure_player_wallet(&identity.username, &identity.player_id)
            .await?;
        let saved_player =
            PlayerStateStore::load_player_state(&self.shared.storage, &identity.player_id).await?;
        self.shared
            .runtime
            .set_or_create_player(saved_player, &identity.player_id, LOCAL_PLAYER_ID)
            .await?;
        let player_to_save = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        PlayerStateStore::save_player_state(&self.shared.storage, &player_to_save).await?;
        self.shared.presence.lock().await.mark_online(
            self.connection_id,
            identity.player_id.clone(),
            user.to_owned(),
            player_to_save.current_view.clone(),
        );
        let presence = self.shared.presence.lock().await;
        eprintln!(
            "accepted SSH public key auth for user={user} player_id={} peer={:?} online_for_player={} online_users={:?}",
            identity.player_id,
            self.peer_addr,
            presence.online_count_for_player(&identity.player_id),
            presence.users()
        );
        drop(presence);
        self.identity = Some(AuthIdentity {
            user: identity.username,
            fingerprint: authorized.fingerprint,
            player_id: identity.player_id,
        });
        self.chrome = Some(self.shared.runtime.chrome().await);
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channel = Some(channel.id());
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        self.shared.presence.lock().await.attach_channel(
            self.connection_id,
            session.handle(),
            channel,
        );
        self.send_initial_observation(channel, session, true).await
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.handle_exec_request(channel, data, session).await
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.handle_terminal_input(channel, data, session).await
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let had_buffered_input = !self.input_buffer.trim().is_empty();
        if !self.input_buffer.trim().is_empty() {
            let line = std::mem::take(&mut self.input_buffer);
            self.handle_command_line(channel, line.trim(), session, false)
                .await?;
        }
        send_stdin_closed_guidance(session, channel, self.commands_seen, had_buffered_input)?;
        session.exit_status_request(channel, 0)?;
        session.close(channel)?;
        Ok(())
    }
}

impl Drop for ConnectionHandler {
    fn drop(&mut self) {
        let shared = Arc::clone(&self.shared);
        let connection_id = self.connection_id;
        tokio::spawn(async move {
            shared.presence.lock().await.remove(connection_id);
        });
    }
}

fn send_text_observation(
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

fn overlay_parcel_observation(observation: &mut JsonObservation, parcel: &StoredParcel) {
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

fn render_parcel_list(parcels: &[StoredParcel]) -> String {
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

fn render_parcel_detail(parcel: &StoredParcel) -> String {
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

fn custom_command_preview(parcel: &StoredParcel, raw_input: &str) -> Option<String> {
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

fn build_help() -> &'static str {
    "Build commands for the current owned parcel:\r\n\
     /build title <shop title>\r\n\
     /build description <shop description>\r\n\
     /build style <style note>\r\n\
     /build prompt <operator prompt shown to visitors>\r\n\
     /build commands <custom command help>\r\n\
     /build publish\r\n"
}

fn send_prompt(session: &mut Session, channel: ChannelId) -> Result<()> {
    session.data(channel, Chrome::PROMPT.as_bytes().to_vec())?;
    Ok(())
}

fn send_stdin_closed_guidance(
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

fn send_message_list(
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

fn send_operator_command_list(
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

fn send_payment_request_list(
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

fn render_payment_popup(request: &StoredPaymentRequest) -> String {
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

fn render_paid_request(request: &StoredPaymentRequest, sender_balance: i64) -> String {
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

async fn send_mailbox_summary(
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

async fn send_balance_summary(
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

async fn deliver_live_message(recipients: Vec<PresenceDelivery>, message: &str) {
    let payload = format!("\r\n{message}\r\n{}", Chrome::PROMPT);
    for recipient in recipients {
        let _ = recipient
            .handle
            .data(recipient.channel_id, payload.clone())
            .await;
    }
}

fn exec_help() -> &'static str {
    "Xagora is an open world served over SSH, not a general-purpose Unix shell.\n\
     Open an SSH shell: ssh -p <port> <user>@<host>\n\
     Keep the SSH connection open, read each observation, choose one Available command, send it, and continue.\n\
     Common commands inside the session: /look, /go east, /go west, /inspect board, /read board, /help.\n\
     Wallet commands: /balance, /pay <user> <amount> [memo], /pay requests, /pay accept <id>."
}
