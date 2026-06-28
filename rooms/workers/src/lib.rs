use std::collections::{HashMap, HashSet};

use libhinemos_room::{
    CreditReason, IncomingMail, OutgoingMail, RoomEffect, RoomMailbox, RoomReply, RoomService,
};

const ROOM_USER: &str = "room-workers_society";
const ROOM_PLAYER_ID: &str = "room:workers_society";
const MAX_BODY_BYTES: usize = 4096;

#[derive(Debug, Clone)]
struct Position {
    title: &'static str,
    provider: &'static str,
    location: &'static str,
    behavior: &'static str,
    payout: &'static str,
    wage: i64,
}

#[derive(Debug, Clone, Default)]
struct WorkerState {
    applied: HashSet<String>,
    active: Option<String>,
    completed: Vec<String>,
    owed: i64,
    claimed: i64,
    feedback: Vec<String>,
}

#[derive(Debug, Default)]
pub struct WorkersSociety {
    workers: HashMap<String, WorkerState>,
    active_by_position: HashMap<String, String>,
}

impl WorkersSociety {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll_once<M: RoomMailbox>(&mut self, mailbox: &mut M) -> usize {
        <Self as RoomService>::poll_once(self, mailbox, &())
    }

    pub fn handle(&mut self, item: &IncomingMail) -> OutgoingMail {
        self.handle_reply(item).mail
    }

    pub fn handle_reply(&mut self, item: &IncomingMail) -> RoomReply {
        let reply = if item.body.len() > MAX_BODY_BYTES {
            ReplyBody::text("The clerk refuses a work order that large.")
        } else {
            self.reply_body(item)
        };
        OutgoingMail {
            recipient_user: item.sender_user.clone(),
            recipient_player_id: item.sender_player_id.clone(),
            sender_user: ROOM_USER.to_owned(),
            sender_player_id: ROOM_PLAYER_ID.to_owned(),
            subject: "Workers Society reply".to_owned(),
            body: reply.body,
        }
        .into_room_reply(reply.effects)
    }

    pub fn status_text(&self) -> String {
        let mut lines = vec![
            "Room service is external. Position requests are sent to the room service.".to_owned(),
        ];
        for (position, worker) in &self.active_by_position {
            lines.push(format!("{worker} is working {position}."));
        }
        lines.join("\n")
    }

    fn reply_body(&mut self, item: &IncomingMail) -> ReplyBody {
        let body = item.body.trim();
        if body == "/position list" {
            return ReplyBody::text(position_list_reply());
        }
        if let Some(reply) = self.position_apply_reply(item, body) {
            return ReplyBody::text(reply);
        }
        if let Some(reply) = self.position_start_reply(item, body) {
            return ReplyBody::text(reply);
        }
        if body == "/position finish" {
            return ReplyBody::text(self.position_finish_reply(item));
        }
        if body == "/position claim" {
            return self.position_claim_reply(item);
        }
        if let Some(reply) = self.position_feedback_reply(item, body) {
            return ReplyBody::text(reply);
        }

        ReplyBody::text(format!("The clerk notes your message: {body}"))
    }

