use crate::*;
use hinemos_core::generated_grid_label;

impl<S> AppService<S> {
    /// Returns true when a room binding should be visible given the current observation entities.
    #[must_use]
    pub fn room_binding_is_visible(
        &self,
        binding: &impl RoomBindingEntryView,
        visible_entity_ids: &[String],
    ) -> bool {
        binding
            .front_entity_id()
            .is_none_or(|front_entity_id| visible_entity_ids.iter().any(|id| id == front_entity_id))
    }

    /// Normalizes `/enter` targets for protocol-neutral matching.
    #[must_use]
    pub fn normalize_enter_target(&self, target: &str) -> String {
        target.trim().to_ascii_lowercase()
    }

    /// Returns true when a normalized `/enter` target matches a room binding.
    #[must_use]
    pub fn room_binding_enter_matches(
        &self,
        binding: &impl RoomBindingEntryView,
        normalized: &str,
    ) -> bool {
        self.normalize_enter_target(binding.address()) == normalized
            || self.normalize_enter_target(binding.label()) == normalized
            || binding
                .enter_aliases()
                .iter()
                .any(|alias| self.normalize_enter_target(alias) == normalized)
    }

    /// Resolves a visible room entrance from already-loaded room bindings.
    #[must_use]
    pub fn visible_room_enter_events(
        &self,
        target: &str,
        visible_entity_ids: &[String],
        bindings: &[impl RoomBindingEntryView],
    ) -> Option<Vec<UiEvent>> {
        let normalized = self.normalize_enter_target(target);
        if normalized.is_empty() {
            return None;
        }
        let binding = bindings.iter().find(|binding| {
            self.room_binding_is_visible(*binding, visible_entity_ids)
                && self.room_binding_enter_matches(*binding, &normalized)
        })?;
        Some(vec![
            UiEvent::Text(format!("You enter {}.\r\n", binding.label())),
            UiEvent::Relocate {
                target_view: RoomBindingEntryView::view_id(binding).to_owned(),
                direction: None,
                message: None,
            },
        ])
    }

    /// Builds a local error for an `/enter` target that is not visible here.
    #[must_use]
    pub fn unavailable_room_enter_events(
        &self,
        target: &str,
        current_title: &str,
        visible_entity_ids: &[String],
        bindings: &[impl RoomBindingEntryView],
    ) -> Vec<UiEvent> {
        let available = bindings
            .iter()
            .filter(|binding| self.room_binding_is_visible(*binding, visible_entity_ids))
            .map(|binding| format!("/enter {}", binding.address()))
            .collect::<Vec<_>>();
        let trimmed_target = target.trim();
        let target_label = if trimmed_target.is_empty() {
            "that place"
        } else {
            trimmed_target
        };
        let text = if available.is_empty() {
            format!(
                "No entrance named {target_label} is visible from {current_title}. Move with /go until the place appears in Available.\r\n"
            )
        } else {
            format!(
                "No entrance named {target_label} is visible from {current_title}. Available entrances here: {}.\r\n",
                available.join(", ")
            )
        };
        vec![UiEvent::Text(text)]
    }

    /// Returns true when a room binding forwards the given raw input line.
    #[must_use]
    pub fn room_binding_accepts_input(
        &self,
        binding: &impl RoomCommandPolicyView,
        raw_input: &str,
    ) -> bool {
        binding.forwards_all_input()
            || binding
                .listed_commands()
                .iter()
                .any(|command| command_template_matches_input(command, raw_input))
    }
}

pub(crate) fn command_template_matches_input(command: &str, raw_input: &str) -> bool {
    let command = command.trim();
    if command.is_empty() {
        return false;
    }
    let prefix = command.split('<').next().unwrap_or(command).trim_end();
    if prefix.is_empty() {
        return false;
    }
    let command = prefix.to_ascii_lowercase();
    raw_input
        .trim_start()
        .to_ascii_lowercase()
        .starts_with(&command)
}

pub(crate) fn recovery_command_template_matches_input(command: &str, raw_input: &str) -> bool {
    let command = command.trim();
    if command.is_empty() {
        return false;
    }
    let has_placeholder = command.contains('<');
    let prefix = command.split('<').next().unwrap_or(command).trim_end();
    if prefix.is_empty() {
        return false;
    }
    let command = prefix.to_ascii_lowercase();
    let raw_input = raw_input.trim().to_ascii_lowercase();
    if !has_placeholder {
        return raw_input == command;
    }
    raw_input
        .strip_prefix(&command)
        .is_some_and(has_placeholder_argument)
}

