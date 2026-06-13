use super::handler_helpers::*;
use super::*;
use hinemos_app::{AppIdentity, AppRequest, LiveInboxNotice, UiEvent};
use hinemos_core::{JsonObservation, PlayerState, SemanticCommand};

impl ConnectionHandler {
    pub(super) async fn send_app_request(
        &self,
        channel: ChannelId,
        session: &mut Session,
        identity: &AuthIdentity,
        request: AppRequest<'_>,
    ) -> Result<()> {
        let events = self.app_request_events(identity, request).await?;
        self.send_ui_events(channel, session, events).await
    }

    pub(super) async fn app_request_events(
        &self,
        identity: &AuthIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>> {
        let app = self.shared.app_service().await;
        let app_identity = AppIdentity::new(identity.user.clone(), identity.player_id.clone());
        Ok(app.handle(&app_identity, request).await?)
    }

    pub(super) async fn send_ui_events(
        &self,
        channel: ChannelId,
        session: &mut Session,
        events: Vec<UiEvent>,
    ) -> Result<()> {
        for event in events {
            self.send_ui_event(channel, session, event).await?;
        }
        Ok(())
    }

    async fn send_ui_event(
        &self,
        channel: ChannelId,
        session: &mut Session,
        event: UiEvent,
    ) -> Result<()> {
        match event {
            UiEvent::Text(text) => send_text_event(session, channel, &text)?,
            UiEvent::Observation(observation) => {
                self.send_text_observation(channel, session, observation)
                    .await?;
            }
            UiEvent::CommandObservation {
                command,
                observation,
            } => {
                self.send_command_observation(channel, session, &command, observation)
                    .await?;
            }
            UiEvent::PersistPlayerState(player_state) => {
                self.persist_player_state_event(player_state).await?;
            }
            UiEvent::Prompt => {
                send_prompt(session, channel)?;
            }
            UiEvent::CloseSession(status) => {
                close_session_event(session, channel, status)?;
            }
            UiEvent::InvalidateRoomCache => {
                self.shared.clear_room_cache().await;
            }
            UiEvent::InvalidateCommercialParcelCache {
                view_id,
                front_view_id,
            } => {
                self.shared
                    .invalidate_room_cache_for_commercial_parcel(&view_id, &front_view_id)
                    .await;
            }
            UiEvent::InvalidateInboxItem { item_id } => {
                self.shared.invalidate_inbox_item(item_id).await;
            }
            UiEvent::LiveMessage {
                target_player_id,
                text,
            } => {
                self.send_live_message_to_player(&target_player_id, &text)
                    .await;
            }
            UiEvent::LiveViewMessage { view_id, text } => {
                self.send_live_message_to_view(&view_id, &text).await;
            }
            UiEvent::LiveInboxNotice {
                target_player_id,
                notice,
            } => {
                self.send_live_inbox_notice(&target_player_id, &notice)
                    .await;
            }
            UiEvent::EnsureWalletAndEnter {
                user,
                player_id,
                agreement_version,
                target_view,
            } => {
                self.ensure_wallet_and_enter(
                    channel,
                    session,
                    &user,
                    &player_id,
                    &agreement_version,
                    &target_view,
                )
                .await?;
            }
            UiEvent::Relocate {
                target_view,
                direction,
                message,
            } => {
                self.send_relocate_event(
                    channel,
                    session,
                    &target_view,
                    direction,
                    message.as_deref(),
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn persist_player_state_event(&self, player_state: PlayerState) -> Result<()> {
        let app = self.shared.app_service().await;
        app.save_player_state(&player_state).await?;
        self.shared
            .presence
            .lock()
            .await
            .update_view(self.connection_id, player_state.current_view);
        Ok(())
    }

    async fn send_live_message_to_player(&self, player_id: &str, text: &str) {
        let recipients = self
            .shared
            .presence
            .lock()
            .await
            .direct_recipients(self.connection_id, player_id);
        if !recipients.is_empty() {
            deliver_live_message(recipients, text).await;
        }
    }

    async fn send_live_message_to_view(&self, view_id: &str, text: &str) {
        let recipients = self
            .shared
            .presence
            .lock()
            .await
            .view_recipients(self.connection_id, view_id);
        if !recipients.is_empty() {
            deliver_live_message(recipients, text).await;
        }
    }

    async fn send_live_inbox_notice(&self, target_player_id: &str, notice: &LiveInboxNotice) {
        let recipients = self
            .shared
            .presence
            .lock()
            .await
            .direct_recipients(self.connection_id, target_player_id);
        if recipients.is_empty() {
            return;
        }

        let rendered = render_inbox_notice_fields(
            notice.id,
            &notice.kind,
            &notice.sender_user,
            &notice.subject,
            &notice.body,
            self.shared.mail_domain.as_deref(),
        );
        deliver_live_message(recipients, &rendered).await;
    }

    async fn ensure_wallet_and_enter(
        &self,
        channel: ChannelId,
        session: &mut Session,
        user: &str,
        player_id: &str,
        agreement_version: &str,
        target_view: &str,
    ) -> Result<()> {
        let app = self.shared.app_service().await;
        app.ensure_player_wallet(user, player_id).await?;
        let balance = app.player_balance(player_id).await?;
        session.data(
            channel,
            app.admission_accepted_text(agreement_version, balance.amount, &balance.asset)
                .into_bytes(),
        )?;
        self.send_relocate_event(channel, session, target_view, None, None)
            .await
    }

    async fn send_relocate_event(
        &self,
        channel: ChannelId,
        session: &mut Session,
        target_view: &str,
        direction: Option<Direction>,
        message: Option<&str>,
    ) -> Result<()> {
        let identity = self
            .identity
            .as_ref()
            .context("relocate event requires authenticated identity")?;
        let target_context = self.shared.room_context_for_view(target_view).await?;
        let observation = self
            .relocate_player_id_to_view_with_context(
                &identity.player_id,
                target_view,
                direction,
                message,
                &target_context,
            )
            .await?;
        self.send_text_observation_with_context(channel, session, observation, &target_context)
            .await
    }

    pub(super) async fn send_text_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        observation: JsonObservation,
    ) -> Result<()> {
        let room_context = self
            .shared
            .room_context_for_view(&observation.view_id)
            .await?;
        self.send_text_observation_with_context(channel, session, observation, &room_context)
            .await
    }

    pub(super) async fn send_text_observation_with_context(
        &self,
        channel: ChannelId,
        session: &mut Session,
        mut observation: JsonObservation,
        room_context: &RoomViewContext,
    ) -> Result<()> {
        match room_context {
            RoomViewContext {
                room_binding: Some(room),
                ..
            } if room.is_service_room() => {
                overlay_service_room(&mut observation, room);
            }
            RoomViewContext {
                room_binding: Some(room),
                ..
            } => {
                overlay_parcel_observation(&mut observation, room);
            }
            RoomViewContext {
                service_room: Some(service_room),
                ..
            } => {
                overlay_service_room(&mut observation, service_room);
            }
            RoomViewContext { front_bindings, .. } if !front_bindings.is_empty() => {
                overlay_room_binding_entries(&mut observation, front_bindings);
            }
            _ => {}
        }
        let player_id = observation.player_id.clone();
        let app = self.shared.app_service().await;
        app.restrict_pending_admission_observation_for_player(&mut observation, &player_id)
            .await?;
        let view_users = self
            .shared
            .presence
            .lock()
            .await
            .view_users(self.connection_id, &observation.view_id);
        observation.online_users = render_online_summary(&view_users, 10);
        let app_config = self.shared.app_config().await;
        send_text_observation(
            session,
            channel,
            &observation,
            self.terminal_cols,
            &app_config.admission_view_id,
        )?;
        Ok(())
    }

    pub(super) async fn send_command_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        command: &SemanticCommand,
        observation: JsonObservation,
    ) -> Result<()> {
        if should_render_full_observation(command) {
            self.send_text_observation(channel, session, observation)
                .await?;
        } else {
            send_text_events(session, channel, &observation)?;
        }
        Ok(())
    }

    pub(super) async fn dispatch_live_message(
        &self,
        command: &SemanticCommand,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let app = self.shared.app_service().await;
        let app_identity = AppIdentity::new(identity.user.clone(), identity.player_id.clone());
        let (recipients, message) = match command {
            SemanticCommand::Mail { target, text } => {
                let target = normalize_mail_target(target, self.shared.mail_domain.as_deref())?;
                app.handle(
                    &app_identity,
                    AppRequest::Mail {
                        target: &target,
                        text,
                    },
                )
                .await?;
                return Ok(());
            }
            SemanticCommand::Broadcast { text } => {
                app.handle(&app_identity, AppRequest::Broadcast { text })
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .broadcast_recipients(self.connection_id);
                (
                    recipients,
                    format!("[broadcast from {}] {text}", identity.user),
                )
            }
            _ => return Ok(()),
        };

        deliver_live_message(recipients, &message).await;
        Ok(())
    }

    pub(super) async fn handle_exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        let command = String::from_utf8_lossy(data).trim().to_owned();
        session.channel_success(channel)?;
        if command == "mailbox" {
            self.start_mailbox_channel(channel, session).await?;
        } else if command.is_empty() {
            session.data(channel, exec_help().replace('\n', "\r\n").into_bytes())?;
            session.exit_status_request(channel, 1)?;
            session.close(channel)?;
        } else {
            session.data(
                channel,
                format!(
                    "{}\r\nSSH exec is not a world command interface. Rejected exec command: {command}\r\n",
                    exec_help()
                )
                .into_bytes(),
            )?;
            session.exit_status_request(channel, 1)?;
            session.close(channel)?;
        }
        Ok(())
    }

    pub(super) async fn start_mailbox_channel(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"ERR authentication required\r\n".to_vec())?;
            session.exit_status_request(channel, 1)?;
            session.close(channel)?;
            return Ok(());
        };

        let app = self.shared.app_service().await;
        let admission = app.player_admission(&identity.player_id).await?;
        if !admission.is_agreed() {
            session.data(
                channel,
                format!(
                    "ERR {}\r\n",
                    app.admission_guidance(&admission).replace('\n', " ")
                )
                .into_bytes(),
            )?;
            session.exit_status_request(channel, 1)?;
            session.close(channel)?;
            return Ok(());
        }

        self.mode = Some(ConnectionMode::Mailbox);
        self.shared.presence.lock().await.attach_channel(
            self.connection_id,
            session.handle(),
            channel,
            PresenceDeliveryMode::Mailbox,
        );
        session.data(
            channel,
            format!(
                "OK HINEMOS-MAIL ready user {}\r\n{}\r\n",
                format_mail_user(&identity.user, self.shared.mail_domain.as_deref()),
                mailbox_help()
            )
            .into_bytes(),
        )?;
        Ok(())
    }

