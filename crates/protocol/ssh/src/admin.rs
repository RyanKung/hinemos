//! Unix domain socket for [`hinemos_admin_protocol::AdminRequest`] control messages.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hinemos_admin_protocol::{AdminRequest, AdminResponse, MAX_ADMIN_FRAME};
use rand::Rng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use super::SharedState;

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

    eprintln!(
        "Hinemos admin socket listening on {}",
        socket_path.display()
    );

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
        AdminRequest::Status => {
            let (session_count, user_count) = {
                let presence = shared.presence.lock().await;
                (presence.session_count(), presence.user_count())
            };
            match shared.runtime.world_counts().await {
                Ok(counts) => AdminResponse::Status {
                    summary: counts.into_status(session_count, user_count),
                },
                Err(error) => AdminResponse::error(error),
            }
        }
        AdminRequest::ListSessions => {
            let sessions = shared.presence.lock().await.admin_sessions();
            AdminResponse::Sessions { sessions }
        }
        AdminRequest::ListUsers => {
            let users = shared.presence.lock().await.admin_users();
            AdminResponse::Users { users }
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
            if let Err(error) = shared.reload_world_from_dir(&dir).await {
                return AdminResponse::error(error);
            }
            match shared.runtime.world_counts().await {
                Ok(counts) => AdminResponse::Ok {
                    message: format!(
                        "reloaded map from {} (views={} entities={} players={})",
                        dir.display(),
                        counts.view_count,
                        counts.entity_count,
                        counts.player_count
                    ),
                },
                Err(error) => AdminResponse::error(error),
            }
        }
        AdminRequest::RoomToken { view_id } => {
            let token = generate_admin_mail_auth_token();
            match shared
                .set_service_room_mail_auth_token(&view_id, &token)
                .await
            {
                Ok(auth) => AdminResponse::RoomToken {
                    view_id,
                    username: auth.username,
                    player_id: auth.player_id,
                    token,
                },
                Err(error) => AdminResponse::error(error),
            }
        }
    }
}

fn generate_admin_mail_auth_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
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
