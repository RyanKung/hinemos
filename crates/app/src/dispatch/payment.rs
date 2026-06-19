use crate::*;

use super::events::text_events;
use super::route::PaymentAppRequest;

impl<S, E> AppService<S>
where
    S: PaymentStore<Error = E>,
{
    pub(super) async fn handle_payment_request(
        &self,
        identity: &AppIdentity,
        request: PaymentAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            PaymentAppRequest::PendingRequests => Ok(text_events(
                self.pending_pay_requests(&identity.player_id).await?.text,
                None,
            )),
            PaymentAppRequest::Direct {
                target,
                amount,
                memo,
            } => self.handle_pay_direct(identity, target, amount, memo).await,
            PaymentAppRequest::Accept { request_id } => {
                self.handle_pay_accept(identity, request_id).await
            }
        }
    }

    async fn handle_pay_direct(
        &self,
        identity: &AppIdentity,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<Vec<UiEvent>, E> {
        let result = self
            .pay_direct(&identity.user, &identity.player_id, target, amount, memo)
            .await?;
        let memo_text = if result.transfer.memo().is_empty() {
            String::new()
        } else {
            format!(" memo={}", result.transfer.memo())
        };
        Ok(vec![
            UiEvent::Text(result.text),
            UiEvent::LiveMessage {
                target_player_id: target.to_owned(),
                text: format!(
                    "[payment from {}] {} {}{}",
                    identity.user,
                    result.transfer.amount(),
                    result.transfer.asset(),
                    memo_text
                ),
            },
        ])
    }

    async fn handle_pay_accept(
        &self,
        identity: &AppIdentity,
        request_id: i64,
    ) -> Result<Vec<UiEvent>, E> {
        let result = self
            .accept_pay_request(&identity.user, &identity.player_id, request_id)
            .await?;
        Ok(vec![
            UiEvent::Text(result.text),
            UiEvent::LiveMessage {
                target_player_id: result.request.payee_player_id().to_owned(),
                text: format!(
                    "[payment request #{} paid by {}] {} {}",
                    result.request.id(),
                    identity.user,
                    result.request.amount(),
                    result.request.asset()
                ),
            },
        ])
    }
}
