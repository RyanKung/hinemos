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
    BuildAction, InboxAction, JsonObservation, LandAction, PayAction, SemanticCommand, ShopAction,
};
use hinemos_runtime::{Chrome, SlashParseError};
use hinemos_storage::{PgStorage, PlayerStateStore, ServiceRoomUpsert};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, ssh_key};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, server};
use serde::Deserialize;
use sqlx::postgres::PgListener;
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
    let world = load_world_from_dir(&cli.world)
        .with_context(|| format!("failed to load world from {}", cli.world.display()))?;
    load_service_room_registrations(&storage, &cli.world).await?;
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

    {
        let shared_inbox = Arc::clone(&shared);
        let listener_database_url = database_url.clone();
        tokio::spawn(async move {
            if let Err(error) =
                run_inbox_mail_notify_listener(listener_database_url, shared_inbox).await
            {
                eprintln!("inbox mail notify listener exited: {error:#}");
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

async fn run_inbox_mail_notify_listener(
    database_url: String,
    shared: Arc<SharedState>,
) -> Result<()> {
    let mut listener = PgListener::connect(&database_url).await?;
    listener.listen("hinemos_inbox_mail").await?;
    loop {
        let notification = listener.recv().await?;
        let Ok(item_id) = notification.payload().parse::<i64>() else {
            continue;
        };
        let item = shared.storage.inbox_item(item_id).await?;
        let recipients = shared
            .presence
            .lock()
            .await
            .direct_recipients(u64::MAX, &item.recipient_player_id);
        if !recipients.is_empty() {
            render::deliver_live_inbox_notice(recipients, &item, shared.mail_domain.as_deref())
                .await;
        }
    }
}

#[derive(Debug, Deserialize)]
struct ServiceRoomRegistration {
    view_id: String,
    #[serde(default)]
    front_view_id: Option<String>,
    #[serde(default)]
    front_entity_id: Option<String>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    enter_aliases: Option<String>,
    room_user: String,
    room_player_id: String,
    #[serde(default)]
    status_text: Option<String>,
    #[serde(default)]
    custom_commands: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

async fn load_service_room_registrations(storage: &PgStorage, world_dir: &Path) -> Result<()> {
    let path = world_dir.join("rooms.ron");
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read room registrations from {}", path.display()))?;
    let registrations: Vec<ServiceRoomRegistration> = ron::from_str(&content)
        .with_context(|| format!("failed to parse room registrations from {}", path.display()))?;
    for registration in registrations {
        storage
            .upsert_service_room(ServiceRoomUpsert {
                view_id: &registration.view_id,
                front_view_id: registration.front_view_id.as_deref(),
                front_entity_id: registration.front_entity_id.as_deref(),
                address: registration.address.as_deref(),
                label: registration.label.as_deref(),
                enter_aliases: registration.enter_aliases.as_deref(),
                room_user: &registration.room_user,
                room_player_id: &registration.room_player_id,
                status_text: registration.status_text.as_deref(),
                custom_commands: registration.custom_commands.as_deref(),
                enabled: registration.enabled,
            })
            .await?;
    }
    Ok(())
}

const fn default_enabled() -> bool {
    true
}

pub(crate) struct SharedState {
    runtime: RuntimeHandle,
    presence: Mutex<PresenceRegistry>,
    next_connection_id: AtomicU64,
    auth_policy: PublicKeyAuthPolicy,
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
