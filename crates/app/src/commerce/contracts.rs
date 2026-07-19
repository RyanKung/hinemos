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

/// Error adapter for app-level shop work validation.
pub trait FromShopWorkValidation {
    /// Builds an invalid shop-work error.
    fn invalid_shop_work(message: &str) -> Self;
}

impl FromShopWorkValidation for std::convert::Infallible {
    fn invalid_shop_work(_message: &str) -> Self {
        unreachable!("infallible test stores do not reject shop-work validation")
    }
}

/// Error adapter for app-level shop badge validation.
pub trait FromShopBadgeValidation {
    /// Builds an invalid shop badge error.
    fn invalid_shop_badge(message: &str) -> Self;
}

impl FromShopBadgeValidation for std::convert::Infallible {
    fn invalid_shop_badge(_message: &str) -> Self {
        unreachable!("infallible test stores do not reject shop badge validation")
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

    /// Active member count.
    fn subscriber_count(&self) -> i64;

    /// Creation timestamp.
    fn created_at(&self) -> &str;
}

/// Protocol-neutral view of a shop mailing-list member.
pub trait ShopMailingListSubscriberView {
    /// Subscriber username.
    fn subscriber_user(&self) -> &str;

    /// Subscriber player id.
    fn subscriber_player_id(&self) -> &str;

    /// Last subscription update timestamp.
    fn updated_at(&self) -> &str;
}

/// Protocol-neutral view of the current player's shop-chat membership.
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

/// Protocol-neutral view of a shop-local work desk.
pub trait ShopWorkDeskView {
    /// Work-desk id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable work-desk slug.
    fn slug(&self) -> &str;

    /// Player-facing title.
    fn title(&self) -> &str;

    /// Desk status.
    fn status(&self) -> &str;

    /// Queued work item count.
    fn queued_count(&self) -> i64;

    /// Active on-site worker count.
    fn active_worker_count(&self) -> i64;

    /// Creation timestamp.
    fn created_at(&self) -> &str;
}

/// Protocol-neutral view of a shop-local staff assignment.
pub trait ShopStaffView {
    /// Assigned worker username.
    fn staff_user(&self) -> &str;

    /// Assignment status.
    fn status(&self) -> &str;

    /// Last update timestamp.
    fn updated_at(&self) -> &str;
}

/// Protocol-neutral view of an in-shop work shift.
pub trait ShopShiftView {
    /// Shift id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable work-desk slug.
    fn slug(&self) -> &str;

    /// Worker username.
    fn worker_user(&self) -> &str;

    /// Shift status.
    fn status(&self) -> &str;

    /// Start timestamp.
    fn started_at(&self) -> &str;

    /// End timestamp when present.
    fn ended_at(&self) -> Option<&str>;
}

/// Protocol-neutral view of one shop-local work item.
pub trait ShopWorkItemView {
    /// Work item id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable work-desk slug.
    fn slug(&self) -> &str;

    /// Work-desk title.
    fn desk_title(&self) -> &str;

    /// Operator command id that produced this item.
    fn operator_command_id(&self) -> i64;

    /// Slash command prefix matched by the route.
    fn command_prefix(&self) -> &str;

    /// Work status.
    fn status(&self) -> &str;

    /// Visitor username.
    fn sender_user(&self) -> &str;

    /// Raw visitor command.
    fn raw_input(&self) -> &str;

    /// Assigned worker username, if claimed.
    fn assignee_user(&self) -> Option<&str>;

    /// Completion result, if done.
    fn result(&self) -> Option<&str>;

    /// Creation timestamp.
    fn created_at(&self) -> &str;

    /// Last update timestamp.
    fn updated_at(&self) -> &str;
}

/// Protocol-neutral view of a shop command route.
pub trait ShopCommandRouteView {
    /// Route id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable work-desk slug.
    fn slug(&self) -> &str;

    /// Work-desk title.
    fn desk_title(&self) -> &str;

