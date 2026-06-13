use crate::*;

impl<S, E> AppService<S>
where
    S: ShopStore<Error = E>,
{
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
        if owner_player_id == &identity.player_id {
            if is_custom_command_input(binding, raw_line) {
                return Ok(Some(vec![UiEvent::Text(format!(
                    "You own this shop. Visitors use {} here; their requests arrive in your inbox and /shop inbox.\r\n",
                    raw_line.split_whitespace().next().unwrap_or("this command")
                ))]));
            }
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

/// Storage boundary for commercial parcel lookup.
pub trait ParcelStore {
    /// Store error type.
    type Error;
    /// Stored parcel type.
    type Parcel: ParcelView;

    /// Lists all commercial parcels.
    async fn list_commercial_parcels(&self) -> Result<Vec<Self::Parcel>, Self::Error>;

    /// Lists parcels visible from a front view.
    async fn commercial_parcels_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::Parcel>, Self::Error>;
}

/// Protocol-neutral view of a commercial parcel.
pub trait ParcelView {
    /// Parcel id shown to players.
    fn parcel_id(&self) -> &str;

    /// Runtime view id for this parcel room.
    fn view_id(&self) -> &str;

    /// Street/front view id where this parcel is visible.
    fn front_view_id(&self) -> &str;

    /// Parcel district.
    fn district(&self) -> &str;

    /// Parcel position inside its district.
    fn position(&self) -> i32;

    /// Owner username, if this parcel is claimed.
    fn owner_user(&self) -> Option<&str>;

    /// Owner player id, if this parcel is claimed.
    fn owner_player_id(&self) -> Option<&str>;

    /// Room mailbox username, if provisioned.
    fn room_user(&self) -> Option<&str>;

    /// Room mailbox player id, if provisioned.
    fn room_player_id(&self) -> Option<&str>;

    /// Parcel status.
    fn status(&self) -> &str;

    /// Built shop title, if any.
    fn title(&self) -> Option<&str>;

    /// Built shop description, if any.
    fn description(&self) -> Option<&str>;

    /// Owner-authored style note, if any.
    fn style(&self) -> Option<&str>;

    /// Owner-authored operator prompt, if any.
    fn operator_prompt(&self) -> Option<&str>;

    /// Owner-authored custom command help, if any.
    fn custom_commands(&self) -> Option<&str>;
}

/// Protocol-neutral view of a mail auth token identity.

pub trait LandStore {
    /// Store error type.
    type Error;
    /// Stored parcel type.
    type Parcel: ParcelView;
    /// Stored mail auth token type.
    type MailAuthToken: MailAuthTokenView;

    /// Loads a commercial parcel by id.
    async fn commercial_parcel(&self, parcel_id: &str) -> Result<Self::Parcel, Self::Error>;

    /// Claims a free parcel for a player.
    async fn claim_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
    ) -> Result<Self::Parcel, Self::Error>;

    /// Transfers a parcel to another player.
    async fn transfer_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<Self::Parcel, Self::Error>;

    /// Sets or rotates the room mailbox token for a parcel.
    async fn set_room_mail_auth_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<Self::MailAuthToken, Self::Error>;
}

/// Storage boundary for parcel build-sheet actions.
pub trait BuildStore {
    /// Store error type.
    type Error;
    /// Stored parcel type.
    type Parcel: ParcelView;

    /// Updates one field on the current parcel build sheet.
    async fn update_parcel_build_field(
        &self,
        view_id: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<Self::Parcel, Self::Error>;

    /// Publishes the current parcel build sheet.
    async fn publish_parcel_build(
        &self,
        view_id: &str,
        owner_player_id: &str,
    ) -> Result<Self::Parcel, Self::Error>;
}

/// Protocol-neutral view of a MARK transfer.
pub trait TransferView {
    /// Transferred amount.
    fn amount(&self) -> i64;

    /// Asset symbol.
    fn asset(&self) -> &str;

    /// Target user that received the transfer.
    fn target_user(&self) -> &str;

    /// Ledger row id.
    fn ledger_id(&self) -> i64;

    /// Sender balance after the transfer.
    fn sender_balance(&self) -> i64;

    /// Transfer memo.
    fn memo(&self) -> &str;
}

/// Protocol-neutral view of a payment request.
pub trait PaymentRequestView {
    /// Payment request id.
    fn id(&self) -> i64;

    /// Operator command id.
    fn operator_command_id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Payer username.
    fn payer_user(&self) -> &str;

    /// Payer player id.
    fn payer_player_id(&self) -> &str;

    /// Payee username.
    fn payee_user(&self) -> &str;

    /// Payee player id.
    fn payee_player_id(&self) -> &str;

    /// Asset symbol.
    fn asset(&self) -> &str;

    /// Requested amount.
    fn amount(&self) -> i64;

    /// Delivery content unlocked after payment.
    fn delivery(&self) -> &str;
}

/// Storage boundary for shop operator actions.
pub trait ShopStore {
    /// Store error type.
    type Error;
    /// Stored parcel type.
    type Parcel: ParcelView;
    /// Stored payment request type.
    type PaymentRequest: PaymentRequestView;
    /// Stored inbox item type.
    type InboxItem: InboxItemView;
    /// Stored operator command type.
    type OperatorCommand: OperatorCommandView;

    /// Persists a visitor command for a shop operator.
    async fn save_operator_command<P>(
        &self,
        parcel: &P,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        delivered: bool,
    ) -> Result<Self::OperatorCommand, Self::Error>
    where
        P: ParcelView + Sync;

    /// Lists recent operator commands for a shop owner.
    async fn recent_operator_commands(
        &self,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::OperatorCommand>, Self::Error>;

    /// Creates a payment request from a shop command.
    async fn create_payment_request(
        &self,
        operator_command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<Self::PaymentRequest, Self::Error>;

    /// Loads an inbox item by idempotent source.
    async fn inbox_item_by_source(
        &self,
        recipient_player_id: &str,
        source_kind: &str,
        source_id: i64,
    ) -> Result<Self::InboxItem, Self::Error>;
}

/// Storage boundary for payment actions.
pub trait PaymentStore {
    /// Store error type.
    type Error;
    /// Stored transfer type.
    type Transfer: TransferView;
    /// Stored payment request type.
    type PaymentRequest: PaymentRequestView;

    /// Lists pending payment requests for a player.
    async fn pending_payment_requests(
        &self,
        payer_player_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::PaymentRequest>, Self::Error>;

    /// Transfers MARK directly to another account.
    async fn transfer_mark(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<Self::Transfer, Self::Error>;

    /// Accepts a pending payment request.
    async fn accept_payment_request(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<(Self::PaymentRequest, i64), Self::Error>;
}

/// Result from creating a shop payment request.

pub struct ShopPaymentRequestResult<I> {
    /// Text to display to the shop operator.
    pub text: String,
    /// Payer player id for live delivery.
    pub payer_player_id: String,
    /// Inbox item generated for the payer.
    pub inbox_item: I,
}

/// Result from a direct payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayDirectResult<T> {
    /// Text to display to the payer.
    pub text: String,
    /// Transfer details for optional live delivery.
    pub transfer: T,
}

/// Result from accepting a payment request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayAcceptResult<R> {
    /// Text to display to the payer.
    pub text: String,
    /// Paid request details for optional live delivery.
    pub request: R,
}

/// Result from a read-only business listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessListResult {
    /// Text to display to the user.
    pub text: String,
}

/// Result from a land operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandCommandResult {
    /// Text to display to the user.
    pub text: String,
    /// Optional commercial parcel cache invalidation.
    pub invalidate: Option<CommercialParcelCacheInvalidation>,
}

/// Result from a build operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildCommandResult {
    /// Text to display to the user.
    pub text: String,
    /// Optional commercial parcel cache invalidation.
    pub invalidate: Option<CommercialParcelCacheInvalidation>,
}

/// Cache key for a commercial parcel and its front view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialParcelCacheInvalidation {
    /// Parcel view id.
    pub view_id: String,
    /// Front/street view where the parcel is visible.
    pub front_view_id: String,
}

/// Default custom commands for a parcel build sheet when the owner omits commands.
#[must_use]
pub const fn default_build_commands() -> &'static str {
    "/hello preview=hello price=25; /status"
}

