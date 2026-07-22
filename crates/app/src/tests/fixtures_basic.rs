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
pub(super) struct TestParcel {
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
pub(super) struct TestParcelFixtureStore {
    pub(super) parcel: Mutex<TestParcel>,
    pub(super) calls: Mutex<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TestCommerceError {
    MailingList(String),
    ParcelWork(String),
    ParcelJobGuide(String),
    ParcelBadge(String),
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
pub(super) struct TestCommandRoute {
    pub(super) slug: String,
    pub(super) command_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestWorkDesk {
    pub(super) slug: String,
    pub(super) title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestParcelJobGuide {
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestParcelStaff {
    pub(super) staff_user: String,
    pub(super) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestParcelShift {
    pub(super) slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestParcelWorkItem {
    pub(super) id: i64,
    pub(super) slug: String,
    pub(super) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestParcelBadge {
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) description: Option<String>,
    pub(super) active_award_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestParcelBadgeAward {
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

impl ParcelView for TestParcel {
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

impl RoomMailboxView for TestParcel {
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

impl RoomBindingKindView for TestParcel {
    fn is_parcel(&self) -> bool {
        true
    }

    fn is_service_room(&self) -> bool {
        false
    }
}

impl RoomCommandPolicyView for TestParcel {
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
        "P1"
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
        "P1"
    }

    fn raw_input(&self) -> &str {
        "input"
    }
}

impl ParcelMailingListView for TestMailingList {
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

impl ParcelMailingListSubscriberView for TestMailingListSubscriber {
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

impl ParcelMailingListSubscriptionView for TestMailingListSubscription {
    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn parcel_title(&self) -> Option<&str> {
        Some("Parcel")
    }

    fn slug(&self) -> &str {
        "updates"
    }

    fn list_title(&self) -> &str {
        "Parcel Updates"
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ParcelMailingListPostView for TestMailingListPost {
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
        "Parcel Updates"
    }

    fn subject(&self) -> &str {
        "Weekly Deal"
    }

    fn recipient_count(&self) -> i64 {
        1
    }
}

impl ParcelCommandRouteView for TestCommandRoute {
    fn id(&self) -> i64 {
        13
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn slug(&self) -> &str {
        &self.slug
    }

    fn desk_title(&self) -> &str {
        "Desk"
    }

    fn command_prefix(&self) -> &str {
        &self.command_prefix
    }

    fn created_at(&self) -> &str {
        "created"
    }
}

impl ParcelWorkDeskView for TestWorkDesk {
    fn id(&self) -> i64 {
        19
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
        "open"
    }

    fn queued_count(&self) -> i64 {
        1
    }

    fn active_worker_count(&self) -> i64 {
        1
    }

    fn created_at(&self) -> &str {
        "created"
    }
}

impl ParcelJobGuideView for TestParcelJobGuide {
    fn id(&self) -> i64 {
        29
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

    fn body(&self) -> &str {
        &self.body
    }

    fn publisher_user(&self) -> &str {
        "owner"
    }

    fn status(&self) -> &str {
        "published"
    }

    fn created_at(&self) -> &str {
        "created"
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ParcelStaffView for TestParcelStaff {
    fn staff_user(&self) -> &str {
        &self.staff_user
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ParcelShiftView for TestParcelShift {
    fn id(&self) -> i64 {
        23
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn slug(&self) -> &str {
        &self.slug
    }

    fn worker_user(&self) -> &str {
        "worker"
    }

    fn status(&self) -> &str {
        "active"
    }

    fn started_at(&self) -> &str {
        "started"
    }

    fn ended_at(&self) -> Option<&str> {
        None
    }
}

impl ParcelWorkItemView for TestParcelWorkItem {
    fn id(&self) -> i64 {
        self.id
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn slug(&self) -> &str {
        &self.slug
    }

    fn desk_title(&self) -> &str {
        "Desk"
    }

    fn operator_command_id(&self) -> i64 {
        1
    }

    fn command_prefix(&self) -> &str {
        "/hello"
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn sender_user(&self) -> &str {
        "visitor"
    }

    fn raw_input(&self) -> &str {
        "/hello"
    }

    fn assignee_user(&self) -> Option<&str> {
        Some("worker")
    }

    fn result(&self) -> Option<&str> {
        None
    }

    fn created_at(&self) -> &str {
        "created"
    }

    fn updated_at(&self) -> &str {
        "updated"
    }
}

impl ParcelBadgeDefinitionView for TestParcelBadge {
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

impl ParcelBadgeAwardView for TestParcelBadgeAward {
    fn id(&self) -> i64 {
        17
    }

    fn parcel_id(&self) -> &str {
        "P1"
    }

    fn parcel_title(&self) -> Option<&str> {
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

impl TestParcelFixtureStore {
    fn parcel(&self) -> TestParcel {
        self.parcel.lock().unwrap().clone()
    }

    fn update_parcel(&self, mutate: impl FnOnce(&mut TestParcel)) -> TestParcel {
        let mut parcel = self.parcel.lock().unwrap();
        mutate(&mut parcel);
        parcel.clone()
    }
}

impl FromMailingListValidation for TestCommerceError {
    fn invalid_mailing_list(message: &str) -> Self {
        Self::MailingList(message.to_owned())
    }
}

impl FromParcelWorkValidation for TestCommerceError {
    fn invalid_parcel_work(message: &str) -> Self {
        Self::ParcelWork(message.to_owned())
    }
}

impl FromParcelJobGuideValidation for TestCommerceError {
    fn invalid_parcel_job_guide(message: &str) -> Self {
        Self::ParcelJobGuide(message.to_owned())
    }
}

impl FromParcelBadgeValidation for TestCommerceError {
    fn invalid_parcel_badge(message: &str) -> Self {
        Self::ParcelBadge(message.to_owned())
    }
}

impl ParcelOwnershipStore for TestParcelFixtureStore {
    type Error = TestCommerceError;
    type Parcel = TestParcel;
    type MailAuthToken = TestMailToken;

    async fn parcel_by_id(&self, parcel_id: &str) -> Result<Self::Parcel, Self::Error> {
        let parcel = self.parcel();
        Ok(if parcel.parcel_id == parcel_id {
            parcel
        } else {
            panic!("unexpected parcel id: {parcel_id}")
        })
    }

    async fn parcel_by_view(&self, view_id: &str) -> Result<Option<Self::Parcel>, Self::Error> {
        let parcel = self.parcel();
        Ok((parcel.view_id == view_id).then_some(parcel))
    }

    async fn claim_parcel(
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

    async fn transfer_parcel(
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

impl BuildStore for TestParcelFixtureStore {
    type Error = TestCommerceError;
    type Parcel = TestParcel;

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

impl ParcelStore for TestParcelFixtureStore {
    type Error = TestCommerceError;
    type Parcel = TestParcel;
    type PaymentRequest = TestPaymentRequest;
    type InboxItem = TestInboxItem;
    type OperatorCommand = TestOperatorCommand;
    type MailingList = TestMailingList;
    type MailingListSubscriber = TestMailingListSubscriber;
    type MailingListSubscription = TestMailingListSubscription;
    type MailingListPost = TestMailingListPost;
    type CommandRoute = TestCommandRoute;
    type WorkDesk = TestWorkDesk;
    type JobGuide = TestParcelJobGuide;
    type Staff = TestParcelStaff;
    type Shift = TestParcelShift;
    type WorkItem = TestParcelWorkItem;
    type BadgeDefinition = TestParcelBadge;
    type BadgeAward = TestParcelBadgeAward;

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
        parcel_id: &str,
        owner_player_id: &str,
        _limit: i64,
    ) -> Result<Vec<Self::OperatorCommand>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("operator-list:{parcel_id}:{owner_player_id}"));
        Ok(Vec::new())
    }

    async fn operator_command(
        &self,
        _command_id: i64,
    ) -> Result<Self::OperatorCommand, Self::Error> {
        Ok(TestOperatorCommand)
    }

    async fn create_payment_request(
        &self,
        _operator_command_id: i64,
        _owner_player_id: &str,
        _amount: i64,
        _delivery: &str,
    ) -> Result<PaymentRequestCreation<Self::PaymentRequest>, Self::Error> {
        Ok(PaymentRequestCreation {
            request: TestPaymentRequest,
            created: true,
        })
    }

    async fn inbox_item_by_source(
        &self,
        _recipient_player_id: &str,
        _source_kind: &str,
        _source_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        Ok(TestInboxItem {
            id: 1,
            kind: "parcel_command",
            sender_user: "alice",
            subject: "hello",
            body: "body",
        })
    }

    async fn create_parcel_mailing_list(
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

    async fn parcel_mailing_lists(
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
            title: "Parcel Updates".to_owned(),
            status: "open".to_owned(),
            subscriber_count: 1,
        }])
    }

    async fn parcel_mailing_list_subscribers(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        _limit: i64,
    ) -> Result<ParcelMailingListSubscriberPage<Self::MailingListSubscriber>, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-subscribers:{parcel_id}:{slug}:{owner_player_id}"
        ));
        Ok(ParcelMailingListSubscriberPage {
            total: 1,
            subscribers: vec![TestMailingListSubscriber],
        })
    }

    async fn close_parcel_mailing_list(
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
            title: "Parcel Updates".to_owned(),
            status: "closed".to_owned(),
            subscriber_count: 1,
        })
    }

    async fn parcel_mailing_list(
        &self,
        _target: &str,
        slug: &str,
    ) -> Result<Self::MailingList, Self::Error> {
        Ok(TestMailingList {
            slug: slug.to_owned(),
            title: "Parcel Updates".to_owned(),
            status: "open".to_owned(),
            subscriber_count: 1,
        })
    }

    async fn subscribe_parcel_mailing_list(
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

    async fn unsubscribe_parcel_mailing_list(
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

    async fn parcel_mailing_list_subscriptions(
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

    async fn send_parcel_mailing_list_post(
        &self,
        parcel_id: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<ParcelMailingListSend<Self::MailingListPost, Self::InboxItem>, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "mailing-list-send:{parcel_id}:{slug}:{sender_user}:{sender_player_id}:{subject}:{body}"
        ));
        Ok(ParcelMailingListSend {
            post: TestMailingListPost,
            deliveries: vec![ParcelMailingListDelivery {
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

    async fn add_parcel_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<Self::CommandRoute, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "route-add:{parcel_id}:{owner_player_id}:{slug}:{command_prefix}"
        ));
        Ok(TestCommandRoute {
            slug: slug.to_owned(),
            command_prefix: command_prefix.to_owned(),
        })
    }

    async fn create_parcel_work_desk(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<Self::WorkDesk, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "desk-create:{parcel_id}:{owner_player_id}:{slug}:{title}"
        ));
        Ok(TestWorkDesk {
            slug: slug.to_owned(),
            title: title.to_owned(),
        })
    }

    async fn parcel_work_desks(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::WorkDesk>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("desk-list:{parcel_id}:{owner_player_id}"));
        Ok(vec![TestWorkDesk {
            slug: "desk".to_owned(),
            title: "Desk".to_owned(),
        }])
    }

    async fn publish_parcel_job_guide(
        &self,
        input: ParcelJobGuidePublish<'_>,
    ) -> Result<Self::JobGuide, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "job-publish:{}:{}:{}:{}:{}:{}:{}",
            input.parcel_id,
            input.owner_player_id,
            input.slug,
            input.title,
            input.body,
            input.publisher_user,
            input.publisher_player_id
        ));
        Ok(TestParcelJobGuide {
            slug: input.slug.to_owned(),
            title: input.title.to_owned(),
            body: input.body.to_owned(),
        })
    }

    async fn parcel_job_guides(&self, parcel_id: &str) -> Result<Vec<Self::JobGuide>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("job-list:{parcel_id}"));
        Ok(vec![TestParcelJobGuide {
            slug: "reporter".to_owned(),
            title: "Reporter JD".to_owned(),
            body: "File a story each game day.".to_owned(),
        }])
    }

    async fn parcel_job_guide(
        &self,
        parcel_id: &str,
        slug: &str,
    ) -> Result<Self::JobGuide, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("job-read:{parcel_id}:{slug}"));
        Ok(TestParcelJobGuide {
            slug: slug.to_owned(),
            title: "Reporter JD".to_owned(),
            body: "File a story each game day.".to_owned(),
        })
    }

    async fn add_parcel_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<Self::Staff, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "staff-add:{parcel_id}:{slug}:{owner_player_id}:{username}"
        ));
        Ok(TestParcelStaff {
            staff_user: username.to_owned(),
            status: "active".to_owned(),
        })
    }

    async fn parcel_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        _limit: i64,
    ) -> Result<Vec<Self::Staff>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("staff-list:{parcel_id}:{slug}:{owner_player_id}"));
        Ok(vec![TestParcelStaff {
            staff_user: "worker".to_owned(),
            status: "active".to_owned(),
        }])
    }

    async fn remove_parcel_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<Self::Staff, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "staff-remove:{parcel_id}:{slug}:{owner_player_id}:{username}"
        ));
        Ok(TestParcelStaff {
            staff_user: username.to_owned(),
            status: "removed".to_owned(),
        })
    }

    async fn start_parcel_shift(
        &self,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<Self::Shift, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "shift-start:{parcel_id}:{slug}:{worker_user}:{worker_player_id}"
        ));
        Ok(TestParcelShift {
            slug: slug.to_owned(),
        })
    }

    async fn end_parcel_shift(
        &self,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<Self::Shift, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "shift-end:{parcel_id}:{slug}:{worker_user}:{worker_player_id}"
        ));
        Ok(TestParcelShift {
            slug: slug.to_owned(),
        })
    }

