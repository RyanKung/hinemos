use crate::*;

impl<S, E> AppService<S>
where
    S: InboxStore<Error = E> + ShopStore<Error = E>,
{
    /// Lists inbox items for display.
    pub async fn list_inbox(
        &self,
        title: &str,
        username: &str,
        player_id: &str,
        filter: &str,
        mail_domain: Option<&str>,
    ) -> Result<BusinessListResult, E> {
        let items = self
            .store
            .list_inbox_items(username, player_id, Some(filter), 20)
            .await?;
        Ok(BusinessListResult {
            text: render_inbox_items(title, &items, mail_domain).replace('\n', "\r\n"),
        })
    }

    /// Reads one inbox item for display.
    pub async fn read_inbox(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
        mail_domain: Option<&str>,
    ) -> Result<BusinessListResult, E> {
        let item = self
            .store
            .read_inbox_item(username, player_id, item_id)
            .await?;
        Ok(BusinessListResult {
            text: render_inbox_item(&item, mail_domain).replace('\n', "\r\n"),
        })
    }

    /// Claims an inbox item for processing.
    pub async fn claim_inbox(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<InboxMutationResult, E> {
        let item = self
            .store
            .claim_inbox_item(username, player_id, item_id)
            .await?;
        Ok(InboxMutationResult {
            text: format!(
                "Claimed inbox #{} kind={} subject={}. Lease until {}.\r\n",
                item.id(),
                item.kind(),
                item.subject(),
                item.lease_until().unwrap_or("unknown")
            ),
        })
    }

    /// Acknowledges an inbox item.
    pub async fn ack_inbox(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<InboxMutationResult, E> {
        let item = self
            .store
            .finish_inbox_item(username, player_id, item_id, INBOX_STATUS_ACKED)
            .await?;
        Ok(InboxMutationResult {
            text: format!("Acked inbox #{} kind={}.\r\n", item.id(), item.kind()),
        })
    }

    /// Archives an inbox item.
    pub async fn archive_inbox(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<InboxMutationResult, E> {
        let item = self
            .store
            .finish_inbox_item(username, player_id, item_id, INBOX_STATUS_ARCHIVED)
            .await?;
        Ok(InboxMutationResult {
            text: format!("Archived inbox #{} kind={}.\r\n", item.id(), item.kind()),
        })
    }
}

impl<S, E> AppService<S>
where
    S: InboxStore<Error = E>,
{
    /// Renders the player's open inbox summary for login banners.
    pub async fn open_inbox_summary(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<Option<String>, E> {
        let items = self
            .store
            .list_inbox_items(username, player_id, Some("open"), 10)
            .await?;
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!("Inbox: {} open item(s).\r\n", items.len())))
    }
}

/// Protocol-neutral view of an inbox item.
pub trait InboxItemView {
    /// Inbox item id.
    fn id(&self) -> i64;

    /// Inbox item kind.
    fn kind(&self) -> &str;

    /// Sender username.
    fn sender_user(&self) -> &str;

    /// Inbox item subject.
    fn subject(&self) -> &str;

    /// Full inbox item body.
    fn body(&self) -> &str;

    /// Inbox item status.
    fn status(&self) -> &str;

    /// Processing claim attempts.
    fn attempts(&self) -> i32;

    /// Lease expiry if the item is claimed.
    fn lease_until(&self) -> Option<&str>;

    /// Creation timestamp.
    fn created_at(&self) -> &str;
}

/// Protocol-neutral view of a shop operator command.
pub trait OperatorCommandView {
    /// Operator command id.
    fn id(&self) -> i64;

    /// Creation timestamp.
    fn created_at(&self) -> &str;

    /// Processing status.
    fn status(&self) -> &str;

    /// Visitor username that sent the command.
    fn sender_user(&self) -> &str;

    /// Shop owner username.
    fn owner_user(&self) -> &str;

    /// Parcel id where the command was entered.
    fn parcel_id(&self) -> &str;

    /// Raw command text.
    fn raw_input(&self) -> &str;
}

/// Storage boundary for inbox item state changes.
pub trait InboxStore {
    /// Store error type.
    type Error;
    /// Stored inbox item type.
    type InboxItem: InboxItemView;

    /// Lists inbox items.
    async fn list_inbox_items(
        &self,
        username: &str,
        player_id: &str,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self::InboxItem>, Self::Error>;

    /// Reads one inbox item.
    async fn read_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<Self::InboxItem, Self::Error>;

    /// Claims an inbox item.
    async fn claim_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<Self::InboxItem, Self::Error>;

    /// Finishes an inbox item with the given status.
    async fn finish_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
        status: &str,
    ) -> Result<Self::InboxItem, Self::Error>;
}

/// Boxed async result for an optional mail auth token.
pub type MailAuthTokenLookup<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<Option<T>, E>> + Send + 'a>>;

/// Boxed async result for one inbox item.
pub type InboxItemLookup<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// Boxed async result for a list of inbox items.
pub type InboxItemListLookup<'a, T, E> =
    Pin<Box<dyn Future<Output = Result<Vec<T>, E>> + Send + 'a>>;

/// Storage boundary for the SMTP/IMAP sidecar.
pub trait MailDaemonStore {
    /// Store error type.
    type Error;
    /// Stored mail auth token type.
    type MailAuthToken: MailAuthTokenView;
    /// Stored inbox item type.
    type InboxItem: InboxItemView;

    /// Verifies a username/token pair for SMTP or IMAP login.
    fn verify_mail_auth_token<'a>(
        &'a self,
        username: &'a str,
        token: &'a str,
    ) -> MailAuthTokenLookup<'a, Self::MailAuthToken, Self::Error>;

    /// Saves a mail message with an explicit subject line.
    fn save_mail_message_with_subject<'a>(
        &'a self,
        sender_user: &'a str,
        sender_player_id: &'a str,
        target: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> InboxItemLookup<'a, Self::InboxItem, Self::Error>;

    /// Lists inbox items.
    fn list_inbox_items<'a>(
        &'a self,
        username: &'a str,
        player_id: &'a str,
        status: Option<&'a str>,
        limit: i64,
    ) -> InboxItemListLookup<'a, Self::InboxItem, Self::Error>;

    /// Finishes an inbox item with the given status.
    fn finish_inbox_item<'a>(
        &'a self,
        username: &'a str,
        player_id: &'a str,
        item_id: i64,
        status: &'a str,
    ) -> InboxItemLookup<'a, Self::InboxItem, Self::Error>;
}

