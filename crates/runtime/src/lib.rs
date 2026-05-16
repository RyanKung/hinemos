#![deny(missing_docs)]

//! Runtime command execution for the Agentopia world.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use agentopia_core::{
    ActionKind, EntityId, EntityObservation, EntityRef, ExitObservation, JsonObservation,
    ObservationEvent, PlayerId, PlayerState, SemanticCommand, TextObservation, View, ViewId,
    WorldState,
};
use thiserror::Error;

/// Provides localized text to the runtime without coupling it to resource files.
pub trait Localizer {
    /// Resolves a text key into the active language.
    fn text(&self, key: &str) -> String;
}

/// Errors produced by command execution and observation building.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RuntimeError {
    /// The requested player does not exist.
    #[error("player not found: {0}")]
    PlayerNotFound(String),
    /// The requested view does not exist.
    #[error("view not found: {0}")]
    ViewNotFound(String),
    /// The requested entity does not exist.
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    /// No exit exists in the requested direction.
    #[error("exit not found: {0}")]
    ExitNotFound(String),
    /// Entity is not visible to the player.
    #[error("entity is not visible: {0}")]
    EntityNotVisible(String),
    /// Entity cannot be carried.
    #[error("entity is not portable: {0}")]
    EntityNotPortable(String),
    /// Runtime state lock was poisoned by a previous panic.
    #[error("runtime state lock poisoned")]
    StatePoisoned,
}

/// Executes commands against static world data and fine-grained mutable runtime state.
#[derive(Debug)]
pub struct GameRuntime {
    world: Arc<StaticWorld>,
    players: RwLock<HashMap<PlayerId, Arc<Mutex<PlayerState>>>>,
    views: HashMap<ViewId, Arc<Mutex<ViewState>>>,
}

#[derive(Debug)]
struct StaticWorld {
    views: HashMap<ViewId, View>,
    entities: HashMap<EntityId, agentopia_core::Entity>,
    template_players: HashMap<PlayerId, PlayerState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewState {
    dropped_entities: Vec<EntityId>,
}

impl GameRuntime {
    /// Creates a runtime from an initial world state.
    #[must_use]
    pub fn new(world: WorldState) -> Self {
        let views = world
            .views
            .iter()
            .map(|(view_id, view)| {
                (
                    view_id.clone(),
                    Arc::new(Mutex::new(ViewState {
                        dropped_entities: view.entities.clone(),
                    })),
                )
            })
            .collect();
        let players = world
            .players
            .iter()
            .map(|(player_id, player)| (player_id.clone(), Arc::new(Mutex::new(player.clone()))))
            .collect();
        let world = StaticWorld {
            views: world.views,
            entities: world.entities,
            template_players: world.players,
        };

        Self {
            world: Arc::new(world),
            players: RwLock::new(players),
            views,
        }
    }

    /// Returns a snapshot of the current world state.
    pub fn world(&self) -> Result<WorldState, RuntimeError> {
        let mut views = self.world.views.clone();
        for (view_id, view) in &mut views {
            let Some(view_state) = self.views.get(view_id) else {
                return Err(RuntimeError::ViewNotFound(view_id.clone()));
            };
            view.entities = view_state
                .lock()
                .map_err(|_| RuntimeError::StatePoisoned)?
                .dropped_entities
                .clone();
        }

        let players = self
            .players
            .read()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .iter()
            .map(|(player_id, player)| {
                Ok((
                    player_id.clone(),
                    player
                        .lock()
                        .map_err(|_| RuntimeError::StatePoisoned)?
                        .clone(),
                ))
            })
            .collect::<Result<HashMap<_, _>, RuntimeError>>()?;

        Ok(WorldState {
            views,
            entities: self.world.entities.clone(),
            players,
        })
    }

