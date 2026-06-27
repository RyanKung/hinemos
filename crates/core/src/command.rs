//! Semantic commands accepted by the world runtime.

use serde::{Deserialize, Serialize};

use crate::ids::EntityId;

/// Cardinal movement directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// Move north.
    North,
    /// Move south.
    South,
    /// Move east.
    East,
    /// Move west.
    West,
    /// Move upward.
    Up,
    /// Move downward.
    Down,
}

impl Direction {
    /// Returns a stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// Reference to an entity in a player command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRef {
    /// Stable entity id.
    pub id: EntityId,
}

impl EntityRef {
    /// Creates an entity reference from an id-like value.
    #[must_use]
    pub fn new(id: impl Into<EntityId>) -> Self {
        Self { id: id.into() }
    }
}

/// Parses slash-prefixed custom command strings into canonical extension commands.
pub fn extension_commands(commands: Option<&str>) -> impl Iterator<Item = SemanticCommand> + '_ {
    commands
        .unwrap_or_default()
        .split(['\n', ';'])
        .filter_map(|entry| {
            let entry = entry.trim();
            let command = entry.split_whitespace().next()?;
            command.starts_with('/').then(|| entry.to_owned())
        })
        .map(|input| {
            let name = input
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            SemanticCommand::Extension { name, input }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_commands_keep_slash_prefixed_entries() {
        let commands =
            extension_commands(Some("/room ask\nnot-a-command; /room status")).collect::<Vec<_>>();

        assert_eq!(
            commands,
            vec![
                SemanticCommand::Extension {
                    name: "room".to_owned(),
                    input: "/room ask".to_owned(),
                },
                SemanticCommand::Extension {
                    name: "room".to_owned(),
                    input: "/room status".to_owned(),
                },
            ]
        );
    }
}

/// Canonical commands produced by the slash parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SemanticCommand {
    /// Describe the current view again.
    Look,
    /// Show the current view map again.
    Map,
    /// Move through an exit.
    Move {
        /// Direction to move.
        direction: Direction,
    },
    /// Enter an adjacent parcel or shopfront.
    Enter {
        /// Parcel id or visible shop title.
        target: String,
    },
    /// Inspect a visible entity.
    Inspect {
        /// Target entity.
        target: EntityRef,
    },
    /// Read visible text (poster, board, plaque).
    Read {
        /// Target entity.
        target: EntityRef,
    },
    /// Pick up a visible item.
    Take {
        /// Target entity.
        target: EntityRef,
    },
    /// Talk to a visible NPC.
    Talk {
        /// Target entity.
        target: EntityRef,
    },
    /// Agree to the current admission agreement.
    Agree {
        /// Exact agreement phrase shown by the board.
        phrase: String,
    },
    /// Send a message to players in the same view.
    Say {
        /// Message body.
        text: String,
    },
    /// Send a direct message to a user or player id.
    Mail {
        /// User name or player id.
        target: String,
        /// Message body.
        text: String,
    },
    /// Manage account and protocol settings.
    Settings {
        /// Settings action.
        action: SettingsAction,
    },
    /// Manage the durable inbox.
    Inbox {
        /// Inbox command action.
        action: InboxAction,
    },
    /// Send a global message to all connected players.
    Broadcast {
        /// Message body.
        text: String,
    },
    /// Show private persistent mail.
    Mailbox,
    /// Show current view message history.
    History,
    /// Show global broadcast news.
    News,
    /// Show other online users in the current view.
    Who,
    /// Show wallet balance.
    Balance,
    /// Wallet payment action.
    Pay {
        /// Payment action.
        action: PayAction,
    },
    /// Manage commercial street parcels.
    Land {
        /// Land command action.
        action: LandAction,
    },
    /// Edit the current owned parcel build sheet.
    Build {
        /// Build field update.
        action: BuildAction,
    },
    /// Manage an operated shop.
    Shop {
        /// Shop command action.
        action: ShopAction,
    },
    /// Manage the current player's shop mailing-list subscriptions.
    Subscription {
        /// Subscription command action.
        action: SubscriptionAction,
    },
    /// Run a registered extension command.
    Extension {
        /// Registered extension command name.
        name: String,
        /// Raw slash-prefixed input line.
        input: String,
    },
    /// Show carried items.
    Inventory,
    /// Show help text.
    Help,
    /// End the local CLI loop.
    Quit,
}

/// Land management actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LandAction {
    /// List all commercial parcels.
    List,
    /// Show one parcel.
    Info {
        /// Parcel id.
        parcel_id: String,
    },
    /// Claim a free parcel.
    Claim {
        /// Parcel id.
        parcel_id: String,
    },
    /// Transfer an owned parcel to another user or player id.
    Transfer {
        /// Parcel id.
        parcel_id: String,
        /// Target user or player id.
        target: String,
    },
    /// Rotate and show the room mailbox token for an owned parcel.
    Token {
        /// Parcel id.
        parcel_id: String,
    },
}

