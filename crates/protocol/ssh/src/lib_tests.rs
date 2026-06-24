use super::{CacheEntry, InboxItemCache, RoomDirectoryCache, RoomViewContext, ViewPresenceCache};
use anyhow::Result;
use hinemos_app::{AppService, ParcelStore, ServiceRoomRegistration};
use hinemos_storage::{StorageError, StoredInboxItem, StoredParcel, StoredServiceRoom};
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct TestParcelStore {
    parcels: Vec<StoredParcel>,
}

impl ParcelStore for TestParcelStore {
    type Error = StorageError;
    type Parcel = StoredParcel;

    async fn list_commercial_parcels(&self) -> Result<Vec<Self::Parcel>, Self::Error> {
        Ok(self.parcels.clone())
    }

    async fn commercial_parcels_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::Parcel>, Self::Error> {
        Ok(self
            .parcels
            .iter()
            .filter(|parcel| parcel.front_view_id == front_view_id)
            .cloned()
            .collect::<Vec<_>>())
    }
}

fn entry<T>(value: T) -> CacheEntry<T> {
    CacheEntry {
        loaded_at: Instant::now(),
        value,
    }
}

fn stale_entry<T>(value: T) -> CacheEntry<T> {
    CacheEntry {
        loaded_at: Instant::now() - Duration::from_secs(31),
        value,
    }
}

fn service_room(
    view_id: &str,
    front_view_id: &str,
    address: &str,
    label: &str,
    room_user: &str,
) -> StoredServiceRoom {
    StoredServiceRoom {
        view_id: view_id.to_owned(),
        front_view_id: Some(front_view_id.to_owned()),
        front_entity_id: None,
        address: Some(address.to_owned()),
        label: Some(label.to_owned()),
        enter_aliases: Some(label.to_ascii_lowercase()),
        room_user: room_user.to_owned(),
        room_player_id: "room-player".to_owned(),
        status_text: None,
        custom_commands: None,
        builtin_handler: None,
        enabled: true,
    }
}

fn parcel(parcel_id: &str, view_id: &str, front_view_id: &str) -> StoredParcel {
    StoredParcel {
        parcel_id: parcel_id.to_owned(),
        view_id: view_id.to_owned(),
        front_view_id: front_view_id.to_owned(),
        district: "north".to_owned(),
        position: 1,
        owner_user: None,
        owner_player_id: None,
        room_user: None,
        room_player_id: None,
        status: "vacant".to_owned(),
        title: None,
        description: None,
        style: None,
        operator_prompt: None,
        custom_commands: None,
    }
}

#[test]
fn room_directory_cache_clears_entries() {
    let mut cache = RoomDirectoryCache::default();
    cache.service_room_views.insert(
        "view".to_owned(),
        entry(Some(service_room(
            "room",
            "street",
            "R1",
            "Room",
            "room-user",
        ))),
    );
    cache.service_room_any_views.insert(
        "view".to_owned(),
        entry(Some(service_room(
            "room",
            "street",
            "R1",
            "Room",
            "room-user",
        ))),
    );
    cache.commercial_parcels_front_views.insert(
        "street".to_owned(),
        entry(vec![parcel("parcel", "parcel_view", "street")]),
    );
    cache.service_room_users.insert(
        "room-user".to_owned(),
        entry(vec![service_room(
            "room",
            "street",
            "R1",
            "Room",
            "room-user",
        )]),
    );
    cache.service_rooms_front_views.insert(
        "street".to_owned(),
        entry(vec![service_room(
            "room",
            "street",
            "R1",
            "Room",
            "room-user",
        )]),
    );
    cache.room_context_views.insert(
        "street".to_owned(),
        entry(RoomViewContext {
            room_binding: None,
            service_room: None,
            front_bindings: Vec::new(),
        }),
    );
    cache.clear();

    assert!(cache.service_room_views.is_empty());
    assert!(cache.service_room_any_views.is_empty());
    assert!(cache.commercial_parcels_front_views.is_empty());
    assert!(cache.service_room_users.is_empty());
    assert!(cache.service_rooms_front_views.is_empty());
    assert!(cache.room_context_views.is_empty());
}

