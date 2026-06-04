#![deny(missing_docs)]

//! Runtime command execution for the Hinemos world.

mod client_shell;
mod reload;

pub use client_shell::{Chrome, SlashParseError, render_text_events, render_text_observation};

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use hinemos_core::{
    ActionKind, Entity, EntityCollection, EntityId, EntityObservation, EntityRef, ExitObservation,
    JsonObservation, ObservationEvent, PlayerId, PlayerState, SemanticCommand, TextObservation,
    View, ViewId, WorldDefinition, WorldState,
};
use thiserror::Error;

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

/// Errors merging a freshly loaded world with an existing [`GameRuntime`] snapshot.
#[derive(Debug, Error)]
pub enum ReloadError {
    /// World files could not be loaded.
    #[error(transparent)]
    World(#[from] hinemos_core::sample_world::WorldLoadError),
    /// Snapshot of the old runtime failed (for example lock poison).
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
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
    entities: HashMap<EntityId, hinemos_core::Entity>,
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
        let definition: WorldDefinition = world.definition();
        let snapshot = world.runtime_snapshot();
        let views = definition
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
        let players = snapshot
            .players
            .iter()
            .map(|(player_id, player)| (player_id.clone(), Arc::new(Mutex::new(player.clone()))))
            .collect();
        let world = StaticWorld {
            views: definition.views,
            entities: definition.entities,
            template_players: snapshot.players,
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

    /// Executes a command and returns a structured observation including feed-forward events.
    pub fn execute(
        &self,
        player_id: &str,
        command: &SemanticCommand,
    ) -> Result<JsonObservation, RuntimeError> {
        let events = match command {
            SemanticCommand::Look | SemanticCommand::Map | SemanticCommand::Inventory => Vec::new(),
            SemanticCommand::Help => vec![message(Chrome::HELP_SUMMARY.to_owned())],
            SemanticCommand::Move { direction } => self.move_player(player_id, *direction)?,
            SemanticCommand::Enter { .. } => {
                vec![message("Enter is available in SSH sessions.".to_owned())]
            }
            SemanticCommand::Inspect { target } => {
                let entity = self.visible_entity(player_id, target)?;
                vec![message(inspect_entity_message(entity))]
            }
            SemanticCommand::Read { target } => {
                let entity = self.visible_entity(player_id, target)?;
                vec![message(read_entity_message(entity))]
            }
            SemanticCommand::Agree { .. } => {
                vec![message(
                    "Admission agreements are handled in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Take { target } => self.take_entity(player_id, target)?,
            SemanticCommand::Talk { target } => {
                let entity = self.visible_entity(player_id, target)?;
                vec![message(talk_entity_message(entity))]
            }
            SemanticCommand::Say { text } => vec![message(format!("You say: {text}"))],
            SemanticCommand::Mail { target, text } => {
                vec![message(format!("You mail {target}: {text}"))]
            }
            SemanticCommand::Settings { .. } => {
                vec![message(
                    "Settings are available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Inbox { .. } => {
                vec![message("Inbox is available in SSH sessions.".to_owned())]
            }
            SemanticCommand::Broadcast { text } => {
                vec![message(format!("You broadcast: {text}"))]
            }
            SemanticCommand::Mailbox => {
                vec![message("Mailbox is available in SSH sessions.".to_owned())]
            }
            SemanticCommand::History => {
                vec![message(
                    "Room history is available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Who => {
                vec![message("Who is available in SSH sessions.".to_owned())]
            }
            SemanticCommand::News => {
                vec![message("News is available in SSH sessions.".to_owned())]
            }
            SemanticCommand::Balance => {
                vec![message("Balance is available in SSH sessions.".to_owned())]
            }
            SemanticCommand::Pay { .. } => {
                vec![message(
                    "Payments are available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Land { .. } => {
                vec![message(
                    "Land tools are available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Build { .. } => {
                vec![message(
                    "Build tools are available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Shop { .. } => {
                vec![message(
                    "Shop tools are available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Extension { .. } => {
                vec![message(
                    "Extension commands are available in SSH sessions.".to_owned(),
                )]
            }
            SemanticCommand::Quit => vec![message(Chrome::FEEDBACK_QUIT.to_owned())],
        };

        self.observe_json(player_id, events)
    }

    /// Builds a structured observation for the player.
    pub fn observe_json(
        &self,
        player_id: &str,
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
                label: exit.label.clone(),
            })
            .collect();

        let visible_entities = self.visible_entities(&current_view)?;
        let entities = visible_entities
            .iter()
            .map(|entity_id| entity_observation(&self.world, entity_id))
            .collect::<Result<Vec<_>, _>>()?;

        let ascii_art = render_ascii_art_for_view(view);

        Ok(JsonObservation {
            player_id: player_id.to_owned(),
            view_id: view.id.clone(),
            title: view.title.clone(),
            ascii_art,
            description: view.description.clone(),
            exits,
            entities,
            online_users: Vec::new(),
            available_commands: available_commands(&self.world, view, &visible_entities)?,
            events,
        })
    }

    /// Builds a human-oriented observation for CLI text mode.
    pub fn observe_text(
        &self,
        player_id: &str,
        events: Vec<String>,
    ) -> Result<TextObservation, RuntimeError> {
        let json = self.observe_json(
            player_id,
            events
                .into_iter()
                .map(|text| ObservationEvent::Message { text })
                .collect(),
        )?;

        Ok(TextObservation {
            title: json.title,
            ascii_art: json.ascii_art.clone(),
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
            online_users: json.online_users,
            events: json
                .events
                .into_iter()
                .map(|event| match event {
                    ObservationEvent::Message { text } => text,
                    ObservationEvent::Move { direction, .. } => {
                        format!("{} {}", Chrome::MOVE_VERB, direction.as_str())
                    }
                })
                .collect(),
        })
    }

    fn move_player(
        &self,
        player_id: &str,
        direction: hinemos_core::Direction,
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

        Ok(vec![message(Chrome::FEEDBACK_TAKE.to_owned())])
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

    fn visible_entity(&self, player_id: &str, target: &EntityRef) -> Result<&Entity, RuntimeError> {
        self.ensure_visible(player_id, target)?;
        self.world
            .entities
            .get(&target.id)
            .ok_or_else(|| RuntimeError::EntityNotFound(target.id.clone()))
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
        SemanticCommand::Map,
        SemanticCommand::Inventory,
        SemanticCommand::Help,
        SemanticCommand::Say {
            text: "<text>".to_owned(),
        },
        SemanticCommand::History,
        SemanticCommand::Who,
        SemanticCommand::Settings {
            action: hinemos_core::SettingsAction::Show,
        },
        SemanticCommand::Inbox {
            action: hinemos_core::InboxAction::List {
                filter: "unread".to_owned(),
            },
        },
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
            ActionKind::Read => SemanticCommand::Read {
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
) -> Result<EntityObservation, RuntimeError> {
    let entity = world
        .entities
        .get(entity_id)
        .ok_or_else(|| RuntimeError::EntityNotFound(entity_id.to_owned()))?;
    Ok(EntityObservation {
        id: entity.id.clone(),
        kind: entity.kind,
        name: entity.name.clone(),
        description: entity.description.clone(),
        actions: entity.actions.clone(),
    })
}

fn inspect_entity_message(entity: &Entity) -> String {
    let mut lines = vec![format!("{}: {}", entity.name, entity.description)];
    if let Some(collection) = &entity.collection {
        match collection {
            EntityCollection::BulletinBoard { items } => {
                let titles = items
                    .iter()
                    .map(|item| item.title.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !titles.is_empty() {
                    lines.push(format!("Readable notices: {titles}."));
                }
            }
            EntityCollection::Dialogue { lines: dialogue } => {
                let speakers = dialogue
                    .iter()
                    .map(|line| line.speaker.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !speakers.is_empty() {
                    lines.push(format!("Ready to talk: {speakers}."));
                }
            }
        }
    }
    if !entity.actions.is_empty() {
        let actions = entity
            .actions
            .iter()
            .map(action_label)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("Actions: {actions}."));
    }
    lines.join("\n")
}

fn read_entity_message(entity: &Entity) -> String {
    match &entity.collection {
        Some(EntityCollection::BulletinBoard { items }) if !items.is_empty() => {
            let mut lines = vec![format!("{}:", entity.name)];
            for item in items {
                lines.push(format!("- {}: {}", item.title, item.body));
            }
            lines.join("\n")
        }
        _ => format!("{} has no readable notices.", entity.name),
    }
}

fn talk_entity_message(entity: &Entity) -> String {
    match &entity.collection {
        Some(EntityCollection::Dialogue { lines }) if !lines.is_empty() => lines
            .iter()
            .map(|line| format!("{}: {}", line.speaker, line.body))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => format!("{} has nothing to say right now.", entity.name),
    }
}

fn action_label(action: &ActionKind) -> &'static str {
    match action {
        ActionKind::Inspect => "inspect",
        ActionKind::Read => "read",
        ActionKind::Take => "take",
        ActionKind::Talk => "talk",
    }
}

fn message(text: String) -> ObservationEvent {
    ObservationEvent::Message { text }
}

fn render_ascii_art_for_view(view: &View) -> Vec<String> {
    if view.ascii_art.is_empty() {
        return Vec::new();
    }

    view.ascii_art.clone()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
    use hinemos_core::{Direction, EntityRef, ObservationEvent, SemanticCommand};

    use super::*;

    fn sample_runtime() -> GameRuntime {
        let world_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worlds/sample");
        GameRuntime::new(load_world_from_dir(world_dir).expect("sample world should load"))
    }

    #[test]
    fn moving_updates_current_view() {
        let runtime = sample_runtime();
        let observation = runtime
            .execute(
                LOCAL_PLAYER_ID,
                &SemanticCommand::Move {
                    direction: Direction::West,
                },
            )
            .expect("move should succeed");

        assert_eq!(observation.view_id, "west_main_street");
    }

    #[test]
    fn reading_bulletin_board_renders_notices() {
        let runtime = sample_runtime();
        let observation = runtime
            .execute(
                LOCAL_PLAYER_ID,
                &SemanticCommand::Read {
                    target: EntityRef::new("cyber_scroll_board"),
                },
            )
            .expect("read should succeed");

        let text = observation
            .events
            .iter()
            .filter_map(|event| match event {
                ObservationEvent::Message { text } => Some(text.as_str()),
                ObservationEvent::Move { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Arrival Skill"));
        assert!(text.contains("go west to Blackstone Izakaya"));
    }
}
