#![deny(missing_docs)]

//! SSH adapter for the Hinemos open-world runtime.

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
mod admin;
mod auth;
mod config;
mod handler;
mod inbox_notify;
mod mail;
mod mail_protocol;
mod presence;
mod render;
mod runtime_state;

use anyhow::{Context, Result};
use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use hinemos_runtime::{Chrome, SlashParseError};
use hinemos_storage::{
    PgStorage, StoredInboxItem, StoredParcel, StoredRoomBinding, StoredServiceRoom,
};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, ssh_key};
use russh::server::{Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, server};
use tokio::sync::Mutex;

use auth::PublicKeyAuthPolicy;
pub use config::SshArgs;
use config::{DaemonConfig, mail_domain_from_env, mask_database_url};
use handler::ConnectionHandler;
use hinemos_app::{AppService, RoomRegistrationCache, WorldAppConfig};
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
    let app_config = AppService::<PgStorage>::load_world_app_config(&cli.world)?;
    AppService::<PgStorage>::load_service_room_registrations(
        &storage,
        &cli.world,
        &world,
        None::<&()>,
    )
    .await?;
    let runtime = RuntimeHandle::new_with_grid_origin(world, app_config.admission_view_id.clone())?;

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
        room_cache: Mutex::new(RoomDirectoryCache::default()),
        inbox_item_cache: Mutex::new(InboxItemCache::default()),
        view_presence_cache: Mutex::new(ViewPresenceCache::default()),
        pending_connection_messages: Mutex::new(HashMap::new()),
        next_connection_id: AtomicU64::new(1),
        auth_policy: PublicKeyAuthPolicy,
        storage,
        mail_domain,
        app_config: Mutex::new(app_config),
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
                inbox_notify::run_inbox_mail_notify_listener(listener_database_url, shared_inbox)
                    .await
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

pub(crate) struct SharedState {
    runtime: RuntimeHandle,
    presence: Mutex<PresenceRegistry>,
    room_cache: Mutex<RoomDirectoryCache>,
    inbox_item_cache: Mutex<InboxItemCache>,
    view_presence_cache: Mutex<ViewPresenceCache>,
    pending_connection_messages: Mutex<HashMap<u64, Vec<String>>>,
    next_connection_id: AtomicU64,
    auth_policy: PublicKeyAuthPolicy,
    storage: PgStorage,
    mail_domain: Option<String>,
    app_config: Mutex<WorldAppConfig>,
}

#[derive(Debug, Default)]
struct RoomDirectoryCache {
    service_room_views: HashMap<String, CacheEntry<Option<StoredServiceRoom>>>,
    service_room_any_views: HashMap<String, CacheEntry<Option<StoredServiceRoom>>>,
    service_room_users: HashMap<String, CacheEntry<Vec<StoredServiceRoom>>>,
    service_rooms_front_views: HashMap<String, CacheEntry<Vec<StoredServiceRoom>>>,
    commercial_parcels_front_views: HashMap<String, CacheEntry<Vec<StoredParcel>>>,
    room_binding_views: HashMap<String, CacheEntry<Option<StoredRoomBinding>>>,
    room_binding_front_views: HashMap<String, CacheEntry<Vec<StoredRoomBinding>>>,
    room_context_views: HashMap<String, CacheEntry<RoomViewContext>>,
}

#[derive(Debug, Default)]
struct InboxItemCache {
    items: HashMap<i64, CacheEntry<StoredInboxItem>>,
}

#[derive(Debug, Default)]
struct ViewPresenceCache {
    items: HashMap<String, CacheEntry<String>>,
}

#[derive(Debug, Clone)]
struct CacheEntry<T> {
    loaded_at: Instant,
    value: T,
}

impl RoomDirectoryCache {
    const TTL: Duration = Duration::from_secs(30);

    fn get<T: Clone>(entry: Option<&CacheEntry<T>>) -> Option<T> {
        let entry = entry?;
        if entry.loaded_at.elapsed() <= Self::TTL {
            Some(entry.value.clone())
        } else {
            None
        }
    }

    fn put<T>(value: T) -> CacheEntry<T> {
        CacheEntry {
            loaded_at: Instant::now(),
            value,
        }
    }

    fn clear(&mut self) {
        self.service_room_views.clear();
        self.service_room_any_views.clear();
        self.service_room_users.clear();
        self.service_rooms_front_views.clear();
        self.commercial_parcels_front_views.clear();
        self.room_binding_views.clear();
        self.room_binding_front_views.clear();
        self.room_context_views.clear();
    }

