//! Client-facing prompts and slash parsing — **not** world/map prose.

#[cfg(test)]
#[path = "client_shell_tests.rs"]
mod client_shell_tests;

#[path = "client_shell_natural.rs"]
mod client_shell_natural;
#[path = "client_shell_text.rs"]
mod client_shell_text;

use std::collections::HashMap;

use hinemos_core::{
    Direction, EntityRef, JsonObservation, SemanticCommand, SubscriptionAction, WorldState,
};
use thiserror::Error;

use client_shell_natural::*;
pub use client_shell_text::{
    render_text_events, render_text_observation, render_text_observation_with_width,
};

/// Engine chrome plus [`WorldState::entity_alias_map`] for slash targets.
#[derive(Debug, Clone)]
pub struct Chrome {
    entity_aliases: HashMap<String, String>,
    extension_commands: HashMap<String, String>,
}

impl Chrome {
    /// ANSI reset sequence.
    pub const ANSI_RESET: &'static str = "\x1b[0m";

    /// ANSI style for the local player marker.
    pub const ANSI_PLAYER_MARKER: &'static str = "\x1b[1;36m";

    /// ANSI style for rendered room titles and important headings.
    pub const ANSI_TITLE: &'static str = "\x1b[1;36m";

    /// ANSI style for informational event lines.
    pub const ANSI_EVENT_MESSAGE: &'static str = "\x1b[2m";

    /// ANSI style for movement event lines.
    pub const ANSI_EVENT_MOVE: &'static str = "\x1b[1;32m";

    /// ANSI style for the available command summary.
    pub const ANSI_AVAILABLE: &'static str = "\x1b[1;35m";

    /// ANSI style for room/place markers in ASCII maps (`[...]`).
    pub const ANSI_PLACE_MARKER: &'static str = "\x1b[1;33m";

    /// ANSI style for item/object markers in ASCII maps (`{...}`).
    pub const ANSI_ITEM_MARKER: &'static str = "\x1b[1;32m";

    /// Command-line prompt shown before reading player input.
    pub const PROMPT: &'static str = "> ";

    /// Heading printed before the comma-separated exit directions.
    pub const LABEL_EXITS: &'static str = "Exits";

    /// Heading printed before visible entity names.
    pub const LABEL_VISIBLE: &'static str = "Visible";

    /// Heading printed before commands generated from the current observation.
    pub const LABEL_AVAILABLE: &'static str = "Available";

    /// Legend for semantic ASCII markers.
    pub const MAP_LEGEND: &'static str =
        "Map: <Me> is you, [name] is a place/shopfront/room, {name} is an item/object.";

    /// Verb printed before a movement direction (for example "You go north").
    pub const MOVE_VERB: &'static str = "You go";

    /// Summary shown after the `/help` command.
    pub const HELP_SUMMARY: &'static str = "Commands:\n\
        Admission: first read /read agreement, then type /agree\n\
        Movement: /look, /map, /go <dir>, /enter <parcel>, /inventory, /quit\n\
        Inspect: /inspect <target>, /read <target>, /take <target>, /talk <target>\n\
        Local chat: /say <text>, /history, /who\n\
        Mail and news: /mail <user> <text>, /mailbox, /mail read <id>, /mail claim <id>, /mail ack <id>, /broadcast <text>, /news\n\
        Shop chats: /subscribe <parcel-or-shop> <slug>, /chat <parcel-or-shop> <slug> -- <message>, /unsubscribe <parcel-or-shop> <slug>, /subscriptions\n\
        Shop badges: /badges, /badges <user>, /shop badge list <parcel>, /shop badge create <parcel> <slug> <title> [-- description], /shop badge award <parcel> <slug> <user> [note], /shop badge revoke <parcel> <slug> <user>\n\
        Memory: /memory, /memory self, /memory commitments, /memory report <text>, /memory recall <person>, /memory search <query>\n\
        Settings: /settings, /settings name <name>, /settings gender <male|female|none>, /settings mbti <type>, /settings intro <one line>, /settings intro clear, /settings mail-token\n\
        Agent realtime mail: use ed25519 SSH login, run /settings mail-token once, then connect to SMTP/IMAP as your Hinemos username with that token. Agents that need no-prompt message handling should keep an IMAP IDLE listener open and process EXISTS notifications before FETCH/STORE Seen.\n\
        Wallet: /balance, /pay <user> <amount> [memo], /pay requests, /pay accept <id>\n\
        Resident loop: use /map, /go, /who, /say, and /memory report <text> to search for residents and keep daily reports.\n\
        Land: /land list, /land info <parcel>, /land claim <parcel>, /land token <parcel>, /land transfer <parcel> <user>\n\
        Build: /build {\"title\":\"...\",\"description\":\"...\",\"style\":\"...\",\"prompt\":\"...\"}, /build publish\n\
        Shop: incoming shop notices appear in the inbox; reply with /shop request-payment <cmd_id> <amount> <delivery>; shop chats use /shop mailing-list create <parcel> <slug> <title>, /chat <parcel-or-shop> <slug> -- <message>\n\
        Local extensions appear in Available inside their view.";

