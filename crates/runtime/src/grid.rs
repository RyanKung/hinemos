use hinemos_core::{
    DEFAULT_ADMISSION_VIEW_ID, EntityId, EntityKind, EntityObservation, GridOrigin, View, ViewId,
    WorldState, generated_map_ascii_with_origin, generated_origin_view, grid_view_with_origin,
    is_grid_view_id,
};

use crate::{GameRuntime, RuntimeError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeGridOrigin {
    pub(super) view_id: ViewId,
    pub(super) label: String,
}

impl GameRuntime {
    pub(super) fn visible_entities(&self, view_id: &str) -> Result<Vec<EntityId>, RuntimeError> {
        if !self.views.contains_key(view_id) && is_grid_view_id(view_id) {
            return Ok(Vec::new());
        }
        Ok(self
            .view_state(view_id)?
            .lock()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .dropped_entities
            .clone())
    }

    pub(super) fn view(&self, view_id: &str) -> Result<View, RuntimeError> {
        if let Some(view) = self.world.views.get(view_id) {
            let origin = self.grid_origin();
            if let Some(origin_view) = generated_origin_view(view, origin) {
                return Ok(origin_view);
            }
            return Ok(view.clone());
        }
        grid_view_with_origin(view_id, self.grid_origin())
            .ok_or_else(|| RuntimeError::ViewNotFound(view_id.to_owned()))
    }

    pub(super) fn ensure_observable_view(&self, view_id: &str) -> Result<(), RuntimeError> {
        self.view(view_id).map(|_| ())
    }

    pub(super) fn render_ascii_art_for_view(
        &self,
        view: &View,
        entities: &[EntityObservation],
    ) -> Vec<String> {
        let mut ascii_art = generated_map_ascii_with_origin(&view.id, self.grid_origin())
            .unwrap_or_else(|| view.ascii_art.clone());
        if self.generated_map_applies_to(&view.id) {
            append_visible_map_objects(&mut ascii_art, entities);
        }
        ascii_art
    }

    fn generated_map_applies_to(&self, view_id: &str) -> bool {
        view_id == self.world.grid_origin.view_id || is_grid_view_id(view_id)
    }

    fn grid_origin(&self) -> GridOrigin<'_> {
        GridOrigin::new(
            &self.world.grid_origin.view_id,
            &self.world.grid_origin.label,
        )
    }
}

pub(super) fn default_grid_origin_view_id(world: &WorldState) -> ViewId {
    world
        .players
        .get(hinemos_core::sample_world::LOCAL_PLAYER_ID)
        .map(|player| player.current_view.clone())
        .or_else(|| {
            world
                .views
                .contains_key(DEFAULT_ADMISSION_VIEW_ID)
                .then(|| DEFAULT_ADMISSION_VIEW_ID.to_owned())
        })
        .or_else(|| world.views.keys().min().cloned())
        .unwrap_or_default()
}

pub(super) fn grid_origin_from_view_id(
    world: &WorldState,
    view_id: ViewId,
) -> Result<RuntimeGridOrigin, RuntimeError> {
    let label = world
        .views
        .get(&view_id)
        .map(|view| view.title.clone())
        .ok_or_else(|| RuntimeError::ViewNotFound(view_id.clone()))?;
    Ok(RuntimeGridOrigin { view_id, label })
}

fn append_visible_map_objects(ascii_art: &mut Vec<String>, entities: &[EntityObservation]) {
    let names = entities
        .iter()
        .filter(|entity| matches!(entity.kind, EntityKind::Item | EntityKind::Object))
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>();
    if !names.is_empty() {
        ascii_art.push(format!("objects: {}", names.join(", ")));
    }
}
