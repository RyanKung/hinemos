use crate::*;

/// Context needed to route a command in the current rendered view.
#[derive(Debug, Clone, Copy)]
pub struct AppViewCommandContext<'a, B> {
    /// Current rendered view id.
    pub current_view: &'a str,
    /// Current rendered view title.
    pub current_title: &'a str,
    /// Current player inventory.
    pub inventory: &'a [String],
    /// Usernames currently visible in this view.
    pub online_users: &'a [String],
    /// Entity ids visible in the current observation.
    pub visible_entity_ids: &'a [String],
    /// Optional active room binding for this view.
    pub room_binding: Option<&'a B>,
    /// Optional configured mail domain.
    pub mail_domain: Option<&'a str>,
    /// Context for business commands.
    pub business: AppCommandContext<'a>,
}

impl<S, E> AppService<S>
where
    S: ShopStore<Error = E>,
{
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
}

impl<S, E> AppService<S>
where
    S: AccountStore<Error = E>
        + AdmissionStore<Error = E>
        + BuildStore<Error = E>
        + InboxStore<Error = E>
        + LandStore<Error = E>
        + MailStore<Error = E>
        + MemoryStore<Error = E>
        + MessageStore<Error = E>
        + ParcelStore<Error = E>
        + PaymentStore<Error = E>
        + RoomStore<Error = E>
        + ShopStore<Error = E>,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
    <S as RoomStore>::RoomBinding: RoomBindingEntryView
        + ParcelView
        + RoomBindingKindView
        + RoomCommandPolicyView
        + RoomMailboxView
        + ServiceRoomView
        + Sync,
{
    /// Handles a protocol-neutral app request and returns UI events.
    pub async fn handle(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            request @ (AppRequest::MemoryContext
            | AppRequest::MemoryCommand { .. }
            | AppRequest::RoomHistory { .. }
            | AppRequest::Inventory { .. }
            | AppRequest::Who { .. }
            | AppRequest::News
            | AppRequest::Balance) => self.handle_read_request(identity, request).await,
            request @ (AppRequest::InboxList { .. }
            | AppRequest::InboxRead { .. }
            | AppRequest::InboxClaim { .. }
            | AppRequest::InboxAck { .. }
            | AppRequest::InboxArchive { .. }) => {
                self.handle_inbox_request(identity, request).await
            }
            request @ (AppRequest::PendingPayRequests
            | AppRequest::PayDirect { .. }
            | AppRequest::PayAccept { .. }) => self.handle_payment_request(identity, request).await,
            request @ (AppRequest::Say { .. }
            | AppRequest::Mail { .. }
            | AppRequest::Broadcast { .. }) => self.handle_message_request(identity, request).await,
            request @ (AppRequest::LandList
            | AppRequest::LandInfo { .. }
            | AppRequest::LandClaim { .. }
            | AppRequest::LandTransfer { .. }
            | AppRequest::LandRotateToken { .. }) => {
                self.handle_land_request(identity, request).await
            }
            request @ (AppRequest::BuildHelp
            | AppRequest::BuildApply { .. }
            | AppRequest::BuildSet { .. }
            | AppRequest::BuildPublish { .. }) => {
                self.handle_build_request(identity, request).await
            }
            request @ (AppRequest::ShopInbox | AppRequest::ShopRequestPayment { .. }) => {
                self.handle_shop_request(identity, request).await
            }
            request @ (AppRequest::ServiceRoomInput { .. }
            | AppRequest::ServiceRoomHelp { .. }
            | AppRequest::ServiceRoomObservation { .. }
            | AppRequest::ServiceRoomBlockedExit
            | AppRequest::ServiceRoomUnavailable
            | AppRequest::ServiceRoomQuit { .. }) => {
                self.handle_service_room_request(identity, request).await
            }
            request @ (AppRequest::AdmissionRead | AppRequest::AdmissionAccept) => {
                self.handle_admission_request(identity, request).await
            }
            request
            @ (AppRequest::Settings { .. } | AppRequest::SettingsRotateMailToken { .. }) => {
                self.handle_settings_request(identity, request).await
            }
        }
    }

    async fn handle_read_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let text = match request {
            AppRequest::MemoryContext => self.memory_context(&identity.player_id).await?.text,
            AppRequest::MemoryCommand { rest } => {
                self.memory_command(&identity.player_id, rest).await?.text
            }
            AppRequest::RoomHistory {
                current_view,
                title,
            } => self.room_history(current_view, title).await?.text,
            AppRequest::Inventory { items } => render_inventory(items),
            AppRequest::Who {
                current_view,
                users,
            } => render_who(current_view, users),
            AppRequest::News => self.world_news().await?.text,
            AppRequest::Balance => {
                render_player_balance(self.store.player_balance(&identity.player_id).await?)
            }
            _ => unreachable!("request was pre-classified as read-only"),
        };
        Ok(text_events(text, None))
    }

    async fn handle_inbox_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AppRequest::InboxList {
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
            AppRequest::InboxRead {
                item_id,
                mail_domain,
            } => Ok(text_events(
                self.read_inbox(&identity.user, &identity.player_id, item_id, mail_domain)
                    .await?
                    .text,
                None,
            )),
            AppRequest::InboxClaim { item_id } => {
                self.handle_inbox_mutation(identity, item_id, InboxMutation::Claim)
                    .await
            }
            AppRequest::InboxAck { item_id } => {
                self.handle_inbox_mutation(identity, item_id, InboxMutation::Ack)
                    .await
            }
            AppRequest::InboxArchive { item_id } => {
                self.handle_inbox_mutation(identity, item_id, InboxMutation::Archive)
                    .await
            }
            _ => unreachable!("request was pre-classified as inbox"),
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

    async fn handle_payment_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AppRequest::PendingPayRequests => Ok(text_events(
                self.pending_pay_requests(&identity.player_id).await?.text,
                None,
            )),
            AppRequest::PayDirect {
                target,
                amount,
                memo,
            } => self.handle_pay_direct(identity, target, amount, memo).await,
            AppRequest::PayAccept { request_id } => {
                self.handle_pay_accept(identity, request_id).await
            }
            _ => unreachable!("request was pre-classified as payment"),
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

    async fn handle_message_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AppRequest::Say { current_view, text } => {
                self.handle_say(identity, current_view, text).await
            }
            AppRequest::Mail { target, text } => self.handle_mail(identity, target, text).await,
            AppRequest::Broadcast { text } => self.handle_broadcast(identity, text).await,
            _ => unreachable!("request was pre-classified as message"),
        }
    }

    async fn handle_land_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let (text, cache) = match request {
            AppRequest::LandList => (self.land_list().await?.text, None),
            AppRequest::LandInfo { parcel_id } => (self.land_info(parcel_id).await?.text, None),
            AppRequest::LandClaim { parcel_id, token } => {
                let result = self
                    .claim_land(parcel_id, &identity.user, &identity.player_id, token)
                    .await?;
                (result.text, result.invalidate)
            }
            AppRequest::LandTransfer {
                parcel_id,
                target,
                token,
            } => {
                let result = self
                    .transfer_land(parcel_id, &identity.player_id, target, token)
                    .await?;
                (result.text, result.invalidate)
            }
            AppRequest::LandRotateToken { parcel_id, token } => {
                let result = self
                    .rotate_land_token(parcel_id, &identity.player_id, token)
                    .await?;
                (result.text, None)
            }
            _ => unreachable!("request was pre-classified as land"),
        };
        Ok(text_events(text, cache.map(commercial_parcel_cache_event)))
    }

    async fn handle_build_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let result = match request {
            AppRequest::BuildHelp => {
                return Ok(text_events(self.build_help_text().to_owned(), None));
            }
            AppRequest::BuildApply {
                current_view,
                sheet,
            } => {
                self.apply_build_sheet(current_view, &identity.player_id, sheet)
                    .await?
            }
            AppRequest::BuildSet {
                current_view,
                field,
                value,
            } => {
                self.set_build_field(current_view, &identity.player_id, field, value)
                    .await?
            }
            AppRequest::BuildPublish { current_view } => {
                self.publish_build(current_view, &identity.player_id)
                    .await?
            }
            _ => unreachable!("request was pre-classified as build"),
        };
        Ok(text_events(
            result.text,
            result.invalidate.map(commercial_parcel_cache_event),
        ))
    }

    async fn handle_shop_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AppRequest::ShopInbox => Ok(text_events(
                self.shop_inbox(&identity.player_id).await?.text,
                None,
            )),
            AppRequest::ShopRequestPayment {
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
            _ => unreachable!("request was pre-classified as shop"),
        }
    }

    async fn handle_service_room_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AppRequest::ServiceRoomInput {
                room_view,
                raw_input,
            } => {
                self.handle_service_room_input(identity, room_view, raw_input)
                    .await
            }
            AppRequest::ServiceRoomHelp { room_view } => {
                let Some(room) = self.service_room_binding_by_view(room_view).await? else {
                    return Ok(text_events(
                        service_room_unavailable_text().to_owned(),
                        None,
                    ));
                };
                Ok(text_events(self.service_room_help_text(&room), None))
            }
            AppRequest::ServiceRoomObservation { room_view } => {
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
            AppRequest::ServiceRoomBlockedExit => Ok(text_events(
                service_room_blocked_exit_text().to_owned(),
                None,
            )),
            AppRequest::ServiceRoomUnavailable => Ok(text_events(
                service_room_unavailable_text().to_owned(),
                None,
            )),
            AppRequest::ServiceRoomQuit { feedback } => Ok(vec![
                UiEvent::Text(format!("{feedback}\r\n")),
                UiEvent::CloseSession(0),
            ]),
            _ => unreachable!("request was pre-classified as service room"),
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

    async fn handle_admission_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AppRequest::AdmissionRead => {
                self.handle_pending_admission_read(&identity.player_id)
                    .await
            }
            AppRequest::AdmissionAccept => {
                match self.accept_admission(&identity.player_id).await? {
                    AdmissionAcceptResult::AlreadyAgreed { text }
                    | AdmissionAcceptResult::NeedsRead { text } => Ok(text_events(text, None)),
                    AdmissionAcceptResult::Accepted => Ok(vec![UiEvent::EnsureWalletAndEnter {
                        user: identity.user.clone(),
                        player_id: identity.player_id.clone(),
                        agreement_version: self.config.agreement_version.clone(),
                        target_view: self.config.admission_view_id.clone(),
                    }]),
                }
            }
            _ => unreachable!("request was pre-classified as admission"),
        }
    }

    async fn handle_settings_request(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let text = match request {
            AppRequest::Settings { mail_address } => {
                self.show_account_settings(&identity.user, &identity.player_id, mail_address)
                    .await?
                    .text
            }
            AppRequest::SettingsRotateMailToken {
                mail_address,
                token,
            } => {
                self.rotate_user_mail_token(
                    &identity.user,
                    &identity.player_id,
                    mail_address,
                    token,
                )
                .await?
                .text
            }
            _ => unreachable!("request was pre-classified as settings"),
        };
        Ok(text_events(text, None))
    }

    /// Handles semantic business commands that do not need core runtime execution.
    pub async fn handle_semantic_business_command(
        &self,
        identity: &AppIdentity,
        command: &SemanticCommand,
        context: AppCommandContext<'_>,
    ) -> Result<Option<Vec<UiEvent>>, E> {
        let mail_address = format_mail_user(&identity.user, context.mail_domain);
        let request = match command {
            SemanticCommand::Settings {
                action: SettingsAction::Show,
            } => AppRequest::Settings {
                mail_address: &mail_address,
            },
            SemanticCommand::Settings {
                action: SettingsAction::MailToken,
            } => AppRequest::SettingsRotateMailToken {
                mail_address: &mail_address,
                token: context.generated_token,
            },
            SemanticCommand::Pay { action } => payment_request(action),
            SemanticCommand::Inbox { action } => inbox_request(action, context.mail_domain),
            SemanticCommand::Land { action } => land_request(action, context.generated_token),
            SemanticCommand::Build { action } => build_request(action, context.current_view),
            SemanticCommand::Shop { action } => shop_request(action),
            _ => return Ok(None),
        };
        Ok(Some(self.handle(identity, request).await?))
    }

    /// Handles semantic commands that are routed from the current room view.
    pub async fn handle_world_view_command(
        &self,
        command: &SemanticCommand,
        player_id: &str,
        current_view: &str,
        current_title: &str,
        inventory: &[String],
        online_users: &[String],
    ) -> Result<Option<Vec<UiEvent>>, E> {
        let events = match command {
            SemanticCommand::Inventory => Some(text_events(render_inventory(inventory), None)),
            SemanticCommand::History => Some(text_events(
                self.room_history(current_view, current_title).await?.text,
                None,
            )),
            SemanticCommand::Who => Some(text_events(render_who(current_view, online_users), None)),
            SemanticCommand::News => Some(text_events(self.world_news().await?.text, None)),
            SemanticCommand::Balance => Some(text_events(
                render_player_balance(self.store.player_balance(player_id).await?),
                None,
            )),
            _ => None,
        };
        Ok(events)
    }

    /// Handles commands routed from the current room or page view.
    pub async fn handle_view_command(
        &self,
        identity: &AppIdentity,
        command: &SemanticCommand,
        context: AppViewCommandContext<'_, <S as RoomStore>::RoomBinding>,
    ) -> Result<Option<Vec<UiEvent>>, E> {
        match command {
            SemanticCommand::Mailbox => {
                let request = AppRequest::InboxList {
                    title: "Mailbox",
                    filter: "open",
                    mail_domain: context.mail_domain,
                };
                return Ok(Some(self.handle(identity, request).await?));
            }
            SemanticCommand::Enter { target } => {
                let bindings = self
                    .store
                    .room_bindings_by_front_view(context.current_view)
                    .await?;
                return Ok(self.visible_room_enter_events(
                    target,
                    context.visible_entity_ids,
                    &bindings,
                ));
            }
            SemanticCommand::Inventory
            | SemanticCommand::History
            | SemanticCommand::Who
            | SemanticCommand::News
            | SemanticCommand::Balance => {
                return self
                    .handle_world_view_command(
                        command,
                        &identity.player_id,
                        context.current_view,
                        context.current_title,
                        context.inventory,
                        context.online_users,
                    )
                    .await;
            }
            _ if context
                .room_binding
                .is_some_and(RoomBindingKindView::is_service_room) =>
            {
                if let Some(binding) = context.room_binding {
                    let events = self
                        .handle_service_room_command_for_binding(
                            identity,
                            context.current_view,
                            binding,
                            command,
                        )
                        .await?;
                    return Ok(Some(events));
                }
            }
            _ => {}
        }
        self.handle_semantic_business_command(identity, command, context.business)
            .await
    }

    /// Handles a raw `/memory` command line if it matches the memory command namespace.
    pub async fn handle_memory_raw_line(
        &self,
        identity: &AppIdentity,
        line: &str,
    ) -> Result<Option<Vec<UiEvent>>, E> {
        let Some(rest) = memory_command_rest(line) else {
            return Ok(None);
        };
        Ok(Some(
            self.handle(identity, AppRequest::MemoryCommand { rest: rest.trim() })
                .await?,
        ))
    }

    /// Rejects free text while admission is pending.
    pub async fn pending_admission_free_text(
        &self,
        identity: &AppIdentity,
    ) -> Result<Option<Vec<UiEvent>>, E> {
        let admission = self.player_admission(&identity.player_id).await?;
        if admission.is_agreed() {
            return Ok(None);
        }
        Ok(Some(text_events(
            format!(
                "{}\r\n",
                self.admission_guidance(&admission).replace('\n', "\r\n")
            ),
            None,
        )))
    }

    /// Handles commands while admission is pending.
    pub async fn handle_pending_admission_command(
        &self,
        identity: &AppIdentity,
        command: &SemanticCommand,
    ) -> Result<PendingAdmissionCommandOutcome, E> {
        let admission = self.player_admission(&identity.player_id).await?;
        if admission.is_agreed() {
            return Ok(PendingAdmissionCommandOutcome::NotPending);
        }

        match command {
            SemanticCommand::Look | SemanticCommand::Help | SemanticCommand::Quit => {
                Ok(PendingAdmissionCommandOutcome::Allow(Vec::new()))
            }
            SemanticCommand::Read { target }
                if target.id == self.config.admission_board_entity_id =>
            {
                let events = self
                    .handle_pending_admission_read(&identity.player_id)
                    .await?;
                Ok(PendingAdmissionCommandOutcome::Allow(events))
            }
            SemanticCommand::Agree { .. } => {
                let events = self.handle(identity, AppRequest::AdmissionAccept).await?;
                Ok(PendingAdmissionCommandOutcome::Block(events))
            }
            _ => Ok(PendingAdmissionCommandOutcome::Block(text_events(
                format!(
                    "{}\r\n",
                    self.admission_guidance(&admission).replace('\n', "\r\n")
                ),
                None,
            ))),
        }
    }

    /// Restricts an observation to admission-safe commands for a player when needed.
    pub async fn restrict_pending_admission_observation_for_player(
        &self,
        observation: &mut JsonObservation,
        player_id: &str,
    ) -> Result<bool, E> {
        let admission = self.player_admission(player_id).await?;
        if admission.is_agreed() {
            return Ok(false);
        }
        self.restrict_pending_admission_observation(
            observation,
            &admission,
            &self.config.admission_board_entity_id,
        );
        Ok(true)
    }
}

