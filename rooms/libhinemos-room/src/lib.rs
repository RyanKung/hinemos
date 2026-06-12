#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMail {
    pub id: i64,
    pub sender_user: String,
    pub sender_player_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingMail {
    pub recipient_user: String,
    pub recipient_player_id: String,
    pub sender_user: String,
    pub sender_player_id: String,
    pub subject: String,
    pub body: String,
}

pub trait RoomMailbox {
    fn unread(&mut self) -> Vec<IncomingMail>;
    fn ack(&mut self, id: i64);
    fn send(&mut self, mail: OutgoingMail);
    fn update_status(&mut self, status: String);
}

#[derive(Debug, Default)]
pub struct FakeMailbox {
    pub unread: Vec<IncomingMail>,
    pub acked: Vec<i64>,
    pub sent: Vec<OutgoingMail>,
    pub status: String,
    next_id: i64,
}

impl FakeMailbox {
    pub fn push(&mut self, sender: &str, body: &str) {
        self.push_owned(sender, body.to_owned());
    }

    pub fn push_owned(&mut self, sender: &str, body: String) {
        self.next_id += 1;
        self.unread.push(IncomingMail {
            id: self.next_id,
            sender_user: sender.to_owned(),
            sender_player_id: format!("player:{sender}"),
            body,
        });
    }

    pub fn last_reply_to(&self, user: &str) -> &OutgoingMail {
        self.sent
            .iter()
            .rev()
            .find(|mail| mail.recipient_user == user)
            .expect("reply for user")
    }

    pub fn sent_count(&self) -> usize {
        self.sent.len()
    }

    pub fn assert_no_delivery(&self) {
        assert!(
            self.sent.is_empty(),
            "mail should not be delivered before poll"
        );
        assert!(
            self.acked.is_empty(),
            "mail should not be acked before poll"
        );
    }
}

impl RoomMailbox for FakeMailbox {
    fn unread(&mut self) -> Vec<IncomingMail> {
        std::mem::take(&mut self.unread)
    }

    fn ack(&mut self, id: i64) {
        self.acked.push(id);
    }

    fn send(&mut self, mail: OutgoingMail) {
        self.sent.push(mail);
    }

    fn update_status(&mut self, status: String) {
        self.status = status;
    }
}
