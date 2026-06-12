use std::collections::{HashMap, HashSet};

use libhinemos_room::{IncomingMail, OutgoingMail, RoomMailbox};

const ROOM_USER: &str = "room-workers_society";
const ROOM_PLAYER_ID: &str = "room:workers_society";
const MAX_BODY_BYTES: usize = 4096;

#[derive(Debug, Clone)]
struct Position {
    title: &'static str,
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
            "The clerk refuses a work order that large.".to_owned()
        } else {
            self.reply_body(item)
        };
        OutgoingMail {
            recipient_user: item.sender_user.clone(),
            recipient_player_id: item.sender_player_id.clone(),
            sender_user: ROOM_USER.to_owned(),
            sender_player_id: ROOM_PLAYER_ID.to_owned(),
            subject: "Workers Society reply".to_owned(),
            body,
        }
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

    fn reply_body(&mut self, item: &IncomingMail) -> String {
        let body = item.body.trim();
        if body == "/position list" {
            return positions()
                .iter()
                .map(|(id, position)| {
                    format!("{id}: {} pays {} MARK", position.title, position.wage)
                })
                .collect::<Vec<_>>()
                .join("; ");
        }

        if let Some(position) = body.strip_prefix("/position apply ") {
            let Some((id, position)) = find_position(position) else {
                return format!("No position named {position}. Try /position list.");
            };
            self.worker_mut(&item.sender_user)
                .applied
                .insert(id.to_owned());
            return format!("Application recorded for {}.", position.title);
        }

        if let Some(position) = body.strip_prefix("/position start ") {
            let Some((id, position)) = find_position(position) else {
                return format!("No position named {position}. Try /position list.");
            };
            if !self
                .workers
                .get(&item.sender_user)
                .is_some_and(|worker| worker.applied.contains(id))
            {
                return format!("Apply for {} before starting.", position.title);
            }
            if let Some(worker) = self.active_by_position.get(id)
                && worker != &item.sender_user
            {
                return format!(
                    "{position_title} is already assigned to {worker}.",
                    position_title = position.title
                );
            }
            let worker = self.worker_mut(&item.sender_user);
            if worker.active.as_deref() == Some(id) {
                return format!("You are already working {}.", position.title);
            }
            if worker.active.is_some() {
                return "Finish your active position before starting another.".to_owned();
            }
            worker.active = Some(id.to_owned());
            self.active_by_position
                .insert(id.to_owned(), item.sender_user.clone());
            return format!("Started {}.", position.title);
        }

        if body == "/position finish" {
            let Some(active) = self
                .workers
                .get(&item.sender_user)
                .and_then(|worker| worker.active.clone())
            else {
                return "You have no active position to finish.".to_owned();
            };
            let (_, position) = find_position(&active).expect("active position exists");
            let worker = self.worker_mut(&item.sender_user);
            worker.active = None;
            worker.completed.push(active.clone());
            worker.owed += position.wage;
            self.active_by_position.remove(&active);
            return format!(
                "Finished {}. Wage owed: {} MARK.",
                position.title, position.wage
            );
        }

        if body == "/position claim" {
            let worker = self.worker_mut(&item.sender_user);
            if worker.owed == 0 {
                return "No wages are ready to claim.".to_owned();
            }
            let amount = worker.owed;
            worker.owed = 0;
            worker.claimed += amount;
            return format!("Claimed {amount} MARK in wages.");
        }

        if let Some(rest) = body.strip_prefix("/position feedback ") {
            let mut parts = rest.splitn(3, ' ');
            let Some(user) = parts.next().filter(|value| !value.is_empty()) else {
                return "Feedback needs a user, score, and comment.".to_owned();
            };
            let Some(score) = parts.next().filter(|value| value.parse::<i32>().is_ok()) else {
                return "Feedback score must be a number.".to_owned();
            };
            let Some(comment) = parts.next().filter(|value| !value.is_empty()) else {
                return "Feedback needs a comment.".to_owned();
            };
            self.worker_mut(user)
                .feedback
                .push(format!("{} from {}: {}", score, item.sender_user, comment));
            return format!("Feedback recorded for {user}.");
        }

        format!("The clerk notes your message: {body}")
    }

    fn worker_mut(&mut self, user: &str) -> &mut WorkerState {
        self.workers.entry(user.to_owned()).or_default()
    }
}

fn positions() -> Vec<(&'static str, Position)> {
    vec![
        (
            "dock-runner",
            Position {
                title: "Dock Runner",
                wage: 40,
            },
        ),
        (
            "ledger-clerk",
            Position {
                title: "Ledger Clerk",
                wage: 55,
            },
        ),
        (
            "street-sweeper",
            Position {
                title: "Street Sweeper",
                wage: 25,
            },
        ),
    ]
}

fn find_position(input: &str) -> Option<(&'static str, Position)> {
    let normalized = normalize(input);
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
    use libhinemos_room::FakeMailbox;

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

        assert!(reply.contains("Dock Runner"));
        assert!(reply.contains("40 MARK"));
        assert!(reply.contains("Ledger Clerk"));
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

        assert!(mailbox.status.contains("alice is working dock-runner"));
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
        assert!(mailbox.last_reply_to("alice").body.contains("Dock Runner"));
    }
}
