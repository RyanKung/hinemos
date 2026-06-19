use super::contracts::{
    BuildStore, LandStore, ParcelStore, ParcelView, PaymentRequestView, PaymentStore, ShopStore,
    TransferView,
};
use super::rendering::{
    custom_command_preview, is_custom_command_input, non_empty, render_parcel_detail,
    render_parcel_list,
};
use super::results::{
    BuildCommandResult, BusinessListResult, CommercialParcelCacheInvalidation, LandCommandResult,
    PayAcceptResult, PayDirectResult, ShopPaymentRequestResult, build_help_text,
    default_build_commands,
};
use crate::{
    AppIdentity, AppService, BuildSheet, LiveInboxNotice, MailAuthTokenView, OperatorCommandView,
    PARCEL_STATUS_BUILT, RoomBindingKindView, RoomCommandPolicyView, UiEvent,
};

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

    /// Creates a payment request for a shop command.
    pub async fn request_shop_payment(
        &self,
        command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<ShopPaymentRequestResult<<S as ShopStore>::InboxItem>, E> {
        let request = self
            .store
            .create_payment_request(command_id, owner_player_id, amount, delivery)
            .await?;
        let inbox_item = self
            .store
            .inbox_item_by_source(request.payer_player_id(), "payment_request", request.id())
            .await?;
        Ok(ShopPaymentRequestResult {
            text: format!(
                "Created payment request #{} for {}: {} {}. Delivery is locked until payment.\r\n",
                request.id(),
                request.payer_user(),
                request.amount(),
                request.asset()
            ),
            payer_player_id: request.payer_player_id().to_owned(),
            inbox_item,
        })
    }

    /// Handles a raw or slash-prefixed input line inside a commercial parcel room.
    pub async fn handle_commercial_parcel_input<P>(
        &self,
        identity: &AppIdentity,
        binding: &P,
        raw_line: &str,
    ) -> Result<Option<Vec<UiEvent>>, E>
    where
        P: ParcelView + RoomBindingKindView + RoomCommandPolicyView + Sync,
    {
        if !self.room_binding_accepts_input(binding, raw_line) {
            return Ok(None);
        }
        if binding.status() != PARCEL_STATUS_BUILT {
            return Ok(None);
        }
        let Some(owner_player_id) = ParcelView::owner_player_id(binding) else {
            return Ok(None);
        };
        if owner_player_id == identity.player_id.as_str() {
            if is_custom_command_input(binding, raw_line) {
                return Ok(Some(vec![UiEvent::Text(format!(
                    "You own this shop. Visitors use {} here; their requests arrive in your inbox and /shop inbox.\r\n",
                    raw_line.split_whitespace().next().unwrap_or("this command")
                ))]));
            }
            return Ok(None);
        }
        if !is_custom_command_input(binding, raw_line) {
            return Ok(None);
        }

        let command = self
            .store
            .save_operator_command(binding, &identity.user, &identity.player_id, raw_line, true)
            .await?;
        let inbox_player_id = ParcelView::room_player_id(binding).unwrap_or(owner_player_id);
        let inbox_item = self
            .store
            .inbox_item_by_source(inbox_player_id, "operator_command", command.id())
            .await?;
        let mut events = vec![UiEvent::Text(format!(
            "Shop request #{} sent to owner {} for parcel {}.\r\nStatus: delivered.\r\n{}",
            command.id(),
            command.owner_user(),
            command.parcel_id(),
            custom_command_preview(binding, raw_line)
                .map(|preview| format!("Preview: {preview}\r\n"))
                .unwrap_or_default()
        ))];
        events.push(UiEvent::LiveInboxNotice {
            target_player_id: owner_player_id.to_owned(),
            notice: LiveInboxNotice::from_item(&inbox_item),
        });
        Ok(Some(events))
    }
}

impl<S, E> AppService<S>
where
    S: ParcelStore<Error = E>,
{
    /// Builds the commercial parcel list text.
    pub async fn land_list(&self) -> Result<BusinessListResult, E> {
        let parcels = self.store.list_commercial_parcels().await?;
        Ok(BusinessListResult {
            text: render_parcel_list(&parcels).replace('\n', "\r\n"),
        })
    }
}

