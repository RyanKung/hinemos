#![deny(missing_docs)]

//! SSH adapter for the Agentopia MUD runtime.

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use agentopia_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use agentopia_core::{JsonObservation, ObservationEvent, SemanticCommand};
use agentopia_i18n::{Catalog, Language, parse_language_command};
use agentopia_runtime::{GameRuntime, Localizer};
use anyhow::{Context, Result};
use clap::Parser;
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, HashAlg, PrivateKey, ssh_key};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, server};
use storage::PgStorage;
use tokio::sync::Mutex;

#[derive(Debug, Parser)]
#[command(name = "agentopia-ssh")]
#[command(about = "SSH adapter for the Agentopia MUD runtime")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:2222")]
    bind: SocketAddr,

    #[arg(long, default_value = "worlds/sample")]
    world: PathBuf,

    #[arg(long, default_value = "en-US")]
    lang: String,

    #[arg(long, default_value = ".agentopia/ssh_host_ed25519_key")]
    host_key: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set in the environment or .env")?;
    let storage = PgStorage::connect(&database_url).await?;
    storage.migrate().await?;
    let default_language = cli.lang.parse::<Language>()?;
    let world = load_world_from_dir(&cli.world)
        .with_context(|| format!("failed to load world from {}", cli.world.display()))?;

    let host_key = load_or_create_host_key(&cli.host_key)
        .with_context(|| format!("failed to load host key from {}", cli.host_key.display()))?;
    let config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: Some(Duration::from_millis(250)),
        keys: vec![host_key],
        ..Default::default()
    });
    let shared = Arc::new(SharedState {
        runtime: GameRuntime::new(world),
        presence: Mutex::new(PresenceRegistry::default()),
        next_connection_id: AtomicU64::new(1),
        default_language,
        storage,
    });

    let mut server = SshServer { shared };
    println!("Agentopia SSH adapter listening on {}", cli.bind);
    println!("Database configured: {}", mask_database_url(&database_url));
    server.run_on_address(config, cli.bind).await?;
    Ok(())
}