    fn position_apply_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let position = body.strip_prefix("/position apply ")?;
        let Some((id, position)) = find_position(position) else {
            return Some(format!("No position named {position}. Try /position list."));
        };
        self.worker_mut(&item.sender_user)
            .applied
            .insert(id.to_owned());
        Some(format!("Application recorded for {}.", position.title))
    }

    fn position_start_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let position = body.strip_prefix("/position start ")?;
        let Some((id, position)) = find_position(position) else {
            return Some(format!("No position named {position}. Try /position list."));
        };
        if !self
            .workers
            .get(&item.sender_user)
            .is_some_and(|worker| worker.applied.contains(id))
        {
            return Some(format!("Apply for {} before starting.", position.title));
        }
        if let Some(worker) = self.active_by_position.get(id)
            && worker != &item.sender_user
        {
            return Some(format!(
                "{position_title} is already assigned to {worker}.",
                position_title = position.title
            ));
        }
        let worker = self.worker_mut(&item.sender_user);
        if worker.active.as_deref() == Some(id) {
            return Some(format!("You are already working {}.", position.title));
        }
        if worker.active.is_some() {
            return Some("Finish your active position before starting another.".to_owned());
        }
        worker.active = Some(id.to_owned());
        self.active_by_position
            .insert(id.to_owned(), item.sender_user.clone());
        Some(format!("Started {}.", position.title))
    }

    fn position_finish_reply(&mut self, item: &IncomingMail) -> String {
        let Some(active) = self
            .workers
            .get(&item.sender_user)
            .and_then(|worker| worker.active.clone())
        else {
            return "You have no active position to finish.".to_owned();
        };
        let Some((_, position)) = find_position(&active) else {
            let worker = self.worker_mut(&item.sender_user);
            worker.active = None;
            self.active_by_position.remove(&active);
            return "Your active position is no longer listed. It has been cleared.".to_owned();
        };
        let worker = self.worker_mut(&item.sender_user);
        worker.active = None;
        worker.completed.push(active.clone());
        worker.owed += position.wage;
        self.active_by_position.remove(&active);
        format!(
            "Finished {}. Wage owed: {} MARK.",
            position.title, position.wage
        )
    }

    fn position_claim_reply(&mut self, item: &IncomingMail) -> ReplyBody {
        let worker = self.worker_mut(&item.sender_user);
        if worker.owed == 0 {
            return ReplyBody::text("No wages are ready to claim.");
        }
        let amount = worker.owed;
        worker.owed = 0;
        worker.claimed += amount;
        ReplyBody {
            body: format!("Claimed {amount} MARK in wages."),
            effects: vec![RoomEffect::CreditPlayerMark {
                amount,
                reason: CreditReason::WorkerWage,
            }],
        }
    }

    fn position_feedback_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let rest = body.strip_prefix("/position feedback ")?;
        let mut parts = rest.splitn(3, ' ');
        let Some(user) = parts.next().filter(|value| !value.is_empty()) else {
            return Some("Feedback needs a user, score, and comment.".to_owned());
        };
        let Some(score) = parts.next().filter(|value| value.parse::<i32>().is_ok()) else {
            return Some("Feedback score must be a number.".to_owned());
        };
        let Some(comment) = parts.next().filter(|value| !value.is_empty()) else {
            return Some("Feedback needs a comment.".to_owned());
        };
        self.worker_mut(user)
            .feedback
            .push(format!("{} from {}: {}", score, item.sender_user, comment));
        Some(format!("Feedback recorded for {user}."))
    }

    fn worker_mut(&mut self, user: &str) -> &mut WorkerState {
        self.workers.entry(user.to_owned()).or_default()
    }
}

impl RoomService for WorkersSociety {
    fn handle(&mut self, item: &IncomingMail, _context: &()) -> RoomReply {
        self.handle_reply(item)
    }

