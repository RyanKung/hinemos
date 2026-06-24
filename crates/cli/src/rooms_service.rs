mod definitions;
mod effects;
mod polling;

use std::time::Duration;

use anyhow::{Context, Result};
use blackstone_izakaya::BlackstoneIzakaya;
use clap::Args;
use definitions::{BANK, BLACKSTONE, SCHOOL};
use hinemos_bank_room::HinemosBank;
use hinemos_newspaper_room::HinemosDailySeer;
use hinemos_school_room::HinemosSchool;
use hinemos_storage::PgStorage;
use polling::{poll_newspaper_room, poll_room, poll_workers_room};
use registry_room::HinemosRegistry;
use workers_society_room::WorkersSociety;

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
    let storage = PgStorage::connect(&database_url).await?;
    let mut rooms = BuiltinRooms::default();
    let interval = Duration::from_millis(args.poll_interval_ms);

    println!(
        "Hinemos built-in room service runner started, polling every {} ms.",
        args.poll_interval_ms
    );
    loop {
        let handled = rooms.poll_once(&storage, args.batch_size).await?;
        if args.once {
            println!("Processed {handled} room request(s).");
            return Ok(());
        }
        if handled == 0 {
            tokio::time::sleep(interval).await;
        }
    }
}

#[derive(Debug, Default)]
struct BuiltinRooms {
    blackstone: BlackstoneIzakaya,
    bank: HinemosBank,
    newspaper: HinemosDailySeer,
    registry: HinemosRegistry,
    school: HinemosSchool,
    workers: WorkersSociety,
}

impl BuiltinRooms {
    async fn poll_once(&mut self, storage: &PgStorage, batch_size: i64) -> Result<usize> {
        let mut handled = 0;
        handled += poll_room(storage, &BLACKSTONE, &mut self.blackstone, batch_size).await?;
        handled += poll_room(storage, &BANK, &mut self.bank, batch_size).await?;
        handled += poll_newspaper_room(storage, &mut self.newspaper, batch_size).await?;
        handled += polling::poll_registry_room(storage, &mut self.registry, batch_size).await?;
        handled += poll_room(storage, &SCHOOL, &mut self.school, batch_size).await?;
        handled += poll_workers_room(storage, &mut self.workers, batch_size).await?;
        Ok(handled)
    }
}
