use super::*;
use hinemos_core::{Direction, JsonObservation, ObservationEvent};

const CLOSED_ROOM_ESCAPE_MESSAGE: &str = "This room is closed. You step back to the street.";

impl ConnectionHandler {
    pub(super) async fn observe_current_json_for_view(
        &self,
        player_id: &str,
        room_context: Option<&RoomViewContext>,
    ) -> Result<JsonObservation> {
        match self.shared.runtime.observe_json(player_id).await {
            Ok(observation) => Ok(observation),
            Err(_) => {
                let context = room_context
                    .expect("room context should be loaded before observing a player view");
                self.recover_current_room_observation(player_id, context)
                    .await
            }
        }
    }

    pub(super) async fn relocate_player_id_to_view(
        &self,
        player_id: &str,
        target_view_id: &str,
        direction: Option<Direction>,
        message: Option<&str>,
    ) -> Result<JsonObservation> {
        let room_context = self.shared.room_context_for_view(target_view_id).await?;
        self.relocate_player_id_to_view_with_context(
            player_id,
            target_view_id,
            direction,
            message,
            &room_context,
        )
        .await
    }

    pub(super) async fn relocate_player_id_to_view_with_context(
        &self,
        player_id: &str,
        target_view_id: &str,
        direction: Option<Direction>,
        message: Option<&str>,
        room_context: &RoomViewContext,
    ) -> Result<JsonObservation> {
        let mut player = self.shared.runtime.player_state(player_id).await?;
        let from = player.current_view.clone();
        player.current_view = target_view_id.to_owned();
        self.shared.runtime.set_player_state(player.clone()).await?;
        let app = self.shared.app_service().await;
        app.save_player_state(&player).await?;
        self.shared
            .presence
            .lock()
            .await
            .update_view(self.connection_id, player.current_view.clone());
        let mut observation = self
            .observe_player_at_view_with_context(&player.id, room_context)
            .await?;
        if let Some(direction) = direction {
            observation.events.push(ObservationEvent::Move {
                from,
                to: target_view_id.to_owned(),
                direction,
            });
        }
        if let Some(text) = message {
            observation.events.push(ObservationEvent::Message {
                text: text.to_owned(),
            });
        }
        Ok(observation)
    }

    async fn observe_player_at_view_with_context(
        &self,
        player_id: &str,
        room_context: &RoomViewContext,
    ) -> Result<JsonObservation> {
        match self.shared.runtime.observe_json(player_id).await {
            Ok(observation) => Ok(observation),
            Err(error) => {
                if let Some(observation) =
                    self.service_room_observation(player_id, room_context).await
                {
                    Ok(observation)
                } else {
                    Err(error.into())
                }
            }
        }
    }

    async fn recover_current_room_observation(
        &self,
        player_id: &str,
        room_context: &RoomViewContext,
    ) -> Result<JsonObservation> {
        if let Some(observation) = self
            .active_service_room_observation(player_id, room_context)
            .await
        {
            return Ok(observation);
        }
        let target = self.closed_room_escape_target(room_context).await;
        self.relocate_player_id_to_view(
            player_id,
            &target,
            Some(Direction::South),
            Some(CLOSED_ROOM_ESCAPE_MESSAGE),
        )
        .await
    }

    async fn active_service_room_observation(
        &self,
        player_id: &str,
        room_context: &RoomViewContext,
    ) -> Option<JsonObservation> {
        let RoomViewContext {
            room_binding: Some(room),
            ..
        } = room_context
        else {
            return None;
        };
        if !room.is_service_room() {
            return None;
        }
        let app = self.shared.app_service().await;
        Some(app.service_room_observation_for(player_id, room))
    }

    async fn service_room_observation(
        &self,
        player_id: &str,
        room_context: &RoomViewContext,
    ) -> Option<JsonObservation> {
        if let Some(observation) = self
            .active_service_room_observation(player_id, room_context)
            .await
        {
            return Some(observation);
        }
        let RoomViewContext {
            service_room: Some(service_room),
            ..
        } = room_context
        else {
            return None;
        };
        let app = self.shared.app_service().await;
        Some(app.service_room_observation_for(player_id, service_room))
    }

    async fn closed_room_escape_target(&self, room_context: &RoomViewContext) -> String {
        let app_config = self.shared.app_config().await;
        if let RoomViewContext {
            room_binding: None,
            service_room: Some(service_room),
            ..
        } = room_context
            && let Some(front_view_id) = &service_room.front_view_id
        {
            return front_view_id.clone();
        }
        app_config.admission_view_id
    }
}
