use anyhow::{Context, Result};
use hinemos_newspaper_room::{NewspaperReply, PressDigest, PressEvent};
use hinemos_storage::{PgStorage, StoredBalance, StoredInboxItem, StoredMemoryEvent};
use libhinemos_room::OutgoingMail;
use workers_society_room::{WagePayment, WorkersReply};

use super::definitions::NEWSPAPER;

pub(super) async fn save_worker_payment(
    storage: &PgStorage,
    request: &StoredInboxItem,
    reply: WorkersReply,
) -> Result<OutgoingMail> {
    let WorkersReply {
        mut mail,
        wage_payment,
    } = reply;
    if let Some(payment) = wage_payment {
        let balance = credit_worker_wage(storage, request, &payment).await?;
        mail.body.push_str(&format!(
            "\nWallet credited. Balance: {} MARK.",
            balance.amount
        ));
    }
    Ok(mail)
}

async fn credit_worker_wage(
    storage: &PgStorage,
    request: &StoredInboxItem,
    payment: &WagePayment,
) -> Result<StoredBalance> {
    let idempotency_key = format!("workers:wage:{}", request.id);
    storage
        .credit_player_mark(
            &payment.recipient_user,
            &payment.recipient_player_id,
            payment.amount,
            "room_wage",
            &format!("Workers Society wage for request #{}", request.id),
            &idempotency_key,
        )
        .await
        .with_context(|| format!("failed to credit worker wage for request {}", request.id))
}

pub(super) async fn load_press_digest(storage: &PgStorage) -> Result<PressDigest> {
    let events = storage
        .recent_public_press_events(16)
        .await
        .context("failed to load public press events")?;
    let issue_date = events
        .first()
        .map(press_event_date)
        .unwrap_or_else(|| "today".to_owned());
    Ok(PressDigest {
        issue_date,
        events: events.into_iter().map(press_event_from_storage).collect(),
    })
}

fn press_event_date(event: &StoredMemoryEvent) -> String {
    event.occurred_at.chars().take(10).collect()
}

fn press_event_from_storage(event: StoredMemoryEvent) -> PressEvent {
    PressEvent {
        occurred_at: event.occurred_at,
        source: event.source,
        event_type: event.event_type,
        content: event.content,
    }
}

pub(super) async fn save_newspaper_broadcast(
    storage: &PgStorage,
    reply: &NewspaperReply,
) -> Result<()> {
    if let Some(broadcast) = &reply.broadcast {
        storage
            .save_broadcast_message(NEWSPAPER.room_user, NEWSPAPER.room_player_id, broadcast)
            .await
            .context("failed to save newspaper broadcast")?;
    }
    Ok(())
}

pub(super) async fn save_room_reply(
    storage: &PgStorage,
    request: &StoredInboxItem,
    reply: &OutgoingMail,
) -> Result<()> {
    storage
        .save_mail_message_to_principal(
            &reply.sender_user,
            &reply.sender_player_id,
            &reply.recipient_user,
            &reply.recipient_player_id,
            &format!("Re: #{}", request.source_id.unwrap_or(request.id)),
            &reply.body,
        )
        .await
        .with_context(|| format!("failed to save room reply for request {}", request.id))?;
    Ok(())
}
