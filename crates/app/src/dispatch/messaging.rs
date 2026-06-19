use crate::*;

use super::route::MessageAppRequest;

impl<S, E> AppService<S>
where
    S: MessageStore<Error = E>,
{
    pub(super) async fn handle_message_request(
        &self,
        identity: &AppIdentity,
        request: MessageAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            MessageAppRequest::Say { current_view, text } => {
                self.handle_say(identity, current_view, text).await
            }
            MessageAppRequest::Mail { target, text } => {
                self.handle_mail(identity, target, text).await
            }
            MessageAppRequest::Broadcast { text } => self.handle_broadcast(identity, text).await,
        }
    }
}
