use super::*;

#[test]
fn load_service_room_registrations_disables_same_front_view_conflicts() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    rt.block_on(async {
        let temp_root = write_registration_fixture();
        let world = load_sample_world();
        let store = registration_store_with_arrival_parcel();

        AppService::<TestRegistrationStore>::load_service_room_registrations(
            &store,
            &temp_root,
            &world,
            None::<&()>,
        )
        .await
        .expect("load registrations");

        assert_disabled_view_ids(&store);
        assert_upsert_calls(&store);
        let _ = fs::remove_dir_all(&temp_root);
    });
}

fn write_registration_fixture() -> std::path::PathBuf {
    let temp_root = std::env::temp_dir().join(format!(
        "hinemos-app-room-reg-load-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).expect("create temp dir");
    fs::write(temp_root.join("rooms.ron"), ROOM_REGISTRATIONS_FIXTURE).expect("write rooms.ron");
    temp_root
}

fn load_sample_world() -> WorldState {
    let world_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worlds/sample");
    hinemos_core::sample_world::load_world_from_dir(&world_dir).expect("load sample world")
}

fn registration_store_with_arrival_parcel() -> TestRegistrationStore {
    let mut store = TestRegistrationStore::default();
    store.parcels_by_front_view.insert(
        "arrival_street".to_owned(),
        vec![RegistrationParcel {
            parcel_id: "P1",
            front_view_id: "arrival_street",
            title: None,
        }],
    );
    store
}

fn assert_disabled_view_ids(store: &TestRegistrationStore) {
    let disable_calls = store.disable_calls.lock().unwrap().clone();
    assert_eq!(disable_calls.len(), 1);
    let mut disabled_view_ids = disable_calls.into_iter().next().unwrap();
    disabled_view_ids.sort();
    assert_eq!(
        disabled_view_ids,
        vec![
            "room_alias_conflict".to_owned(),
            "room_disabled".to_owned(),
            "room_missing_front_view".to_owned(),
            "room_ok".to_owned(),
            "room_parcel_conflict".to_owned(),
        ]
    );
}

fn assert_upsert_calls(store: &TestRegistrationStore) {
    let upsert_calls = store.upsert_calls.lock().unwrap().clone();
    assert_eq!(
        upsert_calls,
        vec![
            ("room_ok".to_owned(), true),
            ("room_alias_conflict".to_owned(), false),
            ("room_parcel_conflict".to_owned(), false),
            ("room_missing_front_view".to_owned(), false),
            ("room_disabled".to_owned(), false),
        ]
    );
}

const ROOM_REGISTRATIONS_FIXTURE: &str = r#"[
(
    view_id: "room_ok",
    front_view_id: Some("arrival_street"),
    front_entity_id: None,
    address: Some("SAFE"),
    label: Some("Safe Room"),
    enter_aliases: Some("safe room"),
    room_user: "room-ok-user",
    room_player_id: "room-ok-player",
    status_text: Some("Safe room."),
    custom_commands: Some("/room ask <question>"),
    enabled: true,
),
(
    view_id: "room_alias_conflict",
    front_view_id: Some("arrival_street"),
    front_entity_id: None,
    address: Some("SAFE"),
    label: Some("Alias Conflict Room"),
    enter_aliases: Some("alias conflict"),
    room_user: "room-alias-user",
    room_player_id: "room-alias-player",
    status_text: Some("Alias conflict room."),
    custom_commands: Some("/room status"),
    enabled: true,
),
(
    view_id: "room_parcel_conflict",
    front_view_id: Some("arrival_street"),
    front_entity_id: None,
    address: Some("P1"),
    label: Some("Parcel Conflict Room"),
    enter_aliases: Some("parcel conflict"),
    room_user: "room-parcel-user",
    room_player_id: "room-parcel-player",
    status_text: Some("Parcel conflict room."),
    custom_commands: Some("/room info"),
    enabled: true,
),
(
    view_id: "room_missing_front_view",
    front_view_id: None,
    front_entity_id: None,
    address: Some("MISSING"),
    label: Some("Missing Front View Room"),
    enter_aliases: Some("missing front view"),
    room_user: "room-missing-user",
    room_player_id: "room-missing-player",
    status_text: Some("Missing front view room."),
    custom_commands: Some("/room missing"),
    enabled: true,
),
(
    view_id: "room_disabled",
    front_view_id: Some("arrival_street"),
    front_entity_id: None,
    address: Some("DISABLED"),
    label: Some("Disabled Room"),
    enter_aliases: Some("disabled room"),
    room_user: "room-disabled-user",
    room_player_id: "room-disabled-player",
    status_text: Some("Disabled room."),
    custom_commands: Some("/room disabled"),
    enabled: false,
),
]"#;
