//! SSH connection handler implementation.

use super::*;

#[path = "handler_helpers.rs"]
mod handler_helpers;
#[path = "handler_io.rs"]
mod handler_io;
#[path = "handler_observation.rs"]
mod handler_observation;
#[path = "handler_task.rs"]
mod handler_task;
mod session;

use crate::auth::AuthIdentity;
use crate::config::{format_mail_user, normalize_mail_target};
use crate::presence::PresenceDeliveryMode;
use crate::render::*;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use handler_helpers::*;
use handler_task::observation_events_from_ui_events;
use hinemos_app::{
    AppCommandContext, AppIdentity, AppRequest, AppViewCommandContext, HungerGateOutcome,
    PendingAdmissionCommandOutcome, RecentPresenceUser, RoomBindingKindView, UiEvent,
    WhoPopulation, service_room_unavailable_text,
};
use hinemos_core::{JsonObservation, ObservationEvent, PlayerState, SemanticCommand};
use rand::Rng;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionMode {
    Shell,
    Mailbox,
}

const PENDING_PRESENCE_VIEW_ID: &str = "__pending_admission";
const MAX_SHELL_INPUT_BYTES: usize = 4096;
const RECENT_ONLINE_WINDOW_SECONDS: i64 = 5 * 60;

pub(crate) struct ConnectionHandler {
    shared: Arc<SharedState>,
    connection_id: u64,
    peer_addr: Option<SocketAddr>,
    identity: Option<AuthIdentity>,
    input_buffer: String,
    discarding_oversized_input: bool,
    commands_seen: u64,
    resident_context_sent: AtomicBool,
    channel: Option<ChannelId>,
    chrome: Option<Chrome>,
    mode: Option<ConnectionMode>,
    terminal_cols: Option<usize>,
}

struct CommandLineState {
    identity: AuthIdentity,
    player: PlayerState,
    room_context: RoomViewContext,
    current_observation: JsonObservation,
}