    /// Inserts or replaces a player state.
    pub fn set_player_state(&self, player: PlayerState) -> Result<(), RuntimeError> {
        for entity_id in &player.inventory {
            for view_state in self.views.values() {
                view_state
                    .lock()
                    .map_err(|_| RuntimeError::StatePoisoned)?
                    .dropped_entities
                    .retain(|visible_entity_id| visible_entity_id != entity_id);
            }
        }

        let player_id = player.id.clone();
        let existing = self
            .players
            .read()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .get(&player_id)
            .cloned();
        if let Some(existing) = existing {
            *existing.lock().map_err(|_| RuntimeError::StatePoisoned)? = player;
            return Ok(());
        }

        self.players
            .write()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .insert(player_id, Arc::new(Mutex::new(player)));
        Ok(())
    }

    /// Returns a cloned player state for persistence.
    pub fn player_state(&self, player_id: &str) -> Result<PlayerState, RuntimeError> {
        self.player(player_id)?
            .lock()
            .map_err(|_| RuntimeError::StatePoisoned)
            .map(|player| player.clone())
    }

    /// Ensures that a player exists by cloning the starting view from a template player.
    pub fn ensure_player_from_template(
        &self,
        player_id: &str,
        template_player_id: &str,
    ) -> Result<(), RuntimeError> {
        if self
            .players
            .read()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .contains_key(player_id)
        {
            return Ok(());
        }

        let template = self
            .world
            .template_players
            .get(template_player_id)
            .ok_or_else(|| RuntimeError::PlayerNotFound(template_player_id.to_owned()))?;
        self.players
            .write()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .insert(
                player_id.to_owned(),
                Arc::new(Mutex::new(PlayerState {
                    id: player_id.to_owned(),
                    current_view: template.current_view.clone(),
                    inventory: Vec::new(),
                })),
            );
        Ok(())
    }

    /// Executes a command and returns localized structured observation.
    pub fn execute(
        &self,
        player_id: &str,
        command: &SemanticCommand,
        localizer: &impl Localizer,
    ) -> Result<JsonObservation, RuntimeError> {
        let events = match command {
            SemanticCommand::Look | SemanticCommand::Inventory => Vec::new(),
            SemanticCommand::Help => vec![message(localizer.text("event.help"))],
            SemanticCommand::Move { direction } => self.move_player(player_id, *direction)?,
            SemanticCommand::Inspect { target } => {
                self.ensure_visible(player_id, target)?;
                vec![message(localizer.text("event.inspect"))]
            }
            SemanticCommand::Take { target } => self.take_entity(player_id, target, localizer)?,
            SemanticCommand::Talk { target } => {
                self.ensure_visible(player_id, target)?;
                vec![message(localizer.text("event.talk"))]
            }
            SemanticCommand::Quit => vec![message(localizer.text("event.quit"))],
        };

        self.observe_json(player_id, localizer, events)
    }

    /// Builds a structured observation for the player.
    pub fn observe_json(
        &self,
        player_id: &str,
        localizer: &impl Localizer,
        events: Vec<ObservationEvent>,
    ) -> Result<JsonObservation, RuntimeError> {
        let player = self.player(player_id)?;
        let current_view = player
            .lock()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .current_view
            .clone();
        let view = self
            .world
            .views
            .get(&current_view)
            .ok_or_else(|| RuntimeError::ViewNotFound(current_view.clone()))?;

        let exits = view
            .exits
            .iter()
            .map(|exit| ExitObservation {
                direction: exit.direction,
                target_known: true,
                label: exit.label_key.as_deref().map(|key| localizer.text(key)),
            })
            .collect();

        let visible_entities = self.visible_entities(&current_view)?;
        let entities = visible_entities
            .iter()
            .map(|entity_id| entity_observation(&self.world, entity_id, localizer))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(JsonObservation {
            player_id: player_id.to_owned(),
            view_id: view.id.clone(),
            title: localizer.text(&view.title_key),
            description: localizer.text(&view.description_key),
            exits,
            entities,
            available_commands: available_commands(&self.world, view, &visible_entities)?,
            events,
        })
    }

