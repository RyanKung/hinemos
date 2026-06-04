//! SSH connection handler implementation.

use super::*;

mod session;

use crate::auth::{AuthIdentity, AuthOnboarding};
use crate::config::{format_mail_user, normalize_mail_target};
use crate::presence::PresenceDeliveryMode;
use crate::render::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hinemos_storage::{StoredAdmission, StoredParcel};
use rand::Rng;
use russh::keys::HashAlg;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionMode {
    Shell,
    Mailbox,
}

const AGREEMENT_VERSION: &str = "2026-06-03";
const AGREEMENT_PHRASE: &str = "I agree to enter Hinemos";
const ADMISSION_VIEW_ID: &str = "arrival_street";
const PENDING_PRESENCE_VIEW_ID: &str = "__pending_admission";
const ADMISSION_BOARD_ENTITY_ID: &str = "cyber_scroll_board";

pub(crate) struct ConnectionHandler {
    shared: Arc<SharedState>,
    connection_id: u64,
    peer_addr: Option<SocketAddr>,
    identity: Option<AuthIdentity>,
    input_buffer: String,
    commands_seen: u64,
    channel: Option<ChannelId>,
    chrome: Option<Chrome>,
    mode: Option<ConnectionMode>,
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
            mode: None,
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
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if admission.is_agreed() {
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
        } else {
            session.data(
                channel,
                admission_guidance(&admission)
                    .replace('\n', "\r\n")
                    .into_bytes(),
            )?;
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
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if admission.is_agreed() {
            self.shared
                .storage
                .ensure_player_wallet(&identity.user, &identity.player_id)
                .await?;
        }
        let saved_player =
            PlayerStateStore::load_player_state(&self.shared.storage, &identity.player_id).await?;
        self.shared
            .runtime
            .set_or_create_player(saved_player, &identity.player_id, LOCAL_PLAYER_ID)
            .await?;
        let mut player_to_save = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        if !admission.is_agreed() {
            player_to_save.current_view = ADMISSION_VIEW_ID.to_owned();
            self.shared
                .runtime
                .set_player_state(player_to_save.clone())
                .await?;
        }
        PlayerStateStore::save_player_state(&self.shared.storage, &player_to_save).await?;
        self.shared.presence.lock().await.mark_online(
            self.connection_id,
            identity.player_id.clone(),
            identity.user.clone(),
            if admission.is_agreed() {
                player_to_save.current_view.clone()
            } else {
                PENDING_PRESENCE_VIEW_ID.to_owned()
            },
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
                .pending_admission_blocks_free_text(channel, session, identity, prompt)
                .await?
        {
            return Ok(());
        }

        if !line.trim_start().starts_with('/')
            && self
                .handle_blackstone_chat(channel, session, line, identity, prompt)
                .await?
        {
            return Ok(());
        }

        let current_observation = match self.shared.runtime.observe_json(&identity.player_id).await
        {
            Ok(observation) => observation,
            Err(error) => {
                send_command_error(session, channel, error.into(), prompt)?;
                return Ok(());
            }
        };

        let command = match runtime_state::parse_command(chrome, Some(&current_observation), line) {
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

        if self
            .handle_pending_admission_command(channel, session, &command, identity, prompt)
            .await?
        {
            return Ok(());
        }

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

    async fn pending_admission_blocks_free_text(
        &self,
        channel: ChannelId,
        session: &mut Session,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if admission.is_agreed() {
            return Ok(false);
        }
        send_pending_admission_rejection(session, channel, &admission, prompt)?;
        Ok(true)
    }

    async fn handle_pending_admission_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        command: &SemanticCommand,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if admission.is_agreed() {
            return Ok(false);
        }

        match command {
            SemanticCommand::Look | SemanticCommand::Help | SemanticCommand::Quit => Ok(false),
            SemanticCommand::Read { target } if target.id == ADMISSION_BOARD_ENTITY_ID => {
                self.shared
                    .storage
                    .mark_agreement_read(&identity.player_id, AGREEMENT_VERSION)
                    .await?;
                Ok(false)
            }
            SemanticCommand::Agree { phrase } => {
                self.handle_agree_command(channel, session, phrase, identity)
                    .await?;
                if prompt {
                    send_prompt(session, channel)?;
                }
                Ok(true)
            }
            _ => {
                send_pending_admission_rejection(session, channel, &admission, prompt)?;
                Ok(true)
            }
        }
    }

    async fn handle_agree_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        phrase: &str,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if admission.is_agreed() {
            session.data(
                channel,
                b"Admission already agreed. Welcome back.\r\n".to_vec(),
            )?;
            return Ok(());
        }
        if !admission.has_read_version(AGREEMENT_VERSION) {
            session.data(
                channel,
                admission_guidance(&admission)
                    .replace('\n', "\r\n")
                    .into_bytes(),
            )?;
            return Ok(());
        }
        if phrase.trim() != AGREEMENT_PHRASE {
            session.data(
                channel,
                format!(
                    "Agreement phrase did not match. Read /read agreement and type exactly: /agree {AGREEMENT_PHRASE}\r\n"
                )
                .into_bytes(),
            )?;
            return Ok(());
        }

        self.shared
            .storage
            .admit_player(&identity.player_id, AGREEMENT_VERSION)
            .await?;
        let balance = self
            .shared
            .storage
            .ensure_player_wallet(&identity.user, &identity.player_id)
            .await?;
        let mut player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        player.current_view = ADMISSION_VIEW_ID.to_owned();
        self.shared.runtime.set_player_state(player.clone()).await?;
        PlayerStateStore::save_player_state(&self.shared.storage, &player).await?;
        self.shared
            .presence
            .lock()
            .await
            .update_view(self.connection_id, player.current_view.clone());

        session.data(
            channel,
            format!(
                "Agreement accepted: version {AGREEMENT_VERSION}. Initial grant issued: {} {}. Welcome to Hinemos.\r\n",
                balance.amount, balance.asset
            )
            .into_bytes(),
        )?;
        let observation = self
            .shared
            .runtime
            .observe_json(&identity.player_id)
            .await?;
        self.send_text_observation(channel, session, observation)
            .await?;
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
                let items = self
                    .shared
                    .storage
                    .list_inbox_items(&identity.user, &identity.player_id, Some("open"), 20)
                    .await?;
                session.data(
                    channel,
                    render_inbox_items("Mailbox", &items, self.shared.mail_domain.as_deref())
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
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
            SemanticCommand::Settings { action } => {
                self.handle_settings_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Pay { action } => {
                self.handle_pay_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Inbox { action } => {
                self.handle_inbox_command(channel, session, action, identity)
                    .await?;
            }
            SemanticCommand::Enter { target } => {
                self.handle_enter_command(channel, session, target, identity)
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
                if hinemos_blackstone::extension_command_names().contains(&name.as_str()) =>
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

    async fn handle_settings_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &SettingsAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            SettingsAction::Show => {
                let settings = self
                    .shared
                    .storage
                    .account_settings(&identity.user, &identity.player_id)
                    .await?;
                let mail_address =
                    format_mail_user(&identity.user, self.shared.mail_domain.as_deref());
                let key = settings.key_fingerprint.as_deref().unwrap_or("not set");
                let mut next_steps = Vec::new();
                if settings.key_fingerprint.is_none() || identity.fingerprint == "password" {
                    next_steps.push(
                        "Create an ed25519 key pair and bind its public key with /settings key <openssh-public-key>.",
                    );
                }
                if !settings.has_mail_token {
                    next_steps.push("Generate your SMTP/IMAP token with /settings mail-token.");
                }
                let next_steps = if next_steps.is_empty() {
                    "Next steps: account settings are complete.\r\n".to_owned()
                } else {
                    format!("Next steps:\r\n- {}\r\n", next_steps.join("\r\n- "))
                };
                let agent_protocol = format!(
                    "Agent realtime mail:\r\n- Login: IMAP/SMTP username `{}`; password/token is generated by `/settings mail-token`.\r\n- Setup: bind an ed25519 SSH key with `/settings key <openssh-public-key>` and keep using that key for SSH login.\r\n- Listen: keep an IMAP IDLE connection open; when EXISTS arrives, FETCH the new message, handle it, then STORE +FLAGS (\\Seen). This is the supported no-prompt path for autonomous agents.\r\n",
                    identity.user
                );
                session.data(
                    channel,
                    format!(
                        "Settings\r\nUser: {}\r\nPlayer: {}\r\nDisplay name: {}\r\nOnline days: {}\r\nSSH key: {}\r\nSSH password: {}\r\nMail address: {}\r\nMail token: {}\r\n{}{}Use /settings mail-token, /settings password <new-password>, or /settings key <openssh-public-key>.\r\n",
                        identity.user,
                        settings.player_id,
                        settings.display_name,
                        settings.online_days,
                        key,
                        enabled_label(settings.has_password),
                        mail_address,
                        enabled_label(settings.has_mail_token),
                        next_steps,
                        agent_protocol,
                    )
                    .into_bytes(),
                )?;
            }
            SettingsAction::MailToken => {
                let token = generate_mail_auth_token();
                self.shared
                    .storage
                    .set_mail_auth_token(&identity.user, &identity.player_id, &token)
                    .await?;
                let address = format_mail_user(&identity.user, self.shared.mail_domain.as_deref());
                session.data(
                    channel,
                    format!(
                        "Generated a new SMTP/IMAP mail auth token.\r\nUsername: {}\r\nAddress: {}\r\nToken: {}\r\nThis token is shown once; run /settings mail-token again to rotate it.\r\nAgent setup: configure SMTP and IMAP with this username/token. For realtime autonomous handling, keep an IMAP IDLE listener open and process EXISTS notifications without waiting for a world prompt.\r\n",
                        identity.user, address, token
                    )
                    .into_bytes(),
                )?;
            }
            SettingsAction::SetPassword { password } => {
                self.shared
                    .storage
                    .set_password_identity(&identity.user, &identity.player_id, password)
                    .await?;
                session.data(channel, b"SSH password login updated.\r\n".to_vec())?;
            }
            SettingsAction::SetKey { public_key } => {
                let fingerprint = public_key_fingerprint(public_key)?;
                self.shared
                    .storage
                    .replace_ssh_identity(&identity.user, &identity.player_id, &fingerprint)
                    .await?;
                session.data(
                    channel,
                    format!("SSH public key binding replaced: {fingerprint}\r\n").into_bytes(),
                )?;
            }
        }
        Ok(())
    }

    async fn handle_enter_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        target: &str,
        identity: &AuthIdentity,
    ) -> Result<()> {
        let mut player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let parcels = self.shared.storage.list_commercial_parcels().await?;
        let visible = visible_street_parcels(&player.current_view, &parcels);
        let parcel = resolve_enter_target(&visible, target)?;

        player.current_view = parcel.view_id.clone();
        self.shared.runtime.set_player_state(player.clone()).await?;
        PlayerStateStore::save_player_state(&self.shared.storage, &player).await?;
        self.shared
            .presence
            .lock()
            .await
            .update_view(self.connection_id, player.current_view.clone());

        session.data(
            channel,
            format!("You enter {}.\r\n", parcel.parcel_id).into_bytes(),
        )?;
        let observation = self
            .shared
            .runtime
            .observe_json(&identity.player_id)
            .await?;
        self.send_text_observation(channel, session, observation)
            .await?;
        Ok(())
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
            if is_custom_command_input(&parcel, line) {
                session.data(
                    channel,
                    format!(
                        "You own this shop. Visitors use {} here; their requests arrive in your inbox and /shop inbox.\r\n",
                        line.split_whitespace().next().unwrap_or("this command")
                    )
                    .into_bytes(),
                )?;
                if prompt {
                    send_prompt(session, channel)?;
                }
                return Ok(true);
            }
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
        let inbox_item = self
            .shared
            .storage
            .inbox_item_by_source(owner_player_id, "operator_command", command.id)
            .await?;
        if delivered {
            deliver_live_inbox_notice(recipients, &inbox_item, self.shared.mail_domain.as_deref())
                .await;
        }
        session.data(
            channel,
            format!(
                "Shop request #{} sent to owner {} for parcel {}.\r\nStatus: {}.\r\n{}",
                command.id,
                command.owner_user,
                command.parcel_id,
                if delivered { "delivered" } else { "queued" },
                custom_command_preview(&parcel, line)
                    .map(|preview| format!("Preview: {preview}\r\n"))
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

    async fn handle_inbox_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        action: &InboxAction,
        identity: &AuthIdentity,
    ) -> Result<()> {
        match action {
            InboxAction::List { filter } => {
                let items = self
                    .shared
                    .storage
                    .list_inbox_items(&identity.user, &identity.player_id, Some(filter), 20)
                    .await?;
                session.data(
                    channel,
                    render_inbox_items("Inbox", &items, self.shared.mail_domain.as_deref())
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
            }
            InboxAction::Read { item_id } => {
                let item = self
                    .shared
                    .storage
                    .read_inbox_item(&identity.user, &identity.player_id, *item_id)
                    .await?;
                session.data(
                    channel,
                    render_inbox_item(&item, self.shared.mail_domain.as_deref())
                        .replace('\n', "\r\n")
                        .into_bytes(),
                )?;
            }
            InboxAction::Claim { item_id } => {
                let item = self
                    .shared
                    .storage
                    .claim_inbox_item(&identity.user, &identity.player_id, *item_id)
                    .await?;
                session.data(
                    channel,
                    format!(
                        "Claimed inbox #{} kind={} subject={}. Lease until {}.\r\n",
                        item.id,
                        item.kind,
                        item.subject,
                        item.lease_until.as_deref().unwrap_or("unknown")
                    )
                    .into_bytes(),
                )?;
            }
            InboxAction::Ack { item_id } => {
                let item = self
                    .shared
                    .storage
                    .finish_inbox_item(&identity.user, &identity.player_id, *item_id, "acked")
                    .await?;
                session.data(
                    channel,
                    format!("Acked inbox #{} kind={}.\r\n", item.id, item.kind).into_bytes(),
                )?;
            }
            InboxAction::Archive { item_id } => {
                let item = self
                    .shared
                    .storage
                    .finish_inbox_item(&identity.user, &identity.player_id, *item_id, "archived")
                    .await?;
                session.data(
                    channel,
                    format!("Archived inbox #{} kind={}.\r\n", item.id, item.kind).into_bytes(),
                )?;
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
                        "Claimed parcel {}. You can build here with /build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /build publish. From the street, enter with /enter {}. Custom commands are auto-filled if omitted.\r\n",
                        parcel.parcel_id, parcel.parcel_id
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
            BuildAction::Apply { sheet } => {
                let mut updated = Vec::new();
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
                    self.shared
                        .storage
                        .update_parcel_build_field(
                            &player.current_view,
                            &identity.player_id,
                            field,
                            value,
                        )
                        .await?;
                    updated.push(field);
                }
                if non_empty(sheet.commands.as_deref()).is_none() {
                    self.shared
                        .storage
                        .update_parcel_build_field(
                            &player.current_view,
                            &identity.player_id,
                            "commands",
                            default_build_commands(),
                        )
                        .await?;
                    updated.push("commands");
                }
                if updated.is_empty() {
                    anyhow::bail!("build JSON did not include editable fields");
                }
                session.data(
                    channel,
                    format!(
                        "Updated build sheet for current parcel: {}.\r\n",
                        updated.join(", ")
                    )
                    .into_bytes(),
                )?;
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
                let inbox_item = self
                    .shared
                    .storage
                    .inbox_item_by_source(&request.payer_player_id, "payment_request", request.id)
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
                    deliver_live_inbox_notice(
                        recipients,
                        &inbox_item,
                        self.shared.mail_domain.as_deref(),
                    )
                    .await;
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
        }
        let player_id = observation.player_id.clone();
        self.shared
            .blackstone
            .decorate_observation(&player_id, &mut observation)
            .await?;
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
                let target = normalize_mail_target(target, self.shared.mail_domain.as_deref())?;
                let inbox_item = self
                    .shared
                    .storage
                    .save_mail_message(&identity.user, &identity.player_id, &target, text)
                    .await?;
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, &target);
                deliver_live_inbox_notice(
                    recipients,
                    &inbox_item,
                    self.shared.mail_domain.as_deref(),
                )
                .await;
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

    async fn handle_exec_request(
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

    async fn start_mailbox_channel(
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

    async fn handle_terminal_input(
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

    async fn handle_mailbox_input(
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

    async fn handle_mailbox_line(
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
                let recipients = self
                    .shared
                    .presence
                    .lock()
                    .await
                    .direct_recipients(self.connection_id, &target);
                deliver_live_inbox_notice(
                    recipients,
                    &inbox_item,
                    self.shared.mail_domain.as_deref(),
                )
                .await;
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

fn visible_street_parcels<'a>(view_id: &str, parcels: &'a [StoredParcel]) -> Vec<&'a StoredParcel> {
    let Some(parcel_id) = street_parcel_id(view_id) else {
        return Vec::new();
    };
    parcels
        .iter()
        .filter(|parcel| parcel.parcel_id == parcel_id)
        .collect()
}

fn street_parcel_id(view_id: &str) -> Option<&str> {
    view_id.strip_prefix("street_")
}

fn resolve_enter_target<'a>(
    visible: &[&'a StoredParcel],
    target: &str,
) -> Result<&'a StoredParcel> {
    let normalized = normalize_enter_target(target);
    if normalized.is_empty() {
        anyhow::bail!("missing command argument");
    }

    visible
        .iter()
        .copied()
        .find(|parcel| {
            normalize_enter_target(&parcel.parcel_id) == normalized
                || parcel
                    .title
                    .as_deref()
                    .is_some_and(|title| normalize_enter_target(title) == normalized)
        })
        .ok_or_else(|| {
            let available = visible
                .iter()
                .map(|parcel| parcel.parcel_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if available.is_empty() {
                anyhow::anyhow!(
                    "no adjacent parcel here; move along the street with /go, then use /enter <parcel>"
                )
            } else {
                anyhow::anyhow!("parcel not adjacent here: {target}. Available: {available}")
            }
        })
}

fn normalize_enter_target(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

fn admission_guidance(admission: &StoredAdmission) -> String {
    let next_step = if admission.has_read_version(AGREEMENT_VERSION) {
        format!("Type exactly: /agree {AGREEMENT_PHRASE}")
    } else {
        "Read the board agreement first: /read agreement".to_owned()
    };
    format!(
        "Admission pending. SSH authentication is complete, but this account is not admitted into the world yet.\n{next_step}. Until then, other commands are blocked."
    )
}

fn send_pending_admission_rejection(
    session: &mut Session,
    channel: ChannelId,
    admission: &StoredAdmission,
    prompt: bool,
) -> Result<()> {
    session.data(
        channel,
        format!(
            "{}\r\n",
            admission_guidance(admission).replace('\n', "\r\n")
        )
        .into_bytes(),
    )?;
    if prompt {
        send_prompt(session, channel)?;
    }
    Ok(())
}

fn restrict_pending_admission_observation(
    observation: &mut JsonObservation,
    admission: &StoredAdmission,
) {
    observation.description = format!(
        "{}\n\n{}",
        observation.description,
        admission_guidance(admission)
    );
    observation.exits.clear();
    observation.available_commands = vec![
        SemanticCommand::Look,
        SemanticCommand::Read {
            target: hinemos_core::EntityRef::new(ADMISSION_BOARD_ENTITY_ID),
        },
        SemanticCommand::Help,
        SemanticCommand::Quit,
    ];
    if admission.has_read_version(AGREEMENT_VERSION) {
        observation.available_commands.push(SemanticCommand::Agree {
            phrase: AGREEMENT_PHRASE.to_owned(),
        });
    }
}

fn mailbox_help() -> &'static str {
    "Commands: HELP, IDLE, LIST [open|unread|claimed|done|all], READ <id>, SEND <user-or-address> <body>, ACK <id>, NOOP, QUIT"
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "set" } else { "not set" }
}

fn generate_mail_auth_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn public_key_fingerprint(public_key: &str) -> Result<String> {
    let public_key = ssh_key::PublicKey::from_openssh(public_key.trim())?;
    if !public_key.algorithm().is_ed25519() {
        anyhow::bail!("settings key only accepts ssh-ed25519 public keys");
    }
    Ok(public_key.fingerprint(HashAlg::Sha256).to_string())
}

fn parse_mailbox_item_id(input: &str) -> Result<i64> {
    let item_id = input
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing inbox item id"))?;
    item_id
        .parse::<i64>()
        .map_err(|error| anyhow::anyhow!("invalid inbox item id: {error}"))
}
