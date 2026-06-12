use std::collections::HashMap;
use std::time::{Duration, Instant};

use libhinemos_room::{IncomingMail, OutgoingMail, RoomMailbox};

const ROOM_USER: &str = "room-blackstone_izakaya";
const ROOM_PLAYER_ID: &str = "room:blackstone_izakaya";
const MAX_BODY_BYTES: usize = 4096;

#[derive(Debug)]
pub struct BlackstoneIzakaya {
    drinks: HashMap<String, Instant>,
    gossip: Vec<String>,
    asks: HashMap<String, usize>,
    drink_ttl: Duration,
}

impl Default for BlackstoneIzakaya {
    fn default() -> Self {
        Self::new()
    }
}

impl BlackstoneIzakaya {
    pub fn new() -> Self {
        Self::with_drink_ttl(Duration::from_secs(60))
    }

    pub fn with_drink_ttl(drink_ttl: Duration) -> Self {
        Self {
            drinks: HashMap::new(),
            gossip: Vec::new(),
            asks: HashMap::new(),
            drink_ttl,
        }
    }

    pub fn poll_once<M: RoomMailbox>(&mut self, mailbox: &mut M) -> usize {
        let mut handled = 0;
        for item in mailbox.unread() {
            mailbox.ack(item.id);
            let reply = self.handle(&item);
            mailbox.send(reply);
            handled += 1;
        }
        mailbox.update_status(self.status_text());
        handled
    }

    pub fn handle(&mut self, item: &IncomingMail) -> OutgoingMail {
        let body = if item.body.len() > MAX_BODY_BYTES {
            "The keeper refuses to read a message that large.".to_owned()
        } else {
            self.reply_body(item)
        };
        OutgoingMail {
            recipient_user: item.sender_user.clone(),
            recipient_player_id: item.sender_player_id.clone(),
            sender_user: ROOM_USER.to_owned(),
            sender_player_id: ROOM_PLAYER_ID.to_owned(),
            subject: "Izakaya reply".to_owned(),
            body,
        }
    }

    pub fn status_text(&self) -> String {
        let mut lines = vec![
            "Room service is external. Commands and chat are sent to the room service.".to_owned(),
        ];
        for user in self.drinks.keys() {
            if self.has_drink(user) {
                lines.push(format!("{user} is enjoying a beer."));
            }
        }
        lines.join("\n")
    }

    fn reply_body(&mut self, item: &IncomingMail) -> String {
        let body = item.body.trim();
        if body == "/buy beer" {
            self.drinks
                .insert(item.sender_user.clone(), Instant::now() + self.drink_ttl);
            return "The keeper sets a beer in front of you.".to_owned();
        }

        if matches_gated_command(body) && !self.has_drink(&item.sender_user) {
            return "You should buy a drink first.".to_owned();
        }

        if let Some(question) = body.strip_prefix("/ask ") {
            let topic = normalize_question(question);
            let count = self.asks.entry(topic).or_insert(0);
            *count += 1;
            let mut reply = format!("The keeper answers: {question}");
            if *count >= 2 {
                reply.push_str(". many people have asked the same question.");
            }
            if question.contains("last cup") {
                reply.push_str(&self.gossip_about("last cup"));
            }
            if question.contains("storm ledger") {
                reply.push_str(&self.gossip_about("storm ledger"));
            }
            return reply;
        }

        if let Some(blame) = body.strip_prefix("/blame ") {
            self.gossip.push(blame.to_owned());
            return format!("The keeper writes down the blame: {blame}");
        }

        if let Some(query) = body.strip_prefix("/grep ") {
            return format!("The keeper checks the gossip for {query}.");
        }

        format!("The keeper replies from the counter: {body}")
    }

    fn has_drink(&self, user: &str) -> bool {
        self.drinks
            .get(user)
            .is_some_and(|expires_at| *expires_at > Instant::now())
    }

    fn gossip_about(&self, topic: &str) -> String {
        self.gossip
            .iter()
            .find(|entry| entry.contains(topic))
            .map(|entry| format!(" Gossip says {entry}."))
            .unwrap_or_default()
    }
}

fn matches_gated_command(body: &str) -> bool {
    body.starts_with("/ask ") || body.starts_with("/blame ") || body.starts_with("/grep ")
}

