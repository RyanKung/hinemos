//! Unix domain socket for [`xagora_runtime::AdminRequest`] control messages.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use xagora_runtime::{AdminRequest, AdminResponse, AdminSession, MAX_ADMIN_FRAME};

use super::{PresenceRegistry, SharedState};

/// Removes stale socket files, binds `socket_path`, chmod `0600`, then accepts until error.
pub(super) async fn run_admin_listener(
    socket_path: PathBuf,
    shared: Arc<SharedState>,
    default_world: PathBuf,
) -> Result<()> {
    if socket_path.exists() {
        fs::remove_file(&socket_path).with_context(|| {
            format!(
                "failed to remove stale admin socket {}",
                socket_path.display()
            )
        })?;
    }
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create admin socket parent {}", parent.display())
        })?;
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind admin unix socket {}", socket_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("chmod admin socket {}", socket_path.display()))?;
    }

    eprintln!("Xagora admin socket listening on {}", socket_path.display());

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("failed to accept admin connection")?;
        let shared = Arc::clone(&shared);
        let default_world = default_world.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_admin_connection(stream, shared, default_world).await {
                eprintln!("admin connection error: {error:#}");
            }
        });
    }
}

async fn handle_admin_connection(
    mut stream: UnixStream,
    shared: Arc<SharedState>,
    default_world: PathBuf,
) -> Result<()> {
    let request: AdminRequest = read_framed_json_async(&mut stream).await?;
    let response = dispatch_admin_request(request, &shared, default_world).await;
    write_framed_json_async(&mut stream, &response).await?;
    Ok(())
}

async fn dispatch_admin_request(
    request: AdminRequest,
    shared: &Arc<SharedState>,
    default_world: PathBuf,
) -> AdminResponse {
    match request {
        AdminRequest::Ping => AdminResponse::Pong,
        AdminRequest::ListSessions => {
            let sessions = shared.presence.lock().await.admin_sessions();
            AdminResponse::Sessions { sessions }
        }
        AdminRequest::KickConnection { connection_id } => {
            let kicked = shared.presence.lock().await.request_kick(connection_id);
            if kicked {
                AdminResponse::Ok {
                    message: format!("disconnect scheduled for connection {connection_id}"),
                }
            } else {
                AdminResponse::Error {
                    message: format!("unknown connection_id {connection_id}"),
                }
            }
        }
        AdminRequest::ReloadWorld { world_dir } => {
            let dir = world_dir.unwrap_or(default_world);
            let new_runtime = match shared.runtime.read() {
                Ok(guard) => match guard.reload_from_world_dir_preserving_players(&dir) {
                    Ok(runtime) => runtime,
                    Err(error) => return AdminResponse::from_reload_error(error),
                },
                Err(_) => return poison_error(),
            };

            match shared.runtime.write() {
                Ok(mut guard) => *guard = new_runtime,
                Err(_) => return poison_error(),
            }

            let aliases = match shared.runtime.read() {
                Ok(guard) => match guard.world() {
                    Ok(world) => world.entity_alias_map(),
                    Err(error) => return AdminResponse::from_runtime_error(error),
                },
                Err(_) => return poison_error(),
            };

            match shared.entity_aliases.write() {
                Ok(mut guard) => *guard = aliases,
                Err(_) => return poison_error(),
            }

            AdminResponse::Ok {
                message: format!("reloaded world from {}", dir.display()),
            }
        }
    }
}

fn poison_error() -> AdminResponse {
    AdminResponse::Error {
        message: "runtime lock poisoned".to_owned(),
    }
}

async fn read_framed_json_async<T: serde::de::DeserializeOwned>(
    stream: &mut UnixStream,
) -> Result<T> {
    let raw = read_framed_raw_async(stream).await?;
    serde_json::from_slice(&raw).context("invalid admin JSON payload")
}

async fn write_framed_json_async<T: serde::Serialize>(
    stream: &mut UnixStream,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value).context("failed to serialize admin response")?;
    write_framed_raw_async(stream, &bytes).await
}

async fn read_framed_raw_async(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut len_buf = [0_u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_ADMIN_FRAME {
        anyhow::bail!("admin frame length {len} exceeds maximum {MAX_ADMIN_FRAME}");
    }
    let mut buf = vec![0_u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_framed_raw_async(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_ADMIN_FRAME {
        anyhow::bail!(
            "admin payload length {} exceeds maximum {MAX_ADMIN_FRAME}",
            bytes.len()
        );
    }
    let len = u32::try_from(bytes.len()).context("admin payload length overflow")?;
    stream.write_all(&len.to_le_bytes()).await?;
    stream.write_all(bytes).await?;
    stream.flush().await?;
    Ok(())
}

impl PresenceRegistry {
    pub(super) fn admin_sessions(&self) -> Vec<AdminSession> {
        self.connections
            .iter()
            .map(|(&connection_id, record)| AdminSession {
                connection_id,
                player_id: record.player_id.clone(),
                user: record.user.clone(),
            })
            .collect()
    }

    pub(super) fn request_kick(&mut self, connection_id: u64) -> bool {
        if self.connections.contains_key(&connection_id) {
            self.pending_kicks.insert(connection_id);
            true
        } else {
            false
        }
    }

    pub(super) fn poll_kick(&mut self, connection_id: u64) -> bool {
        self.pending_kicks.remove(&connection_id)
    }
}