struct HandlerViewCommand<'a> {
    app_identity: &'a AppIdentity,
    command: &'a SemanticCommand,
    player: &'a PlayerState,
    current_observation: &'a JsonObservation,
    room_binding: Option<&'a StoredRoomBinding>,
    prompt: bool,
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
            resident_context_sent: AtomicBool::new(false),
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
        let app = self.shared.app_service().await;
        let admission = app.player_admission(&identity.player_id).await?;
        if admission.is_agreed() {
            match app.balance_summary(&identity.player_id).await {
                Ok(summary) => session.data(channel, summary.into_bytes())?,
                Err(error) => send_command_error(session, channel, error.into(), false)?,
            }
            match app
                .open_inbox_summary(&identity.user, &identity.player_id)
                .await
            {
                Ok(Some(summary)) => session.data(channel, summary.into_bytes())?,
                Ok(None) => {}
                Err(error) => send_command_error(session, channel, error.into(), false)?,
            }
        } else {
            session.data(
                channel,
                app.admission_guidance(&admission)
                    .replace('\n', "\r\n")
                    .into_bytes(),
            )?;
        }
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        let room_context = self
            .shared
            .room_context_for_view(&player.current_view)
            .await?;
        match self
            .observe_current_json_for_view(&identity.player_id, Some(&room_context))
            .await
        {
            Ok(observation) => {
                if let Err(error) = self
                    .send_text_observation_with_context(
                        channel,
                        session,
                        observation,
                        &room_context,
                    )
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
        let app = self.shared.app_service().await;
        let _ = app
            .ensure_player_wallet_if_admitted(&identity.user, &identity.player_id)
            .await?;
        let admission = app.player_admission(&identity.player_id).await?;
        let saved_player = app.load_player_state(&identity.player_id).await?;
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
            let app_config = self.shared.app_config().await;
            player_to_save.current_view = app_config.admission_view_id;
            self.shared
                .runtime
                .set_player_state(player_to_save.clone())
                .await?;
        }
        app.save_player_state(&player_to_save).await?;
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
        let Some(state) = self
            .prepare_command_line_state(channel, session, prompt)
            .await?
        else {
            return Ok(());
        };

        let app_identity = AppIdentity::new(
            state.identity.user.clone(),
            state.identity.player_id.clone(),
        );
        if self
            .handle_pending_free_text(channel, session, &app_identity, line, prompt)
            .await?
        {
            return Ok(());
        }

        let room_binding = state.room_context.room_binding.as_ref();
        if self
            .handle_hunger_for_room_binding_line(
                channel,
                session,
                &app_identity,
                room_binding,
                line,
                prompt,
            )
            .await?
        {
            return Ok(());
        }
        let room_task_command = self.parse_command_line_for_task(&state.current_observation, line);
        if self
            .handle_room_binding_line(
                channel,
                session,
                &app_identity,
                room_binding,
                line,
                &state.current_observation,
                room_task_command.as_ref(),
                prompt,
            )
            .await?
        {
            return Ok(());
        }

        let Some(command) = self.parse_command_line_or_reply(
            channel,
            session,
            line,
            &state.current_observation,
            room_binding,
            prompt,
        )?
        else {
            return Ok(());
        };
        if self
            .handle_pending_admission_command(channel, session, &app_identity, &command, prompt)
            .await?
            || self
                .handle_hunger_for_command(channel, session, &state.identity, &command, prompt)
                .await?
            || self
                .handle_app_view_command(
                    channel,
                    session,
                    HandlerViewCommand {
                        app_identity: &app_identity,
                        command: &command,
                        player: &state.player,
                        current_observation: &state.current_observation,
                        room_binding,
                        prompt,
                    },
                )
                .await?
        {
            return Ok(());
        }

        if let Err(error) = self
            .handle_pre_runtime_command_effects(
                channel,
                session,
                &state.identity,
                &command,
                &state.current_observation,
            )
            .await
        {
            send_command_error(session, channel, error, prompt)?;
            return Ok(());
        }
        self.execute_runtime_command(
            channel,
            session,
            &state.identity,
            &state.current_observation,
            command,
            prompt,
        )
        .await
    }

    async fn prepare_command_line_state(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
        prompt: bool,
    ) -> Result<Option<CommandLineState>> {
        let Some(identity) = self.identity.clone() else {
            session.data(channel, b"Authentication required.\r\n".to_vec())?;
            return Ok(None);
        };

        self.shared.presence.lock().await.touch(self.connection_id);
        for message in self
            .shared
            .drain_connection_messages(self.connection_id)
            .await
        {
            session.data(channel, format!("{message}\r\n").into_bytes())?;
        }
        self.commands_seen += 1;
        let player = self
            .shared
            .runtime
            .player_state(&identity.player_id)
            .await?;
        if let Err(error) = self
            .shared
            .record_view_presence_throttled(
                &identity.user,
                &identity.player_id,
                &player.current_view,
            )
            .await
        {
            send_command_error(session, channel, error, false)?;
        }
        if self.chrome.is_none() {
            session.data(channel, b"Session is not ready.\r\n".to_vec())?;
            return Ok(None);
        }

        let room_context = self
            .shared
            .room_context_for_view(&player.current_view)
            .await?;
        let current_observation = match self
            .observe_current_json_for_view(&identity.player_id, Some(&room_context))
            .await
        {
            Ok(observation) => observation,
            Err(error) => {
                send_command_error(session, channel, error, prompt)?;
                return Ok(None);
            }
        };
        if observation_contains_room_escape(&current_observation) {
            self.send_text_observation_with_context(
                channel,
                session,
                current_observation,
                &room_context,
            )
            .await?;
            send_prompt_if_requested(session, channel, prompt)?;
            return Ok(None);
        }
        let current_observation = self
            .enrich_observation_for_context(current_observation, &room_context)
            .await?;

        Ok(Some(CommandLineState {
            identity,
            player,
            room_context,
            current_observation,
        }))
    }

    async fn handle_pending_free_text(
        &self,
        channel: ChannelId,
        session: &mut Session,
        app_identity: &AppIdentity,
        line: &str,
        prompt: bool,
    ) -> Result<bool> {
        let app = self.shared.app_service().await;
        if !line.trim_start().starts_with('/')
            && let Some(events) = app.pending_admission_free_text(app_identity).await?
        {
            self.send_ui_events(channel, session, events).await?;
            send_prompt_if_requested(session, channel, prompt)?;
            return Ok(true);
        }
        Ok(false)
    }

    async fn handle_room_binding_line(
        &self,
        channel: ChannelId,
        session: &mut Session,
        app_identity: &AppIdentity,
        room_binding: Option<&StoredRoomBinding>,
        line: &str,
        before_observation: &JsonObservation,
        task_command: Option<&SemanticCommand>,
        prompt: bool,
    ) -> Result<bool> {
        let Some(binding) = room_binding else {
            return Ok(false);
        };
        let app = self.shared.app_service().await;
        if let Some(events) = app
            .handle_room_line_for_binding(app_identity, binding, line)
            .await?
        {
            let task_events = observation_events_from_ui_events(&events);
            self.send_ui_events(channel, session, events).await?;
            if let Some(command) = task_command {
                self.record_resident_task_step_after_current_view(
                    app_identity,
                    before_observation,
                    command,
                    task_events,
                )
                .await;
            }
            send_prompt_if_requested(session, channel, prompt)?;
            return Ok(true);
        }
        Ok(false)
    }

    async fn handle_hunger_for_room_binding_line(
        &self,
        channel: ChannelId,
        session: &mut Session,
        app_identity: &AppIdentity,
        room_binding: Option<&StoredRoomBinding>,
        line: &str,
        prompt: bool,
    ) -> Result<bool> {
        let Some(binding) = room_binding else {
            return Ok(false);
        };
        let trimmed = line.trim_start();
        if !trimmed.starts_with('/') {
            return Ok(false);
        }
        if !self.shared.app_config().await.hunger_loop_enabled {
            return Ok(false);
        }
        let app = self.shared.app_service().await;
        let consumed_by_room = if binding.is_commercial_parcel() {
            app.commercial_parcel_consumes_input(binding, trimmed)
        } else {
            app.room_binding_accepts_input(binding, trimmed)
        };
        if !consumed_by_room {
            return Ok(false);
        }
        self.handle_hunger_outcome(
            channel,
            session,
            if binding.is_service_room() {
                app.check_hunger_room_line(&app_identity.player_id, trimmed, binding)
                    .await?
            } else {
                app.check_hunger_raw_line(&app_identity.player_id, trimmed)
                    .await?
            },
            prompt,
        )
        .await
    }

    async fn handle_hunger_for_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        identity: &AuthIdentity,
        command: &SemanticCommand,
        prompt: bool,
    ) -> Result<bool> {
        if !self.shared.app_config().await.hunger_loop_enabled {
            return Ok(false);
        }
        let app = self.shared.app_service().await;
        self.handle_hunger_outcome(
            channel,
            session,
            app.check_hunger_command(&identity.player_id, command)
                .await?,
            prompt,
        )
        .await
    }

    async fn handle_hunger_outcome(
        &self,
        channel: ChannelId,
        session: &mut Session,
        outcome: HungerGateOutcome,
        prompt: bool,
    ) -> Result<bool> {
        match outcome {
            HungerGateOutcome::Allow => Ok(false),
            HungerGateOutcome::Block(text) => {
                self.send_ui_events(channel, session, vec![UiEvent::Text(text)])
                    .await?;
                send_prompt_if_requested(session, channel, prompt)?;
                Ok(true)
            }
        }
    }

    fn parse_command_line_or_reply(
        &self,
        channel: ChannelId,
        session: &mut Session,
        line: &str,
        current_observation: &JsonObservation,
        room_binding: Option<&StoredRoomBinding>,
        prompt: bool,
    ) -> Result<Option<SemanticCommand>> {
        let Some(chrome) = &self.chrome else {
            session.data(channel, b"Session is not ready.\r\n".to_vec())?;
            return Ok(None);
        };
        match runtime_state::parse_command(chrome, Some(current_observation), line) {
            Ok(command) => Ok(Some(command)),
            Err(error) => {
                let message = parse_command_feedback(line, room_binding, &error);
                session.data(channel, format!("{message}\r\n").into_bytes())?;
                send_prompt_if_requested(session, channel, prompt)?;
                Ok(None)
            }
        }
    }

    async fn handle_pending_admission_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        app_identity: &AppIdentity,
        command: &SemanticCommand,
        prompt: bool,
    ) -> Result<bool> {
        let app = self.shared.app_service().await;
        match app
            .handle_pending_admission_command(app_identity, command)
            .await?
        {
            PendingAdmissionCommandOutcome::NotPending
            | PendingAdmissionCommandOutcome::PassThrough => Ok(false),
            PendingAdmissionCommandOutcome::Allow(events) => {
                self.send_ui_events(channel, session, events).await?;
                Ok(true)
            }
            PendingAdmissionCommandOutcome::Block(events) => {
                self.send_ui_events(channel, session, events).await?;
                send_prompt_if_requested(session, channel, prompt)?;
                Ok(true)
            }
        }
    }

    async fn online_view_users(
        &self,
        app: &AppService<PgStorage>,
        view_id: &str,
        excluded_player_id: &str,
    ) -> Result<Vec<RecentPresenceUser>> {
        let connected_users = self
            .shared
            .presence
            .lock()
            .await
            .view_users(self.connection_id, view_id)
            .into_iter()
            .map(|user| user.into_recent_presence_user());
        let recent_users = app
            .recent_active_view_users(view_id, excluded_player_id, RECENT_ONLINE_WINDOW_SECONDS)
            .await?;
        Ok(merge_presence_users(connected_users, recent_users))
    }

    async fn who_population(&self, app: &AppService<PgStorage>) -> Result<WhoPopulation> {
        let connected_users = self
            .shared
            .presence
            .lock()
            .await
            .users_outside_view(PENDING_PRESENCE_VIEW_ID)
            .into_iter()
            .map(|user| user.into_recent_presence_user());
        let recent_users = app
            .recent_active_users(RECENT_ONLINE_WINDOW_SECONDS)
            .await?;
        let online = merge_presence_users(connected_users, recent_users).len();
        let total = app.admitted_player_count().await?;
        Ok(WhoPopulation { total, online })
    }

    async fn handle_app_view_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        request: HandlerViewCommand<'_>,
    ) -> Result<bool> {
        let app = self.shared.app_service().await;
        let (presence_users, who_population) = if matches!(request.command, SemanticCommand::Who) {
            (
                self.online_view_users(
                    &app,
                    &request.current_observation.view_id,
                    &request.player.id,
                )
                .await?,
                self.who_population(&app).await?,
            )
        } else {
            (
                Vec::new(),
                WhoPopulation {
                    total: 0,
                    online: 0,
                },
            )
        };
        let users = presence_usernames(&presence_users);
        let visible_entity_ids = request
            .current_observation
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>();
        let token = generate_mail_auth_token();
        match app
            .handle_view_command(
                request.app_identity,
                request.command,
                AppViewCommandContext {
                    current_view: &request.current_observation.view_id,
                    current_title: &request.current_observation.title,
                    inventory: &request.player.inventory,
                    online_users: &users,
                    who_population,
                    visible_entity_ids: &visible_entity_ids,
                    room_binding: request.room_binding,
                    mail_domain: self.shared.mail_domain.as_deref(),
                    business: AppCommandContext {
                        current_view: &request.player.current_view,
                        mail_domain: self.shared.mail_domain.as_deref(),
                        generated_token: &token,
                    },
                },
            )
            .await
        {
            Ok(Some(events)) => {
                let task_events = observation_events_from_ui_events(&events);
                self.send_ui_events(channel, session, events).await?;
                self.record_resident_task_step_after_current_view(
                    request.app_identity,
                    request.current_observation,
                    request.command,
                    task_events,
                )
                .await;
                send_prompt_if_requested(session, channel, request.prompt)?;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => {
                send_command_error(session, channel, error.into(), request.prompt)?;
                Ok(true)
            }
        }
    }

    async fn handle_pre_runtime_command_effects(
        &self,
        channel: ChannelId,
        session: &mut Session,
        identity: &AuthIdentity,
        command: &SemanticCommand,
        current_observation: &JsonObservation,
    ) -> Result<()> {
        if let SemanticCommand::Say { text } = command {
            let app = self.shared.app_service().await;
            let app_identity = AppIdentity::new(identity.user.clone(), identity.player_id.clone());
            let say_events = app
                .handle(
                    &app_identity,
                    AppRequest::Say {
                        current_view: &current_observation.view_id,
                        text,
                    },
                )
                .await?;
            self.send_ui_events(channel, session, say_events).await?;
        } else {
            self.dispatch_live_message(command, identity).await?;
        }
        Ok(())
    }

    async fn execute_runtime_command(
        &self,
        channel: ChannelId,
        session: &mut Session,
        identity: &AuthIdentity,
        before_observation: &JsonObservation,
        command: SemanticCommand,
        prompt: bool,
    ) -> Result<()> {
        let should_quit = matches!(command, SemanticCommand::Quit);
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
                send_prompt_if_requested(session, channel, prompt)?;
                return Ok(());
            }
        };
        let observation_for_task = observation.clone();
        if let Err(error) = self
            .send_ui_events(
                channel,
                session,
                vec![
                    UiEvent::PersistPlayerState(player_state),
                    UiEvent::CommandObservation {
                        command: command.clone(),
                        observation,
                    },
                ],
            )
            .await
        {
            send_command_error(session, channel, error, prompt)?;
            return Ok(());
        }
        let app_identity = AppIdentity::new(identity.user.clone(), identity.player_id.clone());
        self.record_resident_task_step_after_observation(
            &app_identity,
            before_observation,
            &command,
            observation_for_task,
        )
        .await;
        if should_quit {
            session.exit_status_request(channel, 0)?;
            session.close(channel)?;
        } else {
            send_prompt_if_requested(session, channel, prompt)?;
        }

        Ok(())
    }
}