    pub(super) async fn handle_terminal_input(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        if self.mode == Some(ConnectionMode::Mailbox) {
            return self.handle_mailbox_input(channel, data, session).await;
        }

        if data == [3] {
            session.close(channel)?;
            return Ok(());
        }

        if self
            .shared
            .presence
            .lock()
            .await
            .poll_kick(self.connection_id)
        {
            session.close(channel)?;
            return Ok(());
        }

        let incoming = String::from_utf8_lossy(data);
        for character in incoming.chars() {
            match character {
                '\r' | '\n' => {
                    if self.discarding_oversized_input {
                        self.discarding_oversized_input = false;
                        self.input_buffer.clear();
                        send_prompt(session, channel)?;
                        continue;
                    }
                    if character == '\n' && self.input_buffer.is_empty() {
                        continue;
                    }
                    session.data(channel, b"\r\n".to_vec())?;
                    let line = std::mem::take(&mut self.input_buffer);
                    self.handle_command_line(channel, line.trim(), session, true)
                        .await?;
                }
                '\u{8}' | '\u{7f}' => {
                    if self.input_buffer.pop().is_some() {
                        session.data(channel, b"\x08 \x08".to_vec())?;
                    }
                }
                _ if character.is_control() => {}
                _ => {
                    if self.discarding_oversized_input {
                        continue;
                    }
                    if self.input_buffer.len() + character.len_utf8() > MAX_SHELL_INPUT_BYTES {
                        self.input_buffer.clear();
                        self.discarding_oversized_input = true;
                        session.data(
                            channel,
                            format!(
                                "\r\nInput is too large; limit is {MAX_SHELL_INPUT_BYTES} bytes.\r\n"
                            )
                            .into_bytes(),
                        )?;
                        continue;
                    }
                    self.input_buffer.push(character);
                    session.data(channel, character.to_string().into_bytes())?;
                }
            }
        }

        Ok(())
    }

