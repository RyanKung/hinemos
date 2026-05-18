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
    /// A world object references an id that does not exist.
    #[error("missing world reference: {0}")]
    MissingReference(String),
    /// A view layout has inconsistent row-major data.
    #[error("invalid view layout: {0}")]
    InvalidLayout(String),
}

/// Loads `views.ron`, `entities.ron`, and `players.ron` from a directory.
pub fn load_world_from_dir(dir: impl Into<PathBuf>) -> Result<WorldState, WorldLoadError> {
    let dir = dir.into();
    let views = load_views(&dir)?;
    let entities = load_entities(&dir)?;
    let players = load_players(&dir)?;
    let world = WorldState {
        views,
        entities,
        players,
    };
    validate_world(&world)?;
    Ok(world)
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

fn validate_world(world: &WorldState) -> Result<(), WorldLoadError> {
    for view in world.views.values() {
        for exit in &view.exits {
            if !world.views.contains_key(&exit.target) {
                return Err(WorldLoadError::MissingReference(format!(
                    "view `{}` exit points to missing view `{}`",
                    view.id, exit.target
                )));
            }
        }

        for entity_id in &view.entities {
            if !world.entities.contains_key(entity_id) {
                return Err(WorldLoadError::MissingReference(format!(
                    "view `{}` contains missing entity `{entity_id}`",
                    view.id
                )));
            }
        }

        if let Some(layout) = &view.layout {
            let expected = usize::from(layout.width) * usize::from(layout.height);
            if layout.tiles.len() != expected {
                return Err(WorldLoadError::InvalidLayout(format!(
                    "view `{}` has {} tiles but expected {expected}",
                    view.id,
                    layout.tiles.len()
                )));
            }
            if layout.collision.len() != expected {
                return Err(WorldLoadError::InvalidLayout(format!(
                    "view `{}` has {} collision flags but expected {expected}",
                    view.id,
                    layout.collision.len()
                )));
            }
            for placement in &layout.placements {
                if !world.entities.contains_key(&placement.entity_id) {
                    return Err(WorldLoadError::MissingReference(format!(
                        "view `{}` layout places missing entity `{}`",
                        view.id, placement.entity_id
                    )));
                }
                if placement.x >= layout.width || placement.y >= layout.height {
                    return Err(WorldLoadError::InvalidLayout(format!(
                        "view `{}` places entity `{}` outside layout bounds",
                        view.id, placement.entity_id
                    )));
                }
            }
        }
    }

    for player in world.players.values() {
        if !world.views.contains_key(&player.current_view) {
            return Err(WorldLoadError::MissingReference(format!(
                "player `{}` starts in missing view `{}`",
                player.id, player.current_view
            )));
        }

        for entity_id in &player.inventory {
            if !world.entities.contains_key(entity_id) {
                return Err(WorldLoadError::MissingReference(format!(
                    "player `{}` carries missing entity `{entity_id}`",
                    player.id
                )));
            }
        }
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use crate::{Direction, Entity, EntityKind, Exit, PlayerState, Requirement, View, WorldState};

    use super::{WorldLoadError, validate_world};

    fn valid_world() -> WorldState {
        WorldState {
            views: [(
                "start".to_owned(),
                View {
                    id: "start".to_owned(),
                    title: "Start".to_owned(),
                    description: "A start room.".to_owned(),
                    ascii_art: Vec::new(),
                    exits: vec![Exit {
                        direction: Direction::East,
                        target: "start".to_owned(),
                        label: None,
                        requirements: vec![Requirement::None],
                    }],
                    entities: vec!["item".to_owned()],
                    layout: None,
                },
            )]
            .into_iter()
            .collect(),
            entities: [(
                "item".to_owned(),
                Entity {
                    id: "item".to_owned(),
                    kind: EntityKind::Item,
                    name: "Item".to_owned(),
                    description: "An item.".to_owned(),
                    aliases: Vec::new(),
                    actions: Vec::new(),
                    collection: None,
                    portable: true,
                },
            )]
            .into_iter()
            .collect(),
            players: [(
                "local_player".to_owned(),
                PlayerState {
                    id: "local_player".to_owned(),
                    current_view: "start".to_owned(),
                    inventory: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn validation_rejects_missing_exit_target() {
        let mut world = valid_world();
        world
            .views
            .get_mut("start")
            .expect("start view exists")
            .exits[0]
            .target = "missing".to_owned();

        let err = validate_world(&world).expect_err("missing exit target should fail");
        assert!(matches!(err, WorldLoadError::MissingReference(_)));
    }

    #[test]
    fn validation_rejects_missing_player_view() {
        let mut world = valid_world();
        world
            .players
            .get_mut("local_player")
            .expect("player exists")
            .current_view = "missing".to_owned();

        let err = validate_world(&world).expect_err("missing player view should fail");
        assert!(matches!(err, WorldLoadError::MissingReference(_)));
    }
}
