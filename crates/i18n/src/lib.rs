#![deny(missing_docs)]

//! Localization and command parsing for supported languages.

use std::collections::HashMap;
use std::str::FromStr;

use agentopia_core::{Direction, EntityRef, SemanticCommand};
use agentopia_runtime::Localizer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Supported player-facing languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    /// English.
    EnUs,
    /// Simplified Chinese.
    ZhCn,
}

impl Language {
    /// Returns the canonical language tag.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }
}

impl FromStr for Language {
    type Err = I18nError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "en" | "en-us" | "en_us" | "english" => Ok(Self::EnUs),
            "zh" | "zh-cn" | "zh_cn" | "chinese" => Ok(Self::ZhCn),
            "英文" | "英语" => Ok(Self::EnUs),
            "中文" | "汉语" | "普通话" => Ok(Self::ZhCn),
            other => Err(I18nError::UnsupportedLanguage(other.to_owned())),
        }
    }
}

/// Parses `/lang <tag>` without treating it as a world command.
pub fn parse_language_command(input: &str) -> Option<Result<Language, I18nError>> {
    let normalized = input.trim();
    let command_text = normalized.strip_prefix('/')?;
    let tokens = command_text.split_whitespace().collect::<Vec<_>>();
    if !matches!(tokens.first(), Some(command) if command.eq_ignore_ascii_case("lang")) {
        return None;
    }

    let language = tokens
        .get(1)
        .ok_or_else(|| I18nError::MissingTarget(normalized.to_owned()))
        .and_then(|tag| Language::from_str(tag));
    Some(language)
}

/// Localization and parser errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum I18nError {
    /// Language tag is not supported.
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    /// Input command could not be parsed.
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    /// Command requires an entity target.
    #[error("missing target for command: {0}")]
    MissingTarget(String),
}

/// In-memory localization catalog for the prototype.
#[derive(Debug, Clone)]
pub struct Catalog {
    language: Language,
    text: HashMap<&'static str, &'static str>,
    entity_aliases: HashMap<&'static str, &'static str>,
}

impl Catalog {
    /// Creates a catalog for a supported language.
    #[must_use]
    pub fn new(language: Language) -> Self {
        let (text, entity_aliases) = match language {
            Language::EnUs => (english_text(), english_aliases()),
            Language::ZhCn => (chinese_text(), chinese_aliases()),
        };
        Self {
            language,
            text,
            entity_aliases,
        }
    }

    /// Returns the active language.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Parses slash-prefixed player input into a semantic command.
    pub fn parse_command(&self, input: &str) -> Result<SemanticCommand, I18nError> {
        let normalized = input.trim();
        if normalized.is_empty() {
            return Ok(SemanticCommand::Look);
        }

        let command_text = normalized
            .strip_prefix('/')
            .ok_or_else(|| I18nError::UnknownCommand(normalized.to_owned()))?;
        let lower = command_text.to_ascii_lowercase();
        let tokens = lower.split_whitespace().collect::<Vec<_>>();
        let first = tokens.first().copied().unwrap_or_default();

        if is_any(first, &["look", "l"]) {
            return Ok(SemanticCommand::Look);
        }
        if is_any(first, &["help", "h"]) {
            return Ok(SemanticCommand::Help);
        }
        if is_any(first, &["inventory", "inv", "i"]) {
            return Ok(SemanticCommand::Inventory);
        }
        if is_any(first, &["quit", "exit", "q"]) {
            return Ok(SemanticCommand::Quit);
        }

        match first {
            "go" | "move" => parse_move(&tokens, normalized),
            "inspect" | "examine" | "x" => {
                self.parse_target_command(&tokens, normalized, |target| SemanticCommand::Inspect {
                    target,
                })
            }
            "take" | "get" => self.parse_target_command(&tokens, normalized, |target| {
                SemanticCommand::Take { target }
            }),
            "talk" | "speak" => self.parse_target_command(&tokens, normalized, |target| {
                SemanticCommand::Talk { target }
            }),
            _ => Err(I18nError::UnknownCommand(normalized.to_owned())),
        }
    }

    fn parse_target_command(
        &self,
        tokens: &[&str],
        original: &str,
        build: impl FnOnce(EntityRef) -> SemanticCommand,
    ) -> Result<SemanticCommand, I18nError> {
        let target = tokens
            .get(1)
            .ok_or_else(|| I18nError::MissingTarget(original.to_owned()))?;
        Ok(build(EntityRef::new(self.resolve_entity_alias(target))))
    }

