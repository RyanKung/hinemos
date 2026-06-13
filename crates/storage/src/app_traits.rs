use hinemos_app::{
    AccountSettingsView, AccountStore, AdmissionStore, AdmissionView, BalanceView, BuildStore,
    InboxItemView, InboxStore, LandStore, MailAuthTokenView, MailDaemonStore, MailStore,
    MemoryAtomView, MemoryEventView, MemoryStore, MessageStore, ParcelStore, ParcelView,
    PaymentRequestView, PaymentStore, PlayerStateStore as AppPlayerStateStore,
    RoomBindingEntryView, RoomBindingKindView, RoomCommandPolicyView, RoomMailboxView,
    RoomRegistrationStore, RoomStore, SelfModelView, ServiceRoomRegistrationUpsert,
    ServiceRoomView, ShopStore, SocialEdgeView, TransferView, ViewPresenceStore, WorldMessageView,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

use crate::{
    PgStorage, ServiceRoomUpsert, StorageError, StoredAccountSettings, StoredAdmission,
    StoredAgentSelfModel, StoredBalance, StoredInboxItem, StoredMailAuthToken, StoredMemoryAtom,
    StoredMemoryEvent, StoredOperatorCommand, StoredParcel, StoredPaymentRequest,
    StoredRoomBinding, StoredRoomCommandPolicy, StoredServiceRoom, StoredSocialEdge,
    StoredTransfer, StoredWorldMessage,
};

mod memory_message;

impl RoomStore for PgStorage {
    type Error = StorageError;
    type ServiceRoom = StoredServiceRoom;
    type RoomBinding = StoredRoomBinding;

    async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error> {
        PgStorage::service_room_by_view(self, view_id).await
    }

    async fn room_bindings_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error> {
        PgStorage::room_bindings_by_front_view(self, front_view_id).await
    }

    async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error> {
        PgStorage::room_binding_by_view(self, view_id).await
    }
}

impl RoomRegistrationStore for PgStorage {
    type Error = StorageError;

    async fn disable_service_rooms_except(
        &self,
        view_ids: Vec<String>,
    ) -> Result<u64, Self::Error> {
        let view_ids = view_ids.iter().map(String::as_str).collect::<Vec<_>>();
        PgStorage::disable_service_rooms_except(self, view_ids).await
    }

    async fn upsert_service_room(
        &self,
        registration: ServiceRoomRegistrationUpsert<'_>,
    ) -> Result<(), Self::Error> {
        PgStorage::upsert_service_room(
            self,
            ServiceRoomUpsert {
                view_id: registration.view_id,
                front_view_id: registration.front_view_id,
                front_entity_id: registration.front_entity_id,
                address: registration.address,
                label: registration.label,
                enter_aliases: registration.enter_aliases,
                room_user: registration.room_user,
                room_player_id: registration.room_player_id,
                status_text: registration.status_text,
                custom_commands: registration.custom_commands,
                enabled: registration.enabled,
            },
        )
        .await?;
        Ok(())
    }

    async fn ssh_identity_player_id_for_user(
        &self,
        room_user: &str,
    ) -> Result<Option<String>, Self::Error> {
        PgStorage::ssh_identity_player_id_for_user(self, room_user).await
    }
}

impl RoomCommandPolicyView for StoredRoomBinding {
    fn forwards_all_input(&self) -> bool {
        matches!(self.command_policy, StoredRoomCommandPolicy::ForwardAll)
    }

    fn listed_commands(&self) -> &[String] {
        match &self.command_policy {
            StoredRoomCommandPolicy::ForwardAll => &[],
            StoredRoomCommandPolicy::ForwardListed(commands) => commands,
        }
    }
}

impl RoomBindingEntryView for StoredRoomBinding {
    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn front_entity_id(&self) -> Option<&str> {
        self.front_entity_id.as_deref()
    }

    fn address(&self) -> &str {
        &self.address
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn enter_aliases(&self) -> &[String] {
        &self.enter_aliases
    }
}

impl RoomBindingKindView for StoredRoomBinding {
    fn is_commercial_parcel(&self) -> bool {
        matches!(self.kind, crate::StoredRoomBindingKind::CommercialParcel)
    }

    fn is_service_room(&self) -> bool {
        matches!(self.kind, crate::StoredRoomBindingKind::ServiceRoom)
    }
}

impl RoomMailboxView for StoredServiceRoom {
    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        Some(&self.room_user)
    }

    fn room_player_id(&self) -> Option<&str> {
        Some(&self.room_player_id)
    }
}

impl RoomMailboxView for StoredRoomBinding {
    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        self.room_user.as_deref()
    }

    fn room_player_id(&self) -> Option<&str> {
        self.room_player_id.as_deref()
    }
}

