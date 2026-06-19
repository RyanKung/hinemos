use crate::*;

use super::events::{commercial_parcel_cache_event, text_events};
use super::route::{BuildAppRequest, LandAppRequest, ShopAppRequest};

impl<S, E> AppService<S>
where
    S: LandStore<Error = E> + ParcelStore<Error = E>,
{
    pub(super) async fn handle_land_request(
        &self,
        identity: &AppIdentity,
        request: LandAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let (text, cache) = match request {
            LandAppRequest::List => (self.land_list().await?.text, None),
            LandAppRequest::Info { parcel_id } => (self.land_info(parcel_id).await?.text, None),
            LandAppRequest::Claim { parcel_id, token } => {
                let result = self
                    .claim_land(parcel_id, &identity.user, &identity.player_id, token)
                    .await?;
                (result.text, result.invalidate)
            }
            LandAppRequest::Transfer {
                parcel_id,
                target,
                token,
            } => {
                let result = self
                    .transfer_land(parcel_id, &identity.player_id, target, token)
                    .await?;
                (result.text, result.invalidate)
            }
            LandAppRequest::RotateToken { parcel_id, token } => {
                let result = self
                    .rotate_land_token(parcel_id, &identity.player_id, token)
                    .await?;
                (result.text, None)
            }
        };
        Ok(text_events(text, cache.map(commercial_parcel_cache_event)))
    }
}

impl<S, E> AppService<S>
where
    S: BuildStore<Error = E> + ShopStore<Error = E>,
{
    pub(super) async fn handle_build_request(
        &self,
        identity: &AppIdentity,
        request: BuildAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let result = match request {
            BuildAppRequest::Help => {
                return Ok(text_events(self.build_help_text().to_owned(), None));
            }
            BuildAppRequest::Apply {
                current_view,
                sheet,
            } => {
                self.apply_build_sheet(current_view, &identity.player_id, sheet)
                    .await?
            }
            BuildAppRequest::Set {
                current_view,
                field,
                value,
            } => {
                self.set_build_field(current_view, &identity.player_id, field, value)
                    .await?
            }
            BuildAppRequest::Publish { current_view } => {
                self.publish_build(current_view, &identity.player_id)
                    .await?
            }
        };
        Ok(text_events(
            result.text,
            result.invalidate.map(commercial_parcel_cache_event),
        ))
    }
}

impl<S, E> AppService<S>
where
    S: ShopStore<Error = E>,
{
    pub(super) async fn handle_shop_request(
        &self,
        identity: &AppIdentity,
        request: ShopAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            ShopAppRequest::Inbox => Ok(text_events(
                self.shop_inbox(&identity.player_id).await?.text,
                None,
            )),
            ShopAppRequest::RequestPayment {
                command_id,
                amount,
                delivery,
            } => {
                let result = self
                    .request_shop_payment(command_id, &identity.player_id, amount, delivery)
                    .await?;
                Ok(vec![
                    UiEvent::Text(result.text),
                    UiEvent::LiveInboxNotice {
                        target_player_id: result.payer_player_id,
                        notice: LiveInboxNotice::from_item(&result.inbox_item),
                    },
                ])
            }
        }
    }
}