#[derive(Debug, Clone, Copy)]
enum InboxMutation {
    Claim,
    Ack,
    Archive,
}

fn text_events(text: String, extra: Option<UiEvent>) -> Vec<UiEvent> {
    let mut events = vec![UiEvent::Text(text)];
    if let Some(event) = extra {
        events.push(event);
    }
    events
}

fn commercial_parcel_cache_event(cache: CommercialParcelCacheInvalidation) -> UiEvent {
    UiEvent::InvalidateCommercialParcelCache {
        view_id: cache.view_id,
        front_view_id: cache.front_view_id,
    }
}

fn payment_request(action: &PayAction) -> AppRequest<'_> {
    match action {
        PayAction::Direct {
            target,
            amount,
            memo,
        } => AppRequest::PayDirect {
            target,
            amount: *amount,
            memo,
        },
        PayAction::Requests => AppRequest::PendingPayRequests,
        PayAction::Accept { request_id } => AppRequest::PayAccept {
            request_id: *request_id,
        },
    }
}

fn inbox_request<'a>(action: &'a InboxAction, mail_domain: Option<&'a str>) -> AppRequest<'a> {
    match action {
        InboxAction::List { filter } => AppRequest::InboxList {
            title: "Inbox",
            filter,
            mail_domain,
        },
        InboxAction::Read { item_id } => AppRequest::InboxRead {
            item_id: *item_id,
            mail_domain,
        },
        InboxAction::Claim { item_id } => AppRequest::InboxClaim { item_id: *item_id },
        InboxAction::Ack { item_id } => AppRequest::InboxAck { item_id: *item_id },
        InboxAction::Archive { item_id } => AppRequest::InboxArchive { item_id: *item_id },
    }
}

