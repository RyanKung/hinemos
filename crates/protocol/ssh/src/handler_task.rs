use super::*;
use hinemos_app::{AppIdentity, UiEvent};
use hinemos_core::{
    JsonObservation, ObservationEvent, SemanticCommand, extension_command_input_matches_template,
};

impl ConnectionHandler {
    pub(super) fn parse_command_line_for_task(
        &self,
        current_observation: &JsonObservation,
        line: &str,
    ) -> Option<SemanticCommand> {
        let chrome = self.chrome.as_ref()?;
        runtime_state::parse_command(chrome, Some(current_observation), line)
            .ok()
            .or_else(|| visible_extension_command(current_observation, line))
    }

    pub(super) async fn record_resident_task_step_after_current_view(
        &self,
        identity: &AppIdentity,
        before_observation: &JsonObservation,
        command: &SemanticCommand,
        events: Vec<ObservationEvent>,
    ) {
        match self
            .current_task_observation_with_events(&identity.player_id, events)
            .await
        {
            Ok(after_observation) => {
                self.record_resident_task_step_best_effort(
                    identity,
                    before_observation,
                    command,
                    &after_observation,
                )
                .await;
            }
            Err(error) => {
                eprintln!(
                    "resident task observation refresh failed for {}: {error:#}",
                    identity.player_id
                );
            }
        }
    }

    pub(super) async fn record_resident_task_step_after_observation(
        &self,
        identity: &AppIdentity,
        before_observation: &JsonObservation,
        command: &SemanticCommand,
        after_observation: JsonObservation,
    ) {
        match self.enrich_task_observation(after_observation).await {
            Ok(after_observation) => {
                self.record_resident_task_step_best_effort(
                    identity,
                    before_observation,
                    command,
                    &after_observation,
                )
                .await;
            }
            Err(error) => {
                eprintln!(
                    "resident task observation enrichment failed for {}: {error:#}",
                    identity.player_id
                );
            }
        }
    }

    async fn current_task_observation_with_events(
        &self,
        player_id: &str,
        events: Vec<ObservationEvent>,
    ) -> Result<JsonObservation> {
        let player = self.shared.runtime.player_state(player_id).await?;
        let room_context = self
            .shared
            .room_context_for_view(&player.current_view)
            .await?;
        let mut observation = self
            .observe_current_json_for_view(player_id, Some(&room_context))
            .await?;
        observation.events.extend(events);
        self.enrich_observation_for_context(observation, &room_context)
            .await
    }

    async fn enrich_task_observation(
        &self,
        observation: JsonObservation,
    ) -> Result<JsonObservation> {
        let room_context = self
            .shared
            .room_context_for_view(&observation.view_id)
            .await?;
        self.enrich_observation_for_context(observation, &room_context)
            .await
    }

    async fn record_resident_task_step_best_effort(
        &self,
        identity: &AppIdentity,
        before_observation: &JsonObservation,
        command: &SemanticCommand,
        after_observation: &JsonObservation,
    ) {
        let app = self.shared.app_service().await;
        match app.player_admission(&identity.player_id).await {
            Ok(admission) if admission.is_agreed() => {}
            Ok(_) => return,
            Err(error) => {
                eprintln!(
                    "resident task admission check failed for {} on {:?}: {error:#}",
                    identity.player_id, command
                );
                return;
            }
        }
        if let Err(error) = app
            .record_resident_task_step(
                &identity.user,
                &identity.player_id,
                before_observation,
                command,
                after_observation,
            )
            .await
        {
            eprintln!(
                "resident task step recording failed for {} on {:?}: {error:#}",
                identity.player_id, command
            );
        }
    }
}

fn visible_extension_command(observation: &JsonObservation, line: &str) -> Option<SemanticCommand> {
    let input = line.trim();
    let _command_name = input.strip_prefix('/')?.split_whitespace().next()?;
    observation
        .available_commands
        .iter()
        .find_map(|command| match command {
            SemanticCommand::Extension {
                name,
                input: template,
                ..
            } if extension_command_input_matches_template(template, input) => {
                Some(SemanticCommand::Extension {
                    name: name.clone(),
                    input: input.to_owned(),
                })
            }
            _ => None,
        })
}

pub(super) fn observation_events_from_ui_events(events: &[UiEvent]) -> Vec<ObservationEvent> {
    let mut observations = Vec::new();
    for event in events {
        match event {
            UiEvent::Text(text) => push_text_observation_event(&mut observations, text),
            UiEvent::Observation(observation) | UiEvent::CommandObservation { observation, .. } => {
                observations.extend(observation.events.iter().cloned());
            }
            UiEvent::Relocate {
                message: Some(message),
                ..
            } => push_text_observation_event(&mut observations, message),
            _ => {}
        }
    }
    observations
}

fn push_text_observation_event(observations: &mut Vec<ObservationEvent>, text: &str) {
    let text = text.replace("\r\n", "\n").trim().to_owned();
    if !text.is_empty() {
        observations.push(ObservationEvent::Message { text });
    }
}
