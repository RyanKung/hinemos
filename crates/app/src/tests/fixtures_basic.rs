use super::*;

#[derive(Debug, Default)]
pub(super) struct TestMessageStore {
    pub(super) calls: Mutex<Vec<String>>,
}

#[derive(Debug, Default)]
pub(super) struct TestPlayerStateStore {
    pub(super) calls: Mutex<Vec<String>>,
}

#[derive(Debug, Default)]
pub(super) struct TestPresenceStore {
    pub(super) calls: Mutex<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestWorldMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestBalance;

#[derive(Debug, Default)]
pub(super) struct TestInboxStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestMailToken {
    pub(super) username: String,
    pub(super) player_id: String,
}

#[derive(Debug, Clone)]
pub(super) struct TestCommercialParcel {
    pub(super) parcel_id: &'static str,
    pub(super) view_id: &'static str,
    pub(super) front_view_id: &'static str,
    pub(super) district: &'static str,
    pub(super) position: i32,
    pub(super) owner_user: Option<String>,
    pub(super) owner_player_id: Option<String>,
    pub(super) room_user: Option<String>,
    pub(super) room_player_id: Option<String>,
    pub(super) status: &'static str,
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    pub(super) style: Option<String>,
    pub(super) operator_prompt: Option<String>,
    pub(super) custom_commands: Option<String>,
}

#[derive(Debug)]
pub(super) struct TestCommercialStore {
    pub(super) parcel: Mutex<TestCommercialParcel>,
    pub(super) calls: Mutex<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestPaymentRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestOperatorCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestInboxItem {
    pub(super) id: i64,
    pub(super) kind: &'static str,
    pub(super) sender_user: &'static str,
    pub(super) subject: &'static str,
    pub(super) body: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestMailingList {
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) status: String,
    pub(super) subscriber_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestMailingListSubscriber;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestMailingListSubscription {
    pub(super) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestMailingListPost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestShopBadge {
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) description: Option<String>,
    pub(super) active_award_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestShopBadgeAward {
    pub(super) status: String,
    pub(super) recipient_user: String,
    pub(super) note: Option<String>,
}

impl WorldMessageView for TestWorldMessage {
    fn kind(&self) -> &str {
        "say"
    }

    fn sender_user(&self) -> &str {
        "sender"
    }

    fn body(&self) -> &str {
        "body"
    }

    fn created_at(&self) -> &str {
        "created"
    }

    fn expires_at(&self) -> Option<&str> {
        None
    }
}

impl BalanceView for TestBalance {
    fn account_id(&self) -> &str {
        "account"
    }

    fn asset(&self) -> &str {
        "MARK"
    }

    fn amount(&self) -> i64 {
        1000
    }
}

impl InboxItemView for TestInboxItem {
    fn id(&self) -> i64 {
        self.id
    }

    fn kind(&self) -> &str {
        self.kind
    }

    fn sender_user(&self) -> &str {
        self.sender_user
    }

    fn subject(&self) -> &str {
        self.subject
    }

    fn body(&self) -> &str {
        self.body
    }

    fn status(&self) -> &str {
        "open"
    }

    fn attempts(&self) -> i32 {
        0
    }

    fn lease_until(&self) -> Option<&str> {
        None
    }

    fn created_at(&self) -> &str {
        "created"
    }
}

impl PlayerStateStore for TestPlayerStateStore {
    type Error = std::convert::Infallible;

    async fn load_player_state(&self, player_id: &str) -> Result<Option<PlayerState>, Self::Error> {
        self.calls.lock().unwrap().push(format!("load:{player_id}"));
        Ok(Some(PlayerState {
            id: player_id.to_owned(),
            current_view: "arrival_street".to_owned(),
            inventory: Vec::new(),
        }))
    }

    async fn save_player_state(&self, player: &PlayerState) -> Result<(), Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("save:{}:{}", player.id, player.current_view));
        Ok(())
    }
}

impl ViewPresenceStore for TestPresenceStore {
    type Error = std::convert::Infallible;

