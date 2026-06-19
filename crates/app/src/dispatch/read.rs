use crate::*;

use super::events::text_events;
use super::route::ReadAppRequest;

impl<S, E> AppService<S>
where
    S: MemoryStore<Error = E> + MessageStore<Error = E>,
{
    pub(super) async fn handle_read_request(
        &self,
        identity: &AppIdentity,
        request: ReadAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let text = match request {
            ReadAppRequest::MemoryContext => self.memory_context(&identity.player_id).await?.text,
            ReadAppRequest::MemoryCommand { rest } => {
                self.memory_command(&identity.player_id, rest).await?.text
            }
            ReadAppRequest::RoomHistory {
                current_view,
                title,
            } => self.room_history(current_view, title).await?.text,
            ReadAppRequest::Inventory { items } => render_inventory(items),
            ReadAppRequest::Who {
                current_view,
                users,
            } => render_who(current_view, users),
            ReadAppRequest::News => self.world_news().await?.text,
            ReadAppRequest::Balance => {
                render_player_balance(self.store.player_balance(&identity.player_id).await?)
            }
        };
        Ok(text_events(text, None))
    }
}
