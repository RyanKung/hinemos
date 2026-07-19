use crate::*;

use super::events::{commercial_parcel_cache_event, text_events};
use super::route::{ParcelBuildAppRequest, ParcelOperationAppRequest, ParcelRegistryAppRequest};

impl<S, E> AppService<S>
where
    S: LandStore<Error = E> + ParcelStore<Error = E>,
{
    pub(super) async fn handle_parcel_registry_request(
        &self,
        identity: &AppIdentity,
        request: ParcelRegistryAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let (text, cache) = match request {
            ParcelRegistryAppRequest::List => (self.land_list().await?.text, None),
            ParcelRegistryAppRequest::Info { parcel_id } => {
                (self.land_info(parcel_id).await?.text, None)
            }
            ParcelRegistryAppRequest::Claim { parcel_id, token } => {
                let result = self
                    .claim_land(parcel_id, &identity.user, &identity.player_id, token)
                    .await?;
                (result.text, result.invalidate)
            }
            ParcelRegistryAppRequest::Transfer {
                parcel_id,
                target,
                token,
            } => {
                let result = self
                    .transfer_land(parcel_id, &identity.player_id, target, token)
                    .await?;
                (result.text, result.invalidate)
            }
            ParcelRegistryAppRequest::RotateToken { parcel_id, token } => {
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
    pub(super) async fn handle_parcel_build_request(
        &self,
        identity: &AppIdentity,
        request: ParcelBuildAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let result = match request {
            ParcelBuildAppRequest::Help => {
                return Ok(text_events(self.build_help_text().to_owned(), None));
            }
            ParcelBuildAppRequest::Apply {
                current_view,
                sheet,
            } => {
                self.apply_build_sheet(current_view, &identity.player_id, sheet)
                    .await?
            }
            ParcelBuildAppRequest::Set {
                current_view,
                field,
                value,
            } => {
                self.set_build_field(current_view, &identity.player_id, field, value)
                    .await?
            }
            ParcelBuildAppRequest::Publish { current_view } => {
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
    pub(super) async fn handle_parcel_operation_request(
        &self,
        identity: &AppIdentity,
        request: ParcelOperationAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            ParcelOperationAppRequest::Inbox => Ok(text_events(
                self.shop_inbox(&identity.player_id).await?.text,
                None,
            )),
            ParcelOperationAppRequest::RequestPayment {
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
            ParcelOperationAppRequest::MailingListCreate {
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
            ParcelOperationAppRequest::MailingListList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_mailing_lists(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::MailingListSubscribers {
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
            ParcelOperationAppRequest::MailingListSend {
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
                    "Sent parcel chat post #{} to {} member(s): {}.\r\n",
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
            ParcelOperationAppRequest::MailingListChat {
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
                    "Posted parcel chat message #{} to {} member(s): {}.\r\n",
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
            ParcelOperationAppRequest::MailingListClose {
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
            ParcelOperationAppRequest::MailingListSubscribe {
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
            ParcelOperationAppRequest::MailingListUnsubscribe {
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
            ParcelOperationAppRequest::MailingListSubscriptions => Ok(text_events(
                self.shop_mailing_list_subscriptions(&identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::DeskCreate {
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
            ParcelOperationAppRequest::DeskList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_work_desks(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::StaffAdd {
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
            ParcelOperationAppRequest::StaffList {
                current_view,
                parcel_id,
                slug,
            } => Ok(text_events(
                self.list_shop_staff(current_view, parcel_id, slug, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::StaffRemove {
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
            ParcelOperationAppRequest::ShiftStart {
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
            ParcelOperationAppRequest::ShiftEnd {
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
            ParcelOperationAppRequest::WorkList {
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
            ParcelOperationAppRequest::WorkClaim {
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
            ParcelOperationAppRequest::WorkDone {
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
            ParcelOperationAppRequest::RouteAdd {
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
            ParcelOperationAppRequest::RouteList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_command_routes(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::RouteRemove {
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
            ParcelOperationAppRequest::BadgeList {
                current_view,
                parcel_id,
            } => Ok(text_events(
                self.list_shop_badges(current_view, parcel_id, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::BadgeCreate {
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
            ParcelOperationAppRequest::BadgeAward {
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
            ParcelOperationAppRequest::BadgeRevoke {
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
            ParcelOperationAppRequest::BadgesMine => Ok(text_events(
                self.player_badges(&identity.user, &identity.player_id)
                    .await?
                    .text,
                None,
            )),
            ParcelOperationAppRequest::BadgesUser { target } => {
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
