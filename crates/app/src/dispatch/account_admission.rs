use crate::*;

use super::events::text_events;
use super::route::{AdmissionAppRequest, SettingsAppRequest};

impl<S, E> AppService<S>
where
    S: AdmissionStore<Error = E>,
{
    pub(super) async fn handle_admission_request(
        &self,
        identity: &AppIdentity,
        request: AdmissionAppRequest,
    ) -> Result<Vec<UiEvent>, E> {
        match request {
            AdmissionAppRequest::Read => {
                self.handle_pending_admission_read(&identity.player_id)
                    .await
            }
            AdmissionAppRequest::Accept => {
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
        }
    }
}

impl<S, E> AppService<S>
where
    S: AccountStore<Error = E>,
{
    pub(super) async fn handle_settings_request(
        &self,
        identity: &AppIdentity,
        request: SettingsAppRequest<'_>,
    ) -> Result<Vec<UiEvent>, E> {
        let text = match request {
            SettingsAppRequest::Show { mail_address } => {
                self.show_account_settings(&identity.user, &identity.player_id, mail_address)
                    .await?
                    .text
            }
            SettingsAppRequest::RotateMailToken {
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
        };
        Ok(text_events(text, None))
    }
}
