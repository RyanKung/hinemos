use std::collections::HashMap;

use anyhow::{Context, Result};
use hinemos_storage::{PgStorage, StoredServiceRoom};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum BuiltinHandler {
    Blackstone,
    Bank,
    Newspaper,
    Registry,
    School,
    Workers,
}

impl BuiltinHandler {
    fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "blackstone_izakaya" => Self::Blackstone,
            "hinemos_bank" => Self::Bank,
            "hinemos_daily_seer" => Self::Newspaper,
            "hinemos_registry" => Self::Registry,
            "hinemos_school" => Self::School,
            "workers_society" => Self::Workers,
            _ => return None,
        })
    }
}

pub(super) type BuiltinRoomDefinitions = HashMap<BuiltinHandler, RoomDefinition>;

#[derive(Debug, Clone)]
pub(super) struct RoomDefinition {
    pub(super) handler: BuiltinHandler,
    pub(super) view_id: String,
    pub(super) room_user: String,
    pub(super) room_player_id: String,
}

impl RoomDefinition {
    fn from_service_room(handler: BuiltinHandler, room: &StoredServiceRoom) -> Self {
        Self {
            handler,
            view_id: room.view_id.clone(),
            room_user: room.room_user.clone(),
            room_player_id: room.room_player_id.clone(),
        }
    }
}

pub(super) async fn load_builtin_room_definitions(
    storage: &PgStorage,
) -> Result<BuiltinRoomDefinitions> {
    let mut definitions = HashMap::new();
    for room in storage.builtin_service_rooms().await? {
        let handler_key = room
            .builtin_handler
            .as_deref()
            .context("builtin_service_rooms returned a room without builtin_handler")?;
        let handler = BuiltinHandler::from_key(handler_key).with_context(|| {
            format!(
                "unknown built-in room handler `{}` for {}",
                handler_key, room.view_id
            )
        })?;
        definitions.insert(handler, RoomDefinition::from_service_room(handler, &room));
    }
    Ok(definitions)
}