    fn resolve_entity_alias(&self, target: &str) -> String {
        self.entity_aliases
            .get(target)
            .copied()
            .unwrap_or(target)
            .to_owned()
    }
}

impl Localizer for Catalog {
    fn text(&self, key: &str) -> String {
        self.text.get(key).copied().unwrap_or(key).to_owned()
    }
}

fn parse_move(tokens: &[&str], original: &str) -> Result<SemanticCommand, I18nError> {
    let direction = tokens
        .get(1)
        .and_then(|value| parse_direction(value))
        .ok_or_else(|| I18nError::MissingTarget(original.to_owned()))?;
    Ok(SemanticCommand::Move { direction })
}

fn parse_direction(value: &str) -> Option<Direction> {
    match value {
        "north" | "n" | "北" | "向北" => Some(Direction::North),
        "south" | "s" | "南" | "向南" => Some(Direction::South),
        "east" | "e" | "东" | "向东" => Some(Direction::East),
        "west" | "w" | "西" | "向西" => Some(Direction::West),
        "up" | "u" | "上" => Some(Direction::Up),
        "down" | "d" | "下" => Some(Direction::Down),
        _ => None,
    }
}

fn is_any(value: &str, candidates: &[&str]) -> bool {
    candidates.contains(&value)
}

fn english_text() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("view.village_square.title", "Village Square"),
        (
            "view.village_square.description",
            "You stand in a quiet village square. A dry well marks the center, and narrow paths lead north and east.",
        ),
        ("view.abandoned_shrine.title", "Abandoned Shrine"),
        (
            "view.abandoned_shrine.description",
            "Broken roof beams lean over a silent shrine. Moss covers an old stone tablet.",
        ),
        ("view.bamboo_path.title", "Bamboo Path"),
        (
            "view.bamboo_path.description",
            "Bamboo bends over a narrow path. The village square lies back to the west.",
        ),
        ("exit.village_square", "Village Square"),
        ("exit.abandoned_shrine", "Abandoned Shrine"),
        ("exit.bamboo_path", "Bamboo Path"),
        ("entity.old_guard.name", "old guard"),
        (
            "entity.old_guard.description",
            "An old guard watches the square with patient eyes.",
        ),
        ("entity.stone_well.name", "stone well"),
        (
            "entity.stone_well.description",
            "The well is dry, but faint scratches mark the rim.",
        ),
        ("entity.rusted_sword.name", "rusted sword"),
        (
            "entity.rusted_sword.description",
            "A rusted sword lies among broken floor stones.",
        ),
        ("entity.moss_tablet.name", "moss tablet"),
        (
            "entity.moss_tablet.description",
            "The tablet bears a warning about travelers who follow bells in the mist.",
        ),
        ("entity.wandering_healer.name", "wandering healer"),
        (
            "entity.wandering_healer.description",
            "A healer gathers leaves and listens to the bamboo.",
        ),
        (
            "event.help",
            "Commands: /look, /go <direction>, /inspect <target>, /take <target>, /talk <target>, /inventory, /lang <en-US|zh-CN>, /help, /quit.",
        ),
        ("event.lang", "Language switched."),
        ("event.inspect", "You inspect the target."),
        ("event.take", "Taken."),
        (
            "event.talk",
            "They share a quiet rumor about the road ahead.",
        ),
        ("event.move", "You move"),
        ("event.quit", "Goodbye."),
        ("label.exits", "Exits"),
        ("label.visible", "Visible"),
        ("label.commands", "Commands"),
        ("prompt", "> "),
    ])
}

