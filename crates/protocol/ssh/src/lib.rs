#![deny(missing_docs)]

//! SSH adapter for the Hinemos open-world runtime.

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
mod handler;
mod mail;
mod presence;
mod render;
mod runtime_state;

use anyhow::{Context, Result};
use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use hinemos_core::{
    BuildAction, InboxAction, JsonObservation, LandAction, PayAction, SemanticCommand,
    SettingsAction, ShopAction,
};
use hinemos_runtime::{Chrome, SlashParseError};
use hinemos_storage::{PgStorage, PlayerStateStore};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, ssh_key};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, server};
use tokio::sync::Mutex;

use auth::PublicKeyAuthPolicy;
pub use config::SshArgs;
use config::{DaemonConfig, mail_domain_from_env, mask_database_url};
use handler::ConnectionHandler;
pub use mail::MailArgs;
use presence::PresenceRegistry;
use runtime_state::RuntimeHandle;

/// Runs the SSH server and (on Unix) the admin socket listener until shutdown.
pub async fn run_daemon(args: SshArgs) -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = DaemonConfig::from_args(args);
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set in the environment or .env")?;
    let mail_domain = mail_domain_from_env();
    let storage = PgStorage::connect(&database_url).await?;
    storage.migrate().await?;
    hinemos_blackstone::migrate(&storage).await?;
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
    let blackstone = hinemos_blackstone::BlackstoneService::new(storage.clone());
    println!("Hinemos SSH adapter listening on {}", cli.bind);
    println!("Database configured: {}", mask_database_url(&database_url));
    if let Some(domain) = &mail_domain {
        println!("Mail domain configured: {domain}");
    }
    let shared = Arc::new(SharedState {
        runtime,
        presence: Mutex::new(PresenceRegistry::default()),
        next_connection_id: AtomicU64::new(1),
        auth_policy: PublicKeyAuthPolicy,
        blackstone,
        storage,
        mail_domain,
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
    server.run_on_address(config, cli.bind).await?;
    Ok(())
}

/// Runs the SMTP/IMAP sidecar mail service until shutdown.
pub async fn run_mail_daemon(args: MailArgs) -> Result<()> {
    mail::run_mail_daemon(args).await
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
    blackstone: hinemos_blackstone::BlackstoneService,
    storage: PgStorage,
    mail_domain: Option<String>,
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
        ConnectionHandler::new(Arc::clone(&self.shared), connection_id, peer_addr)
    }

    fn handle_session_error(&mut self, error: <Self::Handler as server::Handler>::Error) {
        eprintln!("SSH session error: {error:#}");
    }
}