    async fn record_view_presence(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<(), Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("presence:{username}:{player_id}:{view_id}"));
        Ok(())
    }

    async fn recent_active_users(
        &self,
        _within_seconds: i64,
    ) -> Result<Vec<RecentPresenceUser>, Self::Error> {
        Ok(Vec::new())
    }

    async fn recent_active_view_users(
        &self,
        _view_id: &str,
        _excluded_player_id: &str,
        _within_seconds: i64,
    ) -> Result<Vec<RecentPresenceUser>, Self::Error> {
        Ok(Vec::new())
    }
}

impl MessageStore for TestMessageStore {
    type Error = std::convert::Infallible;
    type WorldMessage = TestWorldMessage;
    type Balance = TestBalance;

    async fn recent_view_messages(
        &self,
        _view_id: &str,
        _limit: i64,
    ) -> Result<Vec<Self::WorldMessage>, Self::Error> {
        Ok(Vec::new())
    }

    async fn recent_news_messages(
        &self,
        _limit: i64,
    ) -> Result<Vec<Self::WorldMessage>, Self::Error> {
        Ok(Vec::new())
    }

    async fn player_balance(&self, _player_id: &str) -> Result<Self::Balance, Self::Error> {
        Ok(TestBalance)
    }

    async fn save_say_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target_view: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "say:{sender_user}:{sender_player_id}:{target_view}:{body}"
        ));
        Ok(())
    }

    async fn save_mail_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mail:{sender_user}:{sender_player_id}:{target}:{body}"
        ));
        Ok(())
    }

    async fn save_mail_message_with_subject(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mail:{sender_user}:{sender_player_id}:{target}:{subject}:{body}"
        ));
        Ok(())
    }

    async fn save_broadcast_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<(), Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("broadcast:{sender_user}:{sender_player_id}:{body}"));
        Ok(())
    }
}

impl InboxStore for TestInboxStore {
    type Error = std::convert::Infallible;
    type InboxItem = TestInboxItem;

    async fn list_inbox_items(
        &self,
        _username: &str,
        _player_id: &str,
        _status: Option<&str>,
        _limit: i64,
    ) -> Result<Vec<Self::InboxItem>, Self::Error> {
        Ok(vec![
            TestInboxItem {
                id: 1,
                kind: "mail",
                sender_user: "alice",
                subject: "hello",
                body: "body",
            },
            TestInboxItem {
                id: 2,
                kind: "mail",
                sender_user: "bob",
                subject: "hi",
                body: "body2",
            },
        ])
    }

    async fn read_inbox_item(
        &self,
        _username: &str,
        _player_id: &str,
        item_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        Ok(TestInboxItem {
            id: item_id,
            kind: "mail",
            sender_user: "alice",
            subject: "hello",
            body: "body",
        })
    }

    async fn claim_inbox_item(
        &self,
        _username: &str,
        _player_id: &str,
        item_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        self.read_inbox_item("", "", item_id).await
    }

    async fn finish_inbox_item(
        &self,
        _username: &str,
        _player_id: &str,
        item_id: i64,
        _status: &str,
    ) -> Result<Self::InboxItem, Self::Error> {
        self.read_inbox_item("", "", item_id).await
    }
}

impl MailAuthTokenView for TestMailToken {
    fn username(&self) -> &str {
        &self.username
    }

    fn player_id(&self) -> &str {
        &self.player_id
    }
}

impl ParcelView for TestCommercialParcel {
    fn parcel_id(&self) -> &str {
        self.parcel_id
    }

    fn view_id(&self) -> &str {
        self.view_id
    }

    fn front_view_id(&self) -> &str {
        self.front_view_id
    }

    fn district(&self) -> &str {
        self.district
    }

    fn position(&self) -> i32 {
        self.position
    }

    fn owner_user(&self) -> Option<&str> {
        self.owner_user.as_deref()
    }

    fn owner_player_id(&self) -> Option<&str> {
        self.owner_player_id.as_deref()
    }