    /// Builds a human-oriented observation for CLI text mode.
    pub fn observe_text(
        &self,
        player_id: &str,
        localizer: &impl Localizer,
        events: Vec<String>,
    ) -> Result<TextObservation, RuntimeError> {
        let json = self.observe_json(
            player_id,
            localizer,
            events
                .into_iter()
                .map(|text| ObservationEvent::Message { text })
                .collect(),
        )?;

        Ok(TextObservation {
            title: json.title,
            description: json.description,
            exits: json
                .exits
                .into_iter()
                .map(|exit| exit.direction.as_str().to_owned())
                .collect(),
            entities: json
                .entities
                .into_iter()
                .map(|entity| entity.name)
                .collect(),
            events: json
                .events
                .into_iter()
                .map(|event| match event {
                    ObservationEvent::Message { text } => text,
                    ObservationEvent::Move { direction, .. } => {
                        format!("{} {}", localizer.text("event.move"), direction.as_str())
                    }
                })
                .collect(),
        })
    }

    fn move_player(
        &self,
        player_id: &str,
        direction: agentopia_core::Direction,
    ) -> Result<Vec<ObservationEvent>, RuntimeError> {
        let player = self.player(player_id)?;
        let mut player = player.lock().map_err(|_| RuntimeError::StatePoisoned)?;
        let current_view = player.current_view.clone();
        let view = self
            .world
            .views
            .get(&current_view)
            .ok_or_else(|| RuntimeError::ViewNotFound(current_view.clone()))?;
        let exit = view
            .exits
            .iter()
            .find(|exit| exit.direction == direction)
            .ok_or_else(|| RuntimeError::ExitNotFound(direction.as_str().to_owned()))?;
        let target = exit.target.clone();
        player.current_view = target.clone();

        Ok(vec![ObservationEvent::Move {
            from: current_view,
            to: target,
            direction,
        }])
    }

    fn take_entity(
        &self,
        player_id: &str,
        target: &EntityRef,
        localizer: &impl Localizer,
    ) -> Result<Vec<ObservationEvent>, RuntimeError> {
        let entity = self
            .world
            .entities
            .get(&target.id)
            .ok_or_else(|| RuntimeError::EntityNotFound(target.id.clone()))?;
        if !entity.portable {
            return Err(RuntimeError::EntityNotPortable(target.id.clone()));
        }

        let player = self.player(player_id)?;
        let mut player = player.lock().map_err(|_| RuntimeError::StatePoisoned)?;
        let current_view = player.current_view.clone();
        let view_state = self.view_state(&current_view)?;
        let mut view_state = view_state.lock().map_err(|_| RuntimeError::StatePoisoned)?;
        if !view_state.dropped_entities.contains(&target.id) {
            return Err(RuntimeError::EntityNotVisible(target.id.clone()));
        }
        view_state
            .dropped_entities
            .retain(|entity_id| entity_id != &target.id);
        if !player.inventory.contains(&target.id) {
            player.inventory.push(target.id.clone());
        }

        Ok(vec![message(localizer.text("event.take"))])
    }

    fn ensure_visible(&self, player_id: &str, target: &EntityRef) -> Result<(), RuntimeError> {
        let player = self.player(player_id)?;
        let current_view = player
            .lock()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .current_view
            .clone();
        if self.visible_entities(&current_view)?.contains(&target.id) {
            Ok(())
        } else {
            Err(RuntimeError::EntityNotVisible(target.id.clone()))
        }
    }

    fn visible_entities(&self, view_id: &str) -> Result<Vec<EntityId>, RuntimeError> {
        Ok(self
            .view_state(view_id)?
            .lock()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .dropped_entities
            .clone())
    }

    fn view_state(&self, view_id: &str) -> Result<Arc<Mutex<ViewState>>, RuntimeError> {
        self.views
            .get(view_id)
            .cloned()
            .ok_or_else(|| RuntimeError::ViewNotFound(view_id.to_owned()))
    }

