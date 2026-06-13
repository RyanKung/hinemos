use std::collections::HashMap;

use libhinemos_room::{IncomingMail, OutgoingMail, RoomMailbox};

const ROOM_USER: &str = "room-hinemos_bank";
const ROOM_PLAYER_ID: &str = "room:hinemos_bank";
const MAX_BODY_BYTES: usize = 4096;
const STARTING_CASH: i64 = 100;

#[derive(Debug, Clone)]
struct Loan {
    id: i64,
    principal: i64,
    remaining: i64,
}

#[derive(Debug, Clone)]
struct Account {
    cash: i64,
    deposit: i64,
    loans: Vec<Loan>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            cash: STARTING_CASH,
            deposit: 0,
            loans: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct HinemosBank {
    accounts: HashMap<String, Account>,
    next_loan_id: i64,
}

impl HinemosBank {
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
            "The teller refuses a banking request that large.".to_owned()
        } else {
            self.reply_body(item)
        };
        OutgoingMail {
            recipient_user: item.sender_user.clone(),
            recipient_player_id: item.sender_player_id.clone(),
            sender_user: ROOM_USER.to_owned(),
            sender_player_id: ROOM_PLAYER_ID.to_owned(),
            subject: "Bank reply".to_owned(),
            body,
        }
    }

    pub fn status_text(&self) -> String {
        let mut lines = vec![
            "Room service is external. Bank requests are sent to the room service.".to_owned(),
        ];
        for (user, account) in &self.accounts {
            if account.deposit > 0 || !account.loans.is_empty() {
                lines.push(format!("{user} has a bank account on file."));
            }
        }
        lines.join("\n")
    }

    fn reply_body(&mut self, item: &IncomingMail) -> String {
        let body = item.body.trim();
        if body == "/bank balance" {
            return self.balance_reply(item);
        }
        if let Some(reply) = self.deposit_reply(item, body) {
            return reply;
        }
        if let Some(reply) = self.withdraw_reply(item, body) {
            return reply;
        }
        if let Some(reply) = self.borrow_reply(item, body) {
            return reply;
        }
        if body == "/bank loans" {
            return self.loans_reply(item);
        }
        if let Some(reply) = self.repay_reply(item, body) {
            return reply;
        }

        format!("The teller notes your message: {body}")
    }

    fn balance_reply(&mut self, item: &IncomingMail) -> String {
        let account = self.account_mut(&item.sender_user);
        format!(
            "Cash: {} MARK. Deposit: {} MARK. Loan debt: {} MARK.",
            account.cash,
            account.deposit,
            account.loans.iter().map(|loan| loan.remaining).sum::<i64>()
        )
    }

    fn deposit_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let amount = body.strip_prefix("/bank deposit ")?;
        let Ok(amount) = parse_amount(amount) else {
            return Some("Deposit amount must be a positive whole MARK amount.".to_owned());
        };
        let account = self.account_mut(&item.sender_user);
        if account.cash < amount {
            return Some(format!("You only have {} MARK cash available.", account.cash));
        }
        account.cash -= amount;
        account.deposit += amount;
        Some(format!("Deposited {amount} MARK."))
    }

    fn withdraw_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let amount = body.strip_prefix("/bank withdraw ")?;
        let Ok(amount) = parse_amount(amount) else {
            return Some("Withdrawal amount must be a positive whole MARK amount.".to_owned());
        };
        let account = self.account_mut(&item.sender_user);
        if account.deposit < amount {
            return Some(format!("You only have {} MARK deposited.", account.deposit));
        }
        account.deposit -= amount;
        account.cash += amount;
        Some(format!("Withdrew {amount} MARK."))
    }

    fn borrow_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let amount = body.strip_prefix("/bank borrow ")?;
        let Ok(amount) = parse_amount(amount) else {
            return Some("Borrow amount must be a positive whole MARK amount.".to_owned());
        };
        if amount > 500 {
            return Some("The bank limit is 500 MARK per loan.".to_owned());
        }
        self.next_loan_id += 1;
        let loan_id = self.next_loan_id;
        let account = self.account_mut(&item.sender_user);
        account.cash += amount;
        account.loans.push(Loan {
            id: loan_id,
            principal: amount,
            remaining: amount,
        });
        Some(format!("Loan #{loan_id} opened for {amount} MARK."))
    }

    fn loans_reply(&mut self, item: &IncomingMail) -> String {
        let account = self.account_mut(&item.sender_user);
        if account.loans.is_empty() {
            return "No open loans.".to_owned();
        }
        account
            .loans
            .iter()
            .map(|loan| {
                format!(
                    "#{}: principal {} MARK, remaining {} MARK",
                    loan.id, loan.principal, loan.remaining
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn repay_reply(&mut self, item: &IncomingMail, body: &str) -> Option<String> {
        let rest = body.strip_prefix("/bank repay ")?;
        let mut parts = rest.split_whitespace();
        let Some(loan_id) = parts.next().and_then(|value| value.parse::<i64>().ok()) else {
            return Some("Repay needs a loan id and amount.".to_owned());
        };
        let Some(amount) = parts.next().and_then(|value| parse_amount(value).ok()) else {
            return Some("Repay amount must be a positive whole MARK amount.".to_owned());
        };
        Some(self.apply_repayment(item, loan_id, amount))
    }

    fn apply_repayment(&mut self, item: &IncomingMail, loan_id: i64, amount: i64) -> String {
        let account = self.account_mut(&item.sender_user);
        let Some(index) = account.loans.iter().position(|loan| loan.id == loan_id) else {
            return format!("No open loan #{loan_id}.");
        };
        if account.cash < amount {
            return format!("You only have {} MARK cash available.", account.cash);
        }
        let paid = amount.min(account.loans[index].remaining);
        account.cash -= paid;
        account.loans[index].remaining -= paid;
        if account.loans[index].remaining == 0 {
            account.loans.remove(index);
            return format!("Repaid {paid} MARK. Loan #{loan_id} is closed.");
        }
        format!(
            "Repaid {paid} MARK. Loan #{loan_id} remaining: {} MARK.",
            account.loans[index].remaining
        )
    }

    fn account_mut(&mut self, user: &str) -> &mut Account {
        self.accounts.entry(user.to_owned()).or_default()
    }
}

fn parse_amount(input: &str) -> Result<i64, ()> {
    let amount = input.trim().parse::<i64>().map_err(|_| ())?;
    if amount <= 0 {
        return Err(());
    }
    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libhinemos_room::FakeMailbox;

    fn send_turn(
        mailbox: &mut FakeMailbox,
        service: &mut HinemosBank,
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
    fn balance_opens_account_with_starting_cash() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/bank balance");

        assert!(reply.contains("Cash: 100 MARK"));
        assert!(reply.contains("Deposit: 0 MARK"));
    }

    #[test]
    fn deposit_then_withdraw_updates_balance_across_turns() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let deposit = send_turn(&mut mailbox, &mut service, "alice", "/bank deposit 60");
        let withdraw = send_turn(&mut mailbox, &mut service, "alice", "/bank withdraw 25");
        let balance = send_turn(&mut mailbox, &mut service, "alice", "/bank balance");

        assert!(deposit.contains("Deposited 60"));
        assert!(withdraw.contains("Withdrew 25"));
        assert!(balance.contains("Cash: 65 MARK"));
        assert!(balance.contains("Deposit: 35 MARK"));
    }

    #[test]
    fn cannot_deposit_more_cash_than_available() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/bank deposit 101");

        assert!(reply.contains("only have 100 MARK"));
    }

    #[test]
    fn cannot_withdraw_more_than_deposited() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/bank deposit 20");
        let reply = send_turn(&mut mailbox, &mut service, "alice", "/bank withdraw 30");

        assert!(reply.contains("only have 20 MARK deposited"));
    }

    #[test]
    fn borrow_creates_visible_loan_and_cash() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let opened = send_turn(&mut mailbox, &mut service, "alice", "/bank borrow 120");
        let loans = send_turn(&mut mailbox, &mut service, "alice", "/bank loans");
        let balance = send_turn(&mut mailbox, &mut service, "alice", "/bank balance");

        assert!(opened.contains("Loan #1"));
        assert!(loans.contains("remaining 120 MARK"));
        assert!(balance.contains("Cash: 220 MARK"));
    }

    #[test]
    fn loan_limit_is_enforced() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/bank borrow 501");

        assert!(reply.contains("limit is 500"));
    }

    #[test]
    fn repay_can_partially_and_fully_close_loan() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/bank borrow 100");
        let partial = send_turn(&mut mailbox, &mut service, "alice", "/bank repay 1 40");
        let closed = send_turn(&mut mailbox, &mut service, "alice", "/bank repay 1 60");
        let loans = send_turn(&mut mailbox, &mut service, "alice", "/bank loans");

        assert!(partial.contains("remaining: 60 MARK"));
        assert!(closed.contains("closed"));
        assert!(loans.contains("No open loans"));
    }

    #[test]
    fn repay_unknown_loan_is_clear() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let reply = send_turn(&mut mailbox, &mut service, "alice", "/bank repay 9 10");

        assert!(reply.contains("No open loan #9"));
    }

    #[test]
    fn accounts_are_per_user_not_global() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/bank deposit 80");
        let bob = send_turn(&mut mailbox, &mut service, "bob", "/bank balance");
        let alice = send_turn(&mut mailbox, &mut service, "alice", "/bank balance");

        assert!(bob.contains("Deposit: 0 MARK"));
        assert!(alice.contains("Deposit: 80 MARK"));
    }

    #[test]
    fn invalid_amounts_do_not_mutate_account() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        let bad = send_turn(&mut mailbox, &mut service, "alice", "/bank deposit -1");
        let balance = send_turn(&mut mailbox, &mut service, "alice", "/bank balance");

        assert!(bad.contains("positive whole MARK"));
        assert!(balance.contains("Cash: 100 MARK"));
    }

    #[test]
    fn status_mentions_accounts_with_bank_activity() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        send_turn(&mut mailbox, &mut service, "alice", "/bank deposit 10");
        send_turn(&mut mailbox, &mut service, "bob", "/bank balance");

        assert!(mailbox.status.contains("alice has a bank account"));
        assert!(!mailbox.status.contains("bob has a bank account"));
    }

    #[test]
    fn oversized_request_is_rejected_without_account_activity() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push_owned("alice", format!("/bank deposit {}", "1".repeat(5000)));
        assert_eq!(service.poll_once(&mut mailbox), 1);

        assert!(mailbox.last_reply_to("alice").body.contains("that large"));
        assert!(!mailbox.status.contains("alice has a bank account"));
    }

    #[test]
    fn mailbox_polling_is_required_before_reply_or_ack() {
        let mut service = HinemosBank::new();
        let mut mailbox = FakeMailbox::default();

        mailbox.push("alice", "/bank balance");
        mailbox.assert_no_delivery();

        assert_eq!(service.poll_once(&mut mailbox), 1);
        assert_eq!(mailbox.acked, vec![1]);
        assert!(mailbox.last_reply_to("alice").body.contains("Cash"));
    }
}