    pub(super) async fn handle_mailbox_input(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        if data == [3] {
            session.close(channel)?;
            return Ok(());
        }

        if self
            .shared
            .presence
            .lock()
            .await
            .poll_kick(self.connection_id)
        {
            session.close(channel)?;
            return Ok(());
        }

        let incoming = String::from_utf8_lossy(data);
        for character in incoming.chars() {
            match character {
                '\r' | '\n' => {
                    if character == '\n' && self.input_buffer.is_empty() {
                        continue;
                    }
                    let line = std::mem::take(&mut self.input_buffer);
                    self.handle_mailbox_line(channel, line.trim(), session)
                        .await?;
                }
                _ if character.is_control() => {}
                _ => self.input_buffer.push(character),
            }
        }
        Ok(())
    }

    pub(super) async fn handle_mailbox_line(
        &mut self,
        channel: ChannelId,
        line: &str,
        session: &mut Session,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"ERR authentication required\r\n".to_vec())?;
            return Ok(());
        };
        self.shared.presence.lock().await.touch(self.connection_id);
        if line.is_empty() {
            return Ok(());
        }

        let (command, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(command, rest)| (command, rest.trim()));
        let app = self.shared.app_service().await;
        let app_identity = AppIdentity::new(identity.user.clone(), identity.player_id.clone());
        match command.to_ascii_uppercase().as_str() {
            "HELP" => {
                session.data(channel, format!("{}\r\n", mailbox_help()).into_bytes())?;
            }
            "NOOP" => {
                session.data(channel, b"OK NOOP\r\n".to_vec())?;
            }
            "IDLE" => {
                session.data(
                    channel,
                    b"+ IDLE active; new inbox items are pushed as * NEWMAIL\r\n".to_vec(),
                )?;
            }
            "LIST" => {
                let filter = if rest.is_empty() { "open" } else { rest };
                let events = app
                    .handle(
                        &app_identity,
                        AppRequest::InboxList {
                            title: "Mailbox",
                            filter,
                            mail_domain: self.shared.mail_domain.as_deref(),
                        },
                    )
                    .await?;
                self.send_ui_events(channel, session, events).await?;
            }
            "READ" => {
                let item_id = parse_mailbox_item_id(rest)?;
                let events = app
                    .handle(
                        &app_identity,
                        AppRequest::InboxRead {
                            item_id,
                            mail_domain: self.shared.mail_domain.as_deref(),
                        },
                    )
                    .await?;
                self.send_ui_events(channel, session, events).await?;
            }
            "ACK" => {
                let item_id = parse_mailbox_item_id(rest)?;
                let events = app
                    .handle(&app_identity, AppRequest::InboxAck { item_id })
                    .await?;
                self.send_ui_events(channel, session, events).await?;
            }
            "SEND" => {
                let Some((target, body)) = rest.split_once(char::is_whitespace) else {
                    session.data(
                        channel,
                        b"ERR usage: SEND <user-or-address> <body>\r\n".to_vec(),
                    )?;
                    return Ok(());
                };
                let target = normalize_mail_target(target, self.shared.mail_domain.as_deref())?;
                app.handle_mail(&app_identity, &target, body.trim()).await?;
                session.data(
                    channel,
                    format!(
                        "OK SEND TO {}\r\n",
                        format_mail_user(&target, self.shared.mail_domain.as_deref())
                    )
                    .into_bytes(),
                )?;
            }
            "QUIT" => {
                session.data(channel, b"OK goodbye\r\n".to_vec())?;
                session.exit_status_request(channel, 0)?;
                session.close(channel)?;
            }
            _ => {
                session.data(channel, b"ERR unknown mailbox command\r\n".to_vec())?;
            }
        }
        Ok(())
    }
}

fn send_text_event(session: &mut Session, channel: ChannelId, text: &str) -> Result<()> {
    session.data(
        channel,
        text.replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .into_bytes(),
    )?;
    Ok(())
}

fn close_session_event(session: &mut Session, channel: ChannelId, status: i32) -> Result<()> {
    session.exit_status_request(channel, status as u32)?;
    session.close(channel)?;
    Ok(())
}