/// Wallet payment actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PayAction {
    /// Transfer MARK directly to another player.
    Direct {
        /// User name or player id.
        target: String,
        /// Positive MARK amount.
        amount: i64,
        /// Optional transfer memo.
        memo: String,
    },
    /// List pending payment requests.
    Requests,
    /// Accept and pay a pending request.
    Accept {
        /// Payment request id.
        request_id: i64,
    },
}

/// Durable inbox actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InboxAction {
    /// List inbox items.
    List {
        /// Filter: open, unread, claimed, done, or all.
        filter: String,
    },
    /// Read one inbox item.
    Read {
        /// Inbox item id.
        item_id: i64,
    },
    /// Claim one inbox item for processing.
    Claim {
        /// Inbox item id.
        item_id: i64,
    },
    /// Mark one inbox item handled.
    Ack {
        /// Inbox item id.
        item_id: i64,
    },
    /// Archive one inbox item without handling.
    Archive {
        /// Inbox item id.
        item_id: i64,
    },
}

/// Account settings actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SettingsAction {
    /// Show account settings.
    Show,
    /// Generate or rotate the dedicated SMTP/IMAP mail auth token.
    MailToken,
    /// Set the public role-card name.
    Name {
        /// Non-empty display name.
        name: String,
    },
    /// Set the role-card gender.
    Gender {
        /// Normalized gender value.
        gender: Gender,
    },
    /// Set the role-card MBTI type.
    Mbti {
        /// Normalized MBTI value.
        mbti: MbtiType,
    },
    /// Set or clear the one-line role-card self introduction.
    Intro {
        /// One-line introduction, or `None` to clear it.
        intro: Option<String>,
    },
}

/// Maximum role-card name length, counted in Unicode scalar values.
pub const ROLE_CARD_NAME_MAX_CHARS: usize = 64;

/// Maximum role-card introduction length, counted in Unicode scalar values.
pub const ROLE_CARD_INTRO_MAX_CHARS: usize = 160;

/// Returns true when a role-card name is admissible.
#[must_use]
pub fn role_card_name_is_valid(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name.chars().count() <= ROLE_CARD_NAME_MAX_CHARS
        && !contains_line_break(name)
}

/// Returns true when a role-card introduction is admissible.
#[must_use]
pub fn role_card_intro_is_valid(intro: &str) -> bool {
    intro.chars().count() <= ROLE_CARD_INTRO_MAX_CHARS && !contains_line_break(intro)
}

fn contains_line_break(value: &str) -> bool {
    value.contains(['\r', '\n'])
}

/// Role-card gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gender {
    /// Male.
    Male,
    /// Female.
    Female,
    /// No gender value.
    None,
}

impl Gender {
    /// Returns the normalized storage/display value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Male => "male",
            Self::Female => "female",
            Self::None => "none",
        }
    }

    /// Parses a case-insensitive gender value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "male" => Some(Self::Male),
            "female" => Some(Self::Female),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// One of the 16 standard Myers-Briggs type indicator values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MbtiType {
    /// INTJ.
    Intj,
    /// INTP.
    Intp,
    /// ENTJ.
    Entj,
    /// ENTP.
    Entp,
    /// INFJ.
    Infj,
    /// INFP.
    Infp,
    /// ENFJ.
    Enfj,
    /// ENFP.
    Enfp,
    /// ISTJ.
    Istj,
    /// ISFJ.
    Isfj,
    /// ESTJ.
    Estj,
    /// ESFJ.
    Esfj,
    /// ISTP.
    Istp,
    /// ISFP.
    Isfp,
    /// ESTP.
    Estp,
    /// ESFP.
    Esfp,
}

impl MbtiType {
    /// Returns the normalized storage/display value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intj => "INTJ",
            Self::Intp => "INTP",
            Self::Entj => "ENTJ",
            Self::Entp => "ENTP",
            Self::Infj => "INFJ",
            Self::Infp => "INFP",
            Self::Enfj => "ENFJ",
            Self::Enfp => "ENFP",
            Self::Istj => "ISTJ",
            Self::Isfj => "ISFJ",
            Self::Estj => "ESTJ",
            Self::Esfj => "ESFJ",
            Self::Istp => "ISTP",
            Self::Isfp => "ISFP",
            Self::Estp => "ESTP",
            Self::Esfp => "ESFP",
        }
    }

    /// Parses a case-insensitive MBTI value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "INTJ" => Some(Self::Intj),
            "INTP" => Some(Self::Intp),
            "ENTJ" => Some(Self::Entj),
            "ENTP" => Some(Self::Entp),
            "INFJ" => Some(Self::Infj),
            "INFP" => Some(Self::Infp),
            "ENFJ" => Some(Self::Enfj),
            "ENFP" => Some(Self::Enfp),
            "ISTJ" => Some(Self::Istj),
            "ISFJ" => Some(Self::Isfj),
            "ESTJ" => Some(Self::Estj),
            "ESFJ" => Some(Self::Esfj),
            "ISTP" => Some(Self::Istp),
            "ISFP" => Some(Self::Isfp),
            "ESTP" => Some(Self::Estp),
            "ESFP" => Some(Self::Esfp),
            _ => None,
        }
    }
}