/// Help text for parcel build commands.
#[must_use]
pub const fn build_help_text() -> &'static str {
    "Build commands for the current owned parcel:\r\n\
     /build {\"title\":\"shop title\",\"description\":\"shop description\",\"style\":\"style note\",\"prompt\":\"operator prompt\"}\r\n\
     Optional JSON field: \"commands\". If omitted, commands are auto-filled.\r\n\
     Legacy field commands still work for manual correction: /build title <text>, /build description <text>, /build style <text>, /build prompt <text>, /build commands <text>\r\n\
     /build publish\r\n\
     After publishing, visitor slash commands inside the shop become inbox items for the owner.\r\n"
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn render_parcel_list(parcels: &[impl ParcelView]) -> String {
    let mut lines = vec!["Commercial Parcels".to_owned()];
    let mut vacant_count = 0_u32;
    for parcel in parcels {
        match parcel.status() {
            PARCEL_STATUS_BUILT => lines.push(format!(
                "- {}: {}. Owner: {}. Enter from street: /enter {}.",
                parcel.parcel_id(),
                parcel.title().unwrap_or("built shop"),
                parcel.owner_user().unwrap_or("unknown"),
                parcel.parcel_id()
            )),
            PARCEL_STATUS_CLAIMED => lines.push(format!(
                "- {}: claimed by {}; not built yet.",
                parcel.parcel_id(),
                parcel.owner_user().unwrap_or("unknown")
            )),
            _ => {
                vacant_count += 1;
                lines.push(format!(
                    "- {}: vacant. Claim: /land claim {}.",
                    parcel.parcel_id(),
                    parcel.parcel_id()
                ));
            }
        }
    }
    if vacant_count == 0 {
        lines.push("No vacant parcels right now. Use /land info <parcel> for details.".to_owned());
    } else {
        lines.push(format!(
            "{vacant_count} vacant parcel(s). Use /land claim <parcel>, /land token <parcel>, /land info <parcel>, or /land transfer <parcel> <user>."
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_parcel_detail(parcel: &impl ParcelView) -> String {
    format!(
        "Parcel {}\nView: {}\nDistrict: {} {}\nStatus: {}\nOwner: {}\nRoom mail: {}\nTitle: {}\nDescription: {}\nStyle: {}\nPrompt: {}\nCommands: {}\n\n",
        parcel.parcel_id(),
        parcel.view_id(),
        parcel.district(),
        parcel.position(),
        parcel.status(),
        parcel.owner_user().unwrap_or("-"),
        parcel.room_user().unwrap_or("-"),
        parcel.title().unwrap_or("-"),
        parcel.description().unwrap_or("-"),
        parcel.style().unwrap_or("-"),
        parcel.operator_prompt().unwrap_or("-"),
        parcel.custom_commands().unwrap_or("-")
    )
}

fn custom_command_preview(parcel: &impl ParcelView, raw_input: &str) -> Option<String> {
    let command = raw_input.split_whitespace().next()?;
    let commands = parcel.custom_commands()?;
    for entry in commands.split(['\n', ';']) {
        let entry = entry.trim();
        if !entry.starts_with(command) {
            continue;
        }
        let Some(preview) = command_field_value(entry, "preview=") else {
            continue;
        };
        let preview = preview.trim();
        if !preview.is_empty() {
            return Some(preview.to_owned());
        }
    }
    None
}

fn is_custom_command_input(parcel: &impl ParcelView, raw_input: &str) -> bool {
    let Some(input_command) = raw_input.split_whitespace().next() else {
        return false;
    };
    custom_command_inputs(parcel)
        .any(|command| command.split_whitespace().next() == Some(input_command))
}

fn custom_command_inputs(parcel: &impl ParcelView) -> impl Iterator<Item = String> + '_ {
    parcel
        .custom_commands()
        .unwrap_or_default()
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|command| command.starts_with('/'))
        .map(str::to_owned)
}

fn command_field_value(entry: &str, field: &str) -> Option<String> {
    let start = entry.find(field)? + field.len();
    let value = entry[start..].trim_start();
    if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    if let Some(rest) = value.strip_prefix('\'') {
        let end = rest.find('\'').unwrap_or(rest.len());
        return Some(rest[..end].trim().to_owned());
    }
    Some(
        value
            .split_whitespace()
            .next()
            .unwrap_or(value)
            .trim()
            .to_owned(),
    )
}
