#![deny(missing_docs)]

//! Admin control-plane wire protocol and Unix-socket client helpers.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum serialized admin frame (request or response).
pub const MAX_ADMIN_FRAME: usize = 1024 * 1024;

/// Wire protocol request from CLI / tooling to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AdminRequest {
    /// Liveness check.
    Ping,
    /// Runtime and world summary.
    Status,
    /// List authenticated SSH sessions.
    ListSessions,
    /// List online SSH users grouped by username.
    ListUsers,
    /// Disconnect the given connection id (best-effort).
    KickConnection {
        /// Connection id assigned by the daemon at accept time.
        connection_id: u64,
    },
    /// Reload world files from disk while merging existing player states.
    ReloadWorld {
        /// World directory; defaults to the daemon's configured world path.
        #[serde(default)]
        world_dir: Option<PathBuf>,
    },
    /// Generate or rotate the SMTP/IMAP token for an externally registered service room.
    RoomToken {
        /// Service room view id from rooms.ron / service_rooms.
        view_id: String,
    },
}

/// Successful or error payload returned to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AdminResponse {
    /// Generic success with a short message.
    Ok {
        /// Human-readable summary.
        message: String,
    },
    /// Reply to [`AdminRequest::Ping`].
    Pong,
    /// Runtime and world summary.
    Status {
        /// Current runtime status.
        summary: AdminStatus,
    },
    /// Active sessions snapshot.
    Sessions {
        /// One row per authenticated connection.
        sessions: Vec<AdminSession>,
    },
    /// Online users snapshot.
    Users {
        /// One row per online SSH username.
        users: Vec<AdminUser>,
    },
    /// One-time plaintext room mailbox token.
    RoomToken {
        /// Service room view id.
        view_id: String,
        /// SMTP/IMAP username for the room service.
        username: String,
        /// Registered room player id.
        player_id: String,
        /// Plaintext token, shown only in this response.
        token: String,
    },
    /// Operation failed; `message` is safe to show operators.
    Error {
        /// Explanation suitable for logs / CLI.
        message: String,
    },
}

impl AdminResponse {
    /// Builds a generic error response from displayable error text.
    #[must_use]
    pub fn error(error: impl std::fmt::Display) -> Self {
        AdminResponse::Error {
            message: error.to_string(),
        }
    }
}

/// Current runtime and world summary for operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminStatus {
    /// Number of active SSH sessions.
    pub session_count: usize,
    /// Number of distinct SSH usernames online.
    pub user_count: usize,
    /// Number of loaded views in the current world.
    pub view_count: usize,
    /// Number of loaded entities in the current world.
    pub entity_count: usize,
    /// Number of runtime player states.
    pub player_count: usize,
}

/// One connected player session as seen by the admin plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminSession {
    /// Daemon-issued connection id (used with [`AdminRequest::KickConnection`]).
    pub connection_id: u64,
    /// Stable player id in the runtime.
    pub player_id: String,
    /// SSH username offered during authentication.
    pub user: String,
}

/// Online SSH user grouped across one or more sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminUser {
    /// SSH username.
    pub user: String,
    /// Number of active connections using this username.
    pub session_count: usize,
    /// Player ids currently associated with this username.
    pub player_ids: Vec<String>,
}

/// Phase of a single admin RPC (used for stable client-side error text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdminRpcPhase {
    Connect,
    SendRequest,
    ReadResponse,
}

/// Client-side failure reaching the daemon admin socket or completing an RPC.
#[derive(Debug, Error)]
#[error("{summary}")]
pub struct AdminClientError {
    summary: String,
    #[source]
    source: io::Error,
}

impl AdminClientError {
    fn new(socket_path: &Path, phase: AdminRpcPhase, source: io::Error) -> Self {
        Self {
            summary: admin_rpc_failure_message(socket_path, phase, &source),
            source,
        }
    }
}

fn admin_rpc_failure_message(socket_path: &Path, phase: AdminRpcPhase, err: &io::Error) -> String {
    let path = socket_path.display();
    match phase {
        AdminRpcPhase::Connect => match err.kind() {
            io::ErrorKind::NotFound => format!(
                "cannot reach daemon: admin socket does not exist at {path} (daemon not running or wrong --admin-socket?)"
            ),
            io::ErrorKind::ConnectionRefused => format!(
                "cannot reach daemon: nothing is listening on admin socket at {path} (daemon not running?)"
            ),
            io::ErrorKind::PermissionDenied => {
                format!("cannot access admin socket at {path}: permission denied")
            }
            io::ErrorKind::AddrNotAvailable | io::ErrorKind::InvalidInput => {
                format!("invalid admin socket path {path}")
            }
            _ => format!("cannot connect to admin socket at {path}: {err}"),
        },
        AdminRpcPhase::SendRequest => match err.kind() {
            io::ErrorKind::BrokenPipe => format!(
                "daemon closed the admin connection before the request was fully sent (socket {path})"
            ),
            io::ErrorKind::ConnectionReset => {
                format!("lost connection to daemon while sending admin request (socket {path})")
            }
            _ => format!("failed to send admin request to {path}: {err}"),
        },
        AdminRpcPhase::ReadResponse => match err.kind() {
            io::ErrorKind::UnexpectedEof => format!(
                "daemon stopped before sending a complete admin response (socket {path}; daemon likely exited)"
            ),
            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset => format!(
                "daemon closed the admin connection before a complete response (socket {path})"
            ),
            io::ErrorKind::InvalidData => {
                format!("invalid admin response from daemon at {path}: {err}")
            }
            _ => format!("failed to read admin response from {path}: {err}"),
        },
    }
}

