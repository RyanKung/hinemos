use crate::{InboxItemView, MailAuthTokenView, OperatorCommandView};

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

/// Storage boundary for land ownership actions.
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
