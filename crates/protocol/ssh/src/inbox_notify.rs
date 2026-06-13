use std::sync::Arc;

use anyhow::Result;
use sqlx::postgres::PgListener;

use super::{SharedState, render};

pub(super) async fn run_inbox_mail_notify_listener(
    database_url: String,
    shared: Arc<SharedState>,
) -> Result<()> {
    let mut listener = PgListener::connect(&database_url).await?;
    listener.listen("hinemos_inbox_mail").await?;
    loop {
        let notification = listener.recv().await?;
        let Ok(item_id) = notification.payload().parse::<i64>() else {
            continue;
        };
        let item = shared.inbox_item(item_id).await?;
        let sender_rooms = shared.service_rooms_by_room_user(&item.sender_user).await?;
        let (recipients, room_replies) = {
            let presence = shared.presence.lock().await;
            let recipients = presence.direct_recipients(u64::MAX, &item.recipient_player_id);
            let mut room_replies = Vec::new();
            let room_reply_id = render::room_reply_request_id(&item.subject);
            for room in sender_rooms {
                let room_recipients = presence.direct_recipients_in_view(
                    u64::MAX,
                    &item.recipient_player_id,
                    &room.view_id,
                );
                if !room_recipients.is_empty() {
                    let label = room.label.unwrap_or(room.view_id);
                    room_replies.push((
                        room_recipients,
                        render::room_reply_live_notice(&label, room_reply_id, &item.body),
                    ));
                }
            }
            (recipients, room_replies)
        };
        if !recipients.is_empty() {
            render::deliver_live_inbox_notice(recipients, &item, shared.mail_domain.as_deref())
                .await;
        }
        for (room_recipients, message) in room_replies {
            render::deliver_live_message(room_recipients, &message).await;
        }
    }
}