    /// Slash command prefix that is routed.
    fn command_prefix(&self) -> &str;

    /// Creation timestamp.
    fn created_at(&self) -> &str;
}

/// Protocol-neutral view of a shop badge definition.
pub trait ShopBadgeDefinitionView {
    /// Badge definition id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Stable badge slug.
    fn slug(&self) -> &str;

    /// Player-facing badge title.
    fn title(&self) -> &str;

    /// Optional one-line description.
    fn description(&self) -> Option<&str>;

    /// Active award count for this badge.
    fn active_award_count(&self) -> i64;

    /// Creation timestamp.
    fn created_at(&self) -> &str;

    /// Last update timestamp.
    fn updated_at(&self) -> &str;
}

/// Protocol-neutral view of a shop badge award.
pub trait ShopBadgeAwardView {
    /// Badge award id.
    fn id(&self) -> i64;

    /// Parcel id.
    fn parcel_id(&self) -> &str;

    /// Shop title, if the shop is built.
    fn shop_title(&self) -> Option<&str>;

    /// Stable badge slug.
    fn slug(&self) -> &str;

    /// Player-facing badge title.
    fn badge_title(&self) -> &str;

    /// Optional badge description.
    fn badge_description(&self) -> Option<&str>;

    /// Issuer username.
    fn issuer_user(&self) -> &str;

    /// Issuer player id.
    fn issuer_player_id(&self) -> &str;

    /// Recipient username.
    fn recipient_user(&self) -> &str;

    /// Recipient player id.
    fn recipient_player_id(&self) -> &str;

    /// Optional award note.
    fn note(&self) -> Option<&str>;

    /// Award status.
    fn status(&self) -> &str;

    /// Issue timestamp.
    fn awarded_at(&self) -> &str;

    /// Revocation timestamp.
    fn revoked_at(&self) -> Option<&str>;
}

/// Member page for an owner mailing-list inspection.
pub struct ShopMailingListSubscriberPage<S> {
    /// Total active member count.
    pub total: i64,
    /// Recent active members.
    pub subscribers: Vec<S>,
}

/// One inbox delivery created for a mailing-list or shop-chat post.
pub struct ShopMailingListDelivery<I> {
    /// Recipient player id for live notification routing.
    pub recipient_player_id: String,
    /// Inbox item created or reused for the recipient.
    pub inbox_item: I,
}

/// Result from sending a mailing-list or shop-chat post.
pub struct ShopMailingListSend<P, I> {
    /// Stored post.
    pub post: P,
    /// Created or reused inbox deliveries.
    pub deliveries: Vec<ShopMailingListDelivery<I>>,
}

/// Inputs for sending one shop mailing-list post from inside the shop.
pub struct ShopMailingListPostInput<'a> {
    /// Current runtime view id.
    pub current_view: &'a str,
    /// Parcel id or visible shop title.
    pub target: &'a str,
    /// Stable list slug.
    pub slug: &'a str,
    /// Sender username.
    pub sender_user: &'a str,
    /// Sender player id.
    pub sender_player_id: &'a str,
    /// Inbox subject.
    pub subject: &'a str,
    /// Inbox body.
    pub body: &'a str,
}