impl ParcelView for StoredRoomBinding {
    fn parcel_id(&self) -> &str {
        &self.address
    }

    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn front_view_id(&self) -> &str {
        &self.front_view_id
    }

    fn district(&self) -> &str {
        ""
    }

    fn position(&self) -> i32 {
        0
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
        self.parcel_status.as_deref().unwrap_or("unknown")
    }

    fn title(&self) -> Option<&str> {
        self.parcel_title.as_deref()
    }

    fn description(&self) -> Option<&str> {
        self.parcel_description.as_deref()
    }

    fn style(&self) -> Option<&str> {
        self.parcel_style.as_deref()
    }

    fn operator_prompt(&self) -> Option<&str> {
        self.parcel_operator_prompt.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.parcel_custom_commands.as_deref()
    }
}

impl ServiceRoomView for StoredRoomBinding {
    fn label(&self) -> Option<&str> {
        Some(&self.label)
    }

    fn address(&self) -> Option<&str> {
        Some(&self.address)
    }

    fn front_view_id(&self) -> Option<&str> {
        Some(&self.front_view_id)
    }

    fn status_text(&self) -> Option<&str> {
        self.status_text.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.custom_commands.as_deref()
    }
}

impl ServiceRoomView for StoredServiceRoom {
    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    fn front_view_id(&self) -> Option<&str> {
        self.front_view_id.as_deref()
    }

    fn status_text(&self) -> Option<&str> {
        self.status_text.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.custom_commands.as_deref()
    }
}

impl ParcelStore for PgStorage {
    type Error = StorageError;
    type Parcel = StoredParcel;

    async fn list_commercial_parcels(&self) -> Result<Vec<Self::Parcel>, Self::Error> {
        PgStorage::list_commercial_parcels(self).await
    }

    async fn commercial_parcels_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::Parcel>, Self::Error> {
        PgStorage::commercial_parcels_by_front_view(self, front_view_id).await
    }
}

impl ParcelView for StoredParcel {
    fn parcel_id(&self) -> &str {
        &self.parcel_id
    }

    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn front_view_id(&self) -> &str {
        &self.front_view_id
    }

