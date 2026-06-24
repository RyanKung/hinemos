#[derive(Debug, Clone, Copy)]
pub(super) struct RoomDefinition {
    pub(super) view_id: &'static str,
    pub(super) room_user: &'static str,
    pub(super) room_player_id: &'static str,
}

pub(super) const BLACKSTONE: RoomDefinition = RoomDefinition {
    view_id: "blackstone_izakaya",
    room_user: "room-blackstone_izakaya",
    room_player_id: "room:blackstone_izakaya",
};

pub(super) const BANK: RoomDefinition = RoomDefinition {
    view_id: "hinemos_bank",
    room_user: "room-hinemos_bank",
    room_player_id: "room:hinemos_bank",
};

pub(super) const NEWSPAPER: RoomDefinition = RoomDefinition {
    view_id: "hinemos_daily_seer",
    room_user: "room-hinemos_daily_seer",
    room_player_id: "room:hinemos_daily_seer",
};

pub(super) const REGISTRY: RoomDefinition = RoomDefinition {
    view_id: "hinemos_registry",
    room_user: "room-hinemos_registry",
    room_player_id: "room:hinemos_registry",
};

pub(super) const SCHOOL: RoomDefinition = RoomDefinition {
    view_id: "hinemos_school",
    room_user: "room-hinemos_school",
    room_player_id: "room:hinemos_school",
};

pub(super) const WORKERS: RoomDefinition = RoomDefinition {
    view_id: "workers_society",
    room_user: "room-workers_society",
    room_player_id: "room:workers_society",
};
