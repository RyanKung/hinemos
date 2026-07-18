use super::contracts::{
    BuildStore, FromMailingListValidation, FromShopBadgeValidation, FromShopWorkValidation,
    LandStore, ParcelStore, ParcelView, PaymentRequestView, PaymentStore, ShopBadgeAwardView,
    ShopBadgeDefinitionView, ShopCommandRouteView, ShopMailingListSubscriberView,
    ShopMailingListSubscriptionView, ShopMailingListView, ShopShiftView, ShopStaffView, ShopStore,
    ShopWorkDeskView, ShopWorkItemView, TransferView,
};
use super::rendering::{
    custom_command_preview, is_custom_command_input, non_empty, render_parcel_detail,
    render_parcel_list,
};
use super::results::{
    BuildCommandResult, BusinessListResult, CommercialParcelCacheInvalidation, LandCommandResult,
    PayAcceptResult, PayDirectResult, ShopPaymentRequestResult, build_help_text,
    default_build_commands,
};
use crate::{
    AppIdentity, AppService, BuildSheet, LiveInboxNotice, MailAuthTokenView, OperatorCommandView,
    PARCEL_STATUS_BUILT, RoomBindingKindView, UiEvent, shop_badge_description_is_valid,
    shop_badge_note_is_valid, shop_badge_slug_is_valid, shop_badge_title_is_valid,
    shop_command_route_prefix_is_valid, shop_mailing_list_body_is_valid,
    shop_mailing_list_slug_is_valid, shop_mailing_list_subject_is_valid,
    shop_mailing_list_title_is_valid, shop_work_desk_slug_is_valid, shop_work_desk_title_is_valid,
    shop_work_result_is_valid,
};

