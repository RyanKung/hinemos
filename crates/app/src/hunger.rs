use crate::*;

/// MARK balance threshold treated as enough for one food recovery action.
pub const FOOD_RECOVERY_PRICE_MARK: i64 = 20;
/// Hunger points at which non-recovery interactions become restricted.
pub const HUNGER_THRESHOLD_POINTS: i32 = 24;
/// Maximum hunger points persisted for a player.
pub const MAX_HUNGER_POINTS: i32 = 100;
/// Hunger added by one meaningful interaction.
pub const HUNGER_POINTS_PER_INTERACTION: i32 = 1;
/// Cooldown for hungry broke users.
pub const HUNGRY_BROKE_COOLDOWN_SECONDS: i64 = 5 * 60;

/// Decision returned by the hunger gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HungerGateOutcome {
    /// Command may continue.
    Allow,
    /// Command must stop with this player-facing text.
    Block(String),
}

/// Protocol-neutral stored hunger view.
pub trait HungerView {
    /// Stored player id.
    fn player_id(&self) -> &str;

    /// Current hunger points.
    fn hunger_points(&self) -> i32;
}

/// Storage boundary for hunger state.
pub trait HungerStore {
    /// Store error type.
    type Error;
    /// Stored hunger state.
    type Hunger: HungerView;

    /// Loads or creates hunger state for a player.
    async fn player_hunger(&self, player_id: &str) -> Result<Self::Hunger, Self::Error>;

    /// Adds hunger points after a meaningful interaction.
    async fn record_hunger_interaction(
        &self,
        player_id: &str,
        points: i32,
    ) -> Result<Self::Hunger, Self::Error>;

    /// Restores the player's hunger after food is consumed.
    async fn restore_player_hunger(
        &self,
        player_id: &str,
        food: &str,
    ) -> Result<Self::Hunger, Self::Error>;