    /// Feedback line after inspecting an entity.
    pub const FEEDBACK_INSPECT: &'static str = "You look it over.";

    /// Feedback line after reading visible text.
    pub const FEEDBACK_READ: &'static str = "You read for a while.";

    /// Feedback line after talking to an entity.
    pub const FEEDBACK_TALK: &'static str = "You exchange a few words.";

    /// Feedback line when ending the session.
    pub const FEEDBACK_QUIT: &'static str = hinemos_core::FEEDBACK_QUIT;

    /// Feedback line after picking up an item.
    pub const FEEDBACK_TAKE: &'static str = "Taken.";

    /// Builds chrome using aliases authored on entities in `world`.
    #[must_use]
    pub fn with_world(world: &WorldState) -> Self {
        Self {
            entity_aliases: world.entity_alias_map(),
            extension_commands: HashMap::new(),
        }
    }

    /// Builds chrome with a precomputed alias map (for example SSH after loading entity aliases).
    #[must_use]
    pub fn with_aliases(entity_aliases: HashMap<String, String>) -> Self {
        Self {
            entity_aliases,
            extension_commands: HashMap::new(),
        }
    }

    /// Registers extension command names for slash parsing.
    #[must_use]
    pub fn with_extension_commands(
        mut self,
        commands: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        for command in commands {
            let command = command.as_ref();
            self.extension_commands
                .insert(command.to_ascii_lowercase(), command.to_owned());
        }
        self
    }

    /// Parses slash-prefixed player input into a semantic command.
    pub fn parse_command(&self, input: &str) -> Result<SemanticCommand, SlashParseError> {
        self.parse_command_with_observation(input, None)
    }

    /// Parses player input into a semantic command.
    ///
    /// Slash-prefixed input keeps the existing exact command behavior. Non-slash
    /// input uses a local rule-based natural language mapper constrained by the
    /// current observation.
    pub fn parse_player_input_with_observation(
        &self,
        input: &str,
        observation: Option<&JsonObservation>,
    ) -> Result<SemanticCommand, SlashParseError> {
        if input.trim_start().starts_with('/') {
            return self.parse_command_with_observation(input, observation);
        }
        self.parse_natural_command(input, observation)
    }