impl<S, E> AppService<S>
where
    S: ShopStore<Error = E> + LandStore<Error = E>,
    E: FromMailingListValidation + FromShopBadgeValidation + FromShopWorkValidation,
{
    /// Returns true when a commercial parcel will consume this raw input line.
    #[must_use]
    pub fn commercial_parcel_consumes_input<P>(&self, binding: &P, raw_line: &str) -> bool
    where
        P: ParcelView + RoomBindingKindView,
    {
        RoomBindingKindView::is_commercial_parcel(binding)
            && binding.status() == PARCEL_STATUS_BUILT
            && ParcelView::owner_player_id(binding).is_some()
            && is_custom_command_input(binding, raw_line)
    }

    /// Builds the shop operator inbox text.
    pub async fn shop_inbox(&self, owner_player_id: &str) -> Result<BusinessListResult, E> {
        let commands = self
            .store
            .recent_operator_commands(owner_player_id, 20)
            .await?;
        let mut lines = vec![String::new(), "Shop Inbox".to_owned()];
        if commands.is_empty() {
            lines.push("No shop commands.".to_owned());
        } else {
            for command in commands.iter().rev() {
                lines.push(format!(
                    "- #{} [{}] {} from {} in {}: {}",
                    command.id(),
                    command.created_at(),
                    command.status(),
                    command.sender_user(),
                    command.parcel_id(),
                    command.raw_input()
                ));
            }
        }
        lines.push(String::new());
        Ok(BusinessListResult {
            text: lines.join("\r\n"),
        })
    }

    /// Creates a payment request for a shop command.
    pub async fn request_shop_payment(
        &self,
        command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<ShopPaymentRequestResult<<S as ShopStore>::InboxItem>, E> {
        let request = self
            .store
            .create_payment_request(command_id, owner_player_id, amount, delivery)
            .await?;
        let inbox_item = self
            .store
            .inbox_item_by_source(request.payer_player_id(), "payment_request", request.id())
            .await?;
        Ok(ShopPaymentRequestResult {
            text: format!(
                "Created payment request #{} for {}: {} {}. Delivery is locked until payment.\r\n",
                request.id(),
                request.payer_user(),
                request.amount(),
                request.asset()
            ),
            payer_player_id: request.payer_player_id().to_owned(),
            inbox_item,
        })
    }

    /// Creates a mailing list for an owned shop parcel.
    pub async fn create_shop_mailing_list(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<BusinessListResult, E> {
        validate_mailing_list_slug(slug)?;
        validate_mailing_list_title(title)?;
        let list = self
            .store
            .create_shop_mailing_list(parcel_id, owner_player_id, slug, title)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Created shop chat {} for parcel {}: {}.\r\nJoin: /subscribe {} {}\r\nPost: /chat {} {} -- <message>\r\n",
                list.slug(),
                list.parcel_id(),
                list.title(),
                list.parcel_id(),
                list.slug(),
                list.parcel_id(),
                list.slug()
            ),
        })
    }

    /// Lists mailing lists for an owned shop parcel.
    pub async fn list_shop_mailing_lists(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let lists = self
            .store
            .shop_mailing_lists(parcel_id, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_mailing_lists(parcel_id, &lists).replace('\n', "\r\n"),
        })
    }

    /// Shows active subscriber count and recent subscribers.
    pub async fn shop_mailing_list_subscribers(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let page = self
            .store
            .shop_mailing_list_subscribers(parcel_id, slug, owner_player_id, 20)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_mailing_list_subscribers(
                parcel_id,
                slug,
                page.total,
                &page.subscribers,
            )
            .replace('\n', "\r\n"),
        })
    }

    /// Closes an owned mailing list to new subscriptions.
    pub async fn close_shop_mailing_list(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let list = self
            .store
            .close_shop_mailing_list(parcel_id, slug, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Closed shop chat {} for parcel {}. Existing members remain recorded and can unsubscribe.\r\n",
                list.slug(),
                list.parcel_id()
            ),
        })
    }

    /// Subscribes the current player to a shop mailing list.
    pub async fn subscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let subscription = self
            .store
            .subscribe_shop_mailing_list(target, slug, subscriber_user, subscriber_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Joined shop chat {} ({}) at {}.\r\nPost: /chat {} {} -- <message>\r\nUnsubscribe: /unsubscribe {} {}\r\n",
                subscription.list_title(),
                subscription.slug(),
                subscription.parcel_id(),
                subscription.parcel_id(),
                subscription.slug(),
                subscription.parcel_id(),
                subscription.slug()
            ),
        })
    }

    /// Unsubscribes the current player from a shop mailing list.
    pub async fn unsubscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let subscription = self
            .store
            .unsubscribe_shop_mailing_list(target, slug, subscriber_user, subscriber_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Left shop chat {} ({}) at {}.\r\n",
                subscription.list_title(),
                subscription.slug(),
                subscription.parcel_id()
            ),
        })
    }

    /// Lists active mailing-list subscriptions for the current player.
    pub async fn shop_mailing_list_subscriptions(
        &self,
        subscriber_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let subscriptions = self
            .store
            .shop_mailing_list_subscriptions(subscriber_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_mailing_list_subscriptions(&subscriptions).replace('\n', "\r\n"),
        })
    }

    /// Sends an owner-authored mailing-list post to all current active members.
    pub async fn send_shop_mailing_list_post(
        &self,
        target: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<super::contracts::ShopMailingListSend<S::MailingListPost, S::InboxItem>, E> {
        validate_mailing_list_slug(slug)?;
        validate_mailing_list_subject(subject)?;
        validate_mailing_list_body(body)?;
        self.store
            .send_shop_mailing_list_post(target, slug, sender_user, sender_player_id, subject, body)
            .await
    }

    /// Posts a group-chat message to a shop mailing list.
    pub async fn post_shop_mailing_list_chat(
        &self,
        target: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<super::contracts::ShopMailingListSend<S::MailingListPost, S::InboxItem>, E> {
        validate_mailing_list_slug(slug)?;
        validate_mailing_list_body(body)?;
        let subject = format!("Shop chat: {slug}");
        self.store
            .send_shop_mailing_list_post(
                target,
                slug,
                sender_user,
                sender_player_id,
                &subject,
                body,
            )
            .await
    }

    /// Creates a shop-local work desk.
    pub async fn create_shop_work_desk(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        validate_work_desk_title(title)?;
        let desk = self
            .store
            .create_shop_work_desk(parcel_id, owner_player_id, slug, title)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Created shop work desk {} for parcel {}: {}.\r\nRoute commands: /shop route add {} {} <command-prefix>\r\nAssign staff: /shop staff add {} {} <username>\r\n",
                desk.slug(),
                desk.parcel_id(),
                desk.title(),
                desk.parcel_id(),
                desk.slug(),
                desk.parcel_id(),
                desk.slug()
            ),
        })
    }

    /// Lists shop-local work desks for an owned shop parcel.
    pub async fn list_shop_work_desks(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        let desks = self
            .store
            .shop_work_desks(parcel_id, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_work_desks(parcel_id, &desks).replace('\n', "\r\n"),
        })
    }

    /// Adds a worker to one shop-local work desk.
    pub async fn add_shop_staff(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let staff = self
            .store
            .add_shop_staff(parcel_id, slug, owner_player_id, username)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Added shop staff {} to {} for parcel {}.\r\nThey must enter the shop and run /shop shift start {} {} before consuming work.\r\n",
                staff.staff_user(),
                slug,
                parcel_id,
                parcel_id,
                slug
            ),
        })
    }

    /// Lists workers assigned to one shop-local work desk.
    pub async fn list_shop_staff(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let staff = self
            .store
            .shop_staff(parcel_id, slug, owner_player_id, 50)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_staff(parcel_id, slug, &staff).replace('\n', "\r\n"),
        })
    }

    /// Removes a worker from one shop-local work desk.
    pub async fn remove_shop_staff(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let staff = self
            .store
            .remove_shop_staff(parcel_id, slug, owner_player_id, username)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Removed shop staff {} from {} for parcel {}. Status: {}.\r\n",
                staff.staff_user(),
                slug,
                parcel_id,
                staff.status()
            ),
        })
    }

    /// Starts a worker shift inside the target shop.
    pub async fn start_shop_shift(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let shift = self
            .store
            .start_shop_shift(parcel_id, slug, worker_user, worker_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Started shop shift #{} at {} for parcel {}. Work list: /shop work list {} {}\r\n",
                shift.id(),
                shift.slug(),
                shift.parcel_id(),
                shift.parcel_id(),
                shift.slug()
            ),
        })
    }

    /// Ends a worker shift inside the target shop.
    pub async fn end_shop_shift(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let shift = self
            .store
            .end_shop_shift(parcel_id, slug, worker_user, worker_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Ended shop shift #{} at {} for parcel {}.\r\n",
                shift.id(),
                shift.slug(),
                shift.parcel_id()
            ),
        })
    }

    /// Lists work items for an active in-shop worker.
    pub async fn list_shop_work(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: Option<&str>,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        if let Some(slug) = slug {
            validate_work_desk_slug(slug)?;
        }
        let items = self
            .store
            .shop_work_items(parcel_id, worker_user, worker_player_id, slug, 50)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_work_items(parcel_id, slug, &items).replace('\n', "\r\n"),
        })
    }

    /// Claims one work item for an active in-shop worker.
    pub async fn claim_shop_work(
        &self,
        current_view: &str,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        let item = self
            .store
            .claim_shop_work(parcel_id, worker_user, worker_player_id, work_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Claimed shop work #{} at {} for parcel {}.\r\nCommand #{} from {}: {}\r\nComplete: /shop work done {} {} -- <result>\r\n",
                item.id(),
                item.slug(),
                item.parcel_id(),
                item.operator_command_id(),
                item.sender_user(),
                item.raw_input(),
                item.parcel_id(),
                item.id()
            ),
        })
    }

    /// Completes one claimed work item for an active in-shop worker.
    pub async fn finish_shop_work(
        &self,
        current_view: &str,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
        result: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_result(result)?;
        let item = self
            .store
            .finish_shop_work(parcel_id, worker_user, worker_player_id, work_id, result)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Completed shop work #{} at {} for parcel {}.\r\n",
                item.id(),
                item.slug(),
                item.parcel_id()
            ),
        })
    }

    /// Adds a command route from a shop command prefix into a shop-local work desk.
    pub async fn add_shop_command_route(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        validate_command_route_prefix(command_prefix)?;
        let route = self
            .store
            .add_shop_command_route(parcel_id, owner_player_id, slug, command_prefix)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Routed shop commands matching {} to work desk {} ({}) for parcel {}.\r\nWorkers must enter the shop and start a shift before listing or claiming routed work.\r\n",
                route.command_prefix(),
                route.desk_title(),
                route.slug(),
                route.parcel_id()
            ),
        })
    }

    /// Lists command routes for an owned shop parcel.
    pub async fn list_shop_command_routes(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        let routes = self
            .store
            .shop_command_routes(parcel_id, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_shop_command_routes(parcel_id, &routes).replace('\n', "\r\n"),
        })
    }

    /// Removes a command route from a shop-chat stream.
    pub async fn remove_shop_command_route(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_shop(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        validate_command_route_prefix(command_prefix)?;
        let route = self
            .store
            .remove_shop_command_route(parcel_id, owner_player_id, slug, command_prefix)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Removed shop command route {} -> {} for parcel {}.\r\n",
                route.command_prefix(),
                route.slug(),
                route.parcel_id()
            ),
        })
    }

    /// Creates or updates a badge definition for an owned shop parcel.
    pub async fn create_shop_badge(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<BusinessListResult, E> {
        validate_badge_slug(slug)?;
        validate_badge_title(title)?;
        validate_badge_description(description)?;
        let badge = self
            .store
            .create_shop_badge(parcel_id, owner_player_id, slug, title, description)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Saved badge {} for parcel {}: {}.\r\nAward command: /shop badge award {} {} <user> [note]\r\n",
                badge.slug(),
                badge.parcel_id(),
                badge.title(),
                badge.parcel_id(),
                badge.slug()
            ),
        })
    }

    /// Lists badge definitions for an owned shop parcel.
    pub async fn list_shop_badges(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let badges = self.store.shop_badges(parcel_id, owner_player_id).await?;
        Ok(BusinessListResult {
            text: render_shop_badges(parcel_id, &badges).replace('\n', "\r\n"),
        })
    }

    /// Awards a shop badge to a target player.
    pub async fn award_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        issuer_user: &str,
        issuer_player_id: &str,
        target: &str,
        note: Option<&str>,
    ) -> Result<BusinessListResult, E> {
        validate_badge_slug(slug)?;
        validate_badge_note(note)?;
        let award = self
            .store
            .award_shop_badge(parcel_id, slug, issuer_user, issuer_player_id, target, note)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Awarded badge {} ({}) from {} to {}.\r\nIssued: {} by {}.\r\n",
                award.badge_title(),
                award.slug(),
                award.parcel_id(),
                award.recipient_user(),
                award.awarded_at(),
                award.issuer_user()
            ),
        })
    }

    /// Revokes an active shop badge award.
    pub async fn revoke_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<BusinessListResult, E> {
        validate_badge_slug(slug)?;
        let award = self
            .store
            .revoke_shop_badge(parcel_id, slug, owner_player_id, target)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Revoked badge {} ({}) from {}.\r\n",
                award.badge_title(),
                award.slug(),
                award.recipient_user()
            ),
        })
    }

    /// Lists active badges held by a player id.
    pub async fn player_badges(
        &self,
        owner_label: &str,
        player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let awards = self.store.shop_badges_for_player(player_id, 50).await?;
        Ok(BusinessListResult {
            text: render_badge_awards(owner_label, &awards).replace('\n', "\r\n"),
        })
    }

    /// Lists active badges held by a public user target.
    pub async fn target_badges(&self, target: &str) -> Result<BusinessListResult, E> {
        let awards = self.store.shop_badges_for_target(target, 50).await?;
        Ok(BusinessListResult {
            text: render_badge_awards(target, &awards).replace('\n', "\r\n"),
        })
    }

    async fn ensure_inside_shop(&self, current_view: &str, parcel_id: &str) -> Result<(), E> {
        let parcel = self.store.commercial_parcel(parcel_id).await?;
        if parcel.view_id() == current_view {
            Ok(())
        } else {
            Err(E::invalid_shop_work(
                "shop work can only happen while inside that shop",
            ))
        }
    }

    /// Handles a raw or slash-prefixed input line inside a commercial parcel room.
    pub async fn handle_commercial_parcel_input<P>(
        &self,
        identity: &AppIdentity,
        binding: &P,
        raw_line: &str,
    ) -> Result<Option<Vec<UiEvent>>, E>
    where
        P: ParcelView + RoomBindingKindView + Sync,
    {
        if !self.commercial_parcel_consumes_input(binding, raw_line) {
            return Ok(None);
        }
        let Some(owner_player_id) = ParcelView::owner_player_id(binding) else {
            return Ok(None);
        };
        if owner_player_id == identity.player_id.as_str() {
            if is_custom_command_input(binding, raw_line) {
                return Ok(Some(vec![UiEvent::Text(format!(
                    "You own this shop. Visitors use {} here; their requests arrive in your inbox and /shop inbox.\r\n",
                    raw_line.split_whitespace().next().unwrap_or("this command")
                ))]));
            }
            return Ok(None);
        }
        if !is_custom_command_input(binding, raw_line) {
            return Ok(None);
        }

        let command = self
            .store
            .save_operator_command(binding, &identity.user, &identity.player_id, raw_line, true)
            .await?;
        let inbox_player_id = ParcelView::room_player_id(binding).unwrap_or(owner_player_id);
        let inbox_item = self
            .store
            .inbox_item_by_source(inbox_player_id, "operator_command", command.id())
            .await?;
        let work_items = self
            .store
            .dispatch_shop_command_routes(binding, command.id())
            .await?;
        let queued_work_items = work_items.len();
        let route_summary = if queued_work_items == 0 {
            String::new()
        } else {
            format!(
                "Queued {queued_work_items} shop work item(s). Workers must be inside the shop with an active shift to list, claim, or complete them.\r\n"
            )
        };
        let mut events = vec![UiEvent::Text(format!(
            "Shop request #{} sent to owner {} for parcel {}.\r\nStatus: delivered. Payment and fulfillment are pending owner reply; check /mailbox and /pay requests.\r\n{}{}",
            command.id(),
            command.owner_user(),
            command.parcel_id(),
            route_summary,
            custom_command_preview(binding, raw_line)
                .map(|preview| format!("Preview: {preview}\r\n"))
                .unwrap_or_default()
        ))];
        events.push(UiEvent::LiveInboxNotice {
            target_player_id: owner_player_id.to_owned(),
            notice: LiveInboxNotice::from_item(&inbox_item),
        });
        if queued_work_items > 0 {
            events.push(UiEvent::LiveViewMessage {
                view_id: ParcelView::view_id(binding).to_owned(),
                text: format!(
                    "[shop work] {queued_work_items} new item(s) queued for parcel {}.",
                    binding.parcel_id()
                ),
            });
        }
        Ok(Some(events))
    }
}