#[test]
fn room_directory_cache_invalidates_service_room_scopes() {
    let mut cache = RoomDirectoryCache::default();
    cache.service_room_views.insert(
        "room-view".to_owned(),
        entry(Some(service_room(
            "room-view",
            "street-a",
            "ROOM",
            "Room",
            "room-user",
        ))),
    );
    cache.service_room_any_views.insert(
        "room-view".to_owned(),
        entry(Some(service_room(
            "room-view",
            "street-a",
            "ROOM",
            "Room",
            "room-user",
        ))),
    );
    cache.service_room_users.insert(
        "room-user".to_owned(),
        entry(vec![service_room(
            "room-view",
            "street-a",
            "ROOM",
            "Room",
            "room-user",
        )]),
    );
    cache.service_rooms_front_views.insert(
        "street-a".to_owned(),
        entry(vec![service_room(
            "room-view",
            "street-a",
            "ROOM",
            "Room",
            "room-user",
        )]),
    );
    cache.room_binding_views.insert(
        "room-view".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: None,
        },
    );
    cache.room_binding_front_views.insert(
        "street-a".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: Vec::new(),
        },
    );
    cache.commercial_parcels_front_views.insert(
        "street-a".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: Vec::new(),
        },
    );
    cache.invalidate_for_service_room(
        "room-view",
        Some("old-room-user"),
        Some("street-a"),
        Some("room-user"),
        Some("street-b"),
    );

    assert!(!cache.service_room_views.contains_key("room-view"));
    assert!(!cache.service_room_any_views.contains_key("room-view"));
    assert!(!cache.service_room_users.contains_key("old-room-user"));
    assert!(!cache.service_room_users.contains_key("room-user"));
    assert!(!cache.service_rooms_front_views.contains_key("street-a"));
    assert!(!cache.room_binding_views.contains_key("room-view"));
    assert!(!cache.room_binding_front_views.contains_key("street-a"));
    assert!(
        !cache
            .commercial_parcels_front_views
            .contains_key("street-a")
    );
}

#[test]
fn room_directory_cache_invalidates_commercial_parcel_scopes() {
    let mut cache = RoomDirectoryCache::default();
    cache.room_binding_views.insert(
        "parcel-view".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: None,
        },
    );
    cache.room_binding_front_views.insert(
        "street-a".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: Vec::new(),
        },
    );
    cache.commercial_parcels_front_views.insert(
        "street-a".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: vec![StoredParcel {
                parcel_id: "parcel".to_owned(),
                view_id: "parcel-view".to_owned(),
                front_view_id: "street-a".to_owned(),
                district: "north".to_owned(),
                position: 1,
                owner_user: None,
                owner_player_id: None,
                room_user: None,
                room_player_id: None,
                status: "vacant".to_owned(),
                title: None,
                description: None,
                style: None,
                operator_prompt: None,
                custom_commands: None,
            }],
        },
    );
    cache.room_context_views.insert(
        "parcel-view".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: RoomViewContext {
                room_binding: None,
                service_room: None,
                front_bindings: Vec::new(),
            },
        },
    );
    cache.room_context_views.insert(
        "street-a".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: RoomViewContext {
                room_binding: None,
                service_room: None,
                front_bindings: Vec::new(),
            },
        },
    );
    cache.service_room_views.insert(
        "room-view".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: Some(StoredServiceRoom {
                view_id: "room-view".to_owned(),
                front_view_id: Some("street-a".to_owned()),
                front_entity_id: None,
                address: Some("ROOM".to_owned()),
                label: Some("Room".to_owned()),
                enter_aliases: Some("room".to_owned()),
                room_user: "room-user".to_owned(),
                room_player_id: "room-player".to_owned(),
                status_text: None,
                custom_commands: None,
                builtin_handler: None,
                enabled: true,
            }),
        },
    );

    cache.invalidate_for_commercial_parcel("parcel-view", "street-a");

    assert!(!cache.room_binding_views.contains_key("parcel-view"));
    assert!(!cache.room_binding_front_views.contains_key("street-a"));
    assert!(
        !cache
            .commercial_parcels_front_views
            .contains_key("street-a")
    );
    assert!(cache.service_room_views.contains_key("room-view"));
    assert!(!cache.room_context_views.contains_key("parcel-view"));
    assert!(!cache.room_context_views.contains_key("street-a"));
}

#[test]
fn room_directory_cache_caches_service_rooms_by_room_user() {
    let mut cache = RoomDirectoryCache::default();
    cache.service_room_users.insert(
        "room-user".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: vec![StoredServiceRoom {
                view_id: "room-view".to_owned(),
                front_view_id: Some("street-a".to_owned()),
                front_entity_id: None,
                address: Some("ROOM".to_owned()),
                label: Some("Room".to_owned()),
                enter_aliases: Some("room".to_owned()),
                room_user: "room-user".to_owned(),
                room_player_id: "room-player".to_owned(),
                status_text: None,
                custom_commands: None,
                builtin_handler: None,
                enabled: true,
            }],
        },
    );

    let cached = RoomDirectoryCache::get(cache.service_room_users.get("room-user"));

    assert!(cached.is_some());
}

