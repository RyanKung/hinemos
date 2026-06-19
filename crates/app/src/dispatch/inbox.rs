use crate::*;

use super::events::text_events;
use super::route::{InboxAppRequest, InboxMutation};

impl<S, E> AppService<S>
where
    S: InboxStore<Error = E> + ShopStore<Error = E>,
{
    pub(super) async fn handle_inbox_request(
        &self,
        identity: &AppIdentity,
        request: InboxAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            InboxAppRequest::List {
                title,
                filter,
                mail_domain,
            } => Ok(text_events(
                self.list_inbox(
                    title,
                    &identity.user,
                    &identity.player_id,
                    filter,
                    mail_domain,
                )
                .await?
                .text,
                None,
            )),
            InboxAppRequest::Read {
                item_id,
                mail_domain,
            } => Ok(text_events(
                self.read_inbox(&identity.user, &identity.player_id, item_id, mail_domain)
                    .await?
                    .text,
                None,
            )),
            InboxAppRequest::Mutate { item_id, mutation } => {
                self.handle_inbox_mutation(identity, item_id, mutation)
                    .await
            }
        }
    }

    async fn handle_inbox_mutation(
        &self,
        identity: &AppIdentity,
        item_id: i64,
        mutation: InboxMutation,
    ) -> Result<Vec<UiEvent>, E> {
        let result = match mutation {
            InboxMutation::Claim => {
                self.claim_inbox(&identity.user, &identity.player_id, item_id)
                    .await?
            }
            InboxMutation::Ack => {
                self.ack_inbox(&identity.user, &identity.player_id, item_id)
                    .await?
            }
            InboxMutation::Archive => {
                self.archive_inbox(&identity.user, &identity.player_id, item_id)
                    .await?
            }
        };
        Ok(vec![
            UiEvent::Text(result.text),
            UiEvent::InvalidateInboxItem { item_id },
        ])
    }
}
