use crate::{InboxItemView, MailAuthTokenView, OperatorCommandView};

/// Error adapter for app-level mailing-list validation.
pub trait FromMailingListValidation {
    /// Builds an invalid mailing-list error.
    fn invalid_mailing_list(message: &str) -> Self;
}

impl FromMailingListValidation for std::convert::Infallible {
    fn invalid_mailing_list(_message: &str) -> Self {
        unreachable!("infallible test stores do not reject mailing-list validation")
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

/// Protocol-neutral view of a shop mailing list.
pub trait ShopMailingListView {
    /// Mailing-list id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable list slug.
    fn slug(&self) -> &str;

    /// Player-facing title.
    fn title(&self) -> &str;

    /// List status.
    fn status(&self) -> &str;

    /// Active subscriber count.
    fn subscriber_count(&self) -> i64;

    /// Creation timestamp.
    fn created_at(&self) -> &str;
}

/// Protocol-neutral view of a shop mailing-list subscriber.
pub trait ShopMailingListSubscriberView {
    /// Subscriber username.
    fn subscriber_user(&self) -> &str;

    /// Subscriber player id.
    fn subscriber_player_id(&self) -> &str;

    /// Last subscription update timestamp.
    fn updated_at(&self) -> &str;
}

/// Protocol-neutral view of the current player's mailing-list subscription.
pub trait ShopMailingListSubscriptionView {
    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Shop title.
    fn shop_title(&self) -> Option<&str>;

    /// Stable list slug.
    fn slug(&self) -> &str;

    /// Mailing-list title.
    fn list_title(&self) -> &str;

    /// Subscription status.
    fn status(&self) -> &str;

    /// Last subscription update timestamp.
    fn updated_at(&self) -> &str;
}

/// Protocol-neutral view of a mailing-list post.
pub trait ShopMailingListPostView {
    /// Post id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable list slug.
    fn slug(&self) -> &str;

    /// List title.
    fn list_title(&self) -> &str;

    /// Inbox subject.
    fn subject(&self) -> &str;

    /// Recipient count.
    fn recipient_count(&self) -> i64;
}

/// Subscriber page for an owner mailing-list inspection.
pub struct ShopMailingListSubscriberPage<S> {
    /// Total active subscriber count.
    pub total: i64,
    /// Recent active subscribers.
    pub subscribers: Vec<S>,
}

/// One inbox delivery created for a mailing-list post.
pub struct ShopMailingListDelivery<I> {
    /// Recipient player id for live notification routing.
    pub recipient_player_id: String,
    /// Inbox item created or reused for the recipient.
    pub inbox_item: I,
}

/// Result from sending a mailing-list post.
pub struct ShopMailingListSend<P, I> {
    /// Stored post.
    pub post: P,
    /// Created or reused inbox deliveries.
    pub deliveries: Vec<ShopMailingListDelivery<I>>,
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
    /// Stored shop mailing-list type.
    type MailingList: ShopMailingListView;
    /// Stored shop mailing-list subscriber type.
    type MailingListSubscriber: ShopMailingListSubscriberView;
    /// Stored shop mailing-list subscription type.
    type MailingListSubscription: ShopMailingListSubscriptionView;
    /// Stored shop mailing-list post type.
    type MailingListPost: ShopMailingListPostView;

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

    /// Creates a mailing list for an owned shop parcel.
    async fn create_shop_mailing_list(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<Self::MailingList, Self::Error>;

    /// Lists mailing lists for an owned shop parcel.
    async fn shop_mailing_lists(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::MailingList>, Self::Error>;

    /// Lists recent active subscribers for an owned shop mailing list.
    async fn shop_mailing_list_subscribers(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<ShopMailingListSubscriberPage<Self::MailingListSubscriber>, Self::Error>;

    /// Closes an owned shop mailing list to new subscriptions.
    async fn close_shop_mailing_list(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<Self::MailingList, Self::Error>;

    /// Subscribes a player to an open shop mailing list.
    async fn subscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<Self::MailingListSubscription, Self::Error>;

    /// Unsubscribes a player from a shop mailing list.
    async fn unsubscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<Self::MailingListSubscription, Self::Error>;

    /// Lists active subscriptions for a player.
    async fn shop_mailing_list_subscriptions(
        &self,
        subscriber_player_id: &str,
    ) -> Result<Vec<Self::MailingListSubscription>, Self::Error>;

    /// Sends one mailing-list post to all active subscribers.
    async fn send_shop_mailing_list_post(
        &self,
        parcel_id: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<ShopMailingListSend<Self::MailingListPost, Self::InboxItem>, Self::Error>;
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