    fn room_user(&self) -> Option<&str> {
        self.room_user.as_deref()
    }

    fn room_player_id(&self) -> Option<&str> {
        self.room_player_id.as_deref()
    }

    fn status(&self) -> &str {
        self.status
    }

    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn style(&self) -> Option<&str> {
        self.style.as_deref()
    }

    fn operator_prompt(&self) -> Option<&str> {
        self.operator_prompt.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.custom_commands.as_deref()
    }
}

impl RoomMailboxView for TestCommercialParcel {
    fn view_id(&self) -> &str {
        self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        self.room_user.as_deref()
    }

    fn room_player_id(&self) -> Option<&str> {
        self.room_player_id.as_deref()
    }
}

impl RoomBindingKindView for TestCommercialParcel {
    fn is_commercial_parcel(&self) -> bool {
        true
    }

    fn is_service_room(&self) -> bool {
        false
    }
}

impl RoomCommandPolicyView for TestCommercialParcel {
    fn forwards_all_input(&self) -> bool {
        true
    }

    fn listed_commands(&self) -> &[String] {
        &[]
    }
}

impl PaymentRequestView for TestPaymentRequest {
    fn id(&self) -> i64 {
        1
    }

    fn operator_command_id(&self) -> i64 {
        1
    }

    fn parcel_id(&self) -> &str {
        "parcel"
    }

    fn payer_user(&self) -> &str {
        "payer"
    }

    fn payer_player_id(&self) -> &str {
        "payer-player"
    }

    fn payee_user(&self) -> &str {
        "payee"
    }

    fn payee_player_id(&self) -> &str {
        "payee-player"
    }

    fn amount(&self) -> i64 {
        1
    }

    fn delivery(&self) -> &str {
        "delivery"
    }

    fn asset(&self) -> &str {
        "MARK"
    }
}

impl OperatorCommandView for TestOperatorCommand {
    fn id(&self) -> i64 {
        1
    }

    fn created_at(&self) -> &str {
        "created"
    }

    fn status(&self) -> &str {
        "pending"
    }

    fn sender_user(&self) -> &str {
        "sender"
    }

    fn owner_user(&self) -> &str {
        "owner"
    }

    fn parcel_id(&self) -> &str {
        "parcel"
    }

    fn raw_input(&self) -> &str {
        "input"
    }
}

