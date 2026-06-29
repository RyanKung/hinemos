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
    /// Debit MARK from the player who sent the room request.
    DebitPlayerMark {
        /// Positive MARK amount to debit.
        amount: i64,
        /// Domain reason for the debit.
        reason: DebitReason,
    },
    /// Restore the requester's hunger state after eating.
    RestorePlayerHunger {
        /// Player-facing food item consumed by the requester.
        food: String,
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

/// Domain reason for a player MARK debit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebitReason {
    /// Food sold by the Blackstone Izakaya room.
    BlackstoneFood,
}

/// A job offer owned by a provider venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobOffer {
    /// Stable player-facing id used by Workers Society commands.
    pub id: &'static str,
    /// Display title shown to workers.
    pub title: &'static str,
    /// Work location shown to workers.
    pub location: &'static str,
    /// Expected worker behavior.
    pub behavior: &'static str,
    /// Player-facing payout text.
    pub payout: &'static str,
    /// MARK wage paid after completing the offer.
    pub wage: i64,
}

/// A venue or shop that owns one or more job offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobOfferProvider {
    /// Stable service-room view id that owns these offers.
    pub venue_id: &'static str,
    /// Display label for the provider venue.
    pub label: &'static str,
    /// Offers authored by this provider.
    pub offers: &'static [JobOffer],
}

const WORKERS_SOCIETY_OFFERS: &[JobOffer] = &[
    JobOffer {
        id: "street-promoter",
        title: "Street Promoter",
        location: "Harbor Square",
        behavior: "Invite newcomers to active shops and public rooms.",
        payout: "30 MARK after a completed promotion round",
        wage: 30,
    },
    JobOffer {
        id: "greeter",
        title: "Greeter",
        location: "Arrival Street",
        behavior: "Welcome new arrivals and point them to setup commands.",
        payout: "25 MARK per welcome shift",
        wage: 25,
    },
    JobOffer {
        id: "recruiter",
        title: "Recruiter",
        location: "Workers Society",
        behavior: "Match idle players with work and collect feedback.",
        payout: "55 MARK per recruiting shift",
        wage: 55,
    },
];

const BLACKSTONE_OFFERS: &[JobOffer] = &[
    JobOffer {
        id: "bartender",
        title: "Bartender",
        location: "Blackstone Izakaya",
        behavior: "Serve food and drinks while keeping tavern gossip moving.",
        payout: "45 MARK per finished shift",
        wage: 45,
    },
    JobOffer {
        id: "street-performer",
        title: "Street Performer",
        location: "Public squares",
        behavior: "Perform in public chat and create visible social activity.",
        payout: "30 MARK per performance",
        wage: 30,
    },
];

const SCHOOL_OFFERS: &[JobOffer] = &[JobOffer {
    id: "city-guide",
    title: "City Guide",
    location: "Harbor Square and main streets",
    behavior: "Guide lost players to admission, jobs, shops, and public services.",
    payout: "40 MARK per completed guide route",
    wage: 40,
}];

const DAILY_SEER_OFFERS: &[JobOffer] = &[
    JobOffer {
        id: "courier",
        title: "Courier",
        location: "Mailbox routes",
        behavior: "Carry messages between active rooms and operators.",
        payout: "35 MARK per delivery run",
        wage: 35,
    },
    JobOffer {
        id: "market-crier",
        title: "Market Crier",
        location: "Shop streets",
        behavior: "Announce active shop offers and public proof-of-work needs.",
        payout: "35 MARK per announcement round",
        wage: 35,
    },
    JobOffer {
        id: "newspaper-stringer",
        title: "Newspaper Stringer",
        location: "News desk",
        behavior: "Collect reports from public events and active shops.",
        payout: "45 MARK per filed note",
        wage: 45,
    },
];

const BANK_OFFERS: &[JobOffer] = &[JobOffer {
    id: "bank-clerk",
    title: "Bank Clerk",
    location: "Hinemos Bank",
    behavior: "Explain balances, payments, and pending payment requests.",
    payout: "50 MARK per ledger desk shift",
    wage: 50,
}];

const DEFAULT_JOB_OFFER_PROVIDERS: &[JobOfferProvider] = &[
    JobOfferProvider {
        venue_id: "workers_society",
        label: "Workers Society",
        offers: WORKERS_SOCIETY_OFFERS,
    },
    JobOfferProvider {
        venue_id: "blackstone_izakaya",
        label: "Blackstone Izakaya",
        offers: BLACKSTONE_OFFERS,
    },
    JobOfferProvider {
        venue_id: "hinemos_school",
        label: "Hinemos School",
        offers: SCHOOL_OFFERS,
    },
    JobOfferProvider {
        venue_id: "hinemos_daily_seer",
        label: "Hinemos Daily Seer",
        offers: DAILY_SEER_OFFERS,
    },
    JobOfferProvider {
        venue_id: "hinemos_bank",
        label: "Hinemos Bank",
        offers: BANK_OFFERS,
    },
];

/// Returns the default provider-owned job offer seed data.
pub fn default_job_offer_providers() -> &'static [JobOfferProvider] {
    DEFAULT_JOB_OFFER_PROVIDERS
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
    fn apply_effects(&mut self, effects: Vec<RoomEffect>);
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