impl<S, E> AppService<S>
where
    S: LandStore<Error = E>,
{
    /// Builds the commercial parcel detail text.
    pub async fn land_info(&self, parcel_id: &str) -> Result<BusinessListResult, E> {
        let parcel = self.store.commercial_parcel(parcel_id).await?;
        Ok(BusinessListResult {
            text: render_parcel_detail(&parcel).replace('\n', "\r\n"),
        })
    }

    /// Claims a parcel and creates the room mailbox token.
    pub async fn claim_land(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<LandCommandResult, E> {
        let parcel = self
            .store
            .claim_commercial_parcel(parcel_id, owner_user, owner_player_id)
            .await?;
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, owner_player_id, token)
            .await?;
        Ok(LandCommandResult {
            text: format!(
                "Claimed parcel {}. Room mail account: {}. Token: {}\r\nUse this token from the room owner process with SMTP/IMAP. You can rotate it later with /land token {}.\r\nBuild here with /build {{\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}}, then /build publish. From the street, enter with /enter {}. Custom commands are auto-filled if omitted.\r\n",
                parcel.parcel_id(),
                mail.username(),
                token,
                parcel.parcel_id(),
                parcel.parcel_id()
            ),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Transfers a parcel and rotates its room mailbox token for the new owner.
    pub async fn transfer_land(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
        token: &str,
    ) -> Result<LandCommandResult, E> {
        let parcel = self
            .store
            .transfer_commercial_parcel(parcel_id, owner_player_id, target)
            .await?;
        let new_owner_player_id = parcel.owner_player_id().unwrap_or_default();
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, new_owner_player_id, token)
            .await?;
        Ok(LandCommandResult {
            text: format!(
                "Transferred parcel {} to {}.\r\nNew room mail account: {}. Token: {}\r\nGive this token to the new room owner process; the old room token has been rotated.\r\n",
                parcel.parcel_id(),
                parcel.owner_user().unwrap_or("unknown"),
                mail.username(),
                token
            ),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Rotates a parcel room mailbox token.
    pub async fn rotate_land_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<LandCommandResult, E> {
        let mail = self
            .store
            .set_room_mail_auth_token(parcel_id, owner_player_id, token)
            .await?;
        Ok(LandCommandResult {
            text: format!(
                "Room mail account for {}: {}\r\nToken: {}\r\nUse SMTP/IMAP with this username/token. This token is shown once; run /land token {} again to rotate it.\r\n",
                parcel_id,
                mail.username(),
                token,
                parcel_id
            ),
            invalidate: None,
        })
    }
}

impl<S, E> AppService<S>
where
    S: BuildStore<Error = E> + ShopStore<Error = E>,
{
    /// Renders the build help text.
    pub fn build_help_text(&self) -> &'static str {
        build_help_text()
    }

    /// Applies a structured parcel build sheet.
    pub async fn apply_build_sheet(
        &self,
        current_view: &str,
        owner_player_id: &str,
        sheet: &BuildSheet,
    ) -> Result<BuildCommandResult, E> {
        let mut updated = Vec::new();
        let mut latest_parcel = None;
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
            latest_parcel = Some(
                self.store
                    .update_parcel_build_field(current_view, owner_player_id, field, value)
                    .await?,
            );
            updated.push(field);
        }
        if non_empty(sheet.commands.as_deref()).is_none() {
            latest_parcel = Some(
                self.store
                    .update_parcel_build_field(
                        current_view,
                        owner_player_id,
                        "commands",
                        default_build_commands(),
                    )
                    .await?,
            );
            updated.push("commands");
        }
        if updated.is_empty() {
            updated.push("commands");
        }
        Ok(BuildCommandResult {
            text: format!(
                "Updated build sheet for current parcel: {}.\r\n",
                updated.join(", ")
            ),
            invalidate: latest_parcel.map(|parcel| CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Updates one parcel build field.
    pub async fn set_build_field(
        &self,
        current_view: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<BuildCommandResult, E> {
        let parcel = self
            .store
            .update_parcel_build_field(current_view, owner_player_id, field, value)
            .await?;
        Ok(BuildCommandResult {
            text: format!("Updated {} for parcel {}.\r\n", field, parcel.parcel_id()),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }

    /// Publishes the current parcel build sheet.
    pub async fn publish_build(
        &self,
        current_view: &str,
        owner_player_id: &str,
    ) -> Result<BuildCommandResult, E> {
        let parcel = self
            .store
            .publish_parcel_build(current_view, owner_player_id)
            .await?;
        Ok(BuildCommandResult {
            text: format!(
                "Published parcel {} as a built shop.\r\n",
                parcel.parcel_id()
            ),
            invalidate: Some(CommercialParcelCacheInvalidation {
                view_id: parcel.view_id().to_owned(),
                front_view_id: parcel.front_view_id().to_owned(),
            }),
        })
    }
}

impl<S, E> AppService<S>
where
    S: PaymentStore<Error = E>,
{
    /// Builds pending payment request text.
    pub async fn pending_pay_requests(
        &self,
        payer_player_id: &str,
    ) -> Result<BusinessListResult, E> {
        let requests = self
            .store
            .pending_payment_requests(payer_player_id, 20)
            .await?;
        let mut lines = vec![String::new(), "Payment Requests".to_owned()];
        if requests.is_empty() {
            lines.push("No pending payment requests.".to_owned());
        } else {
            for request in requests.iter().rev() {
                lines.push(format!(
                    "\n=== Payment Request #{} ===\nShop: {} ({})\nAmount: {} {}\nFor: shop command #{}\nDelivery: locked until payment\nAccept: /pay accept {}\nReject: ignore this request\n==========================",
                    request.id(),
                    request.parcel_id(),
                    request.payee_user(),
                    request.amount(),
                    request.asset(),
                    request.operator_command_id(),
                    request.id()
                ));
            }
        }
        lines.push(String::new());
        Ok(BusinessListResult {
            text: lines.join("\r\n").replace('\n', "\r\n"),
        })
    }

    /// Executes a direct MARK payment.
    pub async fn pay_direct(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<PayDirectResult<S::Transfer>, E> {
        let transfer = self
            .store
            .transfer_mark(sender_user, sender_player_id, target, amount, memo)
            .await?;
        Ok(PayDirectResult {
            text: format!(
                "Paid {} {} to {}. Ledger #{}. Balance: {} {}.\r\n",
                transfer.amount(),
                transfer.asset(),
                transfer.target_user(),
                transfer.ledger_id(),
                transfer.sender_balance(),
                transfer.asset()
            ),
            transfer,
        })
    }

    /// Accepts a pending payment request.
    pub async fn accept_pay_request(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<PayAcceptResult<S::PaymentRequest>, E> {
        let (request, sender_balance) = self
            .store
            .accept_payment_request(payer_user, payer_player_id, request_id)
            .await?;
        Ok(PayAcceptResult {
            text: format!(
                "Paid payment request #{}: {} {} to {}. Balance: {} {}.\r\nUnlocked content: {}\r\n",
                request.id(),
                request.amount(),
                request.asset(),
                request.payee_user(),
                sender_balance,
                request.asset(),
                request.delivery()
            ),
            request,
        })
    }
}
