pub(crate) fn room_mail_user(parcel_id: &str) -> String {
    format!("room-{parcel_id}")
}

pub(crate) fn room_mail_player_id(parcel_id: &str) -> String {
    format!("room:{parcel_id}")
}

pub(crate) fn room_command_subject(request_id: i64, view_id: &str) -> String {
    format!("Room command #{request_id} for {view_id}")
}

pub(crate) fn room_reply_request_id(subject: &str) -> Option<i64> {
    let request_id = subject.trim().strip_prefix("Re: #")?.trim();
    request_id.parse::<i64>().ok()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn room_reply_subject(request_id: i64) -> String {
    format!("Re: #{request_id}")
}

#[cfg(test)]
mod tests {
    use super::{
        room_command_subject, room_mail_player_id, room_mail_user, room_reply_request_id,
        room_reply_subject,
    };

    #[test]
    fn room_mail_helpers_keep_request_and_reply_subjects_aligned() {
        let view_id = "north_kiosk";
        let request_subject = room_command_subject(42, view_id);

        assert_eq!(request_subject, "Room command #42 for north_kiosk");
        assert_eq!(room_reply_request_id("Re: #42"), Some(42));
        assert_eq!(room_reply_request_id("Room reply"), None);
        assert_eq!(room_reply_subject(42), "Re: #42");
        assert_eq!(room_mail_user("N1"), "room-N1");
        assert_eq!(room_mail_player_id("N1"), "room:N1");
    }
}