#[test]
fn room_directory_cache_caches_room_contexts_by_view() {
    let mut cache = RoomDirectoryCache::default();
    cache.room_context_views.insert(
        "street-a".to_owned(),
        CacheEntry {
            loaded_at: Instant::now(),
            value: RoomViewContext {
                room_binding: None,
                service_room: Some(StoredServiceRoom {
                    view_id: "room-view".to_owned(),
                    front_view_id: Some("street-a".to_owned()),
                    front_entity_id: None,
                    address: Some("ROOM".to_owned()),
                    label: Some("Room".to_owned()),
                    enter_aliases: Some("room".to_owned()),
                    room_user: "room-user".to_owned(),
                    room_player_id: "room-player".to_owned(),
                    status_text: None,
                    custom_commands: None,
                    builtin_handler: None,
                    enabled: true,
                }),
                front_bindings: Vec::new(),
            },
        },
    );

    let cached = RoomDirectoryCache::get(cache.room_context_views.get("street-a"));

    assert!(cached.is_some());
}

#[test]
fn inbox_item_cache_clears_entries() {
    let mut cache = InboxItemCache::default();
    cache.insert(
        7,
        StoredInboxItem {
            id: 7,
            kind: "mail".to_owned(),
            recipient_user: "alice".to_owned(),
            recipient_player_id: "player-1".to_owned(),
            sender_user: "bob".to_owned(),
            sender_player_id: "player-2".to_owned(),
            subject: "hello".to_owned(),
            body: "body".to_owned(),
            source_kind: None,
            source_id: None,
            payload: serde_json::json!({}),
            status: "open".to_owned(),
            attempts: 0,
            lease_until: None,
            created_at: "2026-06-12 00:00:00 UTC".to_owned(),
        },
    );

    cache.clear();

    assert!(cache.items.is_empty());
}

#[test]
fn room_directory_cache_expires_stale_entries() {
    let stale = stale_entry(Some(service_room(
        "room",
        "street",
        "R1",
        "Room",
        "room-user",
    )));
    assert_eq!(RoomDirectoryCache::get(Some(&stale)), None);

    let stale_front_view = stale_entry(vec![service_room(
        "room-list",
        "street",
        "R2",
        "Room List",
        "room-list-user",
    )]);
    assert_eq!(RoomDirectoryCache::get(Some(&stale_front_view)), None);

    let stale_room_user = stale_entry(vec![service_room(
        "room-user-list",
        "street",
        "R3",
        "Room User List",
        "room-user",
    )]);
    assert_eq!(RoomDirectoryCache::get(Some(&stale_room_user)), None);

    let fresh = entry(Some(service_room(
        "room",
        "street",
        "R1",
        "Room",
        "room-user",
    )));
    assert!(RoomDirectoryCache::get(Some(&fresh)).is_some());

    let fresh_front_view = entry(vec![service_room(
        "room-list",
        "street",
        "R2",
        "Room List",
        "room-list-user",
    )]);
    assert!(RoomDirectoryCache::get(Some(&fresh_front_view)).is_some());

    let fresh_room_user = entry(vec![service_room(
        "room-user-list",
        "street",
        "R3",
        "Room User List",
        "room-user",
    )]);
    assert!(RoomDirectoryCache::get(Some(&fresh_room_user)).is_some());
}

#[test]
fn view_presence_cache_throttles_same_view_for_five_seconds() {
    let mut cache = ViewPresenceCache::default();
    assert!(cache.should_record("player-1", "street-a"));
    assert!(!cache.should_record("player-1", "street-a"));
    assert!(cache.should_record("player-1", "street-b"));

    cache.items.insert(
        "player-2".to_owned(),
        CacheEntry {
            loaded_at: Instant::now() - Duration::from_secs(6),
            value: "street-c".to_owned(),
        },
    );
    assert!(cache.should_record("player-2", "street-c"));
}