fn has_placeholder_argument(rest: &str) -> bool {
    rest.chars().next().is_some_and(char::is_whitespace) && !rest.trim().is_empty()
}

impl<S, E> AppService<S>
where
    S: RoomStore<Error = E>,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
    <S as RoomStore>::RoomBinding: RoomBindingKindView,
{
    /// Loads a service-room binding by view id when the room is actually a service room.
    pub async fn service_room_binding_by_view(
        &self,
        room_view: &str,
    ) -> Result<Option<S::RoomBinding>, E> {
        let Some(binding) = self.store.room_binding_by_view(room_view).await? else {
            return Ok(None);
        };
        if binding.is_service_room() {
            Ok(Some(binding))
        } else {
            Ok(None)
        }
    }

    /// Builds service-room contextual help from stored room data.
    pub fn service_room_help_text(&self, room: &impl ServiceRoomView) -> String {
        let mut lines = vec![
            "Room commands:".to_owned(),
            "- /look, /map, /help, /quit".to_owned(),
            "- /go south leaves this room".to_owned(),
            "- /say <text> speaks locally and forwards a copy to the room service".to_owned(),
            "- /inventory, /history, /memory, /who, /settings, /mailbox, /balance remain available"
                .to_owned(),
        ];
        let commands = command_inputs(room.custom_commands()).collect::<Vec<_>>();
        if !commands.is_empty() {
            lines.push(format!("- local: {}", commands.join(", ")));
        }
        let recovery_commands = command_inputs(room.recovery_commands()).collect::<Vec<_>>();
        if !recovery_commands.is_empty() {
            lines.push(format!(
                "- hunger recovery: {}",
                recovery_commands.join(", ")
            ));
        }
        format!("{}\r\n", lines.join("\n"))
    }

    /// Builds the structured observation shown inside an externally hosted service room.
    pub fn service_room_observation_for(
        &self,
        player_id: &str,
        room: &impl ServiceRoomView,
    ) -> JsonObservation {
        let title = room.label().unwrap_or_else(|| room.view_id()).to_owned();
        let return_label = service_room_return_label(room.front_view_id());
        let mut available_commands = vec![
            SemanticCommand::Look,
            SemanticCommand::Map,
            SemanticCommand::Inventory,
            SemanticCommand::History,
            SemanticCommand::Memory {
                rest: "<command>".to_owned(),
            },
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
        available_commands.extend(extension_commands(room.custom_commands()));

        JsonObservation {
            player_id: player_id.to_owned(),
            view_id: room.view_id().to_owned(),
            title: title.clone(),
            ascii_art: vec![
                "============================================================".to_owned(),
                format!("                  {}", title.to_ascii_uppercase()),
                "============================================================".to_owned(),
                "                           <Me>".to_owned(),
                "                            |".to_owned(),
                format!(
                    "                    south to {}",
                    return_label.as_deref().unwrap_or("street")
                ),
            ],
            description:
                "This externally hosted room is connected through the room mailbox protocol."
                    .to_owned(),
            exits: vec![ExitObservation {
                direction: Direction::South,
                target_known: room.front_view_id().is_some(),
                label: return_label,
            }],
            entities: Vec::new(),
            online_users: Vec::new(),
            available_commands,
            events: Vec::new(),
        }
    }
}

fn service_room_return_label(front_view_id: Option<&str>) -> Option<String> {
    let view_id = front_view_id?;
    if let Some(label) = generated_grid_label(view_id) {
        return Some(label);
    }
    Some(match view_id {
        "arrival_street" => "Harbor Square".to_owned(),
        "west_main_street" => "West Hinemos Blvd".to_owned(),
        "official_street" => "East Hinemos Blvd".to_owned(),
        view_id if view_id.starts_with("street_north_") => "Agentopia Blvd North".to_owned(),
        view_id if view_id.starts_with("street_south_") => "Agentopia Blvd South".to_owned(),
        view_id => humanize_view_id(view_id),
    })
}

fn humanize_view_id(view_id: &str) -> String {
    view_id
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = first.to_ascii_uppercase().to_string();
                    word.push_str(chars.as_str());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl<S, E> AppService<S>
where
    S: MailStore<Error = E>,
{
    /// Persists a service-room say message, echoes it locally, and emits a room-view broadcast event.
    pub async fn handle_service_room_say<M>(
        &self,
        identity: &AppIdentity,
        current_view: &str,
        mailbox: &M,
        text: &str,
    ) -> Result<Vec<UiEvent>, E>
    where
        M: RoomMailboxView + Sync,
    {
        let forwarded_input = format!("/say {text}");
        let result = self
            .forward_room_mailbox_input(
                mailbox,
                &identity.user,
                &identity.player_id,
                &forwarded_input,
            )
            .await?;
        Ok(vec![
            UiEvent::Text(format!("You say: {text}\r\n{}", result.text)),
            UiEvent::LiveViewMessage {
                view_id: current_view.to_owned(),
                text: format!("[say from {}] {text}", identity.user),
            },
        ])
    }

    /// Forwards player input to a room mailbox principal.
    pub async fn forward_room_mailbox_input<M>(
        &self,
        mailbox: &M,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
    ) -> Result<RoomInputResult<S::InboxItem>, E>
    where
        M: RoomMailboxView + Sync,
    {
        let inbox_item = self
            .store
            .save_room_mailbox_input(mailbox, sender_user, sender_player_id, raw_input)
            .await?;
        Ok(RoomInputResult {
            text: format!(
                "Sent to room service {} (request #{}). Replies arrive in your mailbox with subject Re: #{}; use /mailbox, then /mail read <inbox-id> for that reply.\r\n",
                mailbox.room_user().unwrap_or("unknown"),
                inbox_item.id(),
                inbox_item.id()
            ),
            inbox_item,
        })
    }
}

impl<S, E> AppService<S>
where
    S: MailStore<Error = E> + RoomStore<Error = E>,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
    <S as RoomStore>::RoomBinding:
        RoomBindingKindView + RoomMailboxView + ServiceRoomView + RoomCommandPolicyView + Sync,
{
    /// Handles a parsed command inside a service room.
    pub async fn handle_service_room_command_for_binding(
        &self,
        identity: &AppIdentity,
        current_view: &str,
        binding: &<S as RoomStore>::RoomBinding,
        command: &SemanticCommand,
    ) -> Result<Vec<UiEvent>, E> {
        Ok(match command {
            SemanticCommand::Look | SemanticCommand::Map => vec![UiEvent::Observation(
                self.service_room_observation_for(&identity.player_id, binding),
            )],
            SemanticCommand::Move {
                direction: Direction::South,
            } => {
                let target_view = ServiceRoomView::front_view_id(binding)
                    .unwrap_or(&self.config.admission_view_id)
                    .to_owned();
                vec![UiEvent::Relocate {
                    target_view,
                    direction: Some(Direction::South),
                    message: Some(service_room_leave_text().to_owned()),
                }]
            }
            SemanticCommand::Move { .. } => {
                vec![UiEvent::Text(service_room_blocked_exit_text().to_owned())]
            }
            SemanticCommand::Say { text } => {
                self.handle_service_room_say(identity, current_view, binding, text)
                    .await?
            }
            SemanticCommand::Help => {
                vec![UiEvent::Text(self.service_room_help_text(binding))]
            }
            SemanticCommand::Quit => vec![
                UiEvent::Text(format!("{}\r\n", FEEDBACK_QUIT)),
                UiEvent::CloseSession(0),
            ],
            SemanticCommand::Extension { input, .. } => {
                if self.room_binding_accepts_input(binding, input) {
                    let result = self
                        .forward_room_mailbox_input(
                            binding,
                            &identity.user,
                            &identity.player_id,
                            input,
                        )
                        .await?;
                    vec![UiEvent::Text(result.text)]
                } else {
                    vec![UiEvent::Text(service_room_unavailable_text().to_owned())]
                }
            }
            _ => vec![UiEvent::Text(service_room_unavailable_text().to_owned())],
        })
    }
}

impl<S, E> AppService<S>
where
    S: AccountStore<Error = E>
        + AdmissionStore<Error = E>
        + BuildStore<Error = E>
        + InboxStore<Error = E>
        + ParcelOwnershipStore<Error = E>
        + MailStore<Error = E>
        + MemoryStore<Error = E>
        + MessageStore<Error = E>
        + ParcelRegistryStore<Error = E>
        + ParcelStore<Error = E>
        + PaymentStore<Error = E>
        + RoomStore<Error = E>,
    E: FromMailingListValidation + FromParcelBadgeValidation + FromParcelWorkValidation,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
    <S as RoomStore>::RoomBinding: RoomBindingEntryView
        + ParcelView
        + RoomBindingKindView
        + RoomCommandPolicyView
        + RoomMailboxView
        + ServiceRoomView
        + Sync,
{
    /// Handles a single raw input line against a known room binding.
    pub async fn handle_room_line_for_binding(
        &self,
        identity: &AppIdentity,
        binding: &<S as RoomStore>::RoomBinding,
        raw_line: &str,
    ) -> Result<Option<Vec<UiEvent>>, E>
    where
        <S as RoomStore>::RoomBinding: RoomBindingKindView + RoomMailboxView,
    {
        if !raw_line.trim_start().starts_with('/') {
            return Ok(None);
        }
        if let Some(events) = self.handle_memory_raw_line(identity, raw_line).await? {
            return Ok(Some(events));
        }
        if RoomBindingKindView::is_parcel(binding)
            && let Some(events) = self
                .handle_parcel_input(identity, binding, raw_line)
                .await?
        {
            return Ok(Some(events));
        }
        if RoomBindingKindView::is_service_room(binding)
            && self.room_binding_accepts_input(binding, raw_line)
        {
            let result = self
                .forward_room_mailbox_input(binding, &identity.user, &identity.player_id, raw_line)
                .await?;
            return Ok(Some(vec![UiEvent::Text(result.text)]));
        }
        Ok(None)
    }
}

/// Storage boundary for room lookup and unified room bindings.
pub trait RoomStore {
    /// Store error type.
    type Error;
    /// Stored service room type.
    type ServiceRoom;
    /// Unified room binding type.
    type RoomBinding;

    /// Loads an enabled service room by runtime view id.
    async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error>;

    /// Loads all room bindings visible from a front view.
    async fn room_bindings_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error>;

    /// Loads a unified room binding by room view id.
    async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error>;
}

/// Protocol-neutral view of a room command forwarding policy.
pub trait RoomCommandPolicyView {
    /// Returns true when every unhandled input line should be forwarded.
    fn forwards_all_input(&self) -> bool;

    /// Returns the explicit command templates forwarded by this room, if any.
    fn listed_commands(&self) -> &[String];
}

/// Protocol-neutral view of a room entrance binding.
pub trait RoomBindingEntryView {
    /// Room view id entered by the player.
    fn view_id(&self) -> &str;

    /// Optional entity that must be visible before this entrance is usable.
    fn front_entity_id(&self) -> Option<&str>;

    /// Short player-entered address.
    fn address(&self) -> &str;

    /// Player-facing label.
    fn label(&self) -> &str;

    /// Explicit enter aliases.
    fn enter_aliases(&self) -> &[String];
}

/// Protocol-neutral view of a room binding's role metadata.
pub trait RoomBindingKindView: RoomMailboxView {
    /// True when this binding represents a parcel room.
    fn is_parcel(&self) -> bool;

    /// True when this binding represents an externally hosted service room.
    fn is_service_room(&self) -> bool;
}

/// Protocol-neutral view of a room mailbox principal.
pub trait RoomMailboxView {
    /// Runtime view id for the room.
    fn view_id(&self) -> &str;

    /// Room-owned mailbox username.
    fn room_user(&self) -> Option<&str>;

    /// Room-owned mailbox player id.
    fn room_player_id(&self) -> Option<&str>;
}

/// Protocol-neutral view of a service room.
pub trait ServiceRoomView: RoomMailboxView {
    /// Optional player-facing label.
    fn label(&self) -> Option<&str>;

    /// Optional entrance address.
    fn address(&self) -> Option<&str>;

    /// Optional street/front view id.
    fn front_view_id(&self) -> Option<&str>;

    /// Optional player-facing status text appended to the room observation.
    fn status_text(&self) -> Option<&str>;

    /// Data-authored command help for this room, if any.
    fn custom_commands(&self) -> Option<&str>;

    /// Data-authored hunger recovery commands for this room, if any.
    fn recovery_commands(&self) -> Option<&str>;
}

/// Text shown when a command is not available inside a service room.
#[must_use]
pub const fn service_room_unavailable_text() -> &'static str {
    "That command is not available inside this room. Leave with /go south.\r\n"
}

/// Text shown when a service room movement direction is unavailable.
#[must_use]
pub const fn service_room_blocked_exit_text() -> &'static str {
    "This room only has an exit to the south.\r\n"
}

/// Text shown after leaving a service room.
#[must_use]
pub const fn service_room_leave_text() -> &'static str {
    "You step back outside."
}

pub(crate) fn command_inputs(commands: Option<&str>) -> impl Iterator<Item = String> + '_ {
    commands
        .unwrap_or_default()
        .split(['\n', ';'])
        .filter_map(|entry| {
            let entry = entry.trim();
            let command = entry.split_whitespace().next()?;
            command.starts_with('/').then(|| entry.to_owned())
        })
}