    fn invalidate_for_service_room(
        &mut self,
        view_id: &str,
        old_room_user: Option<&str>,
        old_front_view_id: Option<&str>,
        new_room_user: Option<&str>,
        new_front_view_id: Option<&str>,
    ) {
        self.service_room_views.remove(view_id);
        self.service_room_any_views.remove(view_id);
        self.room_binding_views.remove(view_id);
        self.room_context_views.remove(view_id);
        if let Some(room_user) = old_room_user {
            self.service_room_users.remove(room_user);
        }
        if let Some(room_user) = new_room_user {
            self.service_room_users.remove(room_user);
        }
        for front_view_id in old_front_view_id.into_iter().chain(new_front_view_id) {
            self.service_rooms_front_views.remove(front_view_id);
            self.room_binding_front_views.remove(front_view_id);
            self.commercial_parcels_front_views.remove(front_view_id);
            self.room_context_views.remove(front_view_id);
        }
    }

    fn invalidate_for_commercial_parcel(&mut self, view_id: &str, front_view_id: &str) {
        self.room_binding_views.remove(view_id);
        self.room_binding_front_views.remove(front_view_id);
        self.commercial_parcels_front_views.remove(front_view_id);
        self.room_context_views.remove(view_id);
        self.room_context_views.remove(front_view_id);
    }
}

impl InboxItemCache {
    fn get(&self, item_id: i64) -> Option<StoredInboxItem> {
        RoomDirectoryCache::get(self.items.get(&item_id))
    }

    fn insert(&mut self, item_id: i64, item: StoredInboxItem) {
        self.items.insert(item_id, RoomDirectoryCache::put(item));
    }

    fn remove(&mut self, item_id: i64) {
        self.items.remove(&item_id);
    }

    fn clear(&mut self) {
        self.items.clear();
    }
}

impl ViewPresenceCache {
    const TTL: Duration = Duration::from_secs(5);

    fn should_record(&mut self, player_id: &str, view_id: &str) -> bool {
        if let Some(entry) = self.items.get(player_id)
            && entry.loaded_at.elapsed() <= Self::TTL
            && entry.value == view_id
        {
            return false;
        }
        self.items.insert(
            player_id.to_owned(),
            RoomDirectoryCache::put(view_id.to_owned()),
        );
        true
    }
}

impl SharedState {
    async fn app_service(&self) -> AppService<PgStorage> {
        AppService::with_config(self.storage.clone(), self.app_config().await)
    }

    async fn invalidate_room_cache_for_service_room(
        &self,
        view_id: &str,
        old_room_user: Option<&str>,
        old_front_view_id: Option<&str>,
        new_room_user: Option<&str>,
        new_front_view_id: Option<&str>,
    ) {
        self.room_cache.lock().await.invalidate_for_service_room(
            view_id,
            old_room_user,
            old_front_view_id,
            new_room_user,
            new_front_view_id,
        );
    }

    async fn invalidate_room_cache_for_commercial_parcel(
        &self,
        view_id: &str,
        front_view_id: &str,
    ) {
        self.room_cache
            .lock()
            .await
            .invalidate_for_commercial_parcel(view_id, front_view_id);
    }

    async fn service_room_by_view_any(&self, view_id: &str) -> Result<Option<StoredServiceRoom>> {
        if let Some(value) = RoomDirectoryCache::get(
            self.room_cache
                .lock()
                .await
                .service_room_any_views
                .get(view_id),
        ) {
            return Ok(value);
        }
        let value = self.storage.service_room_by_view_any(view_id).await?;
        self.room_cache
            .lock()
            .await
            .service_room_any_views
            .insert(view_id.to_owned(), RoomDirectoryCache::put(value.clone()));
        Ok(value)
    }

    async fn room_context_for_view(&self, view_id: &str) -> Result<RoomViewContext> {
        if let Some(value) =
            RoomDirectoryCache::get(self.room_cache.lock().await.room_context_views.get(view_id))
        {
            return Ok(value);
        }
        let value = if let Some(room_binding) = self.room_binding_by_view(view_id).await? {
            RoomViewContext {
                room_binding: Some(room_binding),
                service_room: None,
                front_bindings: Vec::new(),
            }
        } else if let Some(service_room) = self.service_room_by_view_any(view_id).await? {
            RoomViewContext {
                room_binding: None,
                service_room: Some(service_room),
                front_bindings: Vec::new(),
            }
        } else {
            let front_bindings = self.room_bindings_by_front_view(view_id).await?;
            RoomViewContext {
                room_binding: None,
                service_room: None,
                front_bindings,
            }
        };
        self.room_cache
            .lock()
            .await
            .room_context_views
            .insert(view_id.to_owned(), RoomDirectoryCache::put(value.clone()));
        Ok(value)
    }

