//! Load bundled sample worlds from RON files.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::model::{Entity, PlayerState, View, WorldState};

/// Canonical single-player id used by local CLI and tests.
pub const LOCAL_PLAYER_ID: &str = "local_player";

/// Errors loading world files from disk.
#[derive(Debug, Error)]
pub enum WorldLoadError {
    /// Could not read a file.
    #[error("failed to read world file: {0}")]
    Io(#[from] std::io::Error),
    /// Could not parse RON.
    #[error("failed to parse world file: {0}")]
    Ron(#[from] ron::error::SpannedError),
    /// Duplicate canonical id in the same collection.
    #[error("duplicate world id `{0}`")]
    DuplicateId(String),
}

/// Loads `views.ron`, `entities.ron`, and `players.ron` from a directory.
pub fn load_world_from_dir(dir: impl Into<PathBuf>) -> Result<WorldState, WorldLoadError> {
    let dir = dir.into();
    let views = load_views(&dir)?;
    let entities = load_entities(&dir)?;
    let players = load_players(&dir)?;
    Ok(WorldState {
        views,
        entities,
        players,
    })
}

fn load_views(dir: &Path) -> Result<HashMap<String, View>, WorldLoadError> {
    let path = dir.join("views.ron");
    let content = fs::read_to_string(path)?;
    let list: Vec<View> = ron::from_str(&content)?;
    let mut map = HashMap::with_capacity(list.len());
    for view in list {
        let id = view.id.clone();
        if map.insert(id.clone(), view).is_some() {
            return Err(WorldLoadError::DuplicateId(format!("view `{id}`")));
        }
    }
    Ok(map)
}

fn load_entities(dir: &Path) -> Result<HashMap<String, Entity>, WorldLoadError> {
    let path = dir.join("entities.ron");
    let content = fs::read_to_string(path)?;
    let list: Vec<Entity> = ron::from_str(&content)?;
    let mut map = HashMap::with_capacity(list.len());
    for entity in list {
        let id = entity.id.clone();
        if map.insert(id.clone(), entity).is_some() {
            return Err(WorldLoadError::DuplicateId(format!("entity `{id}`")));
        }
    }
    Ok(map)
}

fn load_players(dir: &Path) -> Result<HashMap<String, PlayerState>, WorldLoadError> {
    let path = dir.join("players.ron");
    let content = fs::read_to_string(path)?;
    let list: Vec<PlayerState> = ron::from_str(&content)?;
    let mut map = HashMap::with_capacity(list.len());
    for player in list {
        let id = player.id.clone();
        if map.insert(id.clone(), player).is_some() {
            return Err(WorldLoadError::DuplicateId(format!("player `{id}`")));
        }
    }
    Ok(map)
}
