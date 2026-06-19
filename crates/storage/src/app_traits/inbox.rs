use super::*;

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

    fn source_kind(&self) -> Option<&str> {
        self.source_kind.as_deref()
    }

    fn source_id(&self) -> Option<i64> {
        self.source_id
    }

    fn payload(&self) -> Option<&serde_json::Value> {
        Some(&self.payload)
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
