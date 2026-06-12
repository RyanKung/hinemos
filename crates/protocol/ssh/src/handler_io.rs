use super::handler_helpers::*;
use super::*;

impl ConnectionHandler {
    pub(super) async fn send_text_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        mut observation: JsonObservation,
    ) -> Result<()> {
        let parcels = self.shared.storage.list_commercial_parcels().await?;
        if let Some(parcel) = self
            .shared
            .storage
            .commercial_parcel_by_view(&observation.view_id)
            .await?
        {
            overlay_parcel_observation(&mut observation, &parcel);
        } else {
            let visible = visible_street_parcels(&observation.view_id, &parcels);
            overlay_street_parcels(&mut observation, &visible);
            let rooms = self
                .shared
                .storage
                .service_rooms_by_front_view(&observation.view_id)
                .await?;
            overlay_service_room_entries(&mut observation, &rooms);
        }
        let player_id = observation.player_id.clone();
        if let Some(room) = self
            .shared
            .storage
            .service_room_by_view(&observation.view_id)
            .await?
        {
            overlay_service_room(&mut observation, &room);
        }
        let admission = self.shared.storage.player_admission(&player_id).await?;
        if !admission.is_agreed() {
            restrict_pending_admission_observation(&mut observation, &admission);
        }
        let view_users = self
            .shared
            .presence
            .lock()
            .await
            .view_users(self.connection_id, &observation.view_id);
        observation.online_users = render_online_summary(&view_users, 10);
        send_text_observation(session, channel, &observation, self.terminal_cols)?;
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
        let (recipients, message) = match command {
            SemanticCommand::Say { text } => {
                let player = self
                    .shared
                    .runtime
                    .player_state(&identity.player_id)
                    .await?;
                self.shared
                    .storage
                    .save_say_message(
                        &identity.user,
                        &identity.player_id,
                        &player.current_view,
                        text,
                    )
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .view_recipients(self.connection_id, &player.current_view);
                (recipients, format!("[say from {}] {text}", identity.user))
            }
            SemanticCommand::Mail { target, text } => {
                let target = normalize_mail_target(target, self.shared.mail_domain.as_deref())?;
                self.shared
                    .storage
                    .save_mail_message(&identity.user, &identity.player_id, &target, text)
                    .await?;
                return Ok(());
            }
            SemanticCommand::Broadcast { text } => {
                self.shared
                    .storage
                    .save_broadcast_message(&identity.user, &identity.player_id, text)
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

        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if !admission.is_agreed() {
            session.data(
                channel,
                format!(
                    "ERR {}\r\n",
                    admission_guidance(&admission).replace('\n', " ")
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
                let items = self
                    .shared
                    .storage
                    .list_inbox_items(&identity.user, &identity.player_id, Some(filter), 50)
                    .await?;
                for item in &items {
                    session.data(
                        channel,
                        format!(
                            "* ITEM {} KIND {} STATUS {} FROM {} SUBJECT {}\r\n",
                            item.id,
                            item.kind,
                            item.status,
                            format_mail_user(&item.sender_user, self.shared.mail_domain.as_deref()),
                            item.subject
                        )
                        .into_bytes(),
                    )?;
                }
                session.data(
                    channel,
                    format!("OK LIST {} item(s)\r\n", items.len()).into_bytes(),
                )?;
            }
            "READ" => {
                let item_id = parse_mailbox_item_id(rest)?;
                let item = self
                    .shared
                    .storage
                    .read_inbox_item(&identity.user, &identity.player_id, item_id)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "* MESSAGE {}\r\nKIND {}\r\nSTATUS {}\r\nFROM {}\r\nSUBJECT {}\r\nBODY {}\r\n.\r\nOK READ {}\r\n",
                        item.id,
                        item.kind,
                        item.status,
                        format_mail_user(&item.sender_user, self.shared.mail_domain.as_deref()),
                        item.subject,
                        item.body,
                        item.id
                    )
                    .into_bytes(),
                )?;
            }
            "ACK" => {
                let item_id = parse_mailbox_item_id(rest)?;
                let item = self
                    .shared
                    .storage
                    .finish_inbox_item(&identity.user, &identity.player_id, item_id, "acked")
                    .await?;
                session.data(channel, format!("OK ACK {}\r\n", item.id).into_bytes())?;
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
                let inbox_item = self
                    .shared
                    .storage
                    .save_mail_message(&identity.user, &identity.player_id, &target, body.trim())
                    .await?;
                session.data(
                    channel,
                    format!(
                        "OK SEND {} TO {}\r\n",
                        inbox_item.id,
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
