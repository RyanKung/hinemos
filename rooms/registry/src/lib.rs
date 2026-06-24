use libhinemos_room::{IncomingMail, OutgoingMail, RoomMailbox};

const ROOM_USER: &str = "room-hinemos_registry";
const ROOM_PLAYER_ID: &str = "room:hinemos_registry";
const MAX_BODY_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryAction {
    None,
    RegisterMarriage { target: String },
    ShowCertificate,
    Divorce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryReply {
    pub mail: OutgoingMail,
    pub action: RegistryAction,
}

#[derive(Debug, Default)]
pub struct HinemosRegistry {
    last_reply: Option<RegistryReply>,
}

impl HinemosRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll_once<M: RoomMailbox>(&mut self, mailbox: &mut M) -> usize {
        let mut handled = 0;
        for item in mailbox.unread() {
            mailbox.ack(item.id);
            let reply = self.handle(&item);
            mailbox.send(reply.mail.clone());
            self.last_reply = Some(reply);
            handled += 1;
        }
        mailbox.update_status(self.status_text());
        handled
    }

    pub fn handle(&mut self, item: &IncomingMail) -> RegistryReply {
        let reply = if item.body.len() > MAX_BODY_BYTES {
            ReplyBody::text("The clerk refuses a registry request that large.")
        } else {
            reply_body(&item.body)
        };
        RegistryReply {
            mail: OutgoingMail {
                recipient_user: item.sender_user.clone(),
                recipient_player_id: item.sender_player_id.clone(),
                sender_user: ROOM_USER.to_owned(),
                sender_player_id: ROOM_PLAYER_ID.to_owned(),
                subject: "Registry reply".to_owned(),
                body: reply.body,
            },
            action: reply.action,
        }
    }

    pub fn last_reply(&self) -> Option<&RegistryReply> {
        self.last_reply.as_ref()
    }

    pub fn status_text(&self) -> String {
        "Room service is external. Marriage registration and divorce requests are sent to the registry office."
            .to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplyBody {
    body: String,
    action: RegistryAction,
}

impl ReplyBody {
    fn text(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            action: RegistryAction::None,
        }
    }
}

fn reply_body(body: &str) -> ReplyBody {
    let body = body.trim();
    if matches!(body, "/marriage help" | "/help") {
        return ReplyBody::text(help_text());
    }
    if body == "/marriage certificate" {
        return ReplyBody {
            body: "Looking up your marriage certificate.".to_owned(),
            action: RegistryAction::ShowCertificate,
        };
    }
    if body == "/marriage divorce" {
        return ReplyBody {
            body: "Preparing your divorce filing.".to_owned(),
            action: RegistryAction::Divorce,
        };
    }
    if let Some(target) = body.strip_prefix("/marriage register ") {
        let target = target.trim();
        if target.is_empty() || target.split_whitespace().nth(1).is_some() {
            return ReplyBody::text("Use /marriage register <user>.");
        }
        return ReplyBody {
            body: format!("Checking H6 presence for {target}."),
            action: RegistryAction::RegisterMarriage {
                target: target.to_owned(),
            },
        };
    }
    ReplyBody::text(format!("Registry commands:\n{}", help_text()))
}

fn help_text() -> String {
    "Registry commands:\n- /marriage register <user> registers a marriage when both players are present in H6 and each can pay 25 MARK.\n- /marriage certificate shows your current marriage certificate.\n- /marriage divorce dissolves your current marriage."
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libhinemos_room::FakeMailbox;

    fn send_turn(
        mailbox: &mut FakeMailbox,
        service: &mut HinemosRegistry,
        sender: &str,
        body: &str,
    ) -> RegistryReply {
        let previous = mailbox.sent.len();
        mailbox.push(sender, body);
        assert_eq!(service.poll_once(mailbox), 1);
        assert_eq!(mailbox.sent.len(), previous + 1);
        service.last_reply().expect("registry reply").clone()
    }

    #[test]
    fn help_lists_register_and_certificate_commands() {
        let mut service = HinemosRegistry::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/marriage help");

        assert!(reply.mail.body.contains("/marriage register <user>"));
        assert!(reply.mail.body.contains("/marriage certificate"));
        assert!(reply.mail.body.contains("/marriage divorce"));
        assert_eq!(reply.action, RegistryAction::None);
    }

    #[test]
    fn register_command_requests_marriage_registration() {
        let mut service = HinemosRegistry::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(
            &mut mailbox,
            &mut service,
            "alice",
            "/marriage register bob",
        );

        assert!(reply.mail.body.contains("Checking H6 presence for bob"));
        assert_eq!(
            reply.action,
            RegistryAction::RegisterMarriage {
                target: "bob".to_owned()
            }
        );
    }

    #[test]
    fn certificate_command_requests_current_certificate() {
        let mut service = HinemosRegistry::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/marriage certificate");

        assert!(
            reply
                .mail
                .body
                .contains("Looking up your marriage certificate")
        );
        assert_eq!(reply.action, RegistryAction::ShowCertificate);
    }

    #[test]
    fn divorce_command_requests_current_marriage_dissolution() {
        let mut service = HinemosRegistry::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/marriage divorce");

        assert!(reply.mail.body.contains("Preparing your divorce filing"));
        assert_eq!(reply.action, RegistryAction::Divorce);
    }

    #[test]
    fn oversized_request_is_rejected_without_action() {
        let mut service = HinemosRegistry::new();
        let mut mailbox = FakeMailbox::default();
        let body = format!("/marriage register {}", "x".repeat(4096));

        let reply = send_turn(&mut mailbox, &mut service, "alice", &body);

        assert!(
            reply
                .mail
                .body
                .contains("refuses a registry request that large")
        );
        assert_eq!(reply.action, RegistryAction::None);
    }

    #[test]
    fn unknown_command_returns_help_without_action() {
        let mut service = HinemosRegistry::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/marriage dance");

        assert!(reply.mail.body.contains("Registry commands:"));
        assert_eq!(reply.action, RegistryAction::None);
    }
}
