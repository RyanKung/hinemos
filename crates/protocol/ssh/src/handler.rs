//! SSH connection handler implementation.

use super::*;

#[path = "handler_business.rs"]
mod handler_business;
#[path = "handler_helpers.rs"]
mod handler_helpers;
#[path = "handler_io.rs"]
mod handler_io;
#[path = "handler_memory.rs"]
mod handler_memory;
mod session;

use crate::auth::AuthIdentity;
use crate::config::{format_mail_user, normalize_mail_target};
use crate::presence::PresenceDeliveryMode;
use crate::render::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use handler_helpers::*;
use hinemos_core::{Direction, ExitObservation, SettingsAction};
use hinemos_storage::{
    StoredAdmission, StoredAgentSelfModel, StoredMemoryAtom, StoredMemoryEvent, StoredParcel,
    StoredServiceRoom, StoredSocialEdge,
};
use rand::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionMode {
    Shell,
    Mailbox,
}

const AGREEMENT_VERSION: &str = "2026-06-03";
const ADMISSION_VIEW_ID: &str = "arrival_street";
const PENDING_PRESENCE_VIEW_ID: &str = "__pending_admission";
const ADMISSION_BOARD_ENTITY_ID: &str = "cyber_scroll_board";
const MAX_SHELL_INPUT_BYTES: usize = 4096;

pub(crate) struct ConnectionHandler {
    shared: Arc<SharedState>,
    connection_id: u64,
    peer_addr: Option<SocketAddr>,
    identity: Option<AuthIdentity>,
    input_buffer: String,
    discarding_oversized_input: bool,
    commands_seen: u64,
    channel: Option<ChannelId>,
    chrome: Option<Chrome>,
    mode: Option<ConnectionMode>,
    terminal_cols: Option<usize>,
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
            discarding_oversized_input: false,
            commands_seen: 0,
            channel: None,
            chrome: None,
            mode: None,
            terminal_cols: None,
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