fn validate_mailing_list_slug<E>(slug: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if shop_mailing_list_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list slug"))
    }
}

fn validate_mailing_list_title<E>(title: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if shop_mailing_list_title_is_valid(title) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list title"))
    }
}

fn validate_mailing_list_subject<E>(subject: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if shop_mailing_list_subject_is_valid(subject) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list subject"))
    }
}

fn validate_mailing_list_body<E>(body: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if shop_mailing_list_body_is_valid(body) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list body"))
    }
}

fn validate_work_desk_slug<E>(slug: &str) -> Result<(), E>
where
    E: FromShopWorkValidation,
{
    if shop_work_desk_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(E::invalid_shop_work("invalid shop work desk slug"))
    }
}

fn validate_work_desk_title<E>(title: &str) -> Result<(), E>
where
    E: FromShopWorkValidation,
{
    if shop_work_desk_title_is_valid(title) {
        Ok(())
    } else {
        Err(E::invalid_shop_work("invalid shop work desk title"))
    }
}

fn validate_work_result<E>(result: &str) -> Result<(), E>
where
    E: FromShopWorkValidation,
{
    if shop_work_result_is_valid(result) {
        Ok(())
    } else {
        Err(E::invalid_shop_work("invalid shop work result"))
    }
}

fn validate_command_route_prefix<E>(command_prefix: &str) -> Result<(), E>
where
    E: FromShopWorkValidation,
{
    if shop_command_route_prefix_is_valid(command_prefix) {
        Ok(())
    } else {
        Err(E::invalid_shop_work("invalid shop command route prefix"))
    }
}

