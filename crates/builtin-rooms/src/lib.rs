//! Built-in external room service runner.

mod definitions;
mod effects;
mod polling;

use std::time::Duration;

use anyhow::Result;
use blackstone_izakaya::BlackstoneIzakaya;
use definitions::{BANK, BLACKSTONE, SCHOOL};
use hinemos_bank_room::HinemosBank;
use hinemos_newspaper_room::HinemosDailySeer;
use hinemos_school_room::HinemosSchool;
use hinemos_storage::PgStorage;
use polling::{poll_newspaper_room, poll_room, poll_workers_room};
use registry_room::HinemosRegistry;
use workers_society_room::WorkersSociety;

/// Runtime settings for the built-in room service loop.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinRoomsConfig {
    /// Milliseconds to wait after an empty poll cycle.
    pub poll_interval_ms: u64,
    /// Maximum number of pending room requests to process per room per cycle.
    pub batch_size: i64,
    /// Process one cycle and exit.
    pub once: bool,
}

impl Default for BuiltinRoomsConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 1_000,
            batch_size: 20,
            once: false,
        }
    }
}

/// Connect to storage and run the built-in room service loop.
pub async fn run_builtin_rooms(database_url: &str, config: BuiltinRoomsConfig) -> Result<()> {
    let storage = PgStorage::connect(database_url).await?;
    let mut rooms = BuiltinRooms::default();
    let interval = Duration::from_millis(config.poll_interval_ms);

    println!(
        "Hinemos built-in room service runner started, polling every {} ms.",
        config.poll_interval_ms
    );
    loop {
        let handled = rooms.poll_once(&storage, config.batch_size).await?;
        if config.once {
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
