use super::*;

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
