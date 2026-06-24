use anyhow::{Context, Result};
use hinemos_newspaper_room::{NewspaperReply, PressDigest, PressEvent};
use hinemos_storage::{
    PgStorage, StorageError, StoredBalance, StoredInboxItem, StoredMarriageCertificate,
    StoredMemoryEvent,
};
use libhinemos_room::OutgoingMail;
use registry_room::{RegistryAction, RegistryReply};
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

pub(super) async fn save_registry_effect(
    storage: &PgStorage,
    request: &StoredInboxItem,
    reply: RegistryReply,
) -> Result<OutgoingMail> {
    let RegistryReply { mut mail, action } = reply;
    match action {
        RegistryAction::None => {}
        RegistryAction::RegisterMarriage { target } => {
            mail.body = match storage
                .register_marriage(
                    &request.sender_user,
                    &request.sender_player_id,
                    &target,
                    25,
                    "hinemos_registry",
                )
                .await
            {
                Ok(certificate) => format!(
                    "Marriage registered. Each party paid 25 MARK.\n\n{}",
                    certificate.certificate_text
                ),
                Err(error) => registry_error_text(error),
            };
        }
        RegistryAction::ShowCertificate => {
            mail.body = match storage
                .current_marriage_certificate(&request.sender_player_id)
                .await
            {
                Ok(Some(certificate)) => render_certificate_reply(&certificate),
                Ok(None) => "No active marriage certificate on file.".to_owned(),
                Err(error) => registry_error_text(error),
            };
        }
        RegistryAction::Divorce => {
            mail.body = match storage
                .divorce_marriage(&request.sender_user, &request.sender_player_id)
                .await
            {
                Ok(certificate) => format!(
                    "Marriage dissolved.\n\nCertificate #{} for {} and {} is now divorced.",
                    certificate.id, certificate.party_a_user, certificate.party_b_user
                ),
                Err(error) => registry_error_text(error),
            };
        }
    }
    Ok(mail)
}

fn render_certificate_reply(certificate: &StoredMarriageCertificate) -> String {
    format!(
        "{}\nIssued: {}",
        certificate.certificate_text, certificate.issued_at
    )
}

fn registry_error_text(error: StorageError) -> String {
    match error {
        StorageError::SelfMarriage => "You cannot register marriage with yourself.".to_owned(),
        StorageError::MarriagePartnerNotPresent(target) => {
            format!("{target} must be present in H6 within the last 2 minutes before registration.")
        }
        StorageError::MarriageAlreadyActive(target) => {
            format!("An active marriage already exists for {target}.")
        }
        StorageError::NoActiveMarriage(target) => {
            format!("No active marriage certificate is on file for {target}.")
        }
        StorageError::InsufficientFunds => {
            "Registration failed: one party does not have 25 MARK available.".to_owned()
        }
        StorageError::PaymentTargetNotFound(target) => {
            format!("No player or user named {target} was found.")
        }
        other => format!("Registration failed: {other}"),
    }
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