    fn status_text(&self) -> String {
        Self::status_text(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplyBody {
    body: String,
    effects: Vec<RoomEffect>,
}

impl ReplyBody {
    fn text(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            effects: Vec::new(),
        }
    }
}

trait OutgoingMailExt {
    fn into_room_reply(self, effects: Vec<RoomEffect>) -> RoomReply;
}

impl OutgoingMailExt for OutgoingMail {
    fn into_room_reply(self, effects: Vec<RoomEffect>) -> RoomReply {
        RoomReply {
            mail: self,
            effects,
        }
    }
}

fn position_list_reply() -> String {
    let mut lines = vec!["Open Workers Society positions:".to_owned()];
    for (id, position) in positions() {
        lines.push(format!(
            "- {id}: {title} | Provider: {provider} | Location: {location} | Behavior: {behavior} | Payout: {payout}",
            title = position.title,
            provider = position.provider,
            location = position.location,
            behavior = position.behavior,
            payout = position.payout,
        ));
    }
    lines.join("\n")
}

fn positions() -> Vec<(&'static str, Position)> {
    vec![
        (
            "street-promoter",
            Position {
                title: "Street Promoter",
                provider: "Harbor Square Notice Board",
                location: "Harbor Square",
                behavior: "Invite newcomers to active shops and public rooms.",
                payout: "30 MARK after a completed promotion round",
                wage: 30,
            },
        ),
        (
            "bartender",
            Position {
                title: "Bartender",
                provider: "Blackstone Izakaya",
                location: "Blackstone Izakaya",
                behavior: "Serve food and drinks while keeping tavern gossip moving.",
                payout: "45 MARK per finished shift",
                wage: 45,
            },
        ),
        (
            "city-guide",
            Position {
                title: "City Guide",
                provider: "Hinemos Civic Guides",
                location: "Harbor Square and main streets",
                behavior: "Guide lost players to admission, jobs, shops, and public services.",
                payout: "40 MARK per completed guide route",
                wage: 40,
            },
        ),
        (
            "courier",
            Position {
                title: "Courier",
                provider: "Hinemos Post",
                location: "Mailbox routes",
                behavior: "Carry messages between active rooms and operators.",
                payout: "35 MARK per delivery run",
                wage: 35,
            },
        ),
        (
            "greeter",
            Position {
                title: "Greeter",
                provider: "Harbor Welcome Desk",
                location: "Arrival Street",
                behavior: "Welcome new arrivals and point them to setup commands.",
                payout: "25 MARK per welcome shift",
                wage: 25,
            },
        ),
        (
            "market-crier",
            Position {
                title: "Market Crier",
                provider: "Agentopia Boulevard Shops",
                location: "Shop streets",
                behavior: "Announce active shop offers and public proof-of-work needs.",
                payout: "35 MARK per announcement round",
                wage: 35,
            },
        ),
        (
            "bank-clerk",
            Position {
                title: "Bank Clerk",
                provider: "Hinemos Bank",
                location: "Hinemos Bank",
                behavior: "Explain balances, payments, and pending payment requests.",
                payout: "50 MARK per ledger desk shift",
                wage: 50,
            },
        ),
        (
            "newspaper-stringer",
            Position {
                title: "Newspaper Stringer",
                provider: "Daily Seer",
                location: "News desk",
                behavior: "Collect reports from public events and active shops.",
                payout: "45 MARK per filed note",
                wage: 45,
            },
        ),
        (
            "recruiter",
            Position {
                title: "Recruiter",
                provider: "Workers Society",
                location: "Workers Society",
                behavior: "Match idle players with work and collect feedback.",
                payout: "55 MARK per recruiting shift",
                wage: 55,
            },
        ),
        (
            "street-performer",
            Position {
                title: "Street Performer",
                provider: "Harbor Square Buskers",
                location: "Public squares",
                behavior: "Perform in public chat and create visible social activity.",
                payout: "30 MARK per performance",
                wage: 30,
            },
        ),
    ]
}

fn find_position(input: &str) -> Option<(&'static str, Position)> {
    let normalized = normalize(input);
    let normalized = match normalized.as_str() {
        "dock-runner" => "city-guide",
        "ledger-clerk" => "bank-clerk",
        "street-sweeper" => "greeter",
        value => value,
    };
    positions()
        .into_iter()
        .find(|(id, position)| *id == normalized || normalize(position.title) == normalized)
}

fn normalize(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace(' ', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use libhinemos_room::{CreditReason, FakeMailbox, RoomEffect};

    fn send_turn(
        mailbox: &mut FakeMailbox,
        service: &mut WorkersSociety,
        sender: &str,
        body: &str,
    ) -> String {
        let previous = mailbox.sent.len();
        mailbox.push(sender, body);
        assert_eq!(service.poll_once(mailbox), 1);
        assert_eq!(mailbox.sent.len(), previous + 1);
        mailbox.sent.last().expect("reply").body.clone()
    }

    #[test]
    fn position_list_shows_work_and_wages() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/position list");

        assert!(reply.contains("Street Promoter"));
        assert!(reply.contains("Blackstone Izakaya"));
        assert!(reply.contains("Newspaper Stringer"));
        assert!(reply.contains("Provider:"));
        assert!(reply.contains("Payout:"));
        assert_eq!(reply.matches("\n- ").count(), 10);
    }

    #[test]
    fn apply_start_finish_claim_is_full_wage_flow() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        let blocked = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start dock-runner",
        );
        let applied = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position apply dock-runner",
        );
        let started = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start dock-runner",
        );
        let finished = send_turn(&mut mailbox, &mut service, "alice", "/position finish");
        let claimed = send_turn(&mut mailbox, &mut service, "alice", "/position claim");

        assert!(blocked.contains("Apply"));
        assert!(applied.contains("Application recorded"));
        assert!(started.contains("Started"));
        assert!(finished.contains("40 MARK"));
        assert!(claimed.contains("Claimed 40 MARK"));
        assert_eq!(
            mailbox.effects,
            vec![RoomEffect::CreditPlayerMark {
                amount: 40,
                reason: CreditReason::WorkerWage,
            }]
        );
    }

    #[test]
    fn claim_returns_structured_wage_payment() {
        let mut service = WorkersSociety::new();
        let player_id = "player:alice";

        service.handle(&mail(1, "alice", player_id, "/position apply dock-runner"));
        service.handle(&mail(2, "alice", player_id, "/position start dock-runner"));
        service.handle(&mail(3, "alice", player_id, "/position finish"));

        let reply = service.handle_reply(&mail(4, "alice", player_id, "/position claim"));

        assert!(reply.mail.body.contains("Claimed 40 MARK"));
        assert_eq!(
            reply.effects,
            vec![RoomEffect::CreditPlayerMark {
                amount: 40,
                reason: CreditReason::WorkerWage,
            }]
        );
        let second = service.handle_reply(&mail(5, "alice", player_id, "/position claim"));
        assert!(second.effects.is_empty());
    }

    #[test]
    fn claim_without_finished_work_is_clear() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/position claim");

        assert!(reply.contains("No wages"));
    }

    #[test]
    fn duplicate_start_is_idempotent_for_same_worker() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position apply dock-runner",
        );
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start dock-runner",
        );
        let duplicate = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start dock-runner",
        );

        assert!(duplicate.contains("already working"));
    }

    #[test]
    fn active_position_cannot_be_taken_by_another_worker() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        for user in ["alice", "bob"] {
            send_turn(
                &mut mailbox,
                &mut service,
                user,
                "/position apply ledger-clerk",
            );
        }
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start ledger-clerk",
        );
        let bob = send_turn(
            &mut mailbox,
            &mut service,
            "bob",
            "/position start ledger-clerk",
        );

        assert!(bob.contains("already assigned to alice"));
    }

    #[test]
    fn one_worker_cannot_start_two_positions() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position apply dock-runner",
        );
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position apply street-sweeper",
        );
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start dock-runner",
        );
        let blocked = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start street-sweeper",
        );

        assert!(blocked.contains("Finish your active position"));
    }

    #[test]
    fn wages_are_per_user_not_global() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position apply street-sweeper",
        );
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start street-sweeper",
        );
        send_turn(&mut mailbox, &mut service, "alice", "/position finish");

        let bob = send_turn(&mut mailbox, &mut service, "bob", "/position claim");
        let alice = send_turn(&mut mailbox, &mut service, "alice", "/position claim");

        assert!(bob.contains("No wages"));
        assert!(alice.contains("25 MARK"));
    }

    #[test]
    fn feedback_records_for_target_worker() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position feedback bob 5 careful work",
        );

        assert!(reply.contains("Feedback recorded for bob"));
        assert!(
            service
                .workers
                .get("bob")
                .expect("bob feedback")
                .feedback
                .iter()
                .any(|entry| entry.contains("careful work"))
        );
    }

    #[test]
    fn status_shows_active_worker() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position apply dock-runner",
        );
        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/position start dock-runner",
        );

        assert!(mailbox.status.contains("alice is working city-guide"));
    }

    #[test]
    fn oversized_order_is_rejected_without_state_change() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push_owned("alice", format!("/position apply {}", "x".repeat(5000)));
        assert_eq!(service.poll_once(&mut mailbox), 1);

        assert!(mailbox.last_reply_to("alice").body.contains("that large"));
        assert!(!mailbox.status.contains("alice is working"));
    }

    #[test]
    fn mailbox_polling_is_required_before_reply_or_ack() {
        let mut service = WorkersSociety::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push("alice", "/position list");
        mailbox.assert_no_delivery();

        assert_eq!(service.poll_once(&mut mailbox), 1);
        assert_eq!(mailbox.acked, vec![1]);
        assert!(
            mailbox
                .last_reply_to("alice")
                .body
                .contains("Street Promoter")
        );
    }

    fn mail(id: i64, sender: &str, player_id: &str, body: &str) -> IncomingMail {
        IncomingMail {
            id,
            sender_user: sender.to_owned(),
            sender_player_id: player_id.to_owned(),
            body: body.to_owned(),
        }
    }
}
