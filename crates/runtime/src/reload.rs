//! Runtime world reload support.

use std::path::PathBuf;

use hinemos_core::sample_world::{self, LOCAL_PLAYER_ID};
use hinemos_core::{PlayerState, WorldState};

use crate::{GameRuntime, ReloadError};

impl GameRuntime {
    /// Reloads world files from `dir`, merging player states from `self` so SSH sessions stay anchored when possible.
    ///
    /// Players whose current view no longer exists are moved to the `local_player` spawn view from the new files.
    /// Inventory is filtered to entity ids that still exist.
    pub fn reload_from_world_dir_preserving_players(
        &self,
        dir: impl Into<PathBuf>,
    ) -> Result<Self, ReloadError> {
        let dir = dir.into();
        let fresh = sample_world::load_world_from_dir(&dir)?;
        let old_world = self.world()?;
        let merged = merge_world_reload(fresh, &old_world.players);
        Ok(GameRuntime::new(merged))
    }
}

fn merge_world_reload(
    fresh: WorldState,
    old_players: &std::collections::HashMap<String, PlayerState>,
) -> WorldState {
    let fallback_view = fresh
        .players
        .get(LOCAL_PLAYER_ID)
        .map(|player| player.current_view.clone())
        .or_else(|| fresh.views.keys().next().cloned())
        .unwrap_or_default();

    let mut players = fresh.players.clone();
    for (player_id, state) in old_players {
        let merged = merge_player_for_reload(state.clone(), &fresh, &fallback_view);
        players.insert(player_id.clone(), merged);
    }

    WorldState {
        views: fresh.views,
        entities: fresh.entities,
        players,
    }
}

fn merge_player_for_reload(
    mut player: PlayerState,
    fresh: &WorldState,
    fallback_view: &str,
) -> PlayerState {
    if !fresh.views.contains_key(&player.current_view) {
        player.current_view = fallback_view.to_owned();
    }
    player
        .inventory
        .retain(|entity_id| fresh.entities.contains_key(entity_id));
    player
}
