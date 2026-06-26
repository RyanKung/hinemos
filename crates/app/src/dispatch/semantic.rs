use crate::*;

use super::events::text_events;
use super::request_mapping::{
    build_request, inbox_request, land_request, payment_request, shop_request,
};
use super::route::ReadAppRequest;
use super::{AppDispatchStore, AppViewCommandContext};

struct WorldViewCommandContext<'a> {
    player_id: &'a str,
    current_view: &'a str,
    current_title: &'a str,
    inventory: &'a [String],
    online_users: &'a [String],
    who_population: WhoPopulation,
}

impl<S> AppService<S>
where
    S: AppDispatchStore,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
    <S as RoomStore>::RoomBinding: RoomBindingEntryView
        + ParcelView
        + RoomBindingKindView
        + RoomCommandPolicyView
        + RoomMailboxView
        + ServiceRoomView
        + Sync,
{
    /// Handles semantic business commands that do not need core runtime execution.
    pub async fn handle_semantic_business_command(
        &self,
        identity: &AppIdentity,
        command: &SemanticCommand,
        context: AppCommandContext<'_>,
    ) -> Result<Option<Vec<UiEvent>>, <S as AppDispatchStore>::Error> {
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
            SemanticCommand::Settings { action } => {
                let Some(update) = RoleCardUpdate::from_settings_action(action) else {
                    return Ok(None);
                };
                AppRequest::SettingsUpdateRoleCard {
                    mail_address: &mail_address,
                    update,
                }
            }
            SemanticCommand::Pay { action } => payment_request(action),
            SemanticCommand::Inbox { action } => inbox_request(action, context.mail_domain),
            SemanticCommand::Land { action } => land_request(action, context.generated_token),
            SemanticCommand::Build { action } => build_request(action, context.current_view),
            SemanticCommand::Shop { action } => shop_request(action),
            _ => return Ok(None),
        };
        Ok(Some(self.handle(identity, request).await?))
    }

    /// Handles commands routed from the current room or page view.
    pub async fn handle_view_command(
        &self,
        identity: &AppIdentity,
        command: &SemanticCommand,
        context: AppViewCommandContext<'_, <S as RoomStore>::RoomBinding>,
    ) -> Result<Option<Vec<UiEvent>>, <S as AppDispatchStore>::Error> {
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
                        WorldViewCommandContext {
                            player_id: &identity.player_id,
                            current_view: context.current_view,
                            current_title: context.current_title,
                            inventory: context.inventory,
                            online_users: context.online_users,
                            who_population: context.who_population,
                        },
                    )
                    .await;
            }
            _ => {}
        }
        if matches!(command, SemanticCommand::Inbox { .. }) {
            return self
                .handle_semantic_business_command(identity, command, context.business)
                .await;
        }
        if context
            .room_binding
            .is_some_and(RoomBindingKindView::is_service_room)
            && let Some(binding) = context.room_binding
        {
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
        self.handle_semantic_business_command(identity, command, context.business)
            .await
    }
}

impl<S, E> AppService<S>
where
    S: MessageStore<Error = E>,
{
    /// Handles semantic commands that are routed from the current room view.
    async fn handle_world_view_command(
        &self,
        command: &SemanticCommand,
        context: WorldViewCommandContext<'_>,
    ) -> Result<Option<Vec<UiEvent>>, E> {
        let events = match command {
            SemanticCommand::Inventory => {
                Some(text_events(render_inventory(context.inventory), None))
            }
            SemanticCommand::History => Some(text_events(
                self.room_history(context.current_view, context.current_title)
                    .await?
                    .text,
                None,
            )),
            SemanticCommand::Who => Some(text_events(
                render_who(
                    context.current_view,
                    context.online_users,
                    context.who_population,
                ),
                None,
            )),
            SemanticCommand::News => Some(text_events(self.world_news().await?.text, None)),
            SemanticCommand::Balance => Some(text_events(
                render_player_balance(self.store.player_balance(context.player_id).await?),
                None,
            )),
            _ => None,
        };
        Ok(events)
    }
}

impl<S, E> AppService<S>
where
    S: MemoryStore<Error = E> + MessageStore<Error = E>,
{
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
            self.handle_read_request(
                identity,
                ReadAppRequest::MemoryCommand { rest: rest.trim() },
            )
            .await?,
        ))
    }
}

impl<S, E> AppService<S>
where
    S: AdmissionStore<Error = E>,
{
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
            SemanticCommand::Settings { .. } => Ok(PendingAdmissionCommandOutcome::PassThrough),
            SemanticCommand::Read { target }
                if target.id == self.config.admission_board_entity_id =>
            {
                let events = self
                    .handle_pending_admission_read(&identity.player_id)
                    .await?;
                Ok(PendingAdmissionCommandOutcome::Allow(events))
            }
            SemanticCommand::Agree { .. } => {
                let events = self
                    .handle_admission_request(identity, super::route::AdmissionAppRequest::Accept)
                    .await?;
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