fn mask_database_url(database_url: &str) -> String {
    let Some((scheme, rest)) = database_url.split_once("://") else {
        return "<invalid-url>".to_owned();
    };
    let Some((userinfo, host)) = rest.rsplit_once('@') else {
        return format!("{scheme}://{rest}");
    };
    let Some((user, _password)) = userinfo.split_once(':') else {
        return format!("{scheme}://{userinfo}@{host}");
    };
    format!("{scheme}://{user}:***@{host}")
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

struct SharedState {
    runtime: GameRuntime,
    presence: Mutex<PresenceRegistry>,
    next_connection_id: AtomicU64,
    default_language: Language,
    storage: PgStorage,
}

#[derive(Debug, Default)]
struct PresenceRegistry {
    connections: HashMap<u64, PresenceRecord>,
}

impl PresenceRegistry {
    fn mark_online(&mut self, connection_id: u64, player_id: String, user: String) {
        self.connections.insert(
            connection_id,
            PresenceRecord {
                player_id,
                user,
                connected_at: Instant::now(),
                last_seen_at: Instant::now(),
            },
        );
    }

    fn touch(&mut self, connection_id: u64) {
        if let Some(record) = self.connections.get_mut(&connection_id) {
            let _session_age = record.connected_at.elapsed();
            record.last_seen_at = Instant::now();
        }
    }

    fn remove(&mut self, connection_id: u64) {
        self.connections.remove(&connection_id);
    }

    fn online_count_for_player(&self, player_id: &str) -> usize {
        self.connections
            .values()
            .filter(|record| record.player_id == player_id)
            .count()
    }

    fn users(&self) -> Vec<&str> {
        self.connections
            .values()
            .map(|record| record.user.as_str())
            .collect()
    }
}

#[derive(Debug)]
struct PresenceRecord {
    player_id: String,
    user: String,
    connected_at: Instant,
    last_seen_at: Instant,
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
            catalog: Catalog::new(self.shared.default_language),
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
    catalog: Catalog,
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
        let observation =
            self.shared
                .runtime
                .observe_json(&identity.player_id, &self.catalog, vec![])?;
        send_text_observation(session, channel, &observation, &self.catalog)?;
        send_prompt(session, channel, &self.catalog)?;
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
        if let Some(language) = parse_language_command(line) {
            match language {
                Ok(language) => {
                    self.catalog = Catalog::new(language);
                    session.data(
                        channel,
                        format!("{}\r\n", self.catalog.text("event.lang")).into_bytes(),
                    )?;
                    let observation = self.shared.runtime.observe_json(
                        &identity.player_id,
                        &self.catalog,
                        vec![],
                    )?;
                    send_text_observation(session, channel, &observation, &self.catalog)?;
                }
                Err(error) => {
                    session.data(channel, format!("{error}\r\n").into_bytes())?;
                }
            }
            send_prompt(session, channel, &self.catalog)?;
            return Ok(());
        }

        let command = match self.catalog.parse_command(line) {
            Ok(command) => command,
            Err(error) => {
                session.data(channel, format!("{error}\r\n").into_bytes())?;
                send_prompt(session, channel, &self.catalog)?;
                return Ok(());
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        let observation =
            self.shared
                .runtime
                .execute(&identity.player_id, &command, &self.catalog)?;
        let player_state = self.shared.runtime.player_state(&identity.player_id)?;
        self.shared.storage.save_player_state(&player_state).await?;

        send_text_observation(session, channel, &observation, &self.catalog)?;
        if should_quit {
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
        } else {
            send_prompt(session, channel, &self.catalog)?;
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
        _user: &str,
        _public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_publickey(
        &mut self,
        user: &str,
        public_key: &ssh_key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        let fingerprint = public_key.fingerprint(HashAlg::Sha256).to_string();
        let player_id = player_id_from_key(user, &fingerprint);
        let identity = self
            .shared
            .storage
            .upsert_ssh_identity(user, &fingerprint, &player_id)
            .await?;
        let saved_player = self
            .shared
            .storage
            .load_player_state(&identity.player_id)
            .await?;
        if let Some(player) = saved_player {
            self.shared.runtime.set_player_state(player)?;
        } else {
            self.shared
                .runtime
                .ensure_player_from_template(&identity.player_id, LOCAL_PLAYER_ID)?;
        }
        let player_to_save = self.shared.runtime.player_state(&identity.player_id)?;
        self.shared
            .storage
            .save_player_state(&player_to_save)
            .await?;
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
            fingerprint,
            player_id: identity.player_id,
        });
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

#[derive(Debug, Clone)]
struct AuthIdentity {
    user: String,
    fingerprint: String,
    player_id: String,
}

fn send_text_observation(
    session: &mut Session,
    channel: ChannelId,
    observation: &JsonObservation,
    catalog: &Catalog,
) -> Result<()> {
    session.data(
        channel,
        render_text_observation(observation, catalog).into_bytes(),
    )?;
    Ok(())
}

fn send_prompt(session: &mut Session, channel: ChannelId, catalog: &Catalog) -> Result<()> {
    session.data(channel, catalog.text("prompt").into_bytes())?;
    Ok(())
}

fn render_text_observation(observation: &JsonObservation, catalog: &Catalog) -> String {
    let mut output = String::new();
    output.push_str("\r\n");
    output.push_str(&observation.title);
    output.push_str("\r\n");
    output.push_str(&observation.description);
    output.push_str("\r\n");

    if !observation.exits.is_empty() {
        let exits = observation
            .exits
            .iter()
            .map(|exit| exit.direction.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!("{}: {exits}\r\n", catalog.text("label.exits")));
    }

    if !observation.entities.is_empty() {
        let entities = observation
            .entities
            .iter()
            .map(|entity| entity.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "{}: {entities}\r\n",
            catalog.text("label.visible")
        ));
    }

    for event in &observation.events {
        match event {
            ObservationEvent::Message { text } => {
                output.push_str(text);
                output.push_str("\r\n");
            }
            ObservationEvent::Move { direction, .. } => {
                output.push_str(&format!(
                    "{} {}\r\n",
                    catalog.text("event.move"),
                    direction.as_str()
                ));
            }
        }
    }

    output
}

fn player_id_from_key(user: &str, fingerprint: &str) -> String {
    format!("ssh_{}_{}", sanitize_id(user), sanitize_id(fingerprint))
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