    fn player(&self, player_id: &str) -> Result<Arc<Mutex<PlayerState>>, RuntimeError> {
        self.players
            .read()
            .map_err(|_| RuntimeError::StatePoisoned)?
            .get(player_id)
            .cloned()
            .ok_or_else(|| RuntimeError::PlayerNotFound(player_id.to_owned()))
    }
}

fn available_commands(
    world: &StaticWorld,
    view: &View,
    visible_entities: &[EntityId],
) -> Result<Vec<SemanticCommand>, RuntimeError> {
    let mut commands = vec![
        SemanticCommand::Look,
        SemanticCommand::Inventory,
        SemanticCommand::Help,
    ];

    commands.extend(view.exits.iter().map(|exit| SemanticCommand::Move {
        direction: exit.direction,
    }));

    for entity_id in visible_entities {
        let entity = world
            .entities
            .get(entity_id)
            .ok_or_else(|| RuntimeError::EntityNotFound(entity_id.clone()))?;
        commands.extend(entity.actions.iter().map(|action| match action {
            ActionKind::Inspect => SemanticCommand::Inspect {
                target: EntityRef::new(entity_id.clone()),
            },
            ActionKind::Take => SemanticCommand::Take {
                target: EntityRef::new(entity_id.clone()),
            },
            ActionKind::Talk => SemanticCommand::Talk {
                target: EntityRef::new(entity_id.clone()),
            },
        }));
    }

    Ok(commands)
}

fn entity_observation(
    world: &StaticWorld,
    entity_id: &str,
    localizer: &impl Localizer,
) -> Result<EntityObservation, RuntimeError> {
    let entity = world
        .entities
        .get(entity_id)
        .ok_or_else(|| RuntimeError::EntityNotFound(entity_id.to_owned()))?;
    Ok(EntityObservation {
        id: entity.id.clone(),
        kind: entity.kind,
        name: localizer.text(&entity.name_key),
        description: localizer.text(&entity.description_key),
        actions: entity.actions.clone(),
    })
}

fn message(text: String) -> ObservationEvent {
    ObservationEvent::Message { text }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use agentopia_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
    use agentopia_core::{Direction, EntityRef};

    use super::*;

    struct TestLocalizer;

    impl Localizer for TestLocalizer {
        fn text(&self, key: &str) -> String {
            key.to_owned()
        }
    }

    fn sample_runtime() -> GameRuntime {
        let world_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worlds/sample");
        GameRuntime::new(load_world_from_dir(world_dir).expect("sample world should load"))
    }

    #[test]
    fn moving_updates_current_view() {
        let runtime = sample_runtime();
        let command = SemanticCommand::Move {
            direction: Direction::North,
        };

        let observation = runtime
            .execute(LOCAL_PLAYER_ID, &command, &TestLocalizer)
            .expect("move should succeed");

        assert_eq!(observation.view_id, "abandoned_shrine");
    }

    #[test]
    fn taking_portable_entity_moves_it_to_inventory() {
        let runtime = sample_runtime();
        runtime
            .execute(
                LOCAL_PLAYER_ID,
                &SemanticCommand::Move {
                    direction: Direction::North,
                },
                &TestLocalizer,
            )
            .expect("move should succeed");

        runtime
            .execute(
                LOCAL_PLAYER_ID,
                &SemanticCommand::Take {
                    target: EntityRef::new("rusted_sword"),
                },
                &TestLocalizer,
            )
            .expect("take should succeed");

        let player = runtime
            .player_state(LOCAL_PLAYER_ID)
            .expect("player should exist");
        assert!(player.inventory.contains(&"rusted_sword".to_owned()));

        let observation = runtime
            .observe_json(LOCAL_PLAYER_ID, &TestLocalizer, Vec::new())
            .expect("observation should succeed");
        assert!(
            !observation
                .entities
                .iter()
                .any(|entity| entity.id == "rusted_sword")
        );
    }
}