    async fn service_rooms_by_room_user(&self, room_user: &str) -> Result<Vec<StoredServiceRoom>> {
        if let Some(value) = RoomDirectoryCache::get(
            self.room_cache
                .lock()
                .await
                .service_room_users
                .get(room_user),
        ) {
            return Ok(value);
        }
        let value = self.storage.service_rooms_by_room_user(room_user).await?;
        self.room_cache
            .lock()
            .await
            .service_room_users
            .insert(room_user.to_owned(), RoomDirectoryCache::put(value.clone()));
        Ok(value)
    }

    async fn inbox_item(&self, item_id: i64) -> Result<StoredInboxItem> {
        if let Some(value) = self.inbox_item_cache.lock().await.get(item_id) {
            return Ok(value);
        }
        let value = self.storage.inbox_item(item_id).await?;
        self.inbox_item_cache
            .lock()
            .await
            .insert(item_id, value.clone());
        Ok(value)
    }

    async fn invalidate_inbox_item(&self, item_id: i64) {
        self.inbox_item_cache.lock().await.remove(item_id);
    }

    async fn record_view_presence_throttled(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<()> {
        let should_record = {
            let mut cache = self.view_presence_cache.lock().await;
            cache.should_record(player_id, view_id)
        };
        if should_record {
            let app = self.app_service().await;
            app.record_view_presence(username, player_id, view_id)
                .await?;
        }
        Ok(())
    }

    async fn set_service_room_mail_auth_token(
        &self,
        view_id: &str,
        token: &str,
    ) -> Result<hinemos_storage::StoredMailAuthToken> {
        let room = self
            .storage
            .set_service_room_mail_auth_token(view_id, token)
            .await?;
        self.invalidate_room_cache_for_service_room(
            view_id,
            Some(&room.username),
            None,
            Some(&room.username),
            None,
        )
        .await;
        Ok(room)
    }

    async fn room_bindings_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<StoredRoomBinding>> {
        if let Some(value) = RoomDirectoryCache::get(
            self.room_cache
                .lock()
                .await
                .room_binding_front_views
                .get(front_view_id),
        ) {
            return Ok(value);
        }
        let commercial_parcels = self.commercial_parcels_by_front_view(front_view_id).await?;
        let mut bindings = commercial_parcels
            .into_iter()
            .map(StoredRoomBinding::from_parcel)
            .collect::<Vec<_>>();
        bindings.extend(
            self.service_rooms_by_front_view(front_view_id)
                .await?
                .into_iter()
                .filter_map(StoredRoomBinding::from_service_room),
        );
        bindings.sort_by(|left, right| {
            left.address
                .cmp(&right.address)
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.view_id.cmp(&right.view_id))
        });
        self.room_cache
            .lock()
            .await
            .room_binding_front_views
            .insert(
                front_view_id.to_owned(),
                RoomDirectoryCache::put(bindings.clone()),
            );
        Ok(bindings)
    }

    async fn service_rooms_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<StoredServiceRoom>> {
        if let Some(value) = RoomDirectoryCache::get(
            self.room_cache
                .lock()
                .await
                .service_rooms_front_views
                .get(front_view_id),
        ) {
            return Ok(value);
        }
        let value = self
            .storage
            .service_rooms_by_front_view(front_view_id)
            .await?;
        self.room_cache
            .lock()
            .await
            .service_rooms_front_views
            .insert(
                front_view_id.to_owned(),
                RoomDirectoryCache::put(value.clone()),
            );
        Ok(value)
    }

    async fn commercial_parcels_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<StoredParcel>> {
        if let Some(value) = RoomDirectoryCache::get(
            self.room_cache
                .lock()
                .await
                .commercial_parcels_front_views
                .get(front_view_id),
        ) {
            return Ok(value);
        }
        let value = self
            .storage
            .commercial_parcels_by_front_view(front_view_id)
            .await?;
        self.room_cache
            .lock()
            .await
            .commercial_parcels_front_views
            .insert(
                front_view_id.to_owned(),
                RoomDirectoryCache::put(value.clone()),
            );
        Ok(value)
    }

