use std::time::Duration;

use anyhow::{Context, Result};
use blackstone_izakaya::BlackstoneIzakaya;
use clap::Args;
use hinemos_bank_room::HinemosBank;
use hinemos_school_room::HinemosSchool;
use hinemos_storage::{INBOX_FILTER_OPEN, INBOX_STATUS_ACKED, PgStorage, StoredInboxItem};
use libhinemos_room::{IncomingMail, OutgoingMail};
use workers_society_room::WorkersSociety;

#[derive(Debug, Clone, Args)]
pub(crate) struct RoomsArgs {
    #[arg(long)]
    database_url: Option<String>,

    #[arg(long, default_value_t = 1_000)]
    poll_interval_ms: u64,

    #[arg(long, default_value_t = 20)]
    batch_size: i64,

    #[arg(long)]
    once: bool,
}

pub(crate) async fn run(args: RoomsArgs) -> Result<()> {
    let database_url = args
        .database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .context("DATABASE_URL must be set or passed with --database-url")?;
    let storage = PgStorage::connect(&database_url).await?;
    let mut rooms = BuiltinRooms::default();
    let interval = Duration::from_millis(args.poll_interval_ms);

    println!(
        "Hinemos built-in room service runner started, polling every {} ms.",
        args.poll_interval_ms
    );
    loop {
        let handled = rooms.poll_once(&storage, args.batch_size).await?;
        if args.once {
            println!("Processed {handled} room request(s).");
            return Ok(());
        }
        if handled == 0 {
            tokio::time::sleep(interval).await;
        }
    }
}

#[derive(Debug, Default)]
struct BuiltinRooms {
    blackstone: BlackstoneIzakaya,
    bank: HinemosBank,
    school: HinemosSchool,
    workers: WorkersSociety,
}

impl BuiltinRooms {
    async fn poll_once(&mut self, storage: &PgStorage, batch_size: i64) -> Result<usize> {
        let mut handled = 0;
        handled += poll_room(storage, &BLACKSTONE, &mut self.blackstone, batch_size).await?;
        handled += poll_room(storage, &BANK, &mut self.bank, batch_size).await?;
        handled += poll_room(storage, &SCHOOL, &mut self.school, batch_size).await?;
        handled += poll_room(storage, &WORKERS, &mut self.workers, batch_size).await?;
        Ok(handled)
    }
}

trait BuiltinRoomService {
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

#[derive(Debug, Clone, Copy)]
struct RoomDefinition {
    view_id: &'static str,
    room_user: &'static str,
    room_player_id: &'static str,
}

const BLACKSTONE: RoomDefinition = RoomDefinition {
    view_id: "blackstone_izakaya",
    room_user: "room-blackstone_izakaya",
    room_player_id: "room:blackstone_izakaya",
};

const BANK: RoomDefinition = RoomDefinition {
    view_id: "hinemos_bank",
    room_user: "room-hinemos_bank",
    room_player_id: "room:hinemos_bank",
};

const SCHOOL: RoomDefinition = RoomDefinition {
    view_id: "hinemos_school",
    room_user: "room-hinemos_school",
    room_player_id: "room:hinemos_school",
};

const WORKERS: RoomDefinition = RoomDefinition {
    view_id: "workers_society",
    room_user: "room-workers_society",
    room_player_id: "room:workers_society",
};

async fn poll_room<S>(
    storage: &PgStorage,
    room: &RoomDefinition,
    service: &mut S,
    batch_size: i64,
) -> Result<usize>
where
    S: BuiltinRoomService,
{
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

    let mut handled = 0;
    for item in items {
        handle_room_item(storage, room, service, item).await?;
        handled += 1;
    }
    Ok(handled)
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
    let claimed = storage
        .claim_inbox_item(room.room_user, room.room_player_id, item.id)
        .await
        .with_context(|| format!("failed to claim room inbox item {}", item.id))?;
    let incoming = IncomingMail {
        id: claimed.id,
        sender_user: claimed.sender_user.clone(),
        sender_player_id: claimed.sender_player_id.clone(),
        body: claimed.body.clone(),
    };
    let reply = service.handle_room_mail(&incoming);
    save_room_reply(storage, &claimed, &reply).await?;
    storage
        .finish_inbox_item(
            room.room_user,
            room.room_player_id,
            claimed.id,
            INBOX_STATUS_ACKED,
        )
        .await
        .with_context(|| format!("failed to ack room inbox item {}", claimed.id))?;
    println!(
        "Handled room request #{} for {} from {}.",
        claimed.id, room.view_id, claimed.sender_user
    );
    Ok(())
}

async fn save_room_reply(
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