impl ShopMailingListView for TestMailingList {
    fn id(&self) -> i64 {
        1
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn slug(&self) -> &str {
        &self.slug
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn subscriber_count(&self) -> i64 {
        self.subscriber_count
    }

    fn created_at(&self) -> &str {
        "created"
    }
}

impl ShopMailingListSubscriberView for TestMailingListSubscriber {
    fn subscriber_user(&self) -> &str {
        "visitor"
    }

    fn subscriber_player_id(&self) -> &str {
        "visitor-player"
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ShopMailingListSubscriptionView for TestMailingListSubscription {
    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn shop_title(&self) -> Option<&str> {
        Some("Parcel")
    }

    fn slug(&self) -> &str {
        "updates"
    }

    fn list_title(&self) -> &str {
        "Shop Updates"
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ShopMailingListPostView for TestMailingListPost {
    fn id(&self) -> i64 {
        7
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn slug(&self) -> &str {
        "updates"
    }

    fn list_title(&self) -> &str {
        "Shop Updates"
    }

    fn subject(&self) -> &str {
        "Weekly Deal"
    }

    fn recipient_count(&self) -> i64 {
        1
    }
}

impl ShopBadgeDefinitionView for TestShopBadge {
    fn id(&self) -> i64 {
        11
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn slug(&self) -> &str {
        &self.slug
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn active_award_count(&self) -> i64 {
        self.active_award_count
    }

    fn created_at(&self) -> &str {
        "created"
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ShopBadgeAwardView for TestShopBadgeAward {
    fn id(&self) -> i64 {
        17
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn shop_title(&self) -> Option<&str> {
        Some("Parcel")
    }

    fn slug(&self) -> &str {
        "patron"
    }

    fn badge_title(&self) -> &str {
        "Good Patron"
    }

    fn badge_description(&self) -> Option<&str> {
        Some("Paid and polite")
    }

    fn issuer_user(&self) -> &str {
        "owner"
    }

    fn issuer_player_id(&self) -> &str {
        "owner-player"
    }

    fn recipient_user(&self) -> &str {
        &self.recipient_user
    }

    fn recipient_player_id(&self) -> &str {
        "visitor-player"
    }

    fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn awarded_at(&self) -> &str {
        "awarded"
    }

    fn revoked_at(&self) -> Option<&str> {
        None
    }
}

impl TestCommercialStore {
    fn parcel(&self) -> TestCommercialParcel {
        self.parcel.lock().unwrap().clone()
    }

    fn update_parcel(
        &self,
        mutate: impl FnOnce(&mut TestCommercialParcel),
    ) -> TestCommercialParcel {
        let mut parcel = self.parcel.lock().unwrap();
        mutate(&mut parcel);
        parcel.clone()
    }
}

impl LandStore for TestCommercialStore {
    type Error = std::convert::Infallible;
    type Parcel = TestCommercialParcel;
    type MailAuthToken = TestMailToken;

    async fn commercial_parcel(&self, parcel_id: &str) -> Result<Self::Parcel, Self::Error> {
        let parcel = self.parcel();
        Ok(if parcel.parcel_id == parcel_id {
            parcel
        } else {
            panic!("unexpected parcel id: {parcel_id}")
        })
    }

    async fn claim_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("claim:{parcel_id}:{owner_user}:{owner_player_id}"));
        Ok(self.update_parcel(|parcel| {
            parcel.owner_user = Some(owner_user.to_owned());
            parcel.owner_player_id = Some(owner_player_id.to_owned());
            parcel.status = PARCEL_STATUS_CLAIMED;
        }))
    }

    async fn transfer_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("transfer:{parcel_id}:{owner_player_id}:{target}"));
        Ok(self.update_parcel(|parcel| {
            parcel.owner_user = Some(target.to_owned());
            parcel.owner_player_id = Some(format!("{target}-player"));
        }))
    }

    async fn set_room_mail_auth_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<Self::MailAuthToken, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("token:{parcel_id}:{owner_player_id}:{token}"));
        Ok(TestMailToken {
            username: "room-mail".to_owned(),
            player_id: owner_player_id.to_owned(),
        })
    }
}

impl BuildStore for TestCommercialStore {
    type Error = std::convert::Infallible;
    type Parcel = TestCommercialParcel;

    async fn update_parcel_build_field(
        &self,
        view_id: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("build:{view_id}:{owner_player_id}:{field}:{value}"));
        Ok(self.update_parcel(|parcel| match field {
            "title" => parcel.title = Some(value.to_owned()),
            "description" => parcel.description = Some(value.to_owned()),
            "style" => parcel.style = Some(value.to_owned()),
            "prompt" => parcel.operator_prompt = Some(value.to_owned()),
            "commands" => parcel.custom_commands = Some(value.to_owned()),
            _ => panic!("unexpected field: {field}"),
        }))
    }

    async fn publish_parcel_build(
        &self,
        view_id: &str,
        owner_player_id: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("publish:{view_id}:{owner_player_id}"));
        Ok(self.update_parcel(|parcel| {
            parcel.status = PARCEL_STATUS_BUILT;
        }))
    }
}

impl ShopStore for TestCommercialStore {
    type Error = std::convert::Infallible;
    type Parcel = TestCommercialParcel;
    type PaymentRequest = TestPaymentRequest;
    type InboxItem = TestInboxItem;
    type OperatorCommand = TestOperatorCommand;
    type MailingList = TestMailingList;
    type MailingListSubscriber = TestMailingListSubscriber;
    type MailingListSubscription = TestMailingListSubscription;
    type MailingListPost = TestMailingListPost;
    type BadgeDefinition = TestShopBadge;
    type BadgeAward = TestShopBadgeAward;

    async fn save_operator_command<P>(
        &self,
        parcel: &P,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        delivered: bool,
    ) -> Result<Self::OperatorCommand, Self::Error>
    where
        P: ParcelView + Sync,
    {
        self.calls.lock().unwrap().push(format!(
            "operator:{sender_user}:{sender_player_id}:{}:{raw_input}:{delivered}",
            parcel.parcel_id()
        ));
        Ok(TestOperatorCommand)
    }

    async fn recent_operator_commands(
        &self,
        _owner_player_id: &str,
        _limit: i64,
    ) -> Result<Vec<Self::OperatorCommand>, Self::Error> {
        Ok(Vec::new())
    }

    async fn create_payment_request(
        &self,
        _operator_command_id: i64,
        _owner_player_id: &str,
        _amount: i64,
        _delivery: &str,
    ) -> Result<Self::PaymentRequest, Self::Error> {
        Ok(TestPaymentRequest)
    }

    async fn inbox_item_by_source(
        &self,
        _recipient_player_id: &str,
        _source_kind: &str,
        _source_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        Ok(TestInboxItem {
            id: 1,
            kind: "shop_command",
            sender_user: "alice",
            subject: "hello",
            body: "body",
        })
    }

    async fn create_shop_mailing_list(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<Self::MailingList, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-create:{parcel_id}:{owner_player_id}:{slug}:{title}"
        ));
        Ok(TestMailingList {
            slug: slug.to_owned(),
            title: title.to_owned(),
            status: "open".to_owned(),
            subscriber_count: 0,
        })
    }

    async fn shop_mailing_lists(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::MailingList>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("mailing-list-list:{parcel_id}:{owner_player_id}"));
        Ok(vec![TestMailingList {
            slug: "updates".to_owned(),
            title: "Shop Updates".to_owned(),
            status: "open".to_owned(),
            subscriber_count: 1,
        }])
    }

    async fn shop_mailing_list_subscribers(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        _limit: i64,
    ) -> Result<ShopMailingListSubscriberPage<Self::MailingListSubscriber>, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-subscribers:{parcel_id}:{slug}:{owner_player_id}"
        ));
        Ok(ShopMailingListSubscriberPage {
            total: 1,
            subscribers: vec![TestMailingListSubscriber],
        })
    }

    async fn close_shop_mailing_list(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<Self::MailingList, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-close:{parcel_id}:{slug}:{owner_player_id}"
        ));
        Ok(TestMailingList {
            slug: slug.to_owned(),
            title: "Shop Updates".to_owned(),
            status: "closed".to_owned(),
            subscriber_count: 1,
        })
    }

    async fn subscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<Self::MailingListSubscription, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-subscribe:{target}:{slug}:{subscriber_user}:{subscriber_player_id}"
        ));
        Ok(TestMailingListSubscription {
            status: "active".to_owned(),
        })
    }

    async fn unsubscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<Self::MailingListSubscription, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-unsubscribe:{target}:{slug}:{subscriber_user}:{subscriber_player_id}"
        ));
        Ok(TestMailingListSubscription {
            status: "unsubscribed".to_owned(),
        })
    }

    async fn shop_mailing_list_subscriptions(
        &self,
        subscriber_player_id: &str,
    ) -> Result<Vec<Self::MailingListSubscription>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("mailing-list-subscriptions:{subscriber_player_id}"));
        Ok(vec![TestMailingListSubscription {
            status: "active".to_owned(),
        }])
    }

    async fn send_shop_mailing_list_post(
        &self,
        parcel_id: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<ShopMailingListSend<Self::MailingListPost, Self::InboxItem>, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-send:{parcel_id}:{slug}:{sender_user}:{sender_player_id}:{subject}:{body}"
        ));
        Ok(ShopMailingListSend {
            post: TestMailingListPost,
            deliveries: vec![ShopMailingListDelivery {
                recipient_player_id: "visitor-player".to_owned(),
                inbox_item: TestInboxItem {
                    id: 7,
                    kind: "mail",
                    sender_user: "owner",
                    subject: "Weekly Deal",
                    body: "Body",
                },
            }],
        })
    }

    async fn create_shop_badge(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<Self::BadgeDefinition, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "badge-create:{parcel_id}:{owner_player_id}:{slug}:{title}:{}",
            description.unwrap_or_default()
        ));
        Ok(TestShopBadge {
            slug: slug.to_owned(),
            title: title.to_owned(),
            description: description.map(str::to_owned),
            active_award_count: 0,
        })
    }

    async fn shop_badges(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::BadgeDefinition>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("badge-list:{parcel_id}:{owner_player_id}"));
        Ok(vec![TestShopBadge {
            slug: "patron".to_owned(),
            title: "Good Patron".to_owned(),
            description: Some("Paid and polite".to_owned()),
            active_award_count: 1,
        }])
    }

    async fn award_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        issuer_user: &str,
        issuer_player_id: &str,
        target: &str,
        note: Option<&str>,
    ) -> Result<Self::BadgeAward, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "badge-award:{parcel_id}:{slug}:{issuer_user}:{issuer_player_id}:{target}:{}",
            note.unwrap_or_default()
        ));
        Ok(TestShopBadgeAward {
            status: "active".to_owned(),
            recipient_user: target.to_owned(),
            note: note.map(str::to_owned),
        })
    }

    async fn revoke_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<Self::BadgeAward, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "badge-revoke:{parcel_id}:{slug}:{owner_player_id}:{target}"
        ));
        Ok(TestShopBadgeAward {
            status: "revoked".to_owned(),
            recipient_user: target.to_owned(),
            note: None,
        })
    }

    async fn shop_badges_for_player(
        &self,
        player_id: &str,
        _limit: i64,
    ) -> Result<Vec<Self::BadgeAward>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("badge-player:{player_id}"));
        Ok(vec![TestShopBadgeAward {
            status: "active".to_owned(),
            recipient_user: "visitor".to_owned(),
            note: Some("great work".to_owned()),
        }])
    }

    async fn shop_badges_for_target(
        &self,
        target: &str,
        _limit: i64,
    ) -> Result<Vec<Self::BadgeAward>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("badge-target:{target}"));
        Ok(vec![TestShopBadgeAward {
            status: "active".to_owned(),
            recipient_user: target.to_owned(),
            note: None,
        }])
    }
}

