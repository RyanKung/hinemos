//! Command-line configuration for the SSH daemon.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Args;

/// SSH adapter command-line arguments.
#[derive(Debug, Clone, Args)]
pub struct SshArgs {
    #[arg(long, default_value = "127.0.0.1:2222")]
    pub(crate) bind: SocketAddr,

    #[arg(long, default_value = "worlds/sample")]
    pub(crate) world: PathBuf,

    #[arg(long, default_value = ".xagora/ssh_host_ed25519_key")]
    pub(crate) host_key: PathBuf,

    /// Idle timeout in seconds; 0 disables automatic idle disconnects.
    #[arg(long, default_value_t = 0)]
    pub(crate) idle_timeout_seconds: u64,

    /// Unix domain socket path for local admin commands (`xagora admin`).
    #[cfg(unix)]
    #[arg(long, default_value = ".xagora/admin.sock")]
    pub(crate) admin_socket: PathBuf,
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
    pub(crate) fn from_args(args: SshArgs) -> Self {
        Self {
            bind: args.bind,
            world: args.world,
            host_key: args.host_key,
            idle_timeout_seconds: args.idle_timeout_seconds,
            #[cfg(unix)]
            admin_socket: args.admin_socket,
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

pub(crate) fn mail_domain_from_env() -> Option<String> {
    std::env::var("XAGORA_MAIL_DOMAIN")
        .ok()
        .map(|domain| domain.trim().trim_matches('.').to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
}

pub(crate) fn normalize_mail_target(
    target: &str,
    mail_domain: Option<&str>,
) -> anyhow::Result<String> {
    let target = target.trim();
    let Some((local, domain)) = target.split_once('@') else {
        return Ok(target.to_owned());
    };
    let local = local.trim();
    let domain = domain.trim().trim_matches('.').to_ascii_lowercase();
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        anyhow::bail!("invalid mail address: {target}");
    }
    let Some(local_domain) = mail_domain else {
        anyhow::bail!(
            "mail domain is not configured; use a bare Xagora username or set XAGORA_MAIL_DOMAIN"
        );
    };
    if domain != local_domain {
        anyhow::bail!(
            "external mail domain is not available: {domain}; local domain is {local_domain}"
        );
    }
    Ok(local.to_owned())
}

pub(crate) fn format_mail_user(user: &str, mail_domain: Option<&str>) -> String {
    match mail_domain {
        Some(domain) if !user.contains('@') => format!("{user}@{domain}"),
        _ => user.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_mail_user, normalize_mail_target};

    #[test]
    fn mail_target_accepts_bare_user() {
        assert_eq!(
            normalize_mail_target("alice", Some("xagora.local")).expect("normalize"),
            "alice"
        );
    }

    #[test]
    fn mail_target_accepts_configured_domain_case_insensitively() {
        assert_eq!(
            normalize_mail_target("alice@XAGORA.LOCAL", Some("xagora.local")).expect("normalize"),
            "alice"
        );
    }

    #[test]
    fn mail_target_rejects_address_without_domain_config() {
        let error = normalize_mail_target("alice@xagora.local", None).expect_err("reject");
        assert!(error.to_string().contains("mail domain is not configured"));
    }

    #[test]
    fn mail_target_rejects_external_domain() {
        let error =
            normalize_mail_target("alice@example.com", Some("xagora.local")).expect_err("reject");
        assert!(error.to_string().contains("external mail domain"));
    }

    #[test]
    fn mail_user_formats_configured_domain() {
        assert_eq!(
            format_mail_user("alice", Some("xagora.local")),
            "alice@xagora.local"
        );
        assert_eq!(format_mail_user("alice", None), "alice");
    }
}
