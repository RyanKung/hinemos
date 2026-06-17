use libhinemos_room::{IncomingMail, OutgoingMail, RoomMailbox};

const ROOM_USER: &str = "room-hinemos_daily_seer";
const ROOM_PLAYER_ID: &str = "room:hinemos_daily_seer";
const MAX_BODY_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressEvent {
    pub occurred_at: String,
    pub source: String,
    pub event_type: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressDigest {
    pub issue_date: String,
    pub events: Vec<PressEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewspaperReply {
    pub mail: OutgoingMail,
    pub broadcast: Option<String>,
}

#[derive(Debug, Default)]
pub struct HinemosDailySeer;

impl HinemosDailySeer {
    pub fn new() -> Self {
        Self
    }

    pub fn poll_once<M: RoomMailbox>(&mut self, mailbox: &mut M, digest: &PressDigest) -> usize {
        let mut handled = 0;
        for item in mailbox.unread() {
            mailbox.ack(item.id);
            let reply = self.handle(&item, digest);
            mailbox.send(reply.mail);
            handled += 1;
        }
        mailbox.update_status(self.status_text());
        handled
    }

    pub fn handle(&mut self, item: &IncomingMail, digest: &PressDigest) -> NewspaperReply {
        let body = if item.body.len() > MAX_BODY_BYTES {
            ReplyBody::Mail("The editor refuses a submission that large.".to_owned())
        } else {
            self.reply_body(item, digest)
        };
        let (body, broadcast) = match body {
            ReplyBody::Mail(body) => (body, None),
            ReplyBody::Publish { body, broadcast } => (body, Some(broadcast)),
        };
        NewspaperReply {
            mail: OutgoingMail {
                recipient_user: item.sender_user.clone(),
                recipient_player_id: item.sender_player_id.clone(),
                sender_user: ROOM_USER.to_owned(),
                sender_player_id: ROOM_PLAYER_ID.to_owned(),
                subject: "Daily Seer reply".to_owned(),
                body,
            },
            broadcast,
        }
    }

    pub fn status_text(&self) -> String {
        "Room service is external. The press prints daily summaries and update reports.".to_owned()
    }

    fn reply_body(&mut self, item: &IncomingMail, digest: &PressDigest) -> ReplyBody {
        let body = item.body.trim();
        if matches!(body, "/paper today" | "/paper latest" | "/news today") {
            return ReplyBody::Mail(render_daily_issue(digest));
        }
        if matches!(body, "/paper help" | "/help") {
            return ReplyBody::Mail(help_text());
        }
        if let Some(rest) = body.strip_prefix("/paper publish ") {
            return publish_reply(&item.sender_user, rest);
        }
        if let Some(rest) = body.strip_prefix("/paper report ") {
            return publish_reply(&item.sender_user, rest);
        }
        if let Some(rest) = body.strip_prefix("/paper submit ") {
            return ReplyBody::Mail(format!(
                "News tip filed for the editor: {}\nUse /paper publish <headline> | <body> when this should become an island update report.",
                rest.trim()
            ));
        }
        ReplyBody::Mail(format!(
            "The editor cannot print that line yet.\n{}",
            help_text()
        ))
    }
}

enum ReplyBody {
    Mail(String),
    Publish { body: String, broadcast: String },
}

fn publish_reply(sender: &str, rest: &str) -> ReplyBody {
    let Some((headline, story)) = parse_report(rest) else {
        return ReplyBody::Mail(
            "Use /paper publish <headline> | <body> so the report has a headline and story."
                .to_owned(),
        );
    };
    let broadcast = format!("Daily Seer Update: {headline}\n{story}\nFiled by {sender}.");
    ReplyBody::Publish {
        body: format!("Printed update report: {headline}. It is now visible in /news."),
        broadcast,
    }
}

fn parse_report(rest: &str) -> Option<(String, String)> {
    let (headline, story) = rest.split_once('|')?;
    let headline = headline.trim();
    let story = story.trim();
    if headline.is_empty() || story.is_empty() {
        return None;
    }
    Some((headline.to_owned(), story.to_owned()))
}

fn render_daily_issue(digest: &PressDigest) -> String {
    let mut lines = vec![
        "The Hinemos Daily Seer".to_owned(),
        format!("Issue: {}", digest.issue_date),
        "The ink rearranges itself into the day's public record.".to_owned(),
        String::new(),
    ];
    if digest.events.is_empty() {
        lines.push("No public island events have reached the press desk today.".to_owned());
    } else {
        lines.push("Today on the island:".to_owned());
        for event in digest.events.iter().take(8) {
            lines.push(format!(
                "- [{}] {}: {}",
                event.occurred_at,
                event_label(event),
                event.content
            ));
        }
    }
    lines.push(String::new());
    lines.push("Commands: /paper today, /paper latest, /paper submit <note>, /paper report <headline> | <body>.".to_owned());
    lines.join("\n")
}

fn event_label(event: &PressEvent) -> String {
    match event.source.as_str() {
        "broadcast" => "Update report".to_owned(),
        "chat" => "Street talk".to_owned(),
        "trade" => "Market ledger".to_owned(),
        "shop" => "Shop desk".to_owned(),
        other => format!("{other} {}", event.event_type),
    }
}

fn help_text() -> String {
    "Daily Seer commands:\n- /paper today prints today's public island summary.\n- /paper latest is the same summary.\n- /paper submit <note> files a tip for the editor.\n- /paper report <headline> | <body> sends a persistent island update report.\n- /paper publish <headline> | <body> is the same report command.".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libhinemos_room::FakeMailbox;

    fn digest() -> PressDigest {
        PressDigest {
            issue_date: "2026-06-16".to_owned(),
            events: vec![
                PressEvent {
                    occurred_at: "09:00".to_owned(),
                    source: "broadcast".to_owned(),
                    event_type: "broadcast_sent".to_owned(),
                    content: "Broadcast: rooms are online".to_owned(),
                },
                PressEvent {
                    occurred_at: "09:05".to_owned(),
                    source: "trade".to_owned(),
                    event_type: "mark_transfer_sent".to_owned(),
                    content: "Transferred 5 MARK".to_owned(),
                },
            ],
        }
    }

    fn send_turn(service: &mut HinemosDailySeer, body: &str) -> NewspaperReply {
        let item = IncomingMail {
            id: 1,
            sender_user: "alice".to_owned(),
            sender_player_id: "player:alice".to_owned(),
            body: body.to_owned(),
        };
        service.handle(&item, &digest())
    }

    #[test]
    fn today_prints_digest_without_broadcasting() {
        let mut service = HinemosDailySeer::new();
        let reply = send_turn(&mut service, "/paper today");
        assert!(reply.mail.body.contains("The Hinemos Daily Seer"));
        assert!(reply.mail.body.contains("Update report"));
        assert!(reply.broadcast.is_none());
    }

    #[test]
    fn publish_creates_broadcast_report() {
        let mut service = HinemosDailySeer::new();
        let reply = send_turn(
            &mut service,
            "/paper publish Room Services | H1 through H5 are staffed.",
        );
        assert!(
            reply
                .mail
                .body
                .contains("Printed update report: Room Services")
        );
        assert_eq!(
            reply.broadcast,
            Some(
                "Daily Seer Update: Room Services\nH1 through H5 are staffed.\nFiled by alice."
                    .to_owned()
            )
        );
    }

    #[test]
    fn malformed_publish_is_not_broadcast() {
        let mut service = HinemosDailySeer::new();
        let reply = send_turn(&mut service, "/paper publish missing separator");
        assert!(reply.mail.body.contains("Use /paper publish"));
        assert!(reply.broadcast.is_none());
    }

    #[test]
    fn mailbox_polling_requires_poll_before_reply() {
        let mut service = HinemosDailySeer::new();
        let mut mailbox = FakeMailbox::default();
        mailbox.push("alice", "/paper today");
        mailbox.assert_no_delivery();
        assert_eq!(service.poll_once(&mut mailbox, &digest()), 1);
        assert_eq!(mailbox.acked, vec![1]);
        assert!(mailbox.last_reply_to("alice").body.contains("Daily Seer"));
    }
}