fn normalize_question(question: &str) -> String {
    question
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || character.is_ascii_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use libhinemos_room::FakeMailbox;

    fn send_turn(
        mailbox: &mut FakeMailbox,
        service: &mut BlackstoneIzakaya,
        sender: &str,
        body: &str,
    ) -> String {
        let previous = mailbox.sent.len();
        mailbox.push(sender, body);
        assert_eq!(service.poll_once(mailbox), 1, "one turn should be polled");
        assert_eq!(
            mailbox.sent.len(),
            previous + 1,
            "one room reply should be generated"
        );
        mailbox
            .sent
            .last()
            .expect("reply should exist")
            .body
            .clone()
    }

    #[test]
    fn requires_drink_before_ask_blame_or_grep_across_turns() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        let ask = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/ask where is the harbor gossip?",
        );
        let blame = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/blame the lighthouse keeper",
        );
        let grep = send_turn(&mut mailbox, &mut service, "alice", "/grep harbor");

        assert!(ask.contains("buy a drink first"));
        assert!(blame.contains("buy a drink first"));
        assert!(grep.contains("buy a drink first"));
    }

    #[test]
    fn buy_beer_allows_current_user_until_drink_window_expires() {
        let mut service = BlackstoneIzakaya::with_drink_ttl(Duration::from_millis(25));
        let mut mailbox = FakeMailbox::default();

        assert!(
            send_turn(
                &mut mailbox,
                &mut service,
                "alice",
                "/ask what rumor is fresh tonight?"
            )
            .contains("buy a drink first")
        );
        assert!(send_turn(&mut mailbox, &mut service, "alice", "/buy beer").contains("beer"));
        assert!(
            send_turn(
                &mut mailbox,
                &mut service,
                "alice",
                "/ask what rumor is fresh tonight?"
            )
            .contains("fresh tonight")
        );
        std::thread::sleep(Duration::from_millis(35));
        assert!(
            send_turn(
                &mut mailbox,
                &mut service,
                "alice",
                "/ask is the drink still good?"
            )
            .contains("buy a drink first")
        );
    }

    #[test]
    fn drink_state_is_per_user_not_global() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/buy beer");
        let alice = send_turn(&mut mailbox, &mut service, "alice", "/ask who is here?");
        let bob = send_turn(&mut mailbox, &mut service, "bob", "/ask who is here?");

        assert!(alice.contains("who is here"));
        assert!(bob.contains("buy a drink first"));
    }

    #[test]
    fn blame_creates_gossip_that_later_ask_can_use() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/buy beer");
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/blame lighthouse keeper took the last cup",
        );
        send_turn(&mut mailbox, &mut service, "bob", "/buy beer");
        let bob = send_turn(
            &mut mailbox,
            &mut service,
            "bob",
            "/ask who took the last cup?",
        );

        assert!(bob.contains("lighthouse keeper"));
    }

    #[test]
    fn repeated_questions_return_many_people_asked_answer() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        for user in ["alice", "bob", "carol", "dave", "erin"] {
            send_turn(&mut mailbox, &mut service, user, "/buy beer");
            send_turn(
                &mut mailbox,
                &mut service,
                user,
                "/ask why is the north lantern dim?",
            );
        }

        let erin = mailbox.last_reply_to("erin");
        assert!(erin.body.contains("many people"));
        assert!(erin.body.contains("north lantern"));
    }

    #[test]
    fn plain_chat_at_counter_gets_service_reply_across_multiple_turns() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        let first = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "any rumors from the counter tonight?",
        );
        let second = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "and what about the harbor?",
        );

        assert!(first.contains("The keeper replies"));
        assert!(first.contains("counter tonight"));
        assert!(second.contains("harbor"));
    }

    #[test]
    fn five_user_interleaved_flow_preserves_identity_and_shared_gossip() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        for user in ["alice", "bob", "carol", "dave"] {
            send_turn(&mut mailbox, &mut service, user, "/buy beer");
        }
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/blame the fishmonger hid the storm ledger",
        );
        let bob = send_turn(
            &mut mailbox,
            &mut service,
            "bob",
            "/ask who hid the storm ledger?",
        );
        let erin = send_turn(
            &mut mailbox,
            &mut service,
            "erin",
            "/ask who hid the storm ledger?",
        );
        let carol = send_turn(
            &mut mailbox,
            &mut service,
            "carol",
            "/ask who hid the storm ledger?",
        );
        let dave = send_turn(
            &mut mailbox,
            &mut service,
            "dave",
            "tell me about the storm ledger",
        );

        assert!(bob.contains("fishmonger"));
        assert!(erin.contains("buy a drink first"));
        assert!(carol.contains("many people"));
        assert!(dave.contains("storm ledger"));
    }

    #[test]
    fn status_lists_users_who_are_enjoying_beer() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/buy beer");
        send_turn(&mut mailbox, &mut service, "bob", "hello");

        assert!(mailbox.status.contains("alice is enjoying a beer."));
        assert!(!mailbox.status.contains("bob is enjoying a beer."));
    }

    #[test]
    fn oversized_mail_body_is_rejected_without_mutating_gossip_or_ask_state() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();
        let huge = format!("/ask {}", "x".repeat(MAX_BODY_BYTES + 1));

        mailbox.push_owned("alice", huge);
        assert_eq!(service.poll_once(&mut mailbox), 1);
        assert!(mailbox.last_reply_to("alice").body.contains("that large"));

        send_turn(&mut mailbox, &mut service, "alice", "/buy beer");
        let reply = send_turn(&mut mailbox, &mut service, "alice", "/ask short?");
        assert!(!reply.contains("many people"));
    }

    #[test]
    fn mailbox_polling_is_required_before_any_reply_or_ack_exists() {
        let mut service = BlackstoneIzakaya::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push("alice", "/buy beer");
        assert!(
            mailbox.sent.is_empty(),
            "enqueueing mail does not create replies"
        );
        assert!(
            mailbox.acked.is_empty(),
            "enqueueing mail does not ack mail"
        );

        assert_eq!(service.poll_once(&mut mailbox), 1);
        assert_eq!(mailbox.acked, vec![1]);
        assert!(mailbox.last_reply_to("alice").body.contains("beer"));
    }
}
