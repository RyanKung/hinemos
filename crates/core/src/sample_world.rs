//! RON-backed sample world loading for the first CLI prototype.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::model::{Entity, PlayerState, View, WorldState};

/// Default local player id.
pub const LOCAL_PLAYER_ID: &str = "local_player";

/// Loads a world from a directory containing `views.ron`, `entities.ron`, and `players.ron`.
pub fn load_world_from_dir(path: impl AsRef<Path>) -> Result<WorldState, WorldLoadError> {
    let path = path.as_ref();
    let views = load_indexed_file::<View>(path.join("views.ron"), |view| &view.id)?;
    let entities = load_indexed_file::<Entity>(path.join("entities.ron"), |entity| &entity.id)?;
    let players = load_indexed_file::<PlayerState>(path.join("players.ron"), |player| &player.id)?;

    Ok(WorldState {
        views,
        entities,
        players,
    })
}

fn load_indexed_file<T>(
    path: impl AsRef<Path>,
    id: impl Fn(&T) -> &str,
) -> Result<HashMap<String, T>, WorldLoadError>
where
    T: serde::de::DeserializeOwned,
{
    let path = path.as_ref();
    let content = fs::read_to_string(path).map_err(|source| WorldLoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let items = ron::from_str::<Vec<T>>(&content).map_err(|source| WorldLoadError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;

    Ok(items
        .into_iter()
        .map(|item| (id(&item).to_owned(), item))
        .collect())
}

/// Errors produced while loading RON world files.
#[derive(Debug)]
pub enum WorldLoadError {
    /// A RON file could not be read.
    Read {
        /// File path that failed.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A RON file could not be parsed.
    Parse {
        /// File path that failed.
        path: std::path::PathBuf,
        /// Underlying parse error.
        source: Box<ron::error::SpannedError>,
    },
}

impl std::fmt::Display for WorldLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "failed to parse {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for WorldLoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sample_world_from_ron_files() {
        let world_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worlds/sample");
        let world = load_world_from_dir(world_dir).expect("sample world should load from RON");

        assert!(world.views.contains_key("village_square"));
        assert!(world.entities.contains_key("rusted_sword"));
        assert!(world.players.contains_key(LOCAL_PLAYER_ID));
    }
}