fn land_request<'a>(action: &'a LandAction, token: &'a str) -> AppRequest<'a> {
    match action {
        LandAction::List => AppRequest::LandList,
        LandAction::Info { parcel_id } => AppRequest::LandInfo { parcel_id },
        LandAction::Claim { parcel_id } => AppRequest::LandClaim { parcel_id, token },
        LandAction::Transfer { parcel_id, target } => AppRequest::LandTransfer {
            parcel_id,
            target,
            token,
        },
        LandAction::Token { parcel_id } => AppRequest::LandRotateToken { parcel_id, token },
    }
}

fn build_request<'a>(action: &'a BuildAction, current_view: &'a str) -> AppRequest<'a> {
    match action {
        BuildAction::Help => AppRequest::BuildHelp,
        BuildAction::Apply { sheet } => AppRequest::BuildApply {
            current_view,
            sheet,
        },
        BuildAction::Set { field, value } => AppRequest::BuildSet {
            current_view,
            field,
            value,
        },
        BuildAction::Publish => AppRequest::BuildPublish { current_view },
    }
}

fn shop_request(action: &ShopAction) -> AppRequest<'_> {
    match action {
        ShopAction::Inbox => AppRequest::ShopInbox,
        ShopAction::RequestPayment {
            command_id,
            amount,
            delivery,
        } => AppRequest::ShopRequestPayment {
            command_id: *command_id,
            amount: *amount,
            delivery,
        },
    }
}