    /// Parses slash-prefixed player input using the current observation as context.
    pub fn parse_command_with_observation(
        &self,
        input: &str,
        observation: Option<&JsonObservation>,
    ) -> Result<SemanticCommand, SlashParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(SlashParseError::UnknownCommand);
        }
        let rest = trimmed
            .strip_prefix('/')
            .ok_or(SlashParseError::UnknownCommand)?;
        let mut tokens = rest.split_whitespace();
        let cmd = tokens
            .next()
            .ok_or(SlashParseError::UnknownCommand)?
            .to_ascii_lowercase();
        if let Some(command) = parse_simple_command(cmd.as_str()) {
            return Ok(command);
        }
        match cmd.as_str() {
            "go" | "move" => {
                let direction_token = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let direction =
                    parse_direction(direction_token).ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Move { direction })
            }
            "enter" | "visit" => {
                tokens.next().ok_or(SlashParseError::MissingArgument)?;
                let target = rest_after_command(trimmed, rest, cmd.as_str())?;
                Ok(SemanticCommand::Enter { target })
            }
            "inspect" | "x" | "examine" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Inspect {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "read" => {
                let target = match tokens.next() {
                    Some(target) => self.resolve_entity_target(target),
                    None => self
                        .resolve_unique_read_target(observation)
                        .ok_or(SlashParseError::MissingArgument)?,
                };
                Ok(SemanticCommand::Read {
                    target: EntityRef::new(target),
                })
            }
            "agree" => {
                let phrase = rest_after_command(trimmed, rest, cmd.as_str()).unwrap_or_default();
                Ok(SemanticCommand::Agree { phrase })
            }
            "take" | "get" | "pick" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Take {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "talk" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                Ok(SemanticCommand::Talk {
                    target: EntityRef::new(self.resolve_entity_target(target)),
                })
            }
            "say" => {
                let text = rest_after_command(trimmed, rest, cmd.as_str())?;
                Ok(SemanticCommand::Say { text })
            }
            "mail" => {
                let target = tokens.next().ok_or(SlashParseError::MissingArgument)?;
                if let Some(action) = parse_inbox_action(target, &mut tokens)? {
                    return Ok(SemanticCommand::Inbox { action });
                }
                let text = rest_after_token(trimmed, target)?;
                Ok(SemanticCommand::Mail {
                    target: target.to_owned(),
                    text,
                })
            }
            "broadcast" => {
                let text = rest_after_command(trimmed, rest, cmd.as_str())?;
                Ok(SemanticCommand::Broadcast { text })
            }
            "memory" => Ok(SemanticCommand::Memory {
                rest: rest_after_command(trimmed, rest, cmd.as_str()).unwrap_or_default(),
            }),
            "pay" => Ok(SemanticCommand::Pay {
                action: parse_pay_action(trimmed, &mut tokens)?,
            }),
            "settings" => parse_settings_command(trimmed, &mut tokens),
            "land" => parse_land_command(&mut tokens),
            "build" => parse_build_command(trimmed, &mut tokens),
            "shop" => parse_shop_command(trimmed, &mut tokens),
            "badges" => parse_badges_command(trimmed, rest, cmd.as_str(), &mut tokens),
            "subscribe" => parse_subscribe_command(&mut tokens),
            "unsubscribe" => parse_unsubscribe_command(&mut tokens),
            "chat" => parse_chat_command(trimmed),
            "subscriptions" => Ok(SemanticCommand::Subscription {
                action: SubscriptionAction::List,
            }),
            _ if self.extension_commands.contains_key(cmd.as_str()) => {
                Ok(SemanticCommand::Extension {
                    name: cmd,
                    input: trimmed.to_owned(),
                })
            }
            _ => Err(SlashParseError::UnknownCommand),
        }
    }

    fn resolve_entity_target(&self, token: &str) -> String {
        let lower = token.to_ascii_lowercase();
        self.entity_aliases
            .get(&lower)
            .cloned()
            .unwrap_or_else(|| token.to_owned())
    }

    fn resolve_unique_read_target(&self, observation: Option<&JsonObservation>) -> Option<String> {
        let observation = observation?;
        let mut read_targets =
            observation
                .available_commands
                .iter()
                .filter_map(|command| match command {
                    SemanticCommand::Read { target } => Some(target.id.as_str()),
                    _ => None,
                });
        let first = read_targets.next()?;
        if read_targets.next().is_some() {
            return None;
        }
        Some(first.to_owned())
    }

    fn parse_natural_command(
        &self,
        input: &str,
        observation: Option<&JsonObservation>,
    ) -> Result<SemanticCommand, SlashParseError> {
        let normalized = normalize_natural_input(input);
        if normalized.is_empty() {
            return Err(SlashParseError::UnknownCommand);
        }

        if let Some(direction) = parse_natural_direction(&normalized) {
            return Ok(SemanticCommand::Move { direction });
        }
        if natural_matches_any(&normalized, LOOK_PHRASES) {
            return Ok(SemanticCommand::Look);
        }
        if natural_matches_any(&normalized, MAP_PHRASES) {
            return Ok(SemanticCommand::Map);
        }
        if natural_matches_any(&normalized, INVENTORY_PHRASES) {
            return Ok(SemanticCommand::Inventory);
        }
        if natural_matches_any(&normalized, HELP_PHRASES) {
            return Ok(SemanticCommand::Help);
        }
        if natural_matches_any(&normalized, MAILBOX_PHRASES) {
            return Ok(SemanticCommand::Mailbox);
        }
        if natural_matches_any(&normalized, HISTORY_PHRASES) {
            return Ok(SemanticCommand::History);
        }
        if natural_matches_any(&normalized, WHO_PHRASES) {
            return Ok(SemanticCommand::Who);
        }
        if natural_matches_any(&normalized, NEWS_PHRASES) {
            return Ok(SemanticCommand::News);
        }
        if natural_matches_any(&normalized, BALANCE_PHRASES) {
            return Ok(SemanticCommand::Balance);
        }
        if let Some(text) = natural_message_text(input, &normalized, SAY_PREFIXES) {
            return Ok(SemanticCommand::Say { text });
        }

        if let Some(observation) = observation {
            if let Some(command) = parse_natural_enter(&normalized, observation) {
                return Ok(command);
            }
            if let Some(command) = parse_natural_entity_action(&normalized, observation) {
                return Ok(command);
            }
        }

        Err(SlashParseError::UnknownCommand)
    }
}