/// Result from an inbox state mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxMutationResult {
    /// Text to display to the user.
    pub text: String,
}

fn render_inbox_items(
    title: &str,
    items: &[impl InboxItemView],
    mail_domain: Option<&str>,
) -> String {
    let mut lines = vec![title.to_owned()];
    if items.is_empty() {
        lines.push("No inbox items.".to_owned());
    } else {
        for item in items {
            let lease = item
                .lease_until()
                .map(|value| format!(" lease until {value}"))
                .unwrap_or_default();
            lines.push(format!(
                "#{} {} {} from {}: {} (attempts {}){}",
                item.id(),
                item.kind(),
                item.status(),
                compact_inbox_field(&format_mail_user(item.sender_user(), mail_domain)),
                compact_inbox_field(item.subject()),
                item.attempts(),
                lease
            ));
        }
        lines.push(
            "Use /mail read <id>, /mail claim <id>, /mail ack <id>, or /mail archive <id>."
                .to_owned(),
        );
    }
    lines.push(String::new());
    lines.join("\n")
}

fn render_inbox_item(item: &impl InboxItemView, mail_domain: Option<&str>) -> String {
    format!(
        "Inbox #{}\nKind: {}\nStatus: {}\nFrom: {}\nSubject: {}\nCreated: {}\nAttempts: {}\nBody: {}\n\n",
        item.id(),
        item.kind(),
        item.status(),
        format_mail_user(item.sender_user(), mail_domain),
        item.subject(),
        item.created_at(),
        item.attempts(),
        item.body()
    )
}

fn compact_inbox_field(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn format_mail_user(user: &str, mail_domain: Option<&str>) -> String {
    match mail_domain {
        Some(domain) if !user.contains('@') => format!("{user}@{domain}"),
        _ => user.to_owned(),
    }
}

pub(crate) fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "set" } else { "not set" }
}
