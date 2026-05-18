//! Command-line configuration for the SSH daemon.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "xagora-ssh")]
#[command(about = "SSH adapter for the Xagora MUD runtime")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:2222")]
    bind: SocketAddr,

    #[arg(long, default_value = "worlds/sample")]
    world: PathBuf,

    #[arg(long, default_value = ".xagora/ssh_host_ed25519_key")]
    host_key: PathBuf,

    /// Idle timeout in seconds; 0 disables automatic idle disconnects.
    #[arg(long, default_value_t = 0)]
    idle_timeout_seconds: u64,

    /// Unix domain socket path for local admin commands (`xagora admin`).
    #[cfg(unix)]
    #[arg(long, default_value = ".xagora/admin.sock")]
    admin_socket: PathBuf,
}

#[derive(Debug)]
pub(crate) struct DaemonConfig {
    pub(crate) bind: SocketAddr,
    pub(crate) world: PathBuf,
    pub(crate) host_key: PathBuf,
    pub(crate) idle_timeout_seconds: u64,
    #[cfg(unix)]
    pub(crate) admin_socket: PathBuf,
}

impl DaemonConfig {
    pub(crate) fn parse() -> Self {
        let cli = Cli::parse();
        Self {
            bind: cli.bind,
            world: cli.world,
            host_key: cli.host_key,
            idle_timeout_seconds: cli.idle_timeout_seconds,
            #[cfg(unix)]
            admin_socket: cli.admin_socket,
        }
    }

    pub(crate) fn idle_timeout(&self) -> Option<std::time::Duration> {
        if self.idle_timeout_seconds == 0 {
            None
        } else {
            Some(std::time::Duration::from_secs(self.idle_timeout_seconds))
        }
    }
}

pub(crate) fn mask_database_url(database_url: &str) -> String {
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