#[test]
fn service_room_enter_tokens_normalize_and_dedupe_aliases() {
    let registration = ServiceRoomRegistration {
        view_id: "room_view".to_owned(),
        front_view_id: Some("arrival_street".to_owned()),
        front_entity_id: None,
        address: Some(" N1 ".to_owned()),
        label: Some("Reload Test Room".to_owned()),
        enter_aliases: Some("alias-one,alias-one;n1".to_owned()),
        room_user: "room-user".to_owned(),
        room_player_id: "room-player".to_owned(),
        status_text: None,
        custom_commands: None,
        builtin_handler: None,
        enabled: true,
    };

    let mut tokens = AppService::<()>::service_room_enter_tokens(&registration);
    tokens.sort();

    assert_eq!(
        tokens,
        vec![
            "alias-one".to_owned(),
            "n1".to_owned(),
            "reload test room".to_owned(),
        ]
    );
}

#[test]
fn service_room_alias_conflict_rejects_same_front_view_tokens() {
    let store = TestParcelStore {
        parcels: Vec::new(),
    };
    let registration = ServiceRoomRegistration {
        view_id: "room-1".to_owned(),
        front_view_id: Some("arrival_street".to_owned()),
        front_entity_id: None,
        address: Some("North Desk".to_owned()),
        label: Some("North Desk".to_owned()),
        enter_aliases: Some("desk".to_owned()),
        room_user: "room-user-1".to_owned(),
        room_player_id: "room-player-1".to_owned(),
        status_text: None,
        custom_commands: None,
        builtin_handler: None,
        enabled: true,
    };
    let mut claimed_aliases = HashMap::new();
    claimed_aliases.insert(
        ("arrival_street".to_owned(), "north desk".to_owned()),
        "existing-room".to_owned(),
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let reason = AppService::<TestParcelStore>::service_room_alias_conflict(
            &store,
            &registration,
            &mut claimed_aliases,
        )
        .await
        .expect("alias conflict check");
        assert_eq!(
            reason.as_deref(),
            Some("`north desk` conflicts with service room existing-room in arrival_street")
        );
    });
}

#[test]
fn service_room_alias_conflict_rejects_address_conflict() {
    let store = TestParcelStore {
        parcels: Vec::new(),
    };
    let registration = ServiceRoomRegistration {
        view_id: "room-1".to_owned(),
        front_view_id: Some("arrival_street".to_owned()),
        front_entity_id: None,
        address: Some("North Desk".to_owned()),
        label: Some("Visitor Desk".to_owned()),
        enter_aliases: Some("desk".to_owned()),
        room_user: "room-user-1".to_owned(),
        room_player_id: "room-player-1".to_owned(),
        status_text: None,
        custom_commands: None,
        builtin_handler: None,
        enabled: true,
    };
    let mut claimed_aliases = HashMap::new();
    claimed_aliases.insert(
        ("arrival_street".to_owned(), "north desk".to_owned()),
        "existing-room".to_owned(),
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let reason = AppService::<TestParcelStore>::service_room_alias_conflict(
            &store,
            &registration,
            &mut claimed_aliases,
        )
        .await
        .expect("alias conflict check");
        assert_eq!(
            reason.as_deref(),
            Some("`north desk` conflicts with service room existing-room in arrival_street")
        );
    });
}

#[test]
fn service_room_alias_conflict_rejects_parcel_id() {
    let store = TestParcelStore {
        parcels: vec![StoredParcel {
            parcel_id: "north kiosk".to_owned(),
            view_id: "parcel_view".to_owned(),
            front_view_id: "arrival_street".to_owned(),
            district: "north".to_owned(),
            position: 1,
            owner_user: None,
            owner_player_id: None,
            room_user: None,
            room_player_id: None,
            status: "vacant".to_owned(),
            title: Some("North Desk".to_owned()),
            description: None,
            style: None,
            operator_prompt: None,
            custom_commands: None,
        }],
    };
    let registration = ServiceRoomRegistration {
        view_id: "room-2".to_owned(),
        front_view_id: Some("arrival_street".to_owned()),
        front_entity_id: None,
        address: Some("north kiosk".to_owned()),
        label: None,
        enter_aliases: None,
        room_user: "room-user-2".to_owned(),
        room_player_id: "room-player-2".to_owned(),
        status_text: None,
        custom_commands: None,
        builtin_handler: None,
        enabled: true,
    };
    let mut claimed_aliases = HashMap::new();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let reason = AppService::<TestParcelStore>::service_room_alias_conflict(
            &store,
            &registration,
            &mut claimed_aliases,
        )
        .await
        .expect("alias conflict check");
        assert_eq!(
            reason.as_deref(),
            Some("`north kiosk` conflicts with parcel north kiosk in arrival_street")
        );
    });
}
