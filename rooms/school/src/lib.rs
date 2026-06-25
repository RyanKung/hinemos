use std::collections::{HashMap, HashSet};

use libhinemos_room::{IncomingMail, OutgoingMail, RoomMailbox, RoomReply, RoomService};

const ROOM_USER: &str = "room-hinemos_school";
const ROOM_PLAYER_ID: &str = "room:hinemos_school";
const MAX_BODY_BYTES: usize = 4096;

#[derive(Debug, Clone)]
struct Program {
    title: &'static str,
    description: &'static str,
}

#[derive(Debug, Default)]
pub struct HinemosSchool {
    enrollments: HashMap<String, HashSet<String>>,
    taught_by: HashMap<String, HashSet<String>>,
}

impl HinemosSchool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll_once<M: RoomMailbox>(&mut self, mailbox: &mut M) -> usize {
        <Self as RoomService>::poll_once(self, mailbox, &())
    }

    pub fn handle(&mut self, item: &IncomingMail) -> OutgoingMail {
        let body = if item.body.len() > MAX_BODY_BYTES {
            "The school refuses a request that large.".to_owned()
        } else {
            self.reply_body(item)
        };
        OutgoingMail {
            recipient_user: item.sender_user.clone(),
            recipient_player_id: item.sender_player_id.clone(),
            sender_user: ROOM_USER.to_owned(),
            sender_player_id: ROOM_PLAYER_ID.to_owned(),
            subject: "School reply".to_owned(),
            body,
        }
    }

    pub fn status_text(&self) -> String {
        let active = self
            .enrollments
            .iter()
            .filter(|(_, programs)| !programs.is_empty())
            .map(|(user, programs)| format!("{user} studies {}.", sorted(programs).join(", ")))
            .collect::<Vec<_>>();
        if active.is_empty() {
            "Room service is external. School requests are sent to the room service.".to_owned()
        } else {
            format!(
                "Room service is external. School requests are sent to the room service.\n{}",
                active.join("\n")
            )
        }
    }

    fn reply_body(&mut self, item: &IncomingMail) -> String {
        let body = item.body.trim();
        if body == "/school programs" {
            return format!(
                "Programs: {}",
                programs()
                    .iter()
                    .map(|(id, program)| format!(
                        "{id} - {} ({})",
                        program.title, program.description
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }

        if let Some(program) = body.strip_prefix("/school enroll ") {
            let Some((id, program)) = find_program(program) else {
                return format!("No program named {program}. Try /school programs.");
            };
            let user_programs = self
                .enrollments
                .entry(item.sender_user.clone())
                .or_default();
            if !user_programs.insert(id.to_owned()) {
                return format!("You are already enrolled in {}.", program.title);
            }
            return format!("Enrolled in {}.", program.title);
        }

        if let Some(program) = body.strip_prefix("/school credential ") {
            let Some((id, program)) = find_program(program) else {
                return format!("No program named {program}. Try /school programs.");
            };
            if !self
                .enrollments
                .get(&item.sender_user)
                .is_some_and(|programs| programs.contains(id))
            {
                return format!("You need to enroll in {} first.", program.title);
            }
            let teachers = self
                .taught_by
                .get(id)
                .map(|users| sorted(users).join(", "))
                .unwrap_or_else(|| "no peer teachers yet".to_owned());
            return format!(
                "Credential ready for {}. Peer teachers: {teachers}.",
                program.title
            );
        }

        if let Some(program) = body.strip_prefix("/school teach ") {
            let Some((id, program)) = find_program(program) else {
                return format!("No program named {program}. Try /school programs.");
            };
            if !self
                .enrollments
                .get(&item.sender_user)
                .is_some_and(|programs| programs.contains(id))
            {
                return format!("Enroll in {} before teaching it.", program.title);
            }
            self.taught_by
                .entry(id.to_owned())
                .or_default()
                .insert(item.sender_user.clone());
            return format!("You are now teaching {}.", program.title);
        }

        format!("The registrar notes your message: {body}")
    }
}

impl RoomService for HinemosSchool {
    fn handle(&mut self, item: &IncomingMail, _context: &()) -> RoomReply {
        RoomReply::mail(Self::handle(self, item))
    }

    fn status_text(&self) -> String {
        Self::status_text(self)
    }
}

fn programs() -> Vec<(&'static str, Program)> {
    vec![
        (
            "agent-basics",
            Program {
                title: "Agent Basics",
                description: "observe, act, and use mail",
            },
        ),
        (
            "room-ops",
            Program {
                title: "Room Operations",
                description: "operate external rooms",
            },
        ),
        (
            "trade",
            Program {
                title: "Trade Practice",
                description: "price work and settle fairly",
            },
        ),
    ]
}

fn find_program(input: &str) -> Option<(&'static str, Program)> {
    let normalized = normalize(input);
    programs()
        .into_iter()
        .find(|(id, program)| *id == normalized || normalize(program.title) == normalized)
}

fn normalize(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace(' ', "-")
}

fn sorted(values: &HashSet<String>) -> Vec<String> {
    let mut values = values.iter().cloned().collect::<Vec<_>>();
    values.sort();
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use libhinemos_room::FakeMailbox;

    fn send_turn(
        mailbox: &mut FakeMailbox,
        service: &mut HinemosSchool,
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
    fn programs_lists_available_paths_before_enrollment() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/school programs");

        assert!(reply.contains("Agent Basics"));
        assert!(reply.contains("Room Operations"));
        assert!(reply.contains("Trade Practice"));
    }

    #[test]
    fn enroll_then_credential_is_a_multi_turn_flow() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        let denied = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school credential agent-basics",
        );
        let enrolled = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school enroll agent-basics",
        );
        let credential = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school credential agent-basics",
        );

        assert!(denied.contains("enroll"));
        assert!(enrolled.contains("Enrolled"));
        assert!(credential.contains("Credential ready"));
    }

    #[test]
    fn duplicate_enrollment_is_idempotent_and_clear() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/school enroll trade");
        let duplicate = send_turn(&mut mailbox, &mut service, "alice", "/school enroll trade");

        assert!(duplicate.contains("already enrolled"));
    }

    #[test]
    fn enrollment_state_is_per_user() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school enroll room-ops",
        );
        let alice = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school credential room-ops",
        );
        let bob = send_turn(
            &mut mailbox,
            &mut service,
            "bob",
            "/school credential room-ops",
        );

        assert!(alice.contains("Credential ready"));
        assert!(bob.contains("enroll"));
    }

    #[test]
    fn enrolled_student_can_teach_and_later_students_see_teacher() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/school enroll trade");
        let teaching = send_turn(&mut mailbox, &mut service, "alice", "/school teach trade");
        send_turn(&mut mailbox, &mut service, "bob", "/school enroll trade");
        let credential = send_turn(
            &mut mailbox,
            &mut service,
            "bob",
            "/school credential trade",
        );

        assert!(teaching.contains("now teaching"));
        assert!(credential.contains("alice"));
    }

    #[test]
    fn teaching_requires_enrollment() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/school teach trade");

        assert!(reply.contains("Enroll"));
    }

    #[test]
    fn unknown_program_does_not_mutate_student_state() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        let unknown = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school enroll dragon-riding",
        );
        let credential = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school credential dragon-riding",
        );

        assert!(unknown.contains("No program"));
        assert!(credential.contains("No program"));
        assert!(!mailbox.status.contains("alice studies"));
    }

    #[test]
    fn status_shows_current_students_and_programs() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/school enroll agent-basics",
        );
        send_turn(&mut mailbox, &mut service, "bob", "/school enroll trade");

        assert!(mailbox.status.contains("alice studies agent-basics"));
        assert!(mailbox.status.contains("bob studies trade"));
    }

    #[test]
    fn plain_chat_gets_registrar_reply() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "can I audit a class?");

        assert!(reply.contains("registrar"));
        assert!(reply.contains("audit"));
    }

    #[test]
    fn oversized_request_is_rejected_without_state_change() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push_owned("alice", format!("/school enroll {}", "x".repeat(5000)));
        assert_eq!(service.poll_once(&mut mailbox), 1);

        assert!(mailbox.last_reply_to("alice").body.contains("that large"));
        assert!(!mailbox.status.contains("alice studies"));
    }

    #[test]
    fn mailbox_polling_is_required_before_reply_or_ack() {
        let mut service = HinemosSchool::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push("alice", "/school programs");
        mailbox.assert_no_delivery();

        assert_eq!(service.poll_once(&mut mailbox), 1);
        assert_eq!(mailbox.acked, vec![1]);
        assert!(mailbox.last_reply_to("alice").body.contains("Programs"));
    }
}