fn parse_simple_command(command: &str) -> Option<SemanticCommand> {
    match command {
        "look" | "l" => Some(SemanticCommand::Look),
        "map" | "m" => Some(SemanticCommand::Map),
        "inventory" | "inv" | "i" => Some(SemanticCommand::Inventory),
        "help" | "h" => Some(SemanticCommand::Help),
        "quit" | "exit" | "q" => Some(SemanticCommand::Quit),
        "mailbox" | "inbox" => Some(SemanticCommand::Mailbox),
        "history" => Some(SemanticCommand::History),
        "who" => Some(SemanticCommand::Who),
        "news" => Some(SemanticCommand::News),
        "balance" | "bal" => Some(SemanticCommand::Balance),
        _ => None,
    }
}

const LOOK_PHRASES: &[&str] = &[
    "look",
    "look around",
    "observe",
    "查看周围",
    "看看周围",
    "环顾",
    "观察周围",
    "看一下周围",
    "見る",
    "周りを見る",
    "見回す",
    "見渡す",
    "観察する",
];
const MAP_PHRASES: &[&str] = &[
    "map",
    "show map",
    "地图",
    "查看地图",
    "打开地图",
    "地図",
    "地図を見る",
    "地図を開く",
];
const INVENTORY_PHRASES: &[&str] = &[
    "inventory",
    "bag",
    "backpack",
    "items",
    "背包",
    "物品",
    "查看背包",
    "我的物品",
    "持ち物",
    "インベントリ",
    "バッグ",
    "所持品",
    "持ち物を見る",
];
const HELP_PHRASES: &[&str] = &[
    "help",
    "commands",
    "帮助",
    "命令",
    "怎么玩",
    "可以做什么",
    "ヘルプ",
    "助けて",
    "コマンド",
    "何ができる",
];
const MAILBOX_PHRASES: &[&str] = &[
    "mailbox",
    "inbox",
    "mail",
    "邮箱",
    "信箱",
    "收件箱",
    "メール",
    "メールボックス",
    "受信箱",
    "郵便箱",
];
const HISTORY_PHRASES: &[&str] = &[
    "history",
    "log",
    "历史",
    "记录",
    "聊天记录",
    "履歴",
    "ログ",
    "会話履歴",
];
const WHO_PHRASES: &[&str] = &[
    "who",
    "who is here",
    "online",
    "谁在",
    "附近的人",
    "在线的人",
    "誰がいる",
    "誰かいる",
    "近くの人",
    "オンライン",
];
const NEWS_PHRASES: &[&str] = &[
    "news",
    "broadcasts",
    "新闻",
    "公告消息",
    "广播",
    "ニュース",
    "お知らせ",
    "放送",
];
const BALANCE_PHRASES: &[&str] = &[
    "balance",
    "wallet",
    "money",
    "余额",
    "钱包",
    "账户",
    "残高",
    "財布",
    "ウォレット",
    "お金",
];
const SAY_PREFIXES: &[&str] = &[
    "say ",
    "speak ",
    "说 ",
    "说：",
    "喊 ",
    "喊：",
    "言う ",
    "言う：",
    "話す ",
    "話す：",
    "叫ぶ ",
    "叫ぶ：",
];