fn validate_badge_slug<E>(slug: &str) -> Result<(), E>
where
    E: FromShopBadgeValidation,
{
    if shop_badge_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(E::invalid_shop_badge("invalid badge slug"))
    }
}

fn validate_badge_title<E>(title: &str) -> Result<(), E>
where
    E: FromShopBadgeValidation,
{
    if shop_badge_title_is_valid(title) {
        Ok(())
    } else {
        Err(E::invalid_shop_badge("invalid badge title"))
    }
}

fn validate_badge_description<E>(description: Option<&str>) -> Result<(), E>
where
    E: FromShopBadgeValidation,
{
    if description.is_none_or(shop_badge_description_is_valid) {
        Ok(())
    } else {
        Err(E::invalid_shop_badge("invalid badge description"))
    }
}

fn validate_badge_note<E>(note: Option<&str>) -> Result<(), E>
where
    E: FromShopBadgeValidation,
{
    if note.is_none_or(shop_badge_note_is_valid) {
        Ok(())
    } else {
        Err(E::invalid_shop_badge("invalid badge note"))
    }
}

fn render_shop_mailing_lists(parcel_id: &str, lists: &[impl ShopMailingListView]) -> String {
    let mut lines = vec![format!("Shop Chats for {parcel_id}")];
    if lists.is_empty() {
        lines.push(
            "No shop chats. Create one with /shop mailing-list create <parcel> <slug> <title>."
                .to_owned(),
        );
    } else {
        for list in lists {
            lines.push(format!(
                "- {} [{}] {} members={} created={}. Join: /subscribe {} {}. Post: /chat {} {} -- <message>",
                list.slug(),
                list.status(),
                list.title(),
                list.subscriber_count(),
                list.created_at(),
                list.parcel_id(),
                list.slug(),
                list.parcel_id(),
                list.slug()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_mailing_list_subscribers(
    parcel_id: &str,
    slug: &str,
    total: i64,
    subscribers: &[impl ShopMailingListSubscriberView],
) -> String {
    let mut lines = vec![format!(
        "Shop Chat Members for {parcel_id} {slug}: {total} active"
    )];
    if subscribers.is_empty() {
        lines.push("No active members.".to_owned());
    } else {
        for subscriber in subscribers {
            lines.push(format!(
                "- {} ({}) since {}",
                subscriber.subscriber_user(),
                subscriber.subscriber_player_id(),
                subscriber.updated_at()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_mailing_list_subscriptions(
    subscriptions: &[impl ShopMailingListSubscriptionView],
) -> String {
    let mut lines = vec!["Shop Chat Memberships".to_owned()];
    if subscriptions.is_empty() {
        lines.push("No active shop chats.".to_owned());
    } else {
        for subscription in subscriptions {
            let shop = subscription
                .shop_title()
                .unwrap_or(subscription.parcel_id());
            lines.push(format!(
                "- {} / {} ({}) status={} updated={}. Post: /chat {} {} -- <message>. Unsubscribe: /unsubscribe {} {}",
                shop,
                subscription.list_title(),
                subscription.slug(),
                subscription.status(),
                subscription.updated_at(),
                subscription.parcel_id(),
                subscription.slug(),
                subscription.parcel_id(),
                subscription.slug()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_work_desks(parcel_id: &str, desks: &[impl ShopWorkDeskView]) -> String {
    let mut lines = vec![format!("Shop Work Desks for {parcel_id}")];
    if desks.is_empty() {
        lines.push(
            "No work desks. Create one with /shop desk create <parcel> <slug> <title>.".to_owned(),
        );
    } else {
        for desk in desks {
            lines.push(format!(
                "- {} [{}] {} queued={} active_workers={} created={}. Route: /shop route add {} {} <command-prefix>",
                desk.slug(),
                desk.status(),
                desk.title(),
                desk.queued_count(),
                desk.active_worker_count(),
                desk.created_at(),
                desk.parcel_id(),
                desk.slug()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_staff(parcel_id: &str, slug: &str, staff: &[impl ShopStaffView]) -> String {
    let mut lines = vec![format!("Shop Staff for {parcel_id} {slug}")];
    if staff.is_empty() {
        lines.push(
            "No assigned staff. Add one with /shop staff add <parcel> <slug> <username>."
                .to_owned(),
        );
    } else {
        for member in staff {
            lines.push(format!(
                "- {} [{}] updated={}",
                member.staff_user(),
                member.status(),
                member.updated_at()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_work_items(
    parcel_id: &str,
    slug: Option<&str>,
    items: &[impl ShopWorkItemView],
) -> String {
    let scope = slug
        .map(|slug| format!("{parcel_id} {slug}"))
        .unwrap_or_else(|| parcel_id.to_owned());
    let mut lines = vec![format!("Shop Work for {scope}")];
    if items.is_empty() {
        lines.push(
            "No visible work. Start a shift in the shop, then wait for routed commands.".to_owned(),
        );
    } else {
        for item in items {
            let assignee = item
                .assignee_user()
                .map(|user| format!(" assignee={user}"))
                .unwrap_or_default();
            let result = item
                .result()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" result={value}"))
                .unwrap_or_default();
            lines.push(format!(
                "- #{} [{}] {} ({}) command=#{} prefix={} from={}{}{} updated={}. Claim: /shop work claim {} {}",
                item.id(),
                item.status(),
                item.slug(),
                item.desk_title(),
                item.operator_command_id(),
                item.command_prefix(),
                item.sender_user(),
                assignee,
                result,
                item.updated_at(),
                item.parcel_id(),
                item.id()
            ));
            lines.push(format!("  {}", item.raw_input()));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_command_routes(parcel_id: &str, routes: &[impl ShopCommandRouteView]) -> String {
    let mut lines = vec![format!("Shop Command Routes for {parcel_id}")];
    if routes.is_empty() {
        lines.push(
            "No command routes. Create one with /shop route add <parcel> <desk-slug> <command-prefix>."
                .to_owned(),
        );
    } else {
        for route in routes {
            lines.push(format!(
                "- {} -> {} ({}) created={}",
                route.command_prefix(),
                route.slug(),
                route.desk_title(),
                route.created_at()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_shop_badges(parcel_id: &str, badges: &[impl ShopBadgeDefinitionView]) -> String {
    let mut lines = vec![format!("Shop Badges for {parcel_id}")];
    if badges.is_empty() {
        lines.push(
            "No badges. Create one with /shop badge create <parcel> <slug> <title> [-- description]."
                .to_owned(),
        );
    } else {
        for badge in badges {
            let description = badge
                .description()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" - {value}"))
                .unwrap_or_default();
            lines.push(format!(
                "- {}: {}{} awards={} updated={}",
                badge.slug(),
                badge.title(),
                description,
                badge.active_award_count(),
                badge.updated_at()
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_badge_awards(owner_label: &str, awards: &[impl ShopBadgeAwardView]) -> String {
    let mut lines = vec![format!("Badges for {owner_label}")];
    if awards.is_empty() {
        lines.push("No active badges.".to_owned());
    } else {
        for award in awards {
            let shop = award.shop_title().unwrap_or(award.parcel_id());
            let note = award
                .note()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" Note: {value}"))
                .unwrap_or_default();
            lines.push(format!(
                "- {} ({}) from {} [{}], issued by {} at {}.{}",
                award.badge_title(),
                award.slug(),
                shop,
                award.parcel_id(),
                award.issuer_user(),
                award.awarded_at(),
                note
            ));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}

impl<S, E> AppService<S>
where
    S: ParcelStore<Error = E>,
{
    /// Builds the commercial parcel list text.
    pub async fn land_list(&self) -> Result<BusinessListResult, E> {
        let parcels = self.store.list_commercial_parcels().await?;
        Ok(BusinessListResult {
            text: render_parcel_list(&parcels).replace('\n', "\r\n"),
        })
    }
}

impl<S, E> AppService<S>
where
    S: LandStore<Error = E>,
{
    /// Builds the commercial parcel detail text.
    pub async fn land_info(&self, parcel_id: &str) -> Result<BusinessListResult, E> {
        let parcel = self.store.commercial_parcel(parcel_id).await?;
        Ok(BusinessListResult {
            text: render_parcel_detail(&parcel).replace('\n', "\r\n"),
        })
    }

    /// Claims a parcel and creates the room mailbox token.
    pub async fn claim_land(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<LandCommandResult, E> {
        let parcel = self
            .store
            .claim_commercial_parcel(parcel_id, owner_user, owner_player_id)
            .await?;
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, owner_player_id, token)
            .await?;
        Ok(LandCommandResult {
            text: format!(
                "Claimed parcel {}. Room mail account: {}. Token: {}\r\nUse this token from the room owner process with SMTP/IMAP. You can rotate it later with /land token {}.\r\nBuild here with /build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /build publish. From the street, enter with /enter {}. Custom commands are auto-filled if omitted.\r\n",
                parcel.parcel_id(),
                mail.username(),
                token,
                parcel.parcel_id(),
                parcel.parcel_id()
            ),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Transfers a parcel and rotates its room mailbox token for the new owner.
    pub async fn transfer_land(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
        token: &str,
    ) -> Result<LandCommandResult, E> {
        let parcel = self
            .store
            .transfer_commercial_parcel(parcel_id, owner_player_id, target)
            .await?;
        let new_owner_player_id = parcel.owner_player_id().unwrap_or_default();
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, new_owner_player_id, token)
            .await?;
        Ok(LandCommandResult {
            text: format!(
                "Transferred parcel {} to {}.\r\nNew room mail account: {}. Token: {}\r\nGive this token to the new room owner process; the old room token has been rotated.\r\n",
                parcel.parcel_id(),
                parcel.owner_user().unwrap_or("unknown"),
                mail.username(),
                token
            ),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Rotates a parcel room mailbox token.
    pub async fn rotate_land_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<LandCommandResult, E> {
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, owner_player_id, token)
            .await?;
        Ok(LandCommandResult {
            text: format!(
                "Room mail account for {}: {}\r\nToken: {}\r\nUse SMTP/IMAP with this username/token. This token is shown once; run /land token {} again to rotate it.\r\n",
                parcel_id,
                mail.username(),
                token,
                parcel_id
            ),
            invalidate: None,
        })
    }
}

impl<S, E> AppService<S>
where
    S: BuildStore<Error = E> + ShopStore<Error = E>,
{
    /// Renders the build help text.
    pub fn build_help_text(&self) -> &'static str {
        build_help_text()
    }

    /// Applies a structured parcel build sheet.
    pub async fn apply_build_sheet(
        &self,
        current_view: &str,
        owner_player_id: &str,
        sheet: &BuildSheet,
    ) -> Result<BuildCommandResult, E> {
        let mut updated = Vec::new();
        let mut latest_parcel = None;
        for (field, value) in [
            ("title", sheet.title.as_deref()),
            ("description", sheet.description.as_deref()),
            ("style", sheet.style.as_deref()),
            ("prompt", sheet.prompt.as_deref()),
            ("commands", sheet.commands.as_deref()),
        ] {
            let Some(value) = non_empty(value) else {
                continue;
            };
            latest_parcel = Some(
                self.store
                    .update_parcel_build_field(current_view, owner_player_id, field, value)
                    .await?,
            );
            updated.push(field);
        }
        if non_empty(sheet.commands.as_deref()).is_none() {
            latest_parcel = Some(
                self.store
                    .update_parcel_build_field(
                        current_view,
                        owner_player_id,
                        "commands",
                        default_build_commands(),
                    )
                    .await?,
            );
            updated.push("commands");
        }
        if updated.is_empty() {
            updated.push("commands");
        }
        Ok(BuildCommandResult {
            text: format!(
                "Updated build sheet for current parcel: {}.\r\n",
                updated.join(", ")
            ),
            invalidate: latest_parcel.map(|parcel| CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Updates one parcel build field.
    pub async fn set_build_field(
        &self,
        current_view: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<BuildCommandResult, E> {
        let parcel = self
            .store
            .update_parcel_build_field(current_view, owner_player_id, field, value)
            .await?;
        Ok(BuildCommandResult {
            text: format!("Updated {} for parcel {}.\r\n", field, parcel.parcel_id()),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Publishes the current parcel build sheet.
    pub async fn publish_build(
        &self,
        current_view: &str,
        owner_player_id: &str,
    ) -> Result<BuildCommandResult, E> {
        let parcel = self
            .store
            .publish_parcel_build(current_view, owner_player_id)
            .await?;
        Ok(BuildCommandResult {
            text: format!(
                "Published parcel {} as a built shop.\r\n",
                parcel.parcel_id()
            ),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }
}

impl<S, E> AppService<S>
where
    S: PaymentStore<Error = E>,
{
    /// Builds pending payment request text.
    pub async fn pending_pay_requests(
        &self,
        payer_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let requests = self
            .store
            .pending_payment_requests(payer_player_id, 20)
            .await?;
        let mut lines = vec![String::new(), "Payment Requests".to_owned()];
        if requests.is_empty() {
            lines.push("No pending payment requests.".to_owned());
        } else {
            for request in requests.iter().rev() {
                lines.push(format!(
                    "\n=== Payment Request #{} ===\nShop: {} ({})\nAmount: {} {}\nFor: shop command #{}\nDelivery: locked until payment\nAccept: /pay accept {}\nReject: ignore this request\n==========================",
                    request.id(),
                    request.parcel_id(),
                    request.payee_user(),
                    request.amount(),
                    request.asset(),
                    request.operator_command_id(),
                    request.id()
                ));
            }
        }
        lines.push(String::new());
        Ok(BusinessListResult {
            text: lines.join("\r\n").replace('\n', "\r\n"),
        })
    }

    /// Executes a direct MARK payment.
    pub async fn pay_direct(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<PayDirectResult<S::Transfer>, E> {
        let transfer = self
            .store
            .transfer_mark(sender_user, sender_player_id, target, amount, memo)
            .await?;
        Ok(PayDirectResult {
            text: format!(
                "Paid {} {} to {}. Ledger #{}. Balance: {} {}.\r\n",
                transfer.amount(),
                transfer.asset(),
                transfer.target_user(),
                transfer.ledger_id(),
                transfer.sender_balance(),
                transfer.asset()
            ),
            transfer,
        })
    }

    /// Accepts a pending payment request.
    pub async fn accept_pay_request(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<PayAcceptResult<S::PaymentRequest>, E> {
        let (request, sender_balance) = self
            .store
            .accept_payment_request(payer_user, payer_player_id, request_id)
            .await?;
        Ok(PayAcceptResult {
            text: format!(
                "Paid payment request #{}: {} {} to {}. Balance: {} {}.\r\nUnlocked content: {}\r\n",
                request.id(),
                request.amount(),
                request.asset(),
                request.payee_user(),
                sender_balance,
                request.asset(),
                request.delivery()
            ),
            request,
        })
    }
}
