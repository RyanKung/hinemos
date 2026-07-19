use super::contracts::{
    BuildStore, FromMailingListValidation, FromParcelBadgeValidation, FromParcelWorkValidation,
    ParcelBadgeAwardInput, ParcelBadgeAwardView, ParcelBadgeDefinitionView, ParcelCommandRouteView,
    ParcelMailingListPostInput, ParcelMailingListSubscriberView, ParcelMailingListSubscriptionView,
    ParcelMailingListView, ParcelOwnershipStore, ParcelRegistryStore, ParcelShiftView,
    ParcelStaffView, ParcelStore, ParcelView, ParcelWorkDeskView, ParcelWorkItemView,
    PaymentRequestView, PaymentStore, TransferView,
};
use super::rendering::{
    custom_command_preview, is_custom_command_input, non_empty, render_parcel_detail,
    render_parcel_list,
};
use super::results::{
    BuildCommandResult, BusinessListResult, ParcelCacheInvalidation, ParcelOwnershipResult,
    ParcelPaymentRequestResult, PayAcceptResult, PayDirectResult, build_help_text,
    default_build_commands,
};
use crate::{
    AppIdentity, AppService, BuildSheet, LiveInboxNotice, MailAuthTokenView, OperatorCommandView,
    PARCEL_STATUS_BUILT, RoomBindingKindView, UiEvent, parcel_badge_description_is_valid,
    parcel_badge_note_is_valid, parcel_badge_slug_is_valid, parcel_badge_title_is_valid,
    parcel_command_route_prefix_is_valid, parcel_mailing_list_body_is_valid,
    parcel_mailing_list_slug_is_valid, parcel_mailing_list_subject_is_valid,
    parcel_mailing_list_title_is_valid, parcel_work_desk_slug_is_valid,
    parcel_work_desk_title_is_valid, parcel_work_result_is_valid,
};