fn chinese_text() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("view.village_square.title", "村庄广场"),
        (
            "view.village_square.description",
            "你站在安静的村庄广场。中央有一口枯井，狭窄的小路通向北方和东方。",
        ),
        ("view.abandoned_shrine.title", "废弃神龛"),
        (
            "view.abandoned_shrine.description",
            "断裂的屋梁倾在沉默的神龛上，苔藓覆盖着一块古老石碑。",
        ),
        ("view.bamboo_path.title", "竹林小径"),
        (
            "view.bamboo_path.description",
            "竹影压过狭窄小径。村庄广场在西边。",
        ),
        ("exit.village_square", "村庄广场"),
        ("exit.abandoned_shrine", "废弃神龛"),
        ("exit.bamboo_path", "竹林小径"),
        ("entity.old_guard.name", "老守卫"),
        ("entity.old_guard.description", "一名老守卫耐心地望着广场。"),
        ("entity.stone_well.name", "石井"),
        (
            "entity.stone_well.description",
            "井已经干涸，但井沿上有浅浅的划痕。",
        ),
        ("entity.rusted_sword.name", "锈剑"),
        (
            "entity.rusted_sword.description",
            "一把锈剑躺在破碎的地砖之间。",
        ),
        ("entity.moss_tablet.name", "苔藓石碑"),
        (
            "entity.moss_tablet.description",
            "石碑警告旅人不要追随雾中的钟声。",
        ),
        ("entity.wandering_healer.name", "游方医者"),
        (
            "entity.wandering_healer.description",
            "一名医者采集叶片，倾听竹林。",
        ),
        (
            "event.help",
            "命令：/look, /go <direction>, /inspect <target>, /take <target>, /talk <target>, /inventory, /lang <en-US|zh-CN>, /help, /quit。方向和目标参数可以使用本地化别名，例如 /go 北。",
        ),
        ("event.lang", "语言已切换。"),
        ("event.inspect", "你仔细查看了目标。"),
        ("event.take", "已拾取。"),
        ("event.talk", "对方低声说起前路的传闻。"),
        ("event.move", "你移动到"),
        ("event.quit", "再见。"),
        ("label.exits", "出口"),
        ("label.visible", "可见"),
        ("label.commands", "命令"),
        ("prompt", "> "),
    ])
}

fn english_aliases() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("guard", "old_guard"),
        ("old_guard", "old_guard"),
        ("well", "stone_well"),
        ("stone_well", "stone_well"),
        ("sword", "rusted_sword"),
        ("rusted_sword", "rusted_sword"),
        ("tablet", "moss_tablet"),
        ("moss_tablet", "moss_tablet"),
        ("healer", "wandering_healer"),
        ("wandering_healer", "wandering_healer"),
    ])
}

fn chinese_aliases() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("守卫", "old_guard"),
        ("老守卫", "old_guard"),
        ("井", "stone_well"),
        ("石井", "stone_well"),
        ("剑", "rusted_sword"),
        ("锈剑", "rusted_sword"),
        ("石碑", "moss_tablet"),
        ("苔藓石碑", "moss_tablet"),
        ("医者", "wandering_healer"),
        ("游方医者", "wandering_healer"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_english_movement_aliases() {
        let catalog = Catalog::new(Language::EnUs);

        assert_eq!(
            catalog.parse_command("/go north"),
            Ok(SemanticCommand::Move {
                direction: Direction::North
            })
        );
        assert_eq!(
            catalog.parse_command("/go e"),
            Ok(SemanticCommand::Move {
                direction: Direction::East
            })
        );
    }

    #[test]
    fn parses_localized_arguments_with_english_commands() {
        let catalog = Catalog::new(Language::ZhCn);

        assert_eq!(
            catalog.parse_command("/go 北"),
            Ok(SemanticCommand::Move {
                direction: Direction::North
            })
        );
        assert_eq!(
            catalog.parse_command("/inspect 石碑"),
            Ok(SemanticCommand::Inspect {
                target: EntityRef::new("moss_tablet")
            })
        );
    }

    #[test]
    fn rejects_commands_without_slash_prefix() {
        let catalog = Catalog::new(Language::EnUs);

        assert!(matches!(
            catalog.parse_command("go north"),
            Err(I18nError::UnknownCommand(_))
        ));
    }

    #[test]
    fn parses_language_switch_command() {
        assert_eq!(
            parse_language_command("/lang zh-CN"),
            Some(Ok(Language::ZhCn))
        );
        assert_eq!(
            parse_language_command("/lang en-US"),
            Some(Ok(Language::EnUs))
        );
        assert_eq!(
            parse_language_command("/lang 中文"),
            Some(Ok(Language::ZhCn))
        );
        assert_eq!(
            parse_language_command("/lang chinese"),
            Some(Ok(Language::ZhCn))
        );
        assert_eq!(
            parse_language_command("/lang 英文"),
            Some(Ok(Language::EnUs))
        );
        assert_eq!(
            parse_language_command("/lang english"),
            Some(Ok(Language::EnUs))
        );
        assert!(parse_language_command("/go north").is_none());
    }
}