        clear_terminal(session, channel)?;
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
            if let Err(error) = self.send_memory_context(channel, session, identity).await {
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
        match self.observe_current_json(&identity.player_id).await {
            Ok(observation) => {
                if let Err(error) = self
                    .send_text_observation(channel, session, observation)
                    .await
                {
                    send_command_error(session, channel, error, false)?;
                }
            }
            Err(error) => send_command_error(session, channel, error, false)?,
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
        if let Ok(player) = self.shared.runtime.player_state(&identity.player_id).await {
            let _ = self
                .shared
                .storage
                .record_view_presence(&identity.user, &identity.player_id, &player.current_view)
                .await;
        }

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
                .handle_service_room_input(channel, session, line, identity, prompt)
                .await?
        {
            return Ok(());
        }

        let mut current_observation = match self.observe_current_json(&identity.player_id).await {
            Ok(observation) => observation,
            Err(error) => {
                send_command_error(session, channel, error, prompt)?;
                return Ok(());
            }
        };
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if !admission.is_agreed() {
            restrict_pending_admission_observation(&mut current_observation, &admission);
        }

        let command = match runtime_state::parse_command(chrome, Some(&current_observation), line) {
            Ok(command) => command,
            Err(error) => {
                if matches!(error, SlashParseError::UnknownCommand)
                    && line.trim_start().starts_with('/')
                {
                    match self
                        .handle_memory_command(channel, session, line, identity, prompt)
                        .await
                    {
                        Ok(true) => return Ok(()),
                        Ok(false) => {}
                        Err(error) => {
                            send_command_error(session, channel, error, prompt)?;
                            return Ok(());
                        }
                    }
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
                    match self
                        .handle_service_room_input(channel, session, line, identity, prompt)
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
                    slash_parse_feedback(line, &error)
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
        match self
            .handle_service_room_local_command(channel, session, &command, identity, prompt)
            .await
        {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => {
                send_command_error(session, channel, error, prompt)?;
                return Ok(());
            }
        }
        let (observation, player_state) = match self
            .shared
            .runtime
            .execute(&identity.player_id, &command)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                session.data(
                    channel,
                    format!("{}\r\n", world_error_feedback(&error.to_string())).into_bytes(),
                )?;
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
        if let Err(error) = self
            .send_admission_next_step_after_read(channel, session, &command, identity)
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

    async fn send_admission_next_step_after_read(
        &self,
        channel: ChannelId,
        session: &mut Session,
        command: &SemanticCommand,
        identity: &AuthIdentity,
    ) -> Result<()> {
        if !matches!(
            command,
            SemanticCommand::Read { target } if target.id == ADMISSION_BOARD_ENTITY_ID
        ) {
            return Ok(());
        }
        let admission = self
            .shared
            .storage
            .player_admission(&identity.player_id)
            .await?;
        if admission.is_agreed() || !admission.has_read_version(AGREEMENT_VERSION) {
            return Ok(());
        }
        session.data(
            channel,
            b"\r\nNext step: type /agree to enter.\r\n".to_vec(),
        )?;
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
            SemanticCommand::Agree { .. } => {
                self.handle_agree_command(channel, session, identity)
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
        let observation = self.observe_current_json(&identity.player_id).await?;
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
                let observation = self.observe_current_json(&identity.player_id).await?;
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
                    format!("You are still in: {}.\r\n", observation.title).into_bytes(),
                )?;
            }
            SemanticCommand::Inventory => {
                let player = self
                    .shared
                    .runtime
                    .player_state(&identity.player_id)
                    .await?;
                if player.inventory.is_empty() {
                    session.data(channel, b"Inventory: empty.\r\n".to_vec())?;
                } else {
                    session.data(
                        channel,
                        format!("Inventory: {}.\r\n", player.inventory.join(", ")).into_bytes(),
                    )?;
                }
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
            _ => return Ok(false),
        }

        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn handle_service_room_input(
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
        let Some(room) = self
            .shared
            .storage
            .service_room_by_view(&player.current_view)
            .await?
        else {
            return Ok(false);
        };
        if input.trim_start().starts_with('/')
            && !service_room_accepts_input(room.custom_commands.as_deref(), input)
        {
            return Ok(false);
        }
        self.shared
            .storage
            .save_service_room_input(&room, &identity.user, &identity.player_id, input)
            .await?;
        session.data(
            channel,
            format!("Sent to room service {}.\r\n", room.room_user).into_bytes(),
        )?;
        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn handle_service_room_local_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        command: &SemanticCommand,
        identity: &AuthIdentity,
        prompt: bool,
    ) -> Result<bool> {
        let mut player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let Some(room) = self
            .shared
            .storage
            .service_room_by_view(&player.current_view)
            .await?
        else {
            return Ok(false);
        };

        match command {
            SemanticCommand::Look | SemanticCommand::Map => {
                let observation = self.service_room_observation(&identity.player_id, &room);
                self.send_text_observation(channel, session, observation)
                    .await?;
            }
            SemanticCommand::Move {
                direction: Direction::South,
            } => {
                let Some(front_view_id) = room.front_view_id.clone() else {
                    return Ok(false);
                };
                player.current_view = front_view_id;
                self.shared.runtime.set_player_state(player.clone()).await?;
                PlayerStateStore::save_player_state(&self.shared.storage, &player).await?;
                self.shared
                    .presence
                    .lock()
                    .await
                    .update_view(self.connection_id, player.current_view.clone());
                let observation = self.observe_current_json(&identity.player_id).await?;
                self.send_text_observation(channel, session, observation)
                    .await?;
            }
            _ => return Ok(false),
        }

        if prompt {
            send_prompt(session, channel)?;
        }
        Ok(true)
    }

    async fn observe_current_json(&self, player_id: &str) -> Result<JsonObservation> {
        match self.shared.runtime.observe_json(player_id).await {
            Ok(observation) => Ok(observation),
            Err(error) => {
                let player = self.shared.runtime.player_state(player_id).await?;
                if let Some(room) = self
                    .shared
                    .storage
                    .service_room_by_view(&player.current_view)
                    .await?
                {
                    Ok(self.service_room_observation(player_id, &room))
                } else {
                    Err(error.into())
                }
            }
        }
    }

    fn service_room_observation(
        &self,
        player_id: &str,
        room: &StoredServiceRoom,
    ) -> JsonObservation {
        let title = room.label.clone().unwrap_or_else(|| room.view_id.clone());
        let return_label = room.address.as_deref().unwrap_or("street");
        let mut available_commands = vec![
            SemanticCommand::Look,
            SemanticCommand::Map,
            SemanticCommand::Inventory,
            SemanticCommand::History,
            SemanticCommand::Help,
            SemanticCommand::Settings {
                action: SettingsAction::Show,
            },
            SemanticCommand::Who,
            SemanticCommand::Say {
                text: String::new(),
            },
            SemanticCommand::Move {
                direction: Direction::South,
            },
        ];
        available_commands.extend(
            command_inputs(room.custom_commands.as_deref()).map(|input| {
                let name = input
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                SemanticCommand::Extension { name, input }
            }),
        );

        JsonObservation {
            player_id: player_id.to_owned(),
            view_id: room.view_id.clone(),
            title: title.clone(),
            ascii_art: vec![
                "============================================================".to_owned(),
                format!("                  {}", title.to_ascii_uppercase()),
                "============================================================".to_owned(),
                "                           <Me>".to_owned(),
                "                            |".to_owned(),
                format!("                    south to {return_label}"),
            ],
            description:
                "This externally hosted room is connected through the room mailbox protocol."
                    .to_owned(),
            exits: vec![ExitObservation {
                direction: Direction::South,
                target_known: room.front_view_id.is_some(),
                label: room.address.clone(),
            }],
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands,
            events: Vec::new(),
        }
    }
}