/// Inputs for awarding a shop badge from inside the shop.
pub struct ShopBadgeAwardInput<'a> {
    /// Current runtime view id.
    pub current_view: &'a str,
    /// Parcel id.
    pub parcel_id: &'a str,
    /// Stable badge slug.
    pub slug: &'a str,
    /// Issuer username.
    pub issuer_user: &'a str,
    /// Issuer player id.
    pub issuer_player_id: &'a str,
    /// Target username or player id.
    pub target: &'a str,
    /// Optional one-line award note.
    pub note: Option<&'a str>,
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
    /// Stored shop command-route type.
    type CommandRoute: ShopCommandRouteView;
    /// Stored shop work-desk type.
    type WorkDesk: ShopWorkDeskView;
    /// Stored shop staff assignment type.
    type Staff: ShopStaffView;
    /// Stored shop shift type.
    type Shift: ShopShiftView;
    /// Stored shop work item type.
    type WorkItem: ShopWorkItemView;
    /// Stored shop badge definition type.
    type BadgeDefinition: ShopBadgeDefinitionView;
    /// Stored shop badge award type.
    type BadgeAward: ShopBadgeAwardView;

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

    /// Loads one stored shop operator command.
    async fn operator_command(&self, command_id: i64)
    -> Result<Self::OperatorCommand, Self::Error>;

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

    /// Resolves a visible shop target and slug to one mailing list.
    async fn shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
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

    /// Sends one mailing-list post to all active members.
    async fn send_shop_mailing_list_post(
        &self,
        target: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<ShopMailingListSend<Self::MailingListPost, Self::InboxItem>, Self::Error>;

    /// Creates a shop-local work desk for an owned shop parcel.
    async fn create_shop_work_desk(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<Self::WorkDesk, Self::Error>;

    /// Lists shop-local work desks for an owned shop parcel.
    async fn shop_work_desks(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::WorkDesk>, Self::Error>;

    /// Adds or reactivates a worker assignment for one work desk.
    async fn add_shop_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<Self::Staff, Self::Error>;

    /// Lists staff assignments for one owned work desk.
    async fn shop_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::Staff>, Self::Error>;

    /// Removes a worker assignment from one work desk.
    async fn remove_shop_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<Self::Staff, Self::Error>;

    /// Starts an active in-shop shift for an assigned worker.
    async fn start_shop_shift(
        &self,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<Self::Shift, Self::Error>;

    /// Ends the worker's active in-shop shift for one desk.
    async fn end_shop_shift(
        &self,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<Self::Shift, Self::Error>;

    /// Lists available or claimed work visible to an active in-shop worker.
    async fn shop_work_items(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        slug: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::WorkItem>, Self::Error>;

    /// Claims one queued work item for an active in-shop worker.
    async fn claim_shop_work(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
    ) -> Result<Self::WorkItem, Self::Error>;

    /// Completes one claimed work item for an active in-shop worker.
    async fn finish_shop_work(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
        result: &str,
    ) -> Result<Self::WorkItem, Self::Error>;

    /// Creates or returns a shop command route for an owned work desk.
    async fn add_shop_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<Self::CommandRoute, Self::Error>;

    /// Lists shop command routes for an owned parcel.
    async fn shop_command_routes(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::CommandRoute>, Self::Error>;

    /// Removes a shop command route from an owned work desk.
    async fn remove_shop_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<Self::CommandRoute, Self::Error>;

    /// Dispatches one saved operator command into matching work queues.
    async fn dispatch_shop_command_routes<P>(
        &self,
        parcel: &P,
        command_id: i64,
    ) -> Result<Vec<Self::WorkItem>, Self::Error>
    where
        P: ParcelView + Sync;

    /// Creates or updates a badge definition for an owned shop parcel.
    async fn create_shop_badge(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<Self::BadgeDefinition, Self::Error>;

    /// Lists badge definitions for an owned shop parcel.
    async fn shop_badges(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::BadgeDefinition>, Self::Error>;

    /// Awards a badge from an owned shop to a target player.
    async fn award_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        issuer_user: &str,
        issuer_player_id: &str,
        target: &str,
        note: Option<&str>,
    ) -> Result<Self::BadgeAward, Self::Error>;

    /// Revokes an active badge award from an owned shop.
    async fn revoke_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<Self::BadgeAward, Self::Error>;

    /// Lists active badges for one player id.
    async fn shop_badges_for_player(
        &self,
        player_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::BadgeAward>, Self::Error>;

    /// Lists active public badges for one username or player id.
    async fn shop_badges_for_target(
        &self,
        target: &str,
        limit: i64,
    ) -> Result<Vec<Self::BadgeAward>, Self::Error>;
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