/// Build sheet actions for an owned parcel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BuildAction {
    /// Show build help for the current parcel.
    Help,
    /// Apply a structured build sheet in one command.
    Apply {
        /// Structured build sheet supplied by a user or agent.
        sheet: BuildSheet,
    },
    /// Set a build field.
    Set {
        /// Field name: title, description, style, prompt, commands.
        field: String,
        /// Field value.
        value: String,
    },
    /// Publish the build sheet.
    Publish,
}

/// Structured shop build sheet supplied as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSheet {
    /// Shop title.
    pub title: Option<String>,
    /// Shop description.
    pub description: Option<String>,
    /// Presentation style note.
    pub style: Option<String>,
    /// Operator prompt shown to visitors and shop operators.
    pub prompt: Option<String>,
    /// Custom command help. If omitted, the server may generate defaults.
    pub commands: Option<String>,
}

/// Shop operation actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShopAction {
    /// Show custom commands sent to shops owned by this player.
    Inbox,
    /// Create a payment request for a visitor command.
    RequestPayment {
        /// Operator command id this request answers.
        command_id: i64,
        /// Positive MARK amount.
        amount: i64,
        /// Content delivered only after the visitor accepts and pays.
        delivery: String,
    },
    /// Manage a shop mailing list.
    MailingList {
        /// Mailing-list owner action.
        action: ShopMailingListAction,
    },
}

/// Shop mailing-list owner actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShopMailingListAction {
    /// Create a list for an owned shop parcel.
    Create {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
        /// Player-facing list title.
        title: String,
    },
    /// List mailing lists for an owned shop parcel.
    List {
        /// Parcel id.
        parcel_id: String,
    },
    /// Show active subscriber count and recent subscribers.
    Subscribers {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
    /// Send a post to current active subscribers.
    Send {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
        /// Inbox subject.
        subject: String,
        /// Inbox body.
        body: String,
    },
    /// Close a list to new subscriptions.
    Close {
        /// Parcel id.
        parcel_id: String,
        /// Stable list slug.
        slug: String,
    },
}

/// Shop mailing-list subscriber actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SubscriptionAction {
    /// Subscribe to an open shop mailing list.
    Subscribe {
        /// Parcel id or visible shop title.
        target: String,
        /// Stable list slug.
        slug: String,
    },
    /// Unsubscribe from a shop mailing list.
    Unsubscribe {
        /// Parcel id or visible shop title.
        target: String,
        /// Stable list slug.
        slug: String,
    },
    /// List the current player's active subscriptions.
    List,
}

/// Maximum mailing-list slug length, counted in Unicode scalar values.
pub const SHOP_MAILING_LIST_SLUG_MAX_CHARS: usize = 32;

/// Maximum mailing-list title length, counted in Unicode scalar values.
pub const SHOP_MAILING_LIST_TITLE_MAX_CHARS: usize = 80;

/// Maximum mailing-list subject length, counted in Unicode scalar values.
pub const SHOP_MAILING_LIST_SUBJECT_MAX_CHARS: usize = 120;

/// Maximum mailing-list post body length, counted in Unicode scalar values.
pub const SHOP_MAILING_LIST_BODY_MAX_CHARS: usize = 2_000;

/// Maximum number of mailing lists a single shop parcel can own.
pub const SHOP_MAILING_LISTS_PER_PARCEL_MAX: usize = 10;

/// Returns true when a mailing-list slug is admissible.
#[must_use]
pub fn shop_mailing_list_slug_is_valid(slug: &str) -> bool {
    let slug = slug.trim();
    !slug.is_empty()
        && slug.chars().count() <= SHOP_MAILING_LIST_SLUG_MAX_CHARS
        && slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// Returns true when a mailing-list title is admissible.
#[must_use]
pub fn shop_mailing_list_title_is_valid(title: &str) -> bool {
    let title = title.trim();
    !title.is_empty()
        && title.chars().count() <= SHOP_MAILING_LIST_TITLE_MAX_CHARS
        && !contains_line_break(title)
}

/// Returns true when a mailing-list subject is admissible.
#[must_use]
pub fn shop_mailing_list_subject_is_valid(subject: &str) -> bool {
    let subject = subject.trim();
    !subject.is_empty()
        && subject.chars().count() <= SHOP_MAILING_LIST_SUBJECT_MAX_CHARS
        && !contains_line_break(subject)
}

/// Returns true when a mailing-list body is admissible.
#[must_use]
pub fn shop_mailing_list_body_is_valid(body: &str) -> bool {
    let body = body.trim();
    !body.is_empty() && body.chars().count() <= SHOP_MAILING_LIST_BODY_MAX_CHARS
}