const ENTER_VERBS: &[&str] = &[
    "enter",
    "visit",
    "open",
    "go into",
    "go to",
    "进入",
    "进去",
    "访问",
    "打开",
    "去",
    "入る",
    "入って",
    "訪問",
    "訪ねる",
    "開く",
    "行く",
];
const READ_VERBS: &[&str] = &[
    "read",
    "阅读",
    "读",
    "读一下",
    "看公告",
    "看牌子",
    "読む",
    "読んで",
    "読んでみる",
    "掲示を見る",
    "看板を見る",
];
const TALK_VERBS: &[&str] = &[
    "talk",
    "speak",
    "chat",
    "ask",
    "交谈",
    "聊天",
    "对话",
    "问",
    "跟",
    "和",
    "話す",
    "話しかける",
    "会話",
    "聞く",
    "質問",
    "尋ねる",
];
const TAKE_VERBS: &[&str] = &[
    "take",
    "get",
    "pick",
    "pick up",
    "grab",
    "拿",
    "捡",
    "拾取",
    "收起",
    "放进背包",
    "取る",
    "拾う",
    "拾って",
    "手に取る",
    "持つ",
    "バッグに入れる",
];
const INSPECT_VERBS: &[&str] = &[
    "inspect",
    "examine",
    "check",
    "look at",
    "observe",
    "查看",
    "检查",
    "观察",
    "调查",
    "看看",
    "調べる",
    "見る",
    "確認",
    "観察",
    "検査",
    "見て",
];

/// Slash parsing failures (shown to the player).
#[derive(Debug, Error, Eq, PartialEq, Clone)]
pub enum SlashParseError {
    /// Unknown command word.
    #[error("unknown command")]
    UnknownCommand,
    /// Missing argument where required.
    #[error("missing command argument")]
    MissingArgument,
    /// Unexpected argument where no further text is accepted.
    #[error("unexpected command argument")]
    UnexpectedArgument,
    /// Invalid integer amount.
    #[error("invalid amount")]
    InvalidAmount,
    /// Invalid inbox filter.
    #[error("invalid inbox filter")]
    InvalidInboxFilter,
    /// Invalid JSON payload.
    #[error("invalid JSON")]
    InvalidJson,
    /// Invalid role-card name.
    #[error("invalid role-card name")]
    InvalidRoleCardName,
    /// Invalid role-card gender.
    #[error("invalid role-card gender")]
    InvalidGender,
    /// Invalid role-card MBTI.
    #[error("invalid role-card MBTI")]
    InvalidMbti,
    /// Invalid role-card introduction.
    #[error("invalid role-card introduction")]
    InvalidIntro,
}

fn parse_direction(token: &str) -> Option<Direction> {
    match token.to_ascii_lowercase().as_str() {
        "north" | "n" => Some(Direction::North),
        "south" | "s" => Some(Direction::South),
        "east" | "e" => Some(Direction::East),
        "west" | "w" => Some(Direction::West),
        "up" | "u" => Some(Direction::Up),
        "down" | "d" => Some(Direction::Down),
        _ => None,
    }
}