    async fn parcel_work_items(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        slug: Option<&str>,
        _limit: i64,
    ) -> Result<Vec<Self::WorkItem>, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "work-list:{parcel_id}:{worker_user}:{worker_player_id}:{}",
            slug.unwrap_or("*")
        ));
        Ok(vec![TestParcelWorkItem {
            id: 3,
            slug: slug.unwrap_or("desk").to_owned(),
            status: "queued".to_owned(),
        }])
    }

    async fn claim_parcel_work(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
    ) -> Result<Self::WorkItem, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "work-claim:{parcel_id}:{worker_user}:{worker_player_id}:{work_id}"
        ));
        Ok(TestParcelWorkItem {
            id: work_id,
            slug: "desk".to_owned(),
            status: "claimed".to_owned(),
        })
    }

    async fn finish_parcel_work(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
        result: &str,
    ) -> Result<Self::WorkItem, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "work-done:{parcel_id}:{worker_user}:{worker_player_id}:{work_id}:{result}"
        ));
        Ok(TestParcelWorkItem {
            id: work_id,
            slug: "desk".to_owned(),
            status: "done".to_owned(),
        })
    }

    async fn parcel_command_routes(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::CommandRoute>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("route-list:{parcel_id}:{owner_player_id}"));
        Ok(vec![TestCommandRoute {
            slug: "updates".to_owned(),
            command_prefix: "/hello".to_owned(),
        }])
    }

    async fn remove_parcel_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<Self::CommandRoute, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "route-remove:{parcel_id}:{owner_player_id}:{slug}:{command_prefix}"
        ));
        Ok(TestCommandRoute {
            slug: slug.to_owned(),
            command_prefix: command_prefix.to_owned(),
        })
    }

    async fn dispatch_parcel_command_routes<P>(
        &self,
        _parcel: &P,
        _command_id: i64,
    ) -> Result<Vec<Self::WorkItem>, Self::Error>
    where
        P: ParcelView + Sync,
    {
        self.calls
            .lock()
            .unwrap()
            .push("dispatch-work:1".to_owned());
        Ok(vec![TestParcelWorkItem {
            id: 1,
            slug: "desk".to_owned(),
            status: "queued".to_owned(),
        }])
    }

    async fn create_parcel_badge(
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
        Ok(TestParcelBadge {
            slug: slug.to_owned(),
            title: title.to_owned(),
            description: description.map(str::to_owned),
            active_award_count: 0,
        })
    }

    async fn parcel_badges(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<Self::BadgeDefinition>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("badge-list:{parcel_id}:{owner_player_id}"));
        Ok(vec![TestParcelBadge {
            slug: "patron".to_owned(),
            title: "Good Patron".to_owned(),
            description: Some("Paid and polite".to_owned()),
            active_award_count: 1,
        }])
    }

    async fn award_parcel_badge(
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
        Ok(TestParcelBadgeAward {
            status: "active".to_owned(),
            recipient_user: target.to_owned(),
            note: note.map(str::to_owned),
        })
    }

    async fn revoke_parcel_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<Self::BadgeAward, Self::Error> {
        self.calls.lock().unwrap().push(format!(
            "badge-revoke:{parcel_id}:{slug}:{owner_player_id}:{target}"
        ));
        Ok(TestParcelBadgeAward {
            status: "revoked".to_owned(),
            recipient_user: target.to_owned(),
            note: None,
        })
    }

    async fn parcel_badges_for_player(
        &self,
        player_id: &str,
        _limit: i64,
    ) -> Result<Vec<Self::BadgeAward>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("badge-player:{player_id}"));
        Ok(vec![TestParcelBadgeAward {
            status: "active".to_owned(),
            recipient_user: "visitor".to_owned(),
            note: Some("great work".to_owned()),
        }])
    }

    async fn parcel_badges_for_target(
        &self,
        target: &str,
        _limit: i64,
    ) -> Result<Vec<Self::BadgeAward>, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("badge-target:{target}"));
        Ok(vec![TestParcelBadgeAward {
            status: "active".to_owned(),
            recipient_user: target.to_owned(),
            note: None,
        }])
    }
}

pub(super) struct TestListedParcel {
    pub(super) parcel_id: &'static str,
    pub(super) view_id: &'static str,
    pub(super) district: &'static str,
    pub(super) position: i32,
    pub(super) owner_user: Option<&'static str>,
    pub(super) room_user: Option<&'static str>,
    pub(super) status: &'static str,
    pub(super) title: Option<&'static str>,
}

impl ParcelView for TestListedParcel {
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