    /// Records one allowed hungry-broke interaction when the cooldown permits it.
    async fn try_record_hungry_broke_interaction(
        &self,
        player_id: &str,
        cooldown_seconds: i64,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HungerCommandProfile {
    recovery: bool,
    counts_as_interaction: bool,
}

impl HungerCommandProfile {
    const EXEMPT: Self = Self {
        recovery: true,
        counts_as_interaction: false,
    };

    const MEANINGFUL: Self = Self {
        recovery: false,
        counts_as_interaction: true,
    };

    const RECOVERY: Self = Self {
        recovery: true,
        counts_as_interaction: false,
    };
}

impl<S, E> AppService<S>
where
    S: HungerStore<Error = E> + MessageStore<Error = E>,
    <S as MessageStore>::Balance: BalanceView,
{
    /// Checks hunger restrictions for a parsed semantic command.
    pub async fn check_hunger_command(
        &self,
        player_id: &str,
        command: &SemanticCommand,
    ) -> Result<HungerGateOutcome, E> {
        self.check_hunger_profile(player_id, profile_for_command(command))
            .await
    }

    /// Checks hunger restrictions for a raw room-service line.
    pub async fn check_hunger_raw_line(
        &self,
        player_id: &str,
        raw_line: &str,
    ) -> Result<HungerGateOutcome, E> {
        self.check_hunger_profile(player_id, profile_for_raw_line(raw_line, None))
            .await
    }

    /// Checks hunger restrictions for a raw line using room-authored recovery commands.
    pub async fn check_hunger_room_line(
        &self,
        player_id: &str,
        raw_line: &str,
        room: &impl ServiceRoomView,
    ) -> Result<HungerGateOutcome, E> {
        self.check_hunger_profile(
            player_id,
            profile_for_raw_line(raw_line, room.recovery_commands()),
        )
        .await
    }

    async fn check_hunger_profile(
        &self,
        player_id: &str,
        profile: HungerCommandProfile,
    ) -> Result<HungerGateOutcome, E> {
        if !profile.counts_as_interaction && profile.recovery {
            return Ok(HungerGateOutcome::Allow);
        }

        let hunger = self.store.player_hunger(player_id).await?;
        if hunger.hunger_points() < HUNGER_THRESHOLD_POINTS {
            if profile.counts_as_interaction {
                self.store
                    .record_hunger_interaction(player_id, HUNGER_POINTS_PER_INTERACTION)
                    .await?;
            }
            return Ok(HungerGateOutcome::Allow);
        }

        if profile.recovery {
            return Ok(HungerGateOutcome::Allow);
        }

        let balance = self.store.player_balance(player_id).await?;
        if balance.amount() >= FOOD_RECOVERY_PRICE_MARK {
            return Ok(HungerGateOutcome::Block(hungry_with_money_text(
                balance.amount(),
            )));
        }

        if self
            .store
            .try_record_hungry_broke_interaction(player_id, HUNGRY_BROKE_COOLDOWN_SECONDS)
            .await?
        {
            if profile.counts_as_interaction {
                self.store
                    .record_hunger_interaction(player_id, HUNGER_POINTS_PER_INTERACTION)
                    .await?;
            }
            return Ok(HungerGateOutcome::Allow);
        }

        Ok(HungerGateOutcome::Block(hungry_broke_limited_text()))
    }
}

fn profile_for_command(command: &SemanticCommand) -> HungerCommandProfile {
    match command {
        SemanticCommand::Look
        | SemanticCommand::Map
        | SemanticCommand::Inventory
        | SemanticCommand::History
        | SemanticCommand::News
        | SemanticCommand::Who
        | SemanticCommand::Balance
        | SemanticCommand::Mailbox
        | SemanticCommand::Inbox { .. }
        | SemanticCommand::Memory { .. }
        | SemanticCommand::Settings { .. }
        | SemanticCommand::Help
        | SemanticCommand::Quit => HungerCommandProfile::EXEMPT,
        SemanticCommand::Move { .. } | SemanticCommand::Enter { .. } => {
            HungerCommandProfile::RECOVERY
        }
        SemanticCommand::Extension { input, .. } => profile_for_raw_line(input, None),
        _ => HungerCommandProfile::MEANINGFUL,
    }
}

fn profile_for_raw_line(raw_line: &str, recovery_commands: Option<&str>) -> HungerCommandProfile {
    let trimmed = raw_line.trim_start();
    let command = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_ascii_lowercase();
    match command.as_str() {
        "" => HungerCommandProfile::EXEMPT,
        "help" | "look" | "map" | "inventory" | "history" | "news" | "who" | "balance"
        | "mailbox" | "mail" | "memory" | "settings" | "quit" => HungerCommandProfile::EXEMPT,
        "go" | "enter" => HungerCommandProfile::RECOVERY,
        _ if room_recovery_command_matches(trimmed, recovery_commands) => {
            HungerCommandProfile::RECOVERY
        }
        _ => HungerCommandProfile::MEANINGFUL,
    }
}

fn room_recovery_command_matches(raw_line: &str, recovery_commands: Option<&str>) -> bool {
    crate::rooms::command_inputs(recovery_commands)
        .any(|command| crate::rooms::recovery_command_template_matches_input(&command, raw_line))
}

fn hungry_with_money_text(balance: i64) -> String {
    format!(
        "You are too hungry to keep working. Food costs {FOOD_RECOVERY_PRICE_MARK} MARK; you have {balance} MARK. Use an in-game room command that buys or eats food, or find paid work if you need more MARK.\r\n"
    )
}

fn hungry_broke_limited_text() -> String {
    format!(
        "You are hungry and broke. Room commands marked as hunger recovery still work: use in-game work commands to earn MARK, then buy or eat food. Until then, non-recovery interactions are limited to one every {} minutes.\r\n",
        HUNGRY_BROKE_COOLDOWN_SECONDS / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestHunger {
        player_id: String,
        hunger_points: i32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestBalance {
        amount: i64,
    }

    #[derive(Debug)]
    struct TestHungerStore {
        hunger_points: std::sync::Mutex<i32>,
        balance: i64,
        hungry_broke_allowances: std::sync::Mutex<Vec<bool>>,
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl HungerView for TestHunger {
        fn player_id(&self) -> &str {
            &self.player_id
        }

        fn hunger_points(&self) -> i32 {
            self.hunger_points
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
            self.amount
        }
    }

    impl HungerStore for TestHungerStore {
        type Error = std::convert::Infallible;
        type Hunger = TestHunger;

        async fn player_hunger(&self, player_id: &str) -> Result<Self::Hunger, Self::Error> {
            Ok(TestHunger {
                player_id: player_id.to_owned(),
                hunger_points: *self.hunger_points.lock().unwrap(),
            })
        }

        async fn record_hunger_interaction(
            &self,
            player_id: &str,
            points: i32,
        ) -> Result<Self::Hunger, Self::Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("interaction:{player_id}:{points}"));
            let mut hunger_points = self.hunger_points.lock().unwrap();
            *hunger_points += points;
            Ok(TestHunger {
                player_id: player_id.to_owned(),
                hunger_points: *hunger_points,
            })
        }

        async fn restore_player_hunger(
            &self,
            player_id: &str,
            food: &str,
        ) -> Result<Self::Hunger, Self::Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("restore:{player_id}:{food}"));
            let mut hunger_points = self.hunger_points.lock().unwrap();
            *hunger_points = 0;
            Ok(TestHunger {
                player_id: player_id.to_owned(),
                hunger_points: 0,
            })
        }

        async fn try_record_hungry_broke_interaction(
            &self,
            player_id: &str,
            cooldown_seconds: i64,
        ) -> Result<bool, Self::Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("allowance:{player_id}:{cooldown_seconds}"));
            Ok(self
                .hungry_broke_allowances
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(false))
        }
    }

    impl MessageStore for TestHungerStore {
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
            Ok(TestBalance {
                amount: self.balance,
            })
        }

        async fn save_say_message(
            &self,
            _sender_user: &str,
            _sender_player_id: &str,
            _target_view: &str,
            _body: &str,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn save_mail_message(
            &self,
            _sender_user: &str,
            _sender_player_id: &str,
            _target: &str,
            _body: &str,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn save_mail_message_with_subject(
            &self,
            _sender_user: &str,
            _sender_player_id: &str,
            _target: &str,
            _subject: &str,
            _body: &str,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn save_broadcast_message(
            &self,
            _sender_user: &str,
            _sender_player_id: &str,
            _body: &str,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestWorldMessage;

    struct TestRoom {
        recovery_commands: &'static str,
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

    impl RoomMailboxView for TestRoom {
        fn view_id(&self) -> &str {
            "test_room"
        }

        fn room_user(&self) -> Option<&str> {
            Some("room-test")
        }

        fn room_player_id(&self) -> Option<&str> {
            Some("room:test")
        }
    }

    impl ServiceRoomView for TestRoom {
        fn label(&self) -> Option<&str> {
            Some("Test Room")
        }

        fn address(&self) -> Option<&str> {
            Some("T1")
        }

        fn front_view_id(&self) -> Option<&str> {
            Some("test_street")
        }

        fn status_text(&self) -> Option<&str> {
            None
        }

        fn custom_commands(&self) -> Option<&str> {
            None
        }

        fn recovery_commands(&self) -> Option<&str> {
            Some(self.recovery_commands)
        }
    }

    fn store(hunger_points: i32, balance: i64, allowances: Vec<bool>) -> TestHungerStore {
        TestHungerStore {
            hunger_points: std::sync::Mutex::new(hunger_points),
            balance,
            hungry_broke_allowances: std::sync::Mutex::new(allowances),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    #[tokio::test]
    async fn meaningful_command_adds_hunger_before_threshold() {
        let store = store(0, 100, Vec::new());
        let app = AppService::new(store);

        let outcome = app
            .check_hunger_command(
                "player",
                &SemanticCommand::Say {
                    text: "hello".to_owned(),
                },
            )
            .await
            .expect("hunger check");

        assert_eq!(outcome, HungerGateOutcome::Allow);
        assert_eq!(
            app.store().calls.lock().unwrap().as_slice(),
            ["interaction:player:1"]
        );
    }

    #[tokio::test]
    async fn hungry_player_with_money_is_sent_to_food_recovery() {
        let store = store(
            HUNGER_THRESHOLD_POINTS,
            FOOD_RECOVERY_PRICE_MARK,
            Vec::new(),
        );
        let app = AppService::new(store);

        let outcome = app
            .check_hunger_command(
                "player",
                &SemanticCommand::Say {
                    text: "hello".to_owned(),
                },
            )
            .await
            .expect("hunger check");

        assert!(matches!(outcome, HungerGateOutcome::Block(text) if text.contains("Food costs")));
    }

    #[tokio::test]
    async fn hungry_broke_player_gets_one_cooldown_interaction() {
        let store = store(HUNGER_THRESHOLD_POINTS, 0, vec![false, true]);
        let app = AppService::new(store);
        let command = SemanticCommand::Say {
            text: "hello".to_owned(),
        };

        let first = app
            .check_hunger_command("player", &command)
            .await
            .expect("first hunger check");
        let second = app
            .check_hunger_command("player", &command)
            .await
            .expect("second hunger check");

        assert_eq!(first, HungerGateOutcome::Allow);
        assert!(
            matches!(second, HungerGateOutcome::Block(text) if text.contains("one every 5 minutes"))
        );
    }

    #[tokio::test]
    async fn room_authored_recovery_commands_bypass_hunger_gate() {
        let store = store(HUNGER_THRESHOLD_POINTS, 0, Vec::new());
        let app = AppService::new(store);
        let room = TestRoom {
            recovery_commands: "/paper submit <article>\n/paper finish",
        };

        let outcome = app
            .check_hunger_room_line("player", "/paper submit market-report", &room)
            .await
            .expect("hunger check");

        assert_eq!(outcome, HungerGateOutcome::Allow);
        assert!(app.store().calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn placeholder_recovery_commands_need_arguments() {
        let store = store(
            HUNGER_THRESHOLD_POINTS,
            FOOD_RECOVERY_PRICE_MARK,
            Vec::new(),
        );
        let app = AppService::new(store);
        let room = TestRoom {
            recovery_commands: "/paper submit <article>",
        };

        let missing = app
            .check_hunger_room_line("player", "/paper submit", &room)
            .await
            .expect("missing article");
        let blank = app
            .check_hunger_room_line("player", "/paper submit   ", &room)
            .await
            .expect("blank article");

        assert!(matches!(missing, HungerGateOutcome::Block(text) if text.contains("Food costs")));
        assert!(matches!(blank, HungerGateOutcome::Block(text) if text.contains("Food costs")));
    }

    #[tokio::test]
    async fn exact_room_recovery_commands_are_not_suffix_matched() {
        let store = store(
            HUNGER_THRESHOLD_POINTS,
            FOOD_RECOVERY_PRICE_MARK,
            Vec::new(),
        );
        let app = AppService::new(store);
        let room = TestRoom {
            recovery_commands: "/buy bread",
        };

        let exact = app
            .check_hunger_room_line("player", " /BUY bread ", &room)
            .await
            .expect("exact bread command");
        let suffixed = app
            .check_hunger_room_line("player", "/buy bread please", &room)
            .await
            .expect("suffixed bread command");

        assert_eq!(exact, HungerGateOutcome::Allow);
        assert!(matches!(suffixed, HungerGateOutcome::Block(text) if text.contains("Food costs")));
    }

    #[tokio::test]
    async fn memory_commands_are_read_only_under_hunger_gate() {
        let store = store(HUNGER_THRESHOLD_POINTS, 0, Vec::new());
        let app = AppService::new(store);

        let parsed = app
            .check_hunger_command(
                "player",
                &SemanticCommand::Memory {
                    rest: "self".to_owned(),
                },
            )
            .await
            .expect("parsed memory");
        let raw = app
            .check_hunger_raw_line("player", "/memory self")
            .await
            .expect("raw memory");

        assert_eq!(parsed, HungerGateOutcome::Allow);
        assert_eq!(raw, HungerGateOutcome::Allow);
        assert!(app.store().calls.lock().unwrap().is_empty());
    }
}
