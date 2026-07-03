use anyhow::{Context, Result, bail};
use hinemos_newspaper_room::{PressDigest, PressEvent};
use hinemos_storage::{
    PgStorage, StorageError, StoredBalance, StoredInboxItem, StoredMarriageCertificate,
    StoredMemoryEvent,
};
use libhinemos_room::{
    CreditReason, DebitReason, MarriageRegistryAction, OutgoingMail, RoomEffect, RoomReply,
};

use super::definitions::{BuiltinHandler, RoomDefinition};

pub(super) async fn apply_room_effects(
    storage: &PgStorage,
    room: &RoomDefinition,
    request: &StoredInboxItem,
    reply: RoomReply,
) -> Result<OutgoingMail> {
    let RoomReply { mut mail, effects } = reply;
    for effect in effects {
        match effect {
            RoomEffect::CreditPlayerMark { amount, reason } => {
                ensure_effect_allowed(room, BuiltinHandler::Workers, "credit MARK")?;
                let balance = credit_player_mark(storage, request, amount, &reason).await?;
                mail.body.push_str(&format!(
                    "\nWallet credited. Balance: {} MARK.",
                    balance.amount
                ));
            }
            RoomEffect::DebitPlayerMark { amount, reason } => {
                ensure_effect_allowed(room, BuiltinHandler::Blackstone, "debit MARK")?;
                match debit_player_mark(storage, request, amount, &reason).await {
                    Ok(balance) => {
                        mail.body.push_str(&format!(
                            "\nWallet debited. Balance: {} MARK.",
                            balance.amount
                        ));
                    }
                    Err(StorageError::InsufficientFunds) => {
                        mail.body = format!(
                            "Bread costs {amount} MARK, but your wallet does not have enough. Earn MARK through in-game work, then buy bread here."
                        );
                        return Ok(mail);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            RoomEffect::RestorePlayerHunger { food } => {
                ensure_effect_allowed(room, BuiltinHandler::Blackstone, "restore hunger")?;
                storage
                    .restore_player_hunger(&request.sender_player_id, &food)
                    .await
                    .with_context(|| {
                        format!("failed to restore hunger for {}", request.sender_player_id)
                    })?;
                mail.body
                    .push_str(&format!("\nHunger restored after eating {food}."));
            }
            RoomEffect::PublishBroadcast { body } => {
                ensure_effect_allowed(room, BuiltinHandler::Newspaper, "publish broadcasts")?;
                save_broadcast(storage, room, &body).await?;
            }
            RoomEffect::MarriageRegistry { action } => {
                ensure_effect_allowed(room, BuiltinHandler::Registry, "use registry actions")?;
                apply_marriage_registry_action(storage, room, request, &mut mail, action).await?;
            }
        }
    }
    Ok(mail)
}

fn ensure_effect_allowed(
    room: &RoomDefinition,
    expected_handler: BuiltinHandler,
    capability: &str,
) -> Result<()> {
    if room.handler == expected_handler {
        return Ok(());
    }
    bail!(
        "built-in room {} with handler {:?} is not allowed to {}",
        room.view_id,
        room.handler,
        capability
    )
}

async fn apply_marriage_registry_action(
    storage: &PgStorage,
    room: &RoomDefinition,
    request: &StoredInboxItem,
    mail: &mut OutgoingMail,
    action: MarriageRegistryAction,
) -> Result<()> {
    match action {
        MarriageRegistryAction::RegisterMarriage { target } => {
            mail.body = match storage
                .register_marriage(
                    &request.sender_user,
                    &request.sender_player_id,
                    &target,
                    25,
                    &room.view_id,
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
        MarriageRegistryAction::ShowCertificate => {
            mail.body = match storage
                .current_marriage_certificate(&request.sender_player_id)
                .await
            {
                Ok(Some(certificate)) => render_certificate_reply(&certificate),
                Ok(None) => "No active marriage certificate on file.".to_owned(),
                Err(error) => registry_error_text(error),
            };
        }
        MarriageRegistryAction::Divorce => {
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
    Ok(())
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

async fn credit_player_mark(
    storage: &PgStorage,
    request: &StoredInboxItem,
    amount: i64,
    reason: &CreditReason,
) -> Result<StoredBalance> {
    let metadata = credit_reason_metadata(reason);
    let idempotency_key = format!("{}:{}", metadata.idempotency_prefix, request.id);
    storage
        .credit_player_mark(
            &request.sender_user,
            &request.sender_player_id,
            amount,
            metadata.ledger_kind,
            &format!("{} for request #{}", metadata.memo_prefix, request.id),
            &idempotency_key,
        )
        .await
        .with_context(|| format!("failed to credit room MARK for request {}", request.id))
}

async fn debit_player_mark(
    storage: &PgStorage,
    request: &StoredInboxItem,
    amount: i64,
    reason: &DebitReason,
) -> Result<StoredBalance, StorageError> {
    let metadata = debit_reason_metadata(reason);
    let idempotency_key = format!("{}:{}", metadata.idempotency_prefix, request.id);
    storage
        .debit_player_mark(
            &request.sender_user,
            &request.sender_player_id,
            amount,
            metadata.ledger_kind,
            &format!("{} for request #{}", metadata.memo_prefix, request.id),
            &idempotency_key,
        )
        .await
}

struct CreditReasonMetadata {
    idempotency_prefix: &'static str,
    ledger_kind: &'static str,
    memo_prefix: &'static str,
}

fn credit_reason_metadata(reason: &CreditReason) -> CreditReasonMetadata {
    match reason {
        CreditReason::WorkerWage => CreditReasonMetadata {
            idempotency_prefix: "workers:wage",
            ledger_kind: "room_wage",
            memo_prefix: "Workers Society wage",
        },
    }
}

struct DebitReasonMetadata {
    idempotency_prefix: &'static str,
    ledger_kind: &'static str,
    memo_prefix: &'static str,
}

fn debit_reason_metadata(reason: &DebitReason) -> DebitReasonMetadata {
    match reason {
        DebitReason::Food => DebitReasonMetadata {
            idempotency_prefix: "blackstone:food",
            ledger_kind: "room_food",
            memo_prefix: "Blackstone Izakaya food",
        },
    }
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

async fn save_broadcast(storage: &PgStorage, room: &RoomDefinition, broadcast: &str) -> Result<()> {
    storage
        .save_broadcast_message(&room.room_user, &room.room_player_id, broadcast)
        .await
        .context("failed to save room broadcast")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_capability_accepts_matching_builtin_handler() {
        let room = room_definition(BuiltinHandler::Workers);

        assert!(ensure_effect_allowed(&room, BuiltinHandler::Workers, "credit MARK").is_ok());
    }

    #[test]
    fn effect_capability_rejects_non_matching_builtin_handler() {
        let room = room_definition(BuiltinHandler::Bank);

        let error = ensure_effect_allowed(&room, BuiltinHandler::Workers, "credit MARK")
            .expect_err("bank room must not be allowed to credit MARK");

        assert!(error.to_string().contains("is not allowed to credit MARK"));
    }

    fn room_definition(handler: BuiltinHandler) -> RoomDefinition {
        RoomDefinition {
            handler,
            view_id: "test_room".to_owned(),
            room_user: "room-test".to_owned(),
            room_player_id: "room:test".to_owned(),
        }
    }
}
