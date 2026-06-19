use crate::*;

use super::events::text_events;
use super::route::ServiceRoomAppRequest;

impl<S, E> AppService<S>
where
    S: MailStore<Error = E> + RoomStore<Error = E>,
    <S as RoomStore>::RoomBinding: RoomBindingKindView + RoomMailboxView + ServiceRoomView + Sync,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
{
    pub(super) async fn handle_service_room_request(
        &self,
        identity: &AppIdentity,
        request: ServiceRoomAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            ServiceRoomAppRequest::Input {
                room_view,
                raw_input,
            } => {
                self.handle_service_room_input(identity, room_view, raw_input)
                    .await
            }
            ServiceRoomAppRequest::Help { room_view } => {
                let Some(room) = self.service_room_binding_by_view(room_view).await? else {
                    return Ok(text_events(
                        service_room_unavailable_text().to_owned(),
                        None,
                    ));
                };
                Ok(text_events(self.service_room_help_text(&room), None))
            }
            ServiceRoomAppRequest::Observation { room_view } => {
                let Some(room) = self.service_room_binding_by_view(room_view).await? else {
                    return Ok(text_events(
                        service_room_unavailable_text().to_owned(),
                        None,
                    ));
                };
                Ok(vec![UiEvent::Observation(
                    self.service_room_observation_for(&identity.player_id, &room),
                )])
            }
            ServiceRoomAppRequest::BlockedExit => Ok(text_events(
                service_room_blocked_exit_text().to_owned(),
                None,
            )),
            ServiceRoomAppRequest::Unavailable => Ok(text_events(
                service_room_unavailable_text().to_owned(),
                None,
            )),
            ServiceRoomAppRequest::Quit { feedback } => Ok(vec![
                UiEvent::Text(format!("{feedback}\r\n")),
                UiEvent::CloseSession(0),
            ]),
        }
    }

    async fn handle_service_room_input(
        &self,
        identity: &AppIdentity,
        room_view: &str,
        raw_input: &str,
    ) -> Result<Vec<UiEvent>, E> {
        let Some(binding) = self.store.room_binding_by_view(room_view).await? else {
            return Ok(text_events(
                service_room_unavailable_text().to_owned(),
                None,
            ));
        };
        if !RoomBindingKindView::is_service_room(&binding) {
            return Ok(text_events(
                service_room_unavailable_text().to_owned(),
                None,
            ));
        }
        let result = self
            .forward_room_mailbox_input(&binding, &identity.user, &identity.player_id, raw_input)
            .await?;
        Ok(text_events(result.text, None))
    }
}