pub(super) struct TestParcel {
    pub(super) parcel_id: &'static str,
    pub(super) view_id: &'static str,
    pub(super) district: &'static str,
    pub(super) position: i32,
    pub(super) owner_user: Option<&'static str>,
    pub(super) room_user: Option<&'static str>,
    pub(super) status: &'static str,
    pub(super) title: Option<&'static str>,
}

impl ParcelView for TestParcel {
    fn parcel_id(&self) -> &str {
        self.parcel_id
    }

    fn view_id(&self) -> &str {
        self.view_id
    }

    fn front_view_id(&self) -> &str {
        "street_north_01"
    }

    fn district(&self) -> &str {
        self.district
    }

    fn position(&self) -> i32 {
        self.position
    }

    fn owner_user(&self) -> Option<&str> {
        self.owner_user
    }

    fn owner_player_id(&self) -> Option<&str> {
        None
    }

    fn room_user(&self) -> Option<&str> {
        self.room_user
    }

    fn room_player_id(&self) -> Option<&str> {
        None
    }

    fn status(&self) -> &str {
        self.status
    }

    fn title(&self) -> Option<&str> {
        self.title
    }

    fn description(&self) -> Option<&str> {
        None
    }

    fn style(&self) -> Option<&str> {
        None
    }

    fn operator_prompt(&self) -> Option<&str> {
        None
    }

    fn custom_commands(&self) -> Option<&str> {
        None
    }
}