    async fn room_binding_by_view(&self, view_id: &str) -> Result<Option<StoredRoomBinding>> {
        if let Some(value) =
            RoomDirectoryCache::get(self.room_cache.lock().await.room_binding_views.get(view_id))
        {
            return Ok(value);
        }
        let value = self.storage.room_binding_by_view(view_id).await?;
        self.room_cache
            .lock()
            .await
            .room_binding_views
            .insert(view_id.to_owned(), RoomDirectoryCache::put(value.clone()));
        Ok(value)
    }

    async fn clear_room_cache(&self) {
        self.room_cache.lock().await.clear();
        self.inbox_item_cache.lock().await.clear();
    }

    async fn app_config(&self) -> WorldAppConfig {
        self.app_config.lock().await.clone()
    }

    async fn set_app_config(&self, app_config: WorldAppConfig) {
        *self.app_config.lock().await = app_config;
    }

    async fn reload_world_from_dir(&self, world_dir: &Path) -> Result<()> {
        let app_config = AppService::<PgStorage>::load_world_app_config(world_dir)?;
        self.runtime
            .reload_from_world_dir_preserving_players(
                world_dir,
                app_config.admission_view_id.clone(),
            )
            .await?;
        let world = load_world_from_dir(world_dir)
            .with_context(|| format!("failed to load world from {}", world_dir.display()))?;
        AppService::<PgStorage>::load_service_room_registrations(
            &self.storage,
            world_dir,
            &world,
            Some(self),
        )
        .await?;
        self.set_app_config(app_config).await;
        self.clear_room_cache().await;
        self.notify_closed_rooms_after_reload().await?;
        Ok(())
    }

    async fn notify_closed_rooms_after_reload(&self) -> Result<()> {
        let connection_views = self.presence.lock().await.connection_views();
        for (connection_id, view_id) in connection_views {
            match self.room_context_for_view(&view_id).await {
                Ok(RoomViewContext {
                    room_binding: None,
                    service_room: Some(service_room),
                    ..
                }) => {
                    let app_config = self.app_config().await;
                    let target_view = service_room
                        .front_view_id
                        .as_deref()
                        .unwrap_or(&app_config.admission_view_id);
                    self.escape_closed_service_room_connection(connection_id, target_view)
                        .await?;
                }
                Ok(_) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn escape_closed_service_room_connection(
        &self,
        connection_id: u64,
        target_view: &str,
    ) -> Result<()> {
        let Some(player_id) = self
            .presence
            .lock()
            .await
            .connection_player_id(connection_id)
        else {
            return Ok(());
        };
        let mut player = self.runtime.player_state(&player_id).await?;
        if player.current_view == target_view {
            self.push_connection_message(
                connection_id,
                "This room is closed. You step back to the street.",
            )
            .await;
            return Ok(());
        }
        player.current_view = target_view.to_owned();
        self.runtime.set_player_state(player.clone()).await?;
        let app = self.app_service().await;
        app.save_player_state(&player).await?;
        self.presence
            .lock()
            .await
            .update_view(connection_id, target_view.to_owned());
        self.push_connection_message(
            connection_id,
            "This room is closed. You step back to the street.",
        )
        .await;
        Ok(())
    }

    async fn push_connection_message(&self, connection_id: u64, message: impl Into<String>) {
        self.pending_connection_messages
            .lock()
            .await
            .entry(connection_id)
            .or_default()
            .push(message.into());
    }

    async fn drain_connection_messages(&self, connection_id: u64) -> Vec<String> {
        self.pending_connection_messages
            .lock()
            .await
            .remove(&connection_id)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
struct RoomViewContext {
    room_binding: Option<StoredRoomBinding>,
    service_room: Option<StoredServiceRoom>,
    front_bindings: Vec<StoredRoomBinding>,
}

impl RoomRegistrationCache for SharedState {
    async fn invalidate_room_cache_for_service_room(
        &self,
        view_id: &str,
        old_room_user: Option<&str>,
        old_front_view_id: Option<&str>,
        new_room_user: Option<&str>,
        new_front_view_id: Option<&str>,
    ) {
        self.invalidate_room_cache_for_service_room(
            view_id,
            old_room_user,
            old_front_view_id,
            new_room_user,
            new_front_view_id,
        )
        .await;
    }
}

#[cfg(test)]
mod lib_tests;

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
