//! SSH daemon process entry point.

#![deny(missing_docs)]

/// Xagora SSH adapter entrypoint.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    xagora_ssh::run_daemon().await
}
