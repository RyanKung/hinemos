//! Shared SDK primitives for Hinemos room services.

#![deny(missing_docs)]

/// Mail delivered from a player to a room service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMail {
    /// Storage identity of the incoming mailbox item.
    pub id: i64,
    /// User name of the player who sent the request.
    pub sender_user: String,
    /// Player identity of the request sender.
    pub sender_player_id: String,
    /// Plain text room command or message body.
    pub body: String,
}

/// Mail produced by a room service for a player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingMail {
    /// User name that receives the reply.
    pub recipient_user: String,
    /// Player identity that receives the reply.
    pub recipient_player_id: String,
    /// User name that sends the reply.
    pub sender_user: String,
    /// Player identity that sends the reply.
    pub sender_player_id: String,
    /// Mail subject shown to the recipient.
    pub subject: String,
    /// Plain text mail body.
    pub body: String,
}

/// A room reply plus host-side effects requested by the room logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomReply {
    /// Mail response sent back to the requester.
    pub mail: OutgoingMail,
    /// Effects the host must interpret after accepting the room response.
    pub effects: Vec<RoomEffect>,
}

impl RoomReply {
    /// Build a reply that only sends mail.
    pub fn mail(mail: OutgoingMail) -> Self {
        Self {
            mail,
            effects: Vec::new(),
        }
    }

    /// Add one host-side effect to the reply.
    pub fn with_effect(mut self, effect: RoomEffect) -> Self {
        self.effects.push(effect);
        self
    }
}

impl From<OutgoingMail> for RoomReply {
    fn from(mail: OutgoingMail) -> Self {
        Self::mail(mail)
    }
}

/// Host-side action requested by pure room logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomEffect {
    /// Credit MARK to the player who sent the room request.
    CreditPlayerMark {
        /// Positive MARK amount to credit.
        amount: i64,
        /// Domain reason for the credit.
        reason: CreditReason,
    },
    /// Publish a room-authored broadcast message.
    PublishBroadcast {
        /// Broadcast body to persist in the host world.
        body: String,
    },
    /// Apply a marriage registry operation.
    MarriageRegistry {
        /// Registry operation requested by the room.
        action: MarriageRegistryAction,
    },
}

/// Domain reason for a player MARK credit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditReason {
    /// Wage paid by the Workers Society room.
    WorkerWage,
}

/// Host-side marriage registry operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarriageRegistryAction {
    /// Register a marriage with the named target user or player.
    RegisterMarriage {
        /// User or player name supplied by the requester.
        target: String,
    },
    /// Show the requester's current certificate.
    ShowCertificate,
    /// Dissolve the requester's active marriage.
    Divorce,
}

/// Service contract implemented by room logic crates.
pub trait RoomService<Context = ()> {
    /// Handle one incoming room mail item.
    fn handle(&mut self, item: &IncomingMail, context: &Context) -> RoomReply;

    /// Render the room status line shown by the host.
    fn status_text(&self) -> String;

    /// Poll a mailbox once and send mail replies for all unread items.
    fn poll_once<M: RoomMailbox>(&mut self, mailbox: &mut M, context: &Context) -> usize {
        let mut handled = 0;
        for item in mailbox.unread() {
            mailbox.ack(item.id);
            let reply = self.handle(&item, context);
            mailbox.apply_effects(reply.effects);
            mailbox.send(reply.mail);
            handled += 1;
        }
        mailbox.update_status(self.status_text());
        handled
    }
}

/// Mailbox transport used by room services.
pub trait RoomMailbox {
    /// Return unread room mail items.
    fn unread(&mut self) -> Vec<IncomingMail>;
    /// Mark one incoming mail item as handled.
    fn ack(&mut self, id: i64);
    /// Send one outgoing room mail reply.
    fn send(&mut self, mail: OutgoingMail);
    /// Apply host-side effects emitted by the room service.
    fn apply_effects(&mut self, effects: Vec<RoomEffect>) {
        drop(effects);
    }
    /// Update the status text shown by the host room.
    fn update_status(&mut self, status: String);
}

/// In-memory mailbox for room tests.
#[derive(Debug, Default)]
pub struct FakeMailbox {
    /// Pending unread messages.
    pub unread: Vec<IncomingMail>,
    /// Message ids acknowledged by the service.
    pub acked: Vec<i64>,
    /// Mail sent by the service.
    pub sent: Vec<OutgoingMail>,
    /// Effects emitted by the service.
    pub effects: Vec<RoomEffect>,
    /// Last status text written by the service.
    pub status: String,
    next_id: i64,
}

impl FakeMailbox {
    /// Push a test message from the named sender.
    pub fn push(&mut self, sender: &str, body: &str) {
        self.push_owned(sender, body.to_owned());
    }

    /// Push a test message with an owned body.
    pub fn push_owned(&mut self, sender: &str, body: String) {
        self.next_id += 1;
        self.unread.push(IncomingMail {
            id: self.next_id,
            sender_user: sender.to_owned(),
            sender_player_id: format!("player:{sender}"),
            body,
        });
    }

    /// Return the last reply sent to a user.
    pub fn last_reply_to(&self, user: &str) -> &OutgoingMail {
        self.sent
            .iter()
            .rev()
            .find(|mail| mail.recipient_user == user)
            .expect("reply for user")
    }

    /// Count sent messages.
    pub fn sent_count(&self) -> usize {
        self.sent.len()
    }

    /// Assert that no message has been sent or acked.
    pub fn assert_no_delivery(&self) {
        assert!(
            self.sent.is_empty(),
            "mail should not be delivered before poll"
        );
        assert!(
            self.acked.is_empty(),
            "mail should not be acked before poll"
        );
        assert!(
            self.effects.is_empty(),
            "room effects should not be emitted before poll"
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

    fn apply_effects(&mut self, effects: Vec<RoomEffect>) {
        self.effects.extend(effects);
    }

    fn update_status(&mut self, status: String) {
        self.status = status;
    }
}
