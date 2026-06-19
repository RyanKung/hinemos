use anyhow::{Context, Result};
use blackstone_izakaya::BlackstoneIzakaya;
use hinemos_bank_room::HinemosBank;
use hinemos_newspaper_room::HinemosDailySeer;
use hinemos_school_room::HinemosSchool;
use hinemos_storage::{INBOX_FILTER_OPEN, INBOX_STATUS_ACKED, PgStorage, StoredInboxItem};
use libhinemos_room::{IncomingMail, OutgoingMail};
use workers_society_room::WorkersSociety;

use super::definitions::{NEWSPAPER, RoomDefinition, WORKERS};
use super::effects::{
    load_press_digest, save_newspaper_broadcast, save_room_reply, save_worker_payment,
};

pub(super) trait BuiltinRoomService {
    fn handle_room_mail(&mut self, item: &IncomingMail) -> OutgoingMail;
}

impl BuiltinRoomService for BlackstoneIzakaya {
    fn handle_room_mail(&mut self, item: &IncomingMail) -> OutgoingMail {
        self.handle(item)
    }
}

impl BuiltinRoomService for HinemosBank {
    fn handle_room_mail(&mut self, item: &IncomingMail) -> OutgoingMail {
        self.handle(item)
    }
}

impl BuiltinRoomService for HinemosSchool {
    fn handle_room_mail(&mut self, item: &IncomingMail) -> OutgoingMail {
        self.handle(item)
    }
}

impl BuiltinRoomService for WorkersSociety {
    fn handle_room_mail(&mut self, item: &IncomingMail) -> OutgoingMail {
        self.handle(item)
    }
}

pub(super) async fn poll_room<S>(
    storage: &PgStorage,
    room: &RoomDefinition,
    service: &mut S,
    batch_size: i64,
) -> Result<usize>
where
    S: BuiltinRoomService,
{
    let items = list_open_room_items(storage, room, batch_size).await?;
    let mut handled = 0;
    for item in items {
        handle_room_item(storage, room, service, item).await?;
        handled += 1;
    }
    Ok(handled)
}

pub(super) async fn poll_newspaper_room(
    storage: &PgStorage,
    service: &mut HinemosDailySeer,
    batch_size: i64,
) -> Result<usize> {
    let items = list_open_room_items(storage, &NEWSPAPER, batch_size).await?;
    let mut handled = 0;
    for item in items {
        handle_newspaper_item(storage, service, item).await?;
        handled += 1;
    }
    Ok(handled)
}

pub(super) async fn poll_workers_room(
    storage: &PgStorage,
    service: &mut WorkersSociety,
    batch_size: i64,
) -> Result<usize> {
    let items = list_open_room_items(storage, &WORKERS, batch_size).await?;
    let mut handled = 0;
    for item in items {
        handle_worker_item(storage, service, item).await?;
        handled += 1;
    }
    Ok(handled)
}

async fn list_open_room_items(
    storage: &PgStorage,
    room: &RoomDefinition,
    batch_size: i64,
) -> Result<Vec<StoredInboxItem>> {
    let mut items = storage
        .list_inbox_items(
            room.room_user,
            room.room_player_id,
            Some(INBOX_FILTER_OPEN),
            batch_size,
        )
        .await
        .with_context(|| format!("failed to list inbox for {}", room.view_id))?;
    items.reverse();
    Ok(items)
}

async fn handle_room_item<S>(
    storage: &PgStorage,
    room: &RoomDefinition,
    service: &mut S,
    item: StoredInboxItem,
) -> Result<()>
where
    S: BuiltinRoomService,
{
    let claimed = claim_room_item(storage, room, item).await?;
    let reply = service.handle_room_mail(&incoming_mail(&claimed));
    save_room_reply(storage, &claimed, &reply).await?;
    ack_room_item(storage, room, &claimed).await?;
    log_handled_room_request(room, &claimed);
    Ok(())
}

async fn handle_worker_item(
    storage: &PgStorage,
    service: &mut WorkersSociety,
    item: StoredInboxItem,
) -> Result<()> {
    let claimed = claim_room_item(storage, &WORKERS, item).await?;
    let reply = service.handle_with_payment(&incoming_mail(&claimed));
    let reply = save_worker_payment(storage, &claimed, reply).await?;
    save_room_reply(storage, &claimed, &reply).await?;
    ack_room_item(storage, &WORKERS, &claimed).await?;
    log_handled_room_request(&WORKERS, &claimed);
    Ok(())
}

async fn handle_newspaper_item(
    storage: &PgStorage,
    service: &mut HinemosDailySeer,
    item: StoredInboxItem,
) -> Result<()> {
    let claimed = claim_room_item(storage, &NEWSPAPER, item).await?;
    let digest = load_press_digest(storage).await?;
    let reply = service.handle(&incoming_mail(&claimed), &digest);
    save_newspaper_broadcast(storage, &reply).await?;
    save_room_reply(storage, &claimed, &reply.mail).await?;
    ack_room_item(storage, &NEWSPAPER, &claimed).await?;
    log_handled_room_request(&NEWSPAPER, &claimed);
    Ok(())
}

async fn claim_room_item(
    storage: &PgStorage,
    room: &RoomDefinition,
    item: StoredInboxItem,
) -> Result<StoredInboxItem> {
    storage
        .claim_inbox_item(room.room_user, room.room_player_id, item.id)
        .await
        .with_context(|| format!("failed to claim room inbox item {}", item.id))
}

fn incoming_mail(item: &StoredInboxItem) -> IncomingMail {
    IncomingMail {
        id: item.id,
        sender_user: item.sender_user.clone(),
        sender_player_id: item.sender_player_id.clone(),
        body: item.body.clone(),
    }
}

async fn ack_room_item(
    storage: &PgStorage,
    room: &RoomDefinition,
    item: &StoredInboxItem,
) -> Result<()> {
    let _ = storage
        .finish_inbox_item(
            room.room_user,
            room.room_player_id,
            item.id,
            INBOX_STATUS_ACKED,
        )
        .await
        .with_context(|| format!("failed to ack room inbox item {}", item.id))?;
    Ok(())
}

fn log_handled_room_request(room: &RoomDefinition, item: &StoredInboxItem) {
    println!(
        "Handled room request #{} for {} from {}.",
        item.id, room.view_id, item.sender_user
    );
}
