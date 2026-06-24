use anyhow::{Context, Result};
use clap::Args;
use hinemos_builtin_rooms::{BuiltinRoomsConfig, run_builtin_rooms};

#[derive(Debug, Clone, Args)]
pub(crate) struct RoomsArgs {
    #[arg(long)]
    database_url: Option<String>,

    #[arg(long, default_value_t = 1_000)]
    poll_interval_ms: u64,

    #[arg(long, default_value_t = 20)]
    batch_size: i64,

    #[arg(long)]
    once: bool,
}

pub(crate) async fn run(args: RoomsArgs) -> Result<()> {
    let database_url = args
        .database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .context("DATABASE_URL must be set or passed with --database-url")?;
    run_builtin_rooms(
        &database_url,
        BuiltinRoomsConfig {
            poll_interval_ms: args.poll_interval_ms,
            batch_size: args.batch_size,
            once: args.once,
        },
    )
    .await
}
