use crate::*;

impl<S, E> AppService<S>
where
    S: MessageStore<Error = E>,
{
    /// Persists a same-view say message and emits a room-view broadcast event.
    pub async fn handle_say(
        &self,
        identity: &AppIdentity,
        current_view: &str,
        text: &str,
    ) -> Result<Vec<UiEvent>, E> {
        self.store
            .save_say_message(&identity.user, &identity.player_id, current_view, text)
            .await?;
        Ok(vec![UiEvent::LiveViewMessage {
            view_id: current_view.to_owned(),
            text: format!("[say from {}] {text}", identity.user),
        }])
    }

    /// Persists a direct mail message.
    pub async fn handle_mail(
        &self,
        identity: &AppIdentity,
        target: &str,
        text: &str,
    ) -> Result<Vec<UiEvent>, E> {
        self.store
            .save_mail_message(&identity.user, &identity.player_id, target, text)
            .await?;
        Ok(Vec::new())
    }

    /// Persists a broadcast message.
    pub async fn handle_broadcast(
        &self,
        identity: &AppIdentity,
        text: &str,
    ) -> Result<Vec<UiEvent>, E> {
        self.store
            .save_broadcast_message(&identity.user, &identity.player_id, text)
            .await?;
        Ok(Vec::new())
    }

    /// Renders the player's wallet summary for login banners.
    pub async fn balance_summary(&self, player_id: &str) -> Result<String, E> {
        let balance = self.store.player_balance(player_id).await?;
        Ok(render_player_balance(balance))
    }
}

/// Protocol-neutral view of a world message.
pub trait WorldMessageView {
    /// Message kind, for example `say` or `broadcast`.
    fn kind(&self) -> &str;

    /// Sender username.
    fn sender_user(&self) -> &str;

    /// Message body.
    fn body(&self) -> &str;

    /// Database formatted creation timestamp.
    fn created_at(&self) -> &str;

    /// Optional database formatted expiry timestamp.
    fn expires_at(&self) -> Option<&str>;
}

/// Protocol-neutral view of a player balance.
pub trait BalanceView {
    /// Account id that owns the balance.
    fn account_id(&self) -> &str;

    /// Asset symbol.
    fn asset(&self) -> &str;

    /// Integer amount.
    fn amount(&self) -> i64;
}

/// Storage boundary for room history, news, and balances.
pub trait MessageStore {
    /// Store error type.
    type Error;
    /// Stored world message type.
    type WorldMessage: WorldMessageView;
    /// Stored balance type.
    type Balance: BalanceView;

    /// Loads recent unexpired messages for one view.
    async fn recent_view_messages(
        &self,
        view_id: &str,
        limit: i64,
    ) -> Result<Vec<Self::WorldMessage>, Self::Error>;

    /// Loads recent world news messages.
    async fn recent_news_messages(
        &self,
        limit: i64,
    ) -> Result<Vec<Self::WorldMessage>, Self::Error>;

    /// Loads the player's current MARK balance.
    async fn player_balance(&self, player_id: &str) -> Result<Self::Balance, Self::Error>;

    /// Persists a same-view say message with a 24 hour expiry.
    async fn save_say_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target_view: &str,
        body: &str,
    ) -> Result<(), Self::Error>;

    /// Persists a direct mail message.
    async fn save_mail_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        body: &str,
    ) -> Result<(), Self::Error>;

    /// Persists a direct mail message with a caller-provided inbox subject.
    async fn save_mail_message_with_subject(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), Self::Error>;

    /// Persists a broadcast message.
    async fn save_broadcast_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<(), Self::Error>;
}

/// Generic text result for message-view commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageViewResult {
    /// Rendered text.
    pub text: String,
}

pub(crate) fn render_message_list(
    title: &str,
    messages: &[impl WorldMessageView],
    empty: &str,
) -> String {
    let mut text = format!("\r\n{title}\r\n");
    if messages.is_empty() {
        text.push_str(&format!("{empty}\r\n"));
        return text;
    }
    for message in messages.iter().rev() {
        let expiry = message
            .expires_at()
            .map(|expires_at| format!(" expires={expires_at}"))
            .unwrap_or_default();
        text.push_str(&format!(
            "- [{}] {} from {}{}: {}\r\n",
            message.created_at(),
            message.kind(),
            message.sender_user(),
            expiry,
            message.body()
        ));
    }
    text
}

pub(crate) fn render_inventory(items: &[String]) -> String {
    if items.is_empty() {
        "Inventory: empty.\r\n".to_owned()
    } else {
        format!("Inventory: {}.\r\n", items.join(", "))
    }
}

pub(crate) fn render_who(current_view: &str, users: &[String]) -> String {
    if users.is_empty() {
        return format!("Online here in {current_view}: nobody else.\r\n");
    }
    format!(
        "Online here in {current_view} ({}): {}\r\n",
        users.len(),
        users.join(", ")
    )
}

pub(crate) fn render_player_balance(balance: impl BalanceView) -> String {
    format!(
        "Balance: {} {} ({})\r\n",
        balance.amount(),
        balance.asset(),
        balance.account_id()
    )
}

/// Storage boundary for mail/inbox actions used by room flows.
pub trait MailStore {
    /// Store error type.
    type Error;
    /// Stored inbox item type.
    type InboxItem: InboxItemView;

    /// Persists player input for a room mailbox principal.
    async fn save_room_mailbox_input<M>(
        &self,
        mailbox: &M,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
    ) -> Result<Self::InboxItem, Self::Error>
    where
        M: RoomMailboxView + Sync;
}

/// Result from forwarding input to a service-room mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomInputResult<I> {
    /// Text to display to the player.
    pub text: String,
    /// Stored inbox item generated for the room service.
    pub inbox_item: I,
}
