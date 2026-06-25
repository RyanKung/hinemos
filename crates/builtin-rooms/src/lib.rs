//! Built-in external room service runner.
#![deny(missing_docs)]

mod definitions;
mod effects;
mod polling;

use std::time::Duration;

use anyhow::Result;
use blackstone_izakaya::BlackstoneIzakaya;
use definitions::{BuiltinHandler, BuiltinRoomDefinitions, load_builtin_room_definitions};
use hinemos_bank_room::HinemosBank;
use hinemos_newspaper_room::HinemosDailySeer;
use hinemos_school_room::HinemosSchool;
use hinemos_storage::PgStorage;
use polling::{poll_newspaper_room, poll_room};
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
    storage.migrate().await?;
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
        let definitions = load_builtin_room_definitions(storage).await?;
        let mut handled = 0;
        if let Some(room) = room(&definitions, BuiltinHandler::Blackstone) {
            handled += poll_room(storage, room, &mut self.blackstone, batch_size).await?;
        }
        if let Some(room) = room(&definitions, BuiltinHandler::Bank) {
            handled += poll_room(storage, room, &mut self.bank, batch_size).await?;
        }
        if let Some(room) = room(&definitions, BuiltinHandler::Newspaper) {
            handled += poll_newspaper_room(storage, room, &mut self.newspaper, batch_size).await?;
        }
        if let Some(room) = room(&definitions, BuiltinHandler::Registry) {
            handled += poll_room(storage, room, &mut self.registry, batch_size).await?;
        }
        if let Some(room) = room(&definitions, BuiltinHandler::School) {
            handled += poll_room(storage, room, &mut self.school, batch_size).await?;
        }
        if let Some(room) = room(&definitions, BuiltinHandler::Workers) {
            handled += poll_room(storage, room, &mut self.workers, batch_size).await?;
        }
        Ok(handled)
    }
}

fn room(
    definitions: &BuiltinRoomDefinitions,
    handler: BuiltinHandler,
) -> Option<&definitions::RoomDefinition> {
    definitions.get(&handler)
}