/// Reads a length-prefixed JSON frame (little-endian `u32` length).
pub fn read_framed_json<R: Read, T: for<'de> Deserialize<'de>>(reader: &mut R) -> io::Result<T> {
    let raw = read_framed_raw(reader)?;
    serde_json::from_slice(&raw).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid admin JSON: {error}"),
        )
    })
}

/// Writes a length-prefixed JSON frame (little-endian `u32` length).
pub fn write_framed_json<W: Write, T: Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to serialize admin payload: {error}"),
        )
    })?;
    write_framed_raw(writer, &bytes)
}

fn read_framed_raw(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len_buf = [0_u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_ADMIN_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("admin frame length {len} exceeds maximum {MAX_ADMIN_FRAME}"),
        ));
    }
    let mut buf = vec![0_u8; len];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

fn write_framed_raw(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_ADMIN_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "admin payload length {} exceeds maximum {MAX_ADMIN_FRAME}",
                bytes.len()
            ),
        ));
    }
    let len = u32::try_from(bytes.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "admin payload length overflow")
    })?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()?;
    Ok(())
}

/// Blocking Unix socket RPC helper for CLI and scripts (Unix only).
///
/// Returns [`AdminClientError`] with stable messages when the daemon is down, the socket path is wrong,
/// or the connection drops mid-RPC.
#[cfg(unix)]
pub fn unix_admin_call(
    socket_path: &Path,
    request: &AdminRequest,
) -> Result<AdminResponse, AdminClientError> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path)
        .map_err(|source| AdminClientError::new(socket_path, AdminRpcPhase::Connect, source))?;
    write_framed_json(&mut stream, request)
        .map_err(|source| AdminClientError::new(socket_path, AdminRpcPhase::SendRequest, source))?;
    read_framed_json(&mut stream)
        .map_err(|source| AdminClientError::new(socket_path, AdminRpcPhase::ReadResponse, source))
}

#[cfg(test)]
#[test]
fn stable_connect_not_found_message() {
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "ENOENT");
    let msg = admin_rpc_failure_message(
        Path::new("/run/hinemos/admin.sock"),
        AdminRpcPhase::Connect,
        &err,
    );
    assert!(
        msg.starts_with("cannot reach daemon:"),
        "unexpected message: {msg}"
    );
    assert!(msg.contains("does not exist"), "unexpected message: {msg}");
}

#[cfg(test)]
#[test]
fn stable_connect_refused_message() {
    let err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "");
    let msg = admin_rpc_failure_message(Path::new("/tmp/x.sock"), AdminRpcPhase::Connect, &err);
    assert!(
        msg.contains("nothing is listening"),
        "unexpected message: {msg}"
    );
}

#[cfg(test)]
#[test]
fn stable_read_unexpected_eof_message() {
    let err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "");
    let msg =
        admin_rpc_failure_message(Path::new("/tmp/x.sock"), AdminRpcPhase::ReadResponse, &err);
    assert!(msg.contains("daemon stopped"), "unexpected message: {msg}");
}

#[cfg(test)]
#[test]
fn status_response_round_trips() {
    let response = AdminResponse::Status {
        summary: AdminStatus {
            session_count: 2,
            user_count: 1,
            view_count: 5,
            entity_count: 8,
            player_count: 3,
        },
    };

    let encoded = serde_json::to_vec(&response).expect("status response should serialize");
    let decoded: AdminResponse =
        serde_json::from_slice(&encoded).expect("status response should deserialize");
    match decoded {
        AdminResponse::Status { summary } => {
            assert_eq!(summary.session_count, 2);
            assert_eq!(summary.user_count, 1);
            assert_eq!(summary.view_count, 5);
            assert_eq!(summary.entity_count, 8);
            assert_eq!(summary.player_count, 3);
        }
        other => panic!("unexpected response: {other:?}"),
    }
}

#[cfg(test)]
#[test]
fn room_token_request_and_response_round_trip() {
    let request = AdminRequest::RoomToken {
        view_id: "example_service_room".to_owned(),
    };
    let encoded = serde_json::to_vec(&request).expect("room token request should serialize");
    let decoded: AdminRequest =
        serde_json::from_slice(&encoded).expect("room token request should deserialize");
    match decoded {
        AdminRequest::RoomToken { view_id } => assert_eq!(view_id, "example_service_room"),
        other => panic!("unexpected request: {other:?}"),
    }

    let response = AdminResponse::RoomToken {
        view_id: "example_service_room".to_owned(),
        username: "example-room-service".to_owned(),
        player_id: "room:example-service".to_owned(),
        token: "secret".to_owned(),
    };
    let encoded = serde_json::to_vec(&response).expect("room token response should serialize");
    let decoded: AdminResponse =
        serde_json::from_slice(&encoded).expect("room token response should deserialize");
    match decoded {
        AdminResponse::RoomToken {
            view_id,
            username,
            player_id,
            token,
        } => {
            assert_eq!(view_id, "example_service_room");
            assert_eq!(username, "example-room-service");
            assert_eq!(player_id, "room:example-service");
            assert_eq!(token, "secret");
        }
        other => panic!("unexpected response: {other:?}"),
    }
}
