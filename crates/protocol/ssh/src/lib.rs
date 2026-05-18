#![deny(missing_docs)]

//! SSH adapter for the Xagora MUD runtime.

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
use xagora_core::SemanticCommand;
use xagora_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use xagora_runtime::{Chrome, render_text_observation};
use xagora_storage::{PgStorage, PlayerStateStore};

use auth::{AuthIdentity, PublicKeyAuthPolicy};
use config::{DaemonConfig, mask_database_url};
use presence::PresenceRegistry;
use runtime_state::RuntimeHandle;

/// Runs the SSH server and (on Unix) the admin socket listener until shutdown.
pub async fn run_daemon() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = DaemonConfig::parse();
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
    channel: Option<ChannelId>,
    chrome: Option<Chrome>,
}

impl ConnectionHandler {
    async fn send_initial_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
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
        let observation = self
            .shared
            .runtime
            .observe_json(&identity.player_id)
            .await?;
        send_text_observation(session, channel, &observation)?;
        send_prompt(session, channel)?;
        Ok(())
    }

    async fn handle_command_line(
        &mut self,
        channel: ChannelId,
        line: &str,
        session: &mut Session,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"Authentication required.\r\n".to_vec())?;
            return Ok(());
        };

        self.shared.presence.lock().await.touch(self.connection_id);

        let Some(chrome) = &self.chrome else {
            session.data(channel, b"Session is not ready.\r\n".to_vec())?;
            return Ok(());
        };

        let command = match runtime_state::parse_command(chrome, line) {
            Ok(command) => command,
            Err(error) => {
                session.data(channel, format!("{error}\r\n").into_bytes())?;
                send_prompt(session, channel)?;
                return Ok(());
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        let (observation, player_state) = self
            .shared
            .runtime
            .execute(&identity.player_id, &command)
            .await?;
        PlayerStateStore::save_player_state(&self.shared.storage, &player_state).await?;

        send_text_observation(session, channel, &observation)?;
        if should_quit {
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
        } else {
            send_prompt(session, channel)?;
        }

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
                    self.handle_command_line(channel, line.trim(), session)
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
        self.send_initial_observation(channel, session).await
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.handle_terminal_input(channel, data, session).await
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

fn send_prompt(session: &mut Session, channel: ChannelId) -> Result<()> {
    session.data(channel, Chrome::PROMPT.as_bytes().to_vec())?;
    Ok(())
}
