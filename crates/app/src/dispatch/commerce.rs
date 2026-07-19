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
    S: ShopStore<Error = E> + LandStore<Error = E>,
    E: FromMailingListValidation + FromShopBadgeValidation + FromShopWorkValidation,
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
                current_view,
                command_id,
                amount,
                delivery,
            } => {
                let result = self
                    .request_shop_payment(
                        current_view,
                        command_id,
                        &identity.player_id,
                        amount,
                        delivery,
                    )
                    .await?;
                Ok(vec![
                    UiEvent::Text(result.text),
                    UiEvent::LiveInboxNotice {
                        target_player_id: result.payer_player_id,
                        notice: LiveInboxNotice::from_item(&result.inbox_item),
                    },
                ])
            }
            ShopAppRequest::MailingListCreate {
                current_view,
                parcel_id,
                slug,
                title,
            } => {
                let text = self
                    .create_shop_mailing_list(
                        current_view,
                        parcel_id,
                        &identity.player_id,
                        slug,
                        title,
                    )
                    .await?
                    .text;
                Ok(text_events(
                    text,
                    Some(self.commercial_parcel_invalidation(parcel_id).await?),
                ))
            }
            ShopAppRequest::MailingListList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_mailing_lists(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::MailingListSubscribers {
                current_view,
                parcel_id,
                slug,
            } => Ok(text_events(
                self.shop_mailing_list_subscribers(
                    current_view,
                    parcel_id,
                    slug,
                    &identity.player_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::MailingListSend {
                current_view,
                parcel_id,
                slug,
                subject,
                body,
            } => {
                let result = self
                    .send_shop_mailing_list_post(ShopMailingListPostInput {
                        current_view,
                        target: parcel_id,
                        slug,
                        sender_user: &identity.user,
                        sender_player_id: &identity.player_id,
                        subject,
                        body,
                    })
                    .await?;
                let mut events = vec![UiEvent::Text(format!(
                    "Sent shop chat post #{} to {} member(s): {}.\r\n",
                    result.post.id(),
                    result.post.recipient_count(),
                    result.post.subject()
                ))];
                events.extend(result.deliveries.into_iter().map(|delivery| {
                    UiEvent::LiveInboxNotice {
                        target_player_id: delivery.recipient_player_id,
                        notice: LiveInboxNotice::from_item(&delivery.inbox_item),
                    }
                }));
                Ok(events)
            }
            ShopAppRequest::MailingListChat {
                current_view,
                target,
                slug,
                body,
            } => {
                let result = self
                    .post_shop_mailing_list_chat(
                        current_view,
                        target,
                        slug,
                        &identity.user,
                        &identity.player_id,
                        body,
                    )
                    .await?;
                let mut events = vec![UiEvent::Text(format!(
                    "Posted shop chat message #{} to {} member(s): {}.\r\n",
                    result.post.id(),
                    result.post.recipient_count(),
                    result.post.subject()
                ))];
                events.extend(result.deliveries.into_iter().map(|delivery| {
                    UiEvent::LiveInboxNotice {
                        target_player_id: delivery.recipient_player_id,
                        notice: LiveInboxNotice::from_item(&delivery.inbox_item),
                    }
                }));
                Ok(events)
            }
            ShopAppRequest::MailingListClose {
                current_view,
                parcel_id,
                slug,
            } => {
                let text = self
                    .close_shop_mailing_list(current_view, parcel_id, slug, &identity.player_id)
                    .await?
                    .text;
                Ok(text_events(
                    text,
                    Some(self.commercial_parcel_invalidation(parcel_id).await?),
                ))
            }
            ShopAppRequest::MailingListSubscribe {
                current_view,
                target,
                slug,
            } => Ok(text_events(
                self.subscribe_shop_mailing_list(
                    current_view,
                    target,
                    slug,
                    &identity.user,
                    &identity.player_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::MailingListUnsubscribe {
                current_view,
                target,
                slug,
            } => Ok(text_events(
                self.unsubscribe_shop_mailing_list(
                    current_view,
                    target,
                    slug,
                    &identity.user,
                    &identity.player_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::MailingListSubscriptions => Ok(text_events(
                self.shop_mailing_list_subscriptions(&identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::DeskCreate {
                current_view,
                parcel_id,
                slug,
                title,
            } => {
                let text = self
                    .create_shop_work_desk(
                        current_view,
                        parcel_id,
                        &identity.player_id,
                        slug,
                        title,
                    )
                    .await?
                    .text;
                Ok(text_events(
                    text,
                    Some(self.commercial_parcel_invalidation(parcel_id).await?),
                ))
            }
            ShopAppRequest::DeskList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_work_desks(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::StaffAdd {
                current_view,
                parcel_id,
                slug,
                username,
            } => Ok(text_events(
                self.add_shop_staff(current_view, parcel_id, slug, &identity.player_id, username)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::StaffList {
                current_view,
                parcel_id,
                slug,
            } => Ok(text_events(
                self.list_shop_staff(current_view, parcel_id, slug, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::StaffRemove {
                current_view,
                parcel_id,
                slug,
                username,
            } => Ok(text_events(
                self.remove_shop_staff(
                    current_view,
                    parcel_id,
                    slug,
                    &identity.player_id,
                    username,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::ShiftStart {
                current_view,
                parcel_id,
                slug,
            } => Ok(text_events(
                self.start_shop_shift(
                    current_view,
                    parcel_id,
                    slug,
                    &identity.user,
                    &identity.player_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::ShiftEnd {
                current_view,
                parcel_id,
                slug,
            } => Ok(text_events(
                self.end_shop_shift(
                    current_view,
                    parcel_id,
                    slug,
                    &identity.user,
                    &identity.player_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::WorkList {
                current_view,
                parcel_id,
                slug,
            } => Ok(text_events(
                self.list_shop_work(
                    current_view,
                    parcel_id,
                    slug,
                    &identity.user,
                    &identity.player_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::WorkClaim {
                current_view,
                parcel_id,
                work_id,
            } => Ok(text_events(
                self.claim_shop_work(
                    current_view,
                    parcel_id,
                    &identity.user,
                    &identity.player_id,
                    work_id,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::WorkDone {
                current_view,
                parcel_id,
                work_id,
                result,
            } => Ok(text_events(
                self.finish_shop_work(
                    current_view,
                    parcel_id,
                    &identity.user,
                    &identity.player_id,
                    work_id,
                    result,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::RouteAdd {
                current_view,
                parcel_id,
                slug,
                command_prefix,
            } => {
                let text = self
                    .add_shop_command_route(
                        current_view,
                        parcel_id,
                        &identity.player_id,
                        slug,
                        command_prefix,
                    )
                    .await?
                    .text;
                Ok(text_events(
                    text,
                    Some(self.commercial_parcel_invalidation(parcel_id).await?),
                ))
            }
            ShopAppRequest::RouteList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_command_routes(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::RouteRemove {
                current_view,
                parcel_id,
                slug,
                command_prefix,
            } => {
                let text = self
                    .remove_shop_command_route(
                        current_view,
                        parcel_id,
                        &identity.player_id,
                        slug,
                        command_prefix,
                    )
                    .await?
                    .text;
                Ok(text_events(
                    text,
                    Some(self.commercial_parcel_invalidation(parcel_id).await?),
                ))
            }
            ShopAppRequest::BadgeList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_badges(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::BadgeCreate {
                current_view,
                parcel_id,
                slug,
                title,
                description,
            } => Ok(text_events(
                self.create_shop_badge(
                    current_view,
                    parcel_id,
                    &identity.player_id,
                    slug,
                    title,
                    description,
                )
                .await?
                .text,
                None,
            )),
            ShopAppRequest::BadgeAward {
                current_view,
                parcel_id,
                slug,
                target,
                note,
            } => Ok(text_events(
                self.award_shop_badge(ShopBadgeAwardInput {
                    current_view,
                    parcel_id,
                    slug,
                    issuer_user: &identity.user,
                    issuer_player_id: &identity.player_id,
                    target,
                    note,
                })
                .await?
                .text,
                None,
            )),
            ShopAppRequest::BadgeRevoke {
                current_view,
                parcel_id,
                slug,
                target,
            } => Ok(text_events(
                self.revoke_shop_badge(current_view, parcel_id, slug, &identity.player_id, target)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::BadgesMine => Ok(text_events(
                self.player_badges(&identity.user, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ShopAppRequest::BadgesUser { target } => {
                Ok(text_events(self.target_badges(target).await?.text, None))
            }
        }
    }

    async fn commercial_parcel_invalidation(&self, parcel_id: &str) -> Result<UiEvent, E> {
        let parcel = self.store.commercial_parcel(parcel_id).await?;
        Ok(commercial_parcel_cache_event(
            CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            },
        ))
    }
}