fn merge_presence_users(
    first: impl IntoIterator<Item = RecentPresenceUser>,
    second: impl IntoIterator<Item = RecentPresenceUser>,
) -> Vec<RecentPresenceUser> {
    let mut by_user = HashMap::<String, u64>::new();
    for RecentPresenceUser { user, age_millis } in first.into_iter().chain(second) {
        by_user
            .entry(user)
            .and_modify(|current_age| *current_age = (*current_age).min(age_millis))
            .or_insert(age_millis);
    }

    let mut users = by_user
        .into_iter()
        .map(|(user, age_millis)| RecentPresenceUser { user, age_millis })
        .collect::<Vec<_>>();
    users.sort_by(|left, right| {
        left.age_millis
            .cmp(&right.age_millis)
            .then_with(|| left.user.cmp(&right.user))
    });
    users
}

fn presence_usernames(users: &[RecentPresenceUser]) -> Vec<String> {
    users.iter().map(|user| user.user.clone()).collect()
}

fn observation_contains_room_escape(observation: &JsonObservation) -> bool {
    observation.events.iter().any(|event| {
        matches!(
            event,
            ObservationEvent::Message { text }
                if text == "This room is closed. You step back to the street."
        )
    })
}

fn parse_command_feedback(
    line: &str,
    room_binding: Option<&StoredRoomBinding>,
    error: &SlashParseError,
) -> String {
    if matches!(error, SlashParseError::UnknownCommand)
        && room_binding.is_some_and(|binding| binding.is_service_room())
        && line.trim_start().starts_with('/')
    {
        service_room_unavailable_text().to_owned()
    } else if matches!(error, SlashParseError::UnknownCommand)
        && !line.trim_start().starts_with('/')
    {
        "World commands start with /. Choose an Available command such as /help or /look."
            .to_owned()
    } else {
        slash_parse_feedback(line, error)
    }
}

fn send_prompt_if_requested(session: &mut Session, channel: ChannelId, prompt: bool) -> Result<()> {
    if prompt {
        send_prompt(session, channel)?;
    }
    Ok(())
}