impl<S, E> AppService<S>
where
    S: ParcelStore<Error = E> + ParcelOwnershipStore<Error = E>,
    E: FromMailingListValidation + FromParcelBadgeValidation + FromParcelWorkValidation,
{
    /// Returns true when a parcel will consume this raw input line.
    #[must_use]
    pub fn parcel_consumes_input<P>(&self, binding: &P, raw_line: &str) -> bool
    where
        P: ParcelView + RoomBindingKindView,
    {
        RoomBindingKindView::is_parcel(binding)
            && binding.status() == PARCEL_STATUS_BUILT
            && ParcelView::owner_player_id(binding).is_some()
            && is_custom_command_input(binding, raw_line)
    }

    /// Builds the parcel operator inbox text.
    pub async fn parcel_inbox(&self, owner_player_id: &str) -> Result<BusinessListResult, E> {
        let commands = self
            .store
            .recent_operator_commands(owner_player_id, 20)
            .await?;
        let mut lines = vec![String::new(), "Parcel Inbox".to_owned()];
        if commands.is_empty() {
            lines.push("No parcel commands.".to_owned());
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

    /// Creates a payment request for a parcel command.
    pub async fn request_parcel_payment(
        &self,
        current_view: &str,
        command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<ParcelPaymentRequestResult<<S as ParcelStore>::InboxItem>, E> {
        let command = self.store.operator_command(command_id).await?;
        self.ensure_inside_parcel(current_view, command.parcel_id())
            .await?;
        let request = self
            .store
            .create_payment_request(command_id, owner_player_id, amount, delivery)
            .await?;
        let inbox_item = self
            .store
            .inbox_item_by_source(request.payer_player_id(), "payment_request", request.id())
            .await?;
        Ok(ParcelPaymentRequestResult {
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

    /// Creates a mailing list for an owned parcel.
    pub async fn create_parcel_mailing_list(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_mailing_list_slug(slug)?;
        validate_mailing_list_title(title)?;
        let list = self
            .store
            .create_parcel_mailing_list(parcel_id, owner_player_id, slug, title)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Created parcel chat {} for parcel {}: {}.\r\nJoin: /parcel subscribe {} {}\r\nPost: /parcel chat {} {} -- <message>\r\n",
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

    /// Lists mailing lists for an owned parcel.
    pub async fn list_parcel_mailing_lists(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let lists = self
            .store
            .parcel_mailing_lists(parcel_id, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_mailing_lists(parcel_id, &lists).replace('\n', "\r\n"),
        })
    }

    /// Shows active subscriber count and recent subscribers.
    pub async fn parcel_mailing_list_subscribers(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let page = self
            .store
            .parcel_mailing_list_subscribers(parcel_id, slug, owner_player_id, 20)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_mailing_list_subscribers(
                parcel_id,
                slug,
                page.total,
                &page.subscribers,
            )
            .replace('\n', "\r\n"),
        })
    }

    /// Closes an owned mailing list to new subscriptions.
    pub async fn close_parcel_mailing_list(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let list = self
            .store
            .close_parcel_mailing_list(parcel_id, slug, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Closed parcel chat {} for parcel {}. Existing members remain recorded and can unsubscribe.\r\n",
                list.slug(),
                list.parcel_id()
            ),
        })
    }

    /// Subscribes the current player to a parcel mailing list.
    pub async fn subscribe_parcel_mailing_list(
        &self,
        current_view: &str,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel_mailing_list_target(current_view, target, slug)
            .await?;
        let subscription = self
            .store
            .subscribe_parcel_mailing_list(target, slug, subscriber_user, subscriber_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Joined parcel chat {} ({}) at {}.\r\nPost: /parcel chat {} {} -- <message>\r\nUnsubscribe: /parcel unsubscribe {} {}\r\n",
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

    /// Unsubscribes the current player from a parcel mailing list.
    pub async fn unsubscribe_parcel_mailing_list(
        &self,
        current_view: &str,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel_mailing_list_target(current_view, target, slug)
            .await?;
        let subscription = self
            .store
            .unsubscribe_parcel_mailing_list(target, slug, subscriber_user, subscriber_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Left parcel chat {} ({}) at {}.\r\n",
                subscription.list_title(),
                subscription.slug(),
                subscription.parcel_id()
            ),
        })
    }

    /// Lists active mailing-list subscriptions for the current player.
    pub async fn parcel_mailing_list_subscriptions(
        &self,
        subscriber_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let subscriptions = self
            .store
            .parcel_mailing_list_subscriptions(subscriber_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_mailing_list_subscriptions(&subscriptions).replace('\n', "\r\n"),
        })
    }

    /// Sends an owner-authored mailing-list post to all current active members.
    pub async fn send_parcel_mailing_list_post(
        &self,
        input: ParcelMailingListPostInput<'_>,
    ) -> Result<super::contracts::ParcelMailingListSend<S::MailingListPost, S::InboxItem>, E> {
        validate_mailing_list_slug(input.slug)?;
        validate_mailing_list_subject(input.subject)?;
        validate_mailing_list_body(input.body)?;
        self.ensure_inside_parcel_mailing_list_target(input.current_view, input.target, input.slug)
            .await?;
        self.store
            .send_parcel_mailing_list_post(
                input.target,
                input.slug,
                input.sender_user,
                input.sender_player_id,
                input.subject,
                input.body,
            )
            .await
    }

    /// Posts a group-chat message to a parcel mailing list.
    pub async fn post_parcel_mailing_list_chat(
        &self,
        current_view: &str,
        target: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<super::contracts::ParcelMailingListSend<S::MailingListPost, S::InboxItem>, E> {
        validate_mailing_list_slug(slug)?;
        validate_mailing_list_body(body)?;
        self.ensure_inside_parcel_mailing_list_target(current_view, target, slug)
            .await?;
        let subject = format!("Parcel chat: {slug}");
        self.store
            .send_parcel_mailing_list_post(
                target,
                slug,
                sender_user,
                sender_player_id,
                &subject,
                body,
            )
            .await
    }

    /// Creates a parcel-local work desk.
    pub async fn create_parcel_work_desk(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        validate_work_desk_title(title)?;
        let desk = self
            .store
            .create_parcel_work_desk(parcel_id, owner_player_id, slug, title)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Created parcel work desk {} for parcel {}: {}.\r\nRoute commands: /parcel route add {} {} <command-prefix>\r\nAssign staff: /parcel staff add {} {} <username>\r\n",
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

    /// Lists parcel-local work desks for an owned parcel.
    pub async fn list_parcel_work_desks(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let desks = self
            .store
            .parcel_work_desks(parcel_id, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_work_desks(parcel_id, &desks).replace('\n', "\r\n"),
        })
    }

    /// Adds a worker to one parcel-local work desk.
    pub async fn add_parcel_staff(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let staff = self
            .store
            .add_parcel_staff(parcel_id, slug, owner_player_id, username)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Added parcel staff {} to {} for parcel {}.\r\nThey must enter the parcel and run /parcel shift start {} {} before consuming work.\r\n",
                staff.staff_user(),
                slug,
                parcel_id,
                parcel_id,
                slug
            ),
        })
    }

    /// Lists workers assigned to one parcel-local work desk.
    pub async fn list_parcel_staff(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let staff = self
            .store
            .parcel_staff(parcel_id, slug, owner_player_id, 50)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_staff(parcel_id, slug, &staff).replace('\n', "\r\n"),
        })
    }

    /// Removes a worker from one parcel-local work desk.
    pub async fn remove_parcel_staff(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let staff = self
            .store
            .remove_parcel_staff(parcel_id, slug, owner_player_id, username)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Removed parcel staff {} from {} for parcel {}. Status: {}.\r\n",
                staff.staff_user(),
                slug,
                parcel_id,
                staff.status()
            ),
        })
    }

    /// Starts a worker shift inside the target parcel.
    pub async fn start_parcel_shift(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let shift = self
            .store
            .start_parcel_shift(parcel_id, slug, worker_user, worker_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Started parcel shift #{} at {} for parcel {}. Work list: /parcel work list {} {}\r\n",
                shift.id(),
                shift.slug(),
                shift.parcel_id(),
                shift.parcel_id(),
                shift.slug()
            ),
        })
    }

    /// Ends a worker shift inside the target parcel.
    pub async fn end_parcel_shift(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        let shift = self
            .store
            .end_parcel_shift(parcel_id, slug, worker_user, worker_player_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Ended parcel shift #{} at {} for parcel {}.\r\n",
                shift.id(),
                shift.slug(),
                shift.parcel_id()
            ),
        })
    }

    /// Lists work items for an active in-parcel worker.
    pub async fn list_parcel_work(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: Option<&str>,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        if let Some(slug) = slug {
            validate_work_desk_slug(slug)?;
        }
        let items = self
            .store
            .parcel_work_items(parcel_id, worker_user, worker_player_id, slug, 50)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_work_items(parcel_id, slug, &items).replace('\n', "\r\n"),
        })
    }

    /// Claims one work item for an active in-parcel worker.
    pub async fn claim_parcel_work(
        &self,
        current_view: &str,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let item = self
            .store
            .claim_parcel_work(parcel_id, worker_user, worker_player_id, work_id)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Claimed parcel work #{} at {} for parcel {}.\r\nCommand #{} from {}: {}\r\nComplete: /parcel work done {} {} -- <result>\r\n",
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

    /// Completes one claimed work item for an active in-parcel worker.
    pub async fn finish_parcel_work(
        &self,
        current_view: &str,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
        result: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_result(result)?;
        let item = self
            .store
            .finish_parcel_work(parcel_id, worker_user, worker_player_id, work_id, result)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Completed parcel work #{} at {} for parcel {}.\r\n",
                item.id(),
                item.slug(),
                item.parcel_id()
            ),
        })
    }

    /// Adds a command route from a parcel command prefix into a parcel-local work desk.
    pub async fn add_parcel_command_route(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        validate_command_route_prefix(command_prefix)?;
        let route = self
            .store
            .add_parcel_command_route(parcel_id, owner_player_id, slug, command_prefix)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Routed parcel commands matching {} to work desk {} ({}) for parcel {}.\r\nWorkers must enter the parcel and start a shift before listing or claiming routed work.\r\n",
                route.command_prefix(),
                route.desk_title(),
                route.slug(),
                route.parcel_id()
            ),
        })
    }

    /// Lists command routes for an owned parcel.
    pub async fn list_parcel_command_routes(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let routes = self
            .store
            .parcel_command_routes(parcel_id, owner_player_id)
            .await?;
        Ok(BusinessListResult {
            text: render_parcel_command_routes(parcel_id, &routes).replace('\n', "\r\n"),
        })
    }

    /// Removes a command route from a parcel-chat stream.
    pub async fn remove_parcel_command_route(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_work_desk_slug(slug)?;
        validate_command_route_prefix(command_prefix)?;
        let route = self
            .store
            .remove_parcel_command_route(parcel_id, owner_player_id, slug, command_prefix)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Removed parcel command route {} -> {} for parcel {}.\r\n",
                route.command_prefix(),
                route.slug(),
                route.parcel_id()
            ),
        })
    }

    /// Creates or updates a badge definition for an owned parcel.
    pub async fn create_parcel_badge(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_badge_slug(slug)?;
        validate_badge_title(title)?;
        validate_badge_description(description)?;
        let badge = self
            .store
            .create_parcel_badge(parcel_id, owner_player_id, slug, title, description)
            .await?;
        Ok(BusinessListResult {
            text: format!(
                "Saved badge {} for parcel {}: {}.\r\nAward command: /parcel badge award {} {} <user> [note]\r\n",
                badge.slug(),
                badge.parcel_id(),
                badge.title(),
                badge.parcel_id(),
                badge.slug()
            ),
        })
    }

    /// Lists badge definitions for an owned parcel.
    pub async fn list_parcel_badges(
        &self,
        current_view: &str,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        let badges = self.store.parcel_badges(parcel_id, owner_player_id).await?;
        Ok(BusinessListResult {
            text: render_parcel_badges(parcel_id, &badges).replace('\n', "\r\n"),
        })
    }

    /// Awards a parcel badge to a target player.
    pub async fn award_parcel_badge(
        &self,
        input: ParcelBadgeAwardInput<'_>,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(input.current_view, input.parcel_id)
            .await?;
        validate_badge_slug(input.slug)?;
        validate_badge_note(input.note)?;
        let award = self
            .store
            .award_parcel_badge(
                input.parcel_id,
                input.slug,
                input.issuer_user,
                input.issuer_player_id,
                input.target,
                input.note,
            )
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

    /// Revokes an active parcel badge award.
    pub async fn revoke_parcel_badge(
        &self,
        current_view: &str,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<BusinessListResult, E> {
        self.ensure_inside_parcel(current_view, parcel_id).await?;
        validate_badge_slug(slug)?;
        let award = self
            .store
            .revoke_parcel_badge(parcel_id, slug, owner_player_id, target)
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
        let awards = self.store.parcel_badges_for_player(player_id, 50).await?;
        Ok(BusinessListResult {
            text: render_badge_awards(owner_label, &awards).replace('\n', "\r\n"),
        })
    }

    /// Lists active badges held by a public user target.
    pub async fn target_badges(&self, target: &str) -> Result<BusinessListResult, E> {
        let awards = self.store.parcel_badges_for_target(target, 50).await?;
        Ok(BusinessListResult {
            text: render_badge_awards(target, &awards).replace('\n', "\r\n"),
        })
    }

    async fn ensure_inside_parcel(&self, current_view: &str, parcel_id: &str) -> Result<(), E> {
        let parcel = self.store.parcel_by_id(parcel_id).await?;
        if parcel.view_id() == current_view {
            Ok(())
        } else {
            Err(E::invalid_parcel_work(
                "parcel actions can only happen while inside that parcel",
            ))
        }
    }

    async fn ensure_inside_parcel_mailing_list_target(
        &self,
        current_view: &str,
        target: &str,
        slug: &str,
    ) -> Result<(), E> {
        let list = self.store.parcel_mailing_list(target, slug).await?;
        self.ensure_inside_parcel(current_view, list.parcel_id())
            .await
    }

    /// Handles a raw or slash-prefixed input line inside a parcel room.
    pub async fn handle_parcel_input<P>(
        &self,
        identity: &AppIdentity,
        binding: &P,
        raw_line: &str,
    ) -> Result<Option<Vec<UiEvent>>, E>
    where
        P: ParcelView + RoomBindingKindView + Sync,
    {
        if !self.parcel_consumes_input(binding, raw_line) {
            return Ok(None);
        }
        let Some(owner_player_id) = ParcelView::owner_player_id(binding) else {
            return Ok(None);
        };
        if owner_player_id == identity.player_id.as_str() {
            if is_custom_command_input(binding, raw_line) {
                return Ok(Some(vec![UiEvent::Text(format!(
                    "You own this parcel. Visitors use {} here; their requests arrive in your inbox and /parcel inbox.\r\n",
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
            .dispatch_parcel_command_routes(binding, command.id())
            .await?;
        let queued_work_items = work_items.len();
        let route_summary = if queued_work_items == 0 {
            String::new()
        } else {
            format!(
                "Queued {queued_work_items} parcel work item(s). Workers must be inside the parcel with an active shift to list, claim, or complete them.\r\n"
            )
        };
        let mut events = vec![UiEvent::Text(format!(
            "Parcel request #{} sent to owner {} for parcel {}.\r\nStatus: delivered. Payment and fulfillment are pending owner reply; check /mailbox and /pay requests.\r\n{}{}",
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
                    "[parcel work] {queued_work_items} new item(s) queued for parcel {}.",
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
    if parcel_mailing_list_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list slug"))
    }
}

fn validate_mailing_list_title<E>(title: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if parcel_mailing_list_title_is_valid(title) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list title"))
    }
}

fn validate_mailing_list_subject<E>(subject: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if parcel_mailing_list_subject_is_valid(subject) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list subject"))
    }
}

fn validate_mailing_list_body<E>(body: &str) -> Result<(), E>
where
    E: FromMailingListValidation,
{
    if parcel_mailing_list_body_is_valid(body) {
        Ok(())
    } else {
        Err(E::invalid_mailing_list("invalid mailing-list body"))
    }
}

fn validate_work_desk_slug<E>(slug: &str) -> Result<(), E>
where
    E: FromParcelWorkValidation,
{
    if parcel_work_desk_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(E::invalid_parcel_work("invalid parcel work desk slug"))
    }
}

fn validate_work_desk_title<E>(title: &str) -> Result<(), E>
where
    E: FromParcelWorkValidation,
{
    if parcel_work_desk_title_is_valid(title) {
        Ok(())
    } else {
        Err(E::invalid_parcel_work("invalid parcel work desk title"))
    }
}

fn validate_work_result<E>(result: &str) -> Result<(), E>
where
    E: FromParcelWorkValidation,
{
    if parcel_work_result_is_valid(result) {
        Ok(())
    } else {
        Err(E::invalid_parcel_work("invalid parcel work result"))
    }
}

fn validate_command_route_prefix<E>(command_prefix: &str) -> Result<(), E>
where
    E: FromParcelWorkValidation,
{
    if parcel_command_route_prefix_is_valid(command_prefix) {
        Ok(())
    } else {
        Err(E::invalid_parcel_work(
            "invalid parcel command route prefix",
        ))
    }
}

fn validate_badge_slug<E>(slug: &str) -> Result<(), E>
where
    E: FromParcelBadgeValidation,
{
    if parcel_badge_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(E::invalid_parcel_badge("invalid badge slug"))
    }
}

fn validate_badge_title<E>(title: &str) -> Result<(), E>
where
    E: FromParcelBadgeValidation,
{
    if parcel_badge_title_is_valid(title) {
        Ok(())
    } else {
        Err(E::invalid_parcel_badge("invalid badge title"))
    }
}

fn validate_badge_description<E>(description: Option<&str>) -> Result<(), E>
where
    E: FromParcelBadgeValidation,
{
    if description.is_none_or(parcel_badge_description_is_valid) {
        Ok(())
    } else {
        Err(E::invalid_parcel_badge("invalid badge description"))
    }
}

fn validate_badge_note<E>(note: Option<&str>) -> Result<(), E>
where
    E: FromParcelBadgeValidation,
{
    if note.is_none_or(parcel_badge_note_is_valid) {
        Ok(())
    } else {
        Err(E::invalid_parcel_badge("invalid badge note"))
    }
}

fn render_parcel_mailing_lists(parcel_id: &str, lists: &[impl ParcelMailingListView]) -> String {
    let mut lines = vec![format!("Parcel Chats for {parcel_id}")];
    if lists.is_empty() {
        lines.push(
            "No parcel chats. Create one with /parcel mailing-list create <parcel> <slug> <title>."
                .to_owned(),
        );
    } else {
        for list in lists {
            lines.push(format!(
                "- {} [{}] {} members={} created={}. Join: /parcel subscribe {} {}. Post: /parcel chat {} {} -- <message>",
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

fn render_parcel_mailing_list_subscribers(
    parcel_id: &str,
    slug: &str,
    total: i64,
    subscribers: &[impl ParcelMailingListSubscriberView],
) -> String {
    let mut lines = vec![format!(
        "Parcel Chat Members for {parcel_id} {slug}: {total} active"
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

fn render_parcel_mailing_list_subscriptions(
    subscriptions: &[impl ParcelMailingListSubscriptionView],
) -> String {
    let mut lines = vec!["Parcel Chat Memberships".to_owned()];
    if subscriptions.is_empty() {
        lines.push("No active parcel chats.".to_owned());
    } else {
        for subscription in subscriptions {
            let parcel = subscription
                .parcel_title()
                .unwrap_or(subscription.parcel_id());
            lines.push(format!(
                "- {} / {} ({}) status={} updated={}. Post: /parcel chat {} {} -- <message>. Unsubscribe: /parcel unsubscribe {} {}",
                parcel,
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

fn render_parcel_work_desks(parcel_id: &str, desks: &[impl ParcelWorkDeskView]) -> String {
    let mut lines = vec![format!("Parcel Work Desks for {parcel_id}")];
    if desks.is_empty() {
        lines.push(
            "No work desks. Create one with /parcel desk create <parcel> <slug> <title>."
                .to_owned(),
        );
    } else {
        for desk in desks {
            lines.push(format!(
                "- {} [{}] {} queued={} active_workers={} created={}. Route: /parcel route add {} {} <command-prefix>",
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

fn render_parcel_staff(parcel_id: &str, slug: &str, staff: &[impl ParcelStaffView]) -> String {
    let mut lines = vec![format!("Parcel Staff for {parcel_id} {slug}")];
    if staff.is_empty() {
        lines.push(
            "No assigned staff. Add one with /parcel staff add <parcel> <slug> <username>."
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

fn render_parcel_work_items(
    parcel_id: &str,
    slug: Option<&str>,
    items: &[impl ParcelWorkItemView],
) -> String {
    let scope = slug
        .map(|slug| format!("{parcel_id} {slug}"))
        .unwrap_or_else(|| parcel_id.to_owned());
    let mut lines = vec![format!("Parcel Work for {scope}")];
    if items.is_empty() {
        lines.push(
            "No visible work. Start a shift in the parcel, then wait for routed commands."
                .to_owned(),
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
                "- #{} [{}] {} ({}) command=#{} prefix={} from={}{}{} updated={}. Claim: /parcel work claim {} {}",
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

fn render_parcel_command_routes(parcel_id: &str, routes: &[impl ParcelCommandRouteView]) -> String {
    let mut lines = vec![format!("Parcel Command Routes for {parcel_id}")];
    if routes.is_empty() {
        lines.push(
            "No command routes. Create one with /parcel route add <parcel> <desk-slug> <command-prefix>."
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

fn render_parcel_badges(parcel_id: &str, badges: &[impl ParcelBadgeDefinitionView]) -> String {
    let mut lines = vec![format!("Parcel Badges for {parcel_id}")];
    if badges.is_empty() {
        lines.push(
            "No badges. Create one with /parcel badge create <parcel> <slug> <title> [-- description]."
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

fn render_badge_awards(owner_label: &str, awards: &[impl ParcelBadgeAwardView]) -> String {
    let mut lines = vec![format!("Badges for {owner_label}")];
    if awards.is_empty() {
        lines.push("No active badges.".to_owned());
    } else {
        for award in awards {
            let parcel = award.parcel_title().unwrap_or(award.parcel_id());
            let note = award
                .note()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!(" Note: {value}"))
                .unwrap_or_default();
            lines.push(format!(
                "- {} ({}) from {} [{}], issued by {} at {}.{}",
                award.badge_title(),
                award.slug(),
                parcel,
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
    S: ParcelRegistryStore<Error = E>,
{
    /// Builds the parcel list text.
    pub async fn parcel_list(&self) -> Result<BusinessListResult, E> {
        let parcels = self.store.list_parcels().await?;
        Ok(BusinessListResult {
            text: render_parcel_list(&parcels).replace('\n', "\r\n"),
        })
    }
}

impl<S, E> AppService<S>
where
    S: ParcelOwnershipStore<Error = E>,
{
    /// Builds the parcel detail text.
    pub async fn parcel_info(&self, parcel_id: &str) -> Result<BusinessListResult, E> {
        let parcel = self.store.parcel_by_id(parcel_id).await?;
        Ok(BusinessListResult {
            text: render_parcel_detail(&parcel).replace('\n', "\r\n"),
        })
    }

    /// Claims a parcel and creates the room mailbox token.
    pub async fn claim_parcel(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<ParcelOwnershipResult, E> {
        let parcel = self
            .store
            .claim_parcel(parcel_id, owner_user, owner_player_id)
            .await?;
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, owner_player_id, token)
            .await?;
        Ok(ParcelOwnershipResult {
            text: format!(
                "Claimed parcel {}. Room mail account: {}. Token: {}\r\nUse this token from the room owner process with SMTP/IMAP. You can rotate it later with /parcel token {}.\r\nBuild here with /parcel build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /parcel build publish. From the street, enter with /enter {}. Custom commands are auto-filled if omitted.\r\n",
                parcel.parcel_id(),
                mail.username(),
                token,
                parcel.parcel_id(),
                parcel.parcel_id()
            ),
            invalidate: Some(ParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Transfers a parcel and rotates its room mailbox token for the new owner.
    pub async fn transfer_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
        token: &str,
    ) -> Result<ParcelOwnershipResult, E> {
        let parcel = self
            .store
            .transfer_parcel(parcel_id, owner_player_id, target)
            .await?;
        let new_owner_player_id = parcel.owner_player_id().unwrap_or_default();
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, new_owner_player_id, token)
            .await?;
        Ok(ParcelOwnershipResult {
            text: format!(
                "Transferred parcel {} to {}.\r\nNew room mail account: {}. Token: {}\r\nGive this token to the new room owner process; the old room token has been rotated.\r\n",
                parcel.parcel_id(),
                parcel.owner_user().unwrap_or("unknown"),
                mail.username(),
                token
            ),
            invalidate: Some(ParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Rotates a parcel room mailbox token.
    pub async fn rotate_parcel_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<ParcelOwnershipResult, E> {
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, owner_player_id, token)
            .await?;
        Ok(ParcelOwnershipResult {
            text: format!(
                "Room mail account for {}: {}\r\nToken: {}\r\nUse SMTP/IMAP with this username/token. This token is shown once; run /parcel token {} again to rotate it.\r\n",
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
    S: BuildStore<Error = E>,
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
            invalidate: latest_parcel.map(|parcel| ParcelCacheInvalidation {
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
            invalidate: Some(ParcelCacheInvalidation {
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
                "Published parcel {} as a built parcel.\r\n",
                parcel.parcel_id()
            ),
            invalidate: Some(ParcelCacheInvalidation {
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
                    "\n=== Payment Request #{} ===\nParcel: {} ({})\nAmount: {} {}\nFor: parcel command #{}\nDelivery: locked until payment\nAccept: /pay accept {}\nReject: ignore this request\n==========================",
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
