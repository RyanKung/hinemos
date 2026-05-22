//! SSH connection handler implementation.

use super::*;

mod session;

use crate::auth::{AuthIdentity, AuthOnboarding};
use crate::render::*;

pub(crate) struct ConnectionHandler {
    shared: Arc<SharedState>,
    connection_id: u64,
    peer_addr: Option<SocketAddr>,
    identity: Option<AuthIdentity>,
    input_buffer: String,
    commands_seen: u64,
    channel: Option<ChannelId>,
    chrome: Option<Chrome>,
}

impl ConnectionHandler {
    pub(crate) fn new(
        shared: Arc<SharedState>,
        connection_id: u64,
        peer_addr: Option<SocketAddr>,
    ) -> Self {
        Self {
            shared,
            connection_id,
            peer_addr,
            identity: None,
            input_buffer: String::new(),
            commands_seen: 0,
            channel: None,
            chrome: None,
        }
    }

    async fn send_initial_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        prompt: bool,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"Authentication required.\r\n".to_vec())?;
            return Ok(());
        };

        session.data(
            channel,
            format!(
                "Authenticated as {} with {}.\r\nPlayer session: {}\r\n",
                identity.user, identity.fingerprint, identity.player_id
            )
            .into_bytes(),
        )?;
        if let Some(notice) = identity.onboarding_notice() {
            session.data(channel, notice.into_bytes())?;
        }
        if let Err(error) =
            send_balance_summary(session, channel, &self.shared.storage, identity).await
        {
            send_command_error(session, channel, error, false)?;
        }
        if let Err(error) =
            send_mailbox_summary(session, channel, &self.shared.storage, identity).await
        {
            send_command_error(session, channel, error, false)?;
        }
        match self.shared.runtime.observe_json(&identity.player_id).await {
            Ok(observation) => {
                if let Err(error) = self
                    .send_text_observation(channel, session, observation)
                    .await
                {
                    send_command_error(session, channel, error, false)?;
                }
            }
            Err(error) => send_command_error(session, channel, error.into(), false)?,
        }
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(())
    }

    async fn finish_authentication(&mut self, identity: AuthIdentity) -> Result<()> {
        self.shared
            .storage
            .ensure_player_wallet(&identity.user, &identity.player_id)
            .await?;
        let saved_player =
            PlayerStateStore::load_player_state(&self.shared.storage, &identity.player_id).await?;
        self.shared
            .runtime
            .set_or_create_player(saved_player, &identity.player_id, LOCAL_PLAYER_ID)
            .await?;
        let player_to_save = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        PlayerStateStore::save_player_state(&self.shared.storage, &player_to_save).await?;
        self.shared.presence.lock().await.mark_online(
            self.connection_id,
            identity.player_id.clone(),
            identity.user.clone(),
            player_to_save.current_view.clone(),
        );
        self.identity = Some(identity);
        self.chrome = Some(self.shared.runtime.chrome().await);
        Ok(())
    }

    async fn handle_command_line(
        &mut self,
        channel: ChannelId,
        line: &str,
        session: &mut Session,
        prompt: bool,
    ) -> Result<()> {
        let Some(identity) = &self.identity else {
            session.data(channel, b"Authentication required.\r\n".to_vec())?;
            return Ok(());
        };

        self.shared.presence.lock().await.touch(self.connection_id);
        self.commands_seen += 1;

        let Some(chrome) = &self.chrome else {
            session.data(channel, b"Session is not ready.\r\n".to_vec())?;
            return Ok(());
        };

        if !line.trim_start().starts_with('/')
            && self
                .handle_blackstone_chat(channel, session, line, identity, prompt)
                .await?
        {
            return Ok(());
        }

        let command = match runtime_state::parse_command(chrome, line) {
            Ok(command) => command,
            Err(error) => {
                if matches!(error, SlashParseError::UnknownCommand)
                    && line.trim_start().starts_with('/')
                {
                    match self
                        .handle_operator_input(channel, session, line, identity, prompt)
                        .await
                    {
                        Ok(true) => return Ok(()),
                        Ok(false) => {}
                        Err(error) => {
                            send_command_error(session, channel, error, prompt)?;
                            return Ok(());
                        }
                    }
                }
                let message = if matches!(error, SlashParseError::UnknownCommand)
                    && !line.trim_start().starts_with('/')
                {
                    "World commands start with /. Choose an Available command such as /help or /look."
                        .to_owned()
                } else {
                    error.to_string()
                };
                session.data(channel, format!("{message}\r\n").into_bytes())?;
                if prompt {
                    send_prompt(session, channel)?;
                }
                return Ok(());
            }
        };

        let should_quit = matches!(command, SemanticCommand::Quit);
        let handled_message_view = match self
            .handle_message_view_command(channel, session, &command, identity, prompt)
            .await
        {
            Ok(handled) => handled,
            Err(error) => {
                send_command_error(session, channel, error, prompt)?;
                return Ok(());
            }
        };
        if handled_message_view {
            return Ok(());
        }
        if let Err(error) = self.dispatch_live_message(&command, identity).await {
            send_command_error(session, channel, error, prompt)?;
            return Ok(());
        }
        let (observation, player_state) = match self
            .shared
            .runtime
            .execute(&identity.player_id, &command)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                session.data(channel, format!("{error}\r\n").into_bytes())?;
                if prompt {
                    send_prompt(session, channel)?;
                }
                return Ok(());
            }
        };
        if let Err(error) =
            PlayerStateStore::save_player_state(&self.shared.storage, &player_state).await
        {
            send_command_error(session, channel, error.into(), prompt)?;
            return Ok(());
        }
        self.shared
            .presence
            .lock()
            .await
            .update_view(self.connection_id, player_state.current_view.clone());

        if let Err(error) = self
            .send_command_observation(channel, session, &command, observation)
            .await
        {
            send_command_error(session, channel, error, prompt)?;
            return Ok(());
        }
        if should_quit {
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
        } else if prompt {
            send_prompt(session, channel)?;
        }

        Ok(())
    }

    async fn handle_message_view_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        command: &SemanticCommand,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        match command {
            SemanticCommand::Mailbox => {
                let messages = self
                    .shared
                    .storage
                    .recent_mailbox_messages(&identity.user, &identity.player_id, 20)
                    .await?;
                send_message_list(session, channel, "Mailbox", &messages, "No mail.")?;
            }
            SemanticCommand::History => {
                let player = self
                    .shared
                    .runtime
                    .player_state(&identity.player_id)
                    .await?;
                let messages = self
                    .shared
                    .storage
                    .recent_view_messages(&player.current_view, 20)
                    .await?;
                send_message_list(
                    session,
                    channel,
                    "Room History",
                    &messages,
                    "No room history.",
                )?;
                session.data(
                    channel,
                    format!("You are still in: {}\r\n", player.current_view).into_bytes(),
                )?;
            }
            SemanticCommand::Who => {
                let player = self
                    .shared
                    .runtime
                    .player_state(&identity.player_id)
                    .await?;
                let users = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .view_users(self.connection_id, &player.current_view);
                session.data(
                    channel,
                    render_who(&player.current_view, &users).into_bytes(),
                )?;
            }
            SemanticCommand::News => {
                let messages = self.shared.storage.recent_news_messages(20).await?;
                send_message_list(session, channel, "News", &messages, "No news.")?;
            }
            SemanticCommand::Balance => {
                let balance = self
                    .shared
                    .storage
                    .player_balance(&identity.player_id)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Balance: {} {} ({})\r\n",
                        balance.amount, balance.asset, balance.account_id
                    )
                    .into_bytes(),
                )?;
            }
            SemanticCommand::Pay { action } => {
                self.handle_pay_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Land { action } => {
                self.handle_land_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Build { action } => {
                self.handle_build_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Shop { action } => {
                self.handle_shop_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Extension { name, input }
                if xagora_blackstone::extension_command_names().contains(&name.as_str()) =>
            {
                self.handle_extension_command(channel, session, input, identity)
                    .await?;
            }
            _ => return Ok(false),
        }

        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn handle_operator_input(
        &self,
        channel: ChannelId,
        session: &mut Session,
        line: &str,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let Some(parcel) = self
            .shared
            .storage
            .commercial_parcel_by_view(&player.current_view)
            .await?
        else {
            return Ok(false);
        };
        if parcel.status != "built" {
            return Ok(false);
        }
        let Some(owner_player_id) = parcel.owner_player_id.as_deref() else {
            return Ok(false);
        };
        if owner_player_id == identity.player_id {
            return Ok(false);
        }

        let recipients = self
            .shared
            .presence
            .lock()
            .await
            .direct_recipients(self.connection_id, owner_player_id);
        let delivered = !recipients.is_empty();
        let command = self
            .shared
            .storage
            .save_operator_command(
                &parcel,
                &identity.user,
                &identity.player_id,
                line,
                delivered,
            )
            .await?;
        if delivered {
            deliver_live_message(
                recipients,
                &format!(
                    "[shop command #{} in {} from {}] {}",
                    command.id, command.parcel_id, command.sender_user, command.raw_input
                ),
            )
            .await;
        }
        session.data(
            channel,
            format!(
                "Sent shop command #{} to {} ({}) for parcel {}.\r\n{}",
                command.id,
                command.owner_user,
                if delivered { "delivered" } else { "queued" },
                command.parcel_id,
                custom_command_preview(&parcel, line)
                    .map(|preview| format!("Trial: {preview}\r\n"))
                    .unwrap_or_default()
            )
            .into_bytes(),
        )?;
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn handle_pay_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &PayAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            PayAction::Direct {
                target,
                amount,
                memo,
            } => {
                let transfer = self
                    .shared
                    .storage
                    .transfer_mark(&identity.user, &identity.player_id, target, *amount, memo)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Paid {} {} to {}. Ledger #{}. Balance: {} {}.\r\n",
                        transfer.amount,
                        transfer.asset,
                        transfer.target_user,
                        transfer.ledger_id,
                        transfer.sender_balance,
                        transfer.asset
                    )
                    .into_bytes(),
                )?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, target);
                if !recipients.is_empty() {
                    let memo_text = if transfer.memo.is_empty() {
                        String::new()
                    } else {
                        format!(" memo={}", transfer.memo)
                    };
                    deliver_live_message(
                        recipients,
                        &format!(
                            "[payment from {}] {} {}{}",
                            identity.user, transfer.amount, transfer.asset, memo_text
                        ),
                    )
                    .await;
                }
            }
            PayAction::Requests => {
                let requests = self
                    .shared
                    .storage
                    .pending_payment_requests(&identity.player_id, 20)
                    .await?;
                send_payment_request_list(session, channel, &requests)?;
            }
            PayAction::Accept { request_id } => {
                let (request, sender_balance) = self
                    .shared
                    .storage
                    .accept_payment_request(&identity.user, &identity.player_id, *request_id)
                    .await?;
                session.data(
                    channel,
                    render_paid_request(&request, sender_balance).into_bytes(),
                )?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, &request.payee_player_id);
                if !recipients.is_empty() {
                    deliver_live_message(
                        recipients,
                        &format!(
                            "[payment request #{} paid by {}] {} {}",
                            request.id, identity.user, request.amount, request.asset
                        ),
                    )
                    .await;
                }
            }
        }
        Ok(())
    }

    async fn handle_land_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &LandAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            LandAction::List => {
                let parcels = self.shared.storage.list_commercial_parcels().await?;
                session.data(
                    channel,
                    render_parcel_list(&parcels)
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
            }
            LandAction::Info { parcel_id } => {
                let parcel = self.shared.storage.commercial_parcel(parcel_id).await?;
                session.data(
                    channel,
                    render_parcel_detail(&parcel)
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
            }
            LandAction::Claim { parcel_id } => {
                let parcel = self
                    .shared
                    .storage
                    .claim_commercial_parcel(parcel_id, &identity.user, &identity.player_id)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Claimed parcel {}. Go to {} and use /build title, /build description, /build prompt, /build commands, then /build publish.\r\n",
                        parcel.parcel_id, parcel.view_id
                    )
                    .into_bytes(),
                )?;
            }
            LandAction::Transfer { parcel_id, target } => {
                let parcel = self
                    .shared
                    .storage
                    .transfer_commercial_parcel(parcel_id, &identity.player_id, target)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Transferred parcel {} to {}.\r\n",
                        parcel.parcel_id,
                        parcel.owner_user.as_deref().unwrap_or("unknown")
                    )
                    .into_bytes(),
                )?;
            }
        }
        Ok(())
    }

    async fn handle_build_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &BuildAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        match action {
            BuildAction::Help => {
                session.data(channel, build_help().as_bytes().to_vec())?;
            }
            BuildAction::Set { field, value } => {
                let parcel = self
                    .shared
                    .storage
                    .update_parcel_build_field(
                        &player.current_view,
                        &identity.player_id,
                        field,
                        value,
                    )
                    .await?;
                session.data(
                    channel,
                    format!("Updated {} for parcel {}.\r\n", field, parcel.parcel_id).into_bytes(),
                )?;
            }
            BuildAction::Publish => {
                let parcel = self
                    .shared
                    .storage
                    .publish_parcel_build(&player.current_view, &identity.player_id)
                    .await?;
                session.data(
                    channel,
                    format!("Published parcel {} as a built shop.\r\n", parcel.parcel_id)
                        .into_bytes(),
                )?;
            }
        }
        Ok(())
    }

    async fn handle_shop_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &ShopAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            ShopAction::Inbox => {
                let commands = self
                    .shared
                    .storage
                    .recent_operator_commands(&identity.player_id, 20)
                    .await?;
                send_operator_command_list(session, channel, &commands)?;
            }
            ShopAction::RequestPayment {
                command_id,
                amount,
                delivery,
            } => {
                let request = self
                    .shared
                    .storage
                    .create_payment_request(*command_id, &identity.player_id, *amount, delivery)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Created payment request #{} for {}: {} {}. Delivery is locked until payment.\r\n",
                        request.id, request.payer_user, request.amount, request.asset
                    )
                    .into_bytes(),
                )?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, &request.payer_player_id);
                if !recipients.is_empty() {
                    deliver_live_message(recipients, &render_payment_popup(&request)).await;
                }
            }
        }
        Ok(())
    }

    async fn handle_extension_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        input: &str,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let response = self
            .shared
            .blackstone
            .handle(
                &identity.user,
                &identity.player_id,
                &player.current_view,
                input,
            )
            .await?;
        session.data(channel, response.into_bytes())?;
        Ok(())
    }

    async fn handle_blackstone_chat(
        &self,
        channel: ChannelId,
        session: &mut Session,
        input: &str,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let Some(response) = self
            .shared
            .blackstone
            .handle_chat(
                &identity.user,
                &identity.player_id,
                &player.current_view,
                input,
            )
            .await?
        else {
            return Ok(false);
        };
        session.data(channel, response.into_bytes())?;
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn send_text_observation(
        &self,
        channel: ChannelId,
        session: &mut Session,
        mut observation: JsonObservation,
    ) -> Result<()> {
        if let Some(parcel) = self
            .shared
            .storage
            .commercial_parcel_by_view(&observation.view_id)
            .await?
        {
            overlay_parcel_observation(&mut observation, &parcel);
        }
        let player_id = observation.player_id.clone();
        self.shared
            .blackstone
            .decorate_observation(&player_id, &mut observation)
            .await?;
        let view_users = self
            .shared
            .presence
            .lock()
            .await
            .view_users(self.connection_id, &observation.view_id);
        observation.online_users = render_online_summary(&view_users, 10);
        send_text_observation(session, channel, &observation)?;
        Ok(())
    }

    async fn send_command_observation(
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

    async fn dispatch_live_message(
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
                self.shared
                    .storage
                    .save_mail_message(&identity.user, &identity.player_id, target, text)
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, target);
                (
                    recipients,
                    format!("[mail from {} to {target}] {text}", identity.user),
                )
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

    async fn handle_exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        let command = String::from_utf8_lossy(data).trim().to_owned();
        session.channel_success(channel)?;
        if command.is_empty() {
            session.data(channel, exec_help().replace('\n', "\r\n").into_bytes())?;
        } else {
            session.data(
                channel,
                format!(
                    "{}\r\nSSH exec is not a world command interface. Rejected exec command: {command}\r\n",
                    exec_help()
                )
                .into_bytes(),
            )?;
        }
        session.exit_status_request(channel, 1)?;
        session.close(channel)?;
        Ok(())
    }

    async fn handle_terminal_input(
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
                    self.input_buffer.push(character);
                    session.data(channel, character.to_string().into_bytes())?;
                }
            }
        }

        Ok(())
    }
}