    fn district(&self) -> &str {
        &self.district
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
        &self.status
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

impl MailAuthTokenView for StoredMailAuthToken {
    fn username(&self) -> &str {
        &self.username
    }

    fn player_id(&self) -> &str {
        &self.player_id
    }
}

impl InboxItemView for StoredInboxItem {
    fn id(&self) -> i64 {
        self.id
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn sender_user(&self) -> &str {
        &self.sender_user
    }

    fn subject(&self) -> &str {
        &self.subject
    }

    fn body(&self) -> &str {
        &self.body
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn attempts(&self) -> i32 {
        self.attempts
    }

    fn lease_until(&self) -> Option<&str> {
        self.lease_until.as_deref()
    }

    fn created_at(&self) -> &str {
        &self.created_at
    }
}

impl hinemos_app::OperatorCommandView for StoredOperatorCommand {
    fn id(&self) -> i64 {
        self.id
    }

    fn created_at(&self) -> &str {
        &self.created_at
    }

    fn status(&self) -> &str {
        &self.status
    }

    fn sender_user(&self) -> &str {
        &self.sender_user
    }

    fn owner_user(&self) -> &str {
        &self.owner_user
    }

    fn parcel_id(&self) -> &str {
        &self.parcel_id
    }

    fn raw_input(&self) -> &str {
        &self.raw_input
    }
}

impl AccountSettingsView for StoredAccountSettings {
    fn player_id(&self) -> &str {
        &self.player_id
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn online_days(&self) -> i32 {
        self.online_days
    }

    fn has_mail_token(&self) -> bool {
        self.has_mail_token
    }

    fn key_fingerprint(&self) -> Option<&str> {
        self.key_fingerprint.as_deref()
    }
}

impl TransferView for StoredTransfer {
    fn amount(&self) -> i64 {
        self.amount
    }

    fn asset(&self) -> &str {
        &self.asset
    }

    fn target_user(&self) -> &str {
        &self.target_user
    }

    fn ledger_id(&self) -> i64 {
        self.ledger_id
    }

    fn sender_balance(&self) -> i64 {
        self.sender_balance
    }

    fn memo(&self) -> &str {
        &self.memo
    }
}

impl PaymentRequestView for StoredPaymentRequest {
    fn id(&self) -> i64 {
        self.id
    }

    fn operator_command_id(&self) -> i64 {
        self.operator_command_id
    }

    fn parcel_id(&self) -> &str {
        &self.parcel_id
    }

    fn payer_user(&self) -> &str {
        &self.payer_user
    }

    fn payer_player_id(&self) -> &str {
        &self.payer_player_id
    }

    fn payee_user(&self) -> &str {
        &self.payee_user
    }

    fn payee_player_id(&self) -> &str {
        &self.payee_player_id
    }

    fn asset(&self) -> &str {
        &self.asset
    }

    fn amount(&self) -> i64 {
        self.amount
    }

    fn delivery(&self) -> &str {
        &self.delivery
    }
}

impl LandStore for PgStorage {
    type Error = StorageError;
    type Parcel = StoredParcel;
    type MailAuthToken = StoredMailAuthToken;

    async fn commercial_parcel(&self, parcel_id: &str) -> Result<Self::Parcel, Self::Error> {
        PgStorage::commercial_parcel(self, parcel_id).await
    }

    async fn claim_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        PgStorage::claim_commercial_parcel(self, parcel_id, owner_user, owner_player_id).await
    }

    async fn transfer_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        PgStorage::transfer_commercial_parcel(self, parcel_id, owner_player_id, target).await
    }

    async fn set_room_mail_auth_token(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        token: &str,
    ) -> Result<Self::MailAuthToken, Self::Error> {
        PgStorage::set_room_mail_auth_token(self, parcel_id, owner_player_id, token).await
    }
}

impl BuildStore for PgStorage {
    type Error = StorageError;
    type Parcel = StoredParcel;

    async fn update_parcel_build_field(
        &self,
        view_id: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        PgStorage::update_parcel_build_field(self, view_id, owner_player_id, field, value).await
    }

    async fn publish_parcel_build(
        &self,
        view_id: &str,
        owner_player_id: &str,
    ) -> Result<Self::Parcel, Self::Error> {
        PgStorage::publish_parcel_build(self, view_id, owner_player_id).await
    }
}

impl ShopStore for PgStorage {
    type Error = StorageError;
    type Parcel = StoredParcel;
    type PaymentRequest = StoredPaymentRequest;
    type InboxItem = StoredInboxItem;
    type OperatorCommand = StoredOperatorCommand;

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
        PgStorage::save_operator_command(
            self,
            parcel,
            sender_user,
            sender_player_id,
            raw_input,
            delivered,
        )
        .await
    }

    async fn recent_operator_commands(
        &self,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::OperatorCommand>, Self::Error> {
        PgStorage::recent_operator_commands(self, owner_player_id, limit).await
    }

    async fn create_payment_request(
        &self,
        operator_command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<Self::PaymentRequest, Self::Error> {
        PgStorage::create_payment_request(
            self,
            operator_command_id,
            owner_player_id,
            amount,
            delivery,
        )
        .await
    }

    async fn inbox_item_by_source(
        &self,
        recipient_player_id: &str,
        source_kind: &str,
        source_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        PgStorage::inbox_item_by_source(self, recipient_player_id, source_kind, source_id).await
    }
}

impl PaymentStore for PgStorage {
    type Error = StorageError;
    type Transfer = StoredTransfer;
    type PaymentRequest = StoredPaymentRequest;

    async fn pending_payment_requests(
        &self,
        payer_player_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::PaymentRequest>, Self::Error> {
        PgStorage::pending_payment_requests(self, payer_player_id, limit).await
    }

    async fn transfer_mark(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<Self::Transfer, Self::Error> {
        PgStorage::transfer_mark(self, sender_user, sender_player_id, target, amount, memo).await
    }

    async fn accept_payment_request(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<(Self::PaymentRequest, i64), Self::Error> {
        PgStorage::accept_payment_request(self, payer_user, payer_player_id, request_id).await
    }
}

impl AccountStore for PgStorage {
    type Error = StorageError;
    type AccountSettings = StoredAccountSettings;
    type MailAuthToken = StoredMailAuthToken;

    async fn account_settings(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<Self::AccountSettings, Self::Error> {
        PgStorage::account_settings(self, username, player_id).await
    }

    async fn set_mail_auth_token(
        &self,
        username: &str,
        player_id: &str,
        token: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::set_mail_auth_token(self, username, player_id, token)
            .await
            .map(|_| ())
    }

    async fn verify_mail_auth_token(
        &self,
        username: &str,
        token: &str,
    ) -> Result<Option<Self::MailAuthToken>, Self::Error> {
        PgStorage::verify_mail_auth_token(self, username, token).await
    }

    async fn ensure_player_wallet(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::ensure_player_wallet(self, username, player_id)
            .await
            .map(|_| ())
    }
}

impl InboxStore for PgStorage {
    type Error = StorageError;
    type InboxItem = StoredInboxItem;

    async fn list_inbox_items(
        &self,
        username: &str,
        player_id: &str,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::InboxItem>, Self::Error> {
        PgStorage::list_inbox_items(self, username, player_id, status, limit).await
    }

    async fn read_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        PgStorage::read_inbox_item(self, username, player_id, item_id).await
    }

    async fn claim_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<Self::InboxItem, Self::Error> {
        PgStorage::claim_inbox_item(self, username, player_id, item_id).await
    }

    async fn finish_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
        status: &str,
    ) -> Result<Self::InboxItem, Self::Error> {
        PgStorage::finish_inbox_item(self, username, player_id, item_id, status).await
    }
}

impl MailDaemonStore for PgStorage {
    type Error = StorageError;
    type MailAuthToken = StoredMailAuthToken;
    type InboxItem = StoredInboxItem;

    fn verify_mail_auth_token<'a>(
        &'a self,
        username: &'a str,
        token: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Self::MailAuthToken>, Self::Error>> + Send + 'a>>
    {
        Box::pin(async move { PgStorage::verify_mail_auth_token(self, username, token).await })
    }

    fn save_mail_message_with_subject<'a>(
        &'a self,
        sender_user: &'a str,
        sender_player_id: &'a str,
        target: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Self::InboxItem, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            PgStorage::save_mail_message_with_subject(
                self,
                sender_user,
                sender_player_id,
                target,
                subject,
                body,
            )
            .await
        })
    }

    fn list_inbox_items<'a>(
        &'a self,
        username: &'a str,
        player_id: &'a str,
        status: Option<&'a str>,
        limit: i64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Self::InboxItem>, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            PgStorage::list_inbox_items(self, username, player_id, status, limit).await
        })
    }

    fn finish_inbox_item<'a>(
        &'a self,
        username: &'a str,
        player_id: &'a str,
        item_id: i64,
        status: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Self::InboxItem, Self::Error>> + Send + 'a>> {
        Box::pin(async move {
            PgStorage::finish_inbox_item(self, username, player_id, item_id, status).await
        })
    }
}

impl AppPlayerStateStore for PgStorage {
    type Error = StorageError;

    async fn load_player_state(
        &self,
        player_id: &str,
    ) -> Result<Option<hinemos_core::PlayerState>, Self::Error> {
        PgStorage::load_player_state(self, player_id).await
    }

    async fn save_player_state(
        &self,
        player: &hinemos_core::PlayerState,
    ) -> Result<(), Self::Error> {
        PgStorage::save_player_state(self, player).await
    }
}

impl ViewPresenceStore for PgStorage {
    type Error = StorageError;

    async fn record_view_presence(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::record_view_presence(self, username, player_id, view_id).await
    }
}
