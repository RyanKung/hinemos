use std::path::Path;

use hinemos_core::sample_world::{LOCAL_PLAYER_ID, load_world_from_dir};
use hinemos_core::{Direction, Exit, SemanticCommand};

use crate::{GameRuntime, RuntimeError};

fn sample_runtime() -> GameRuntime {
    let world_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worlds/sample");
    GameRuntime::new(load_world_from_dir(world_dir).expect("sample world should load"))
        .expect("sample runtime should build")
}

#[test]
fn moving_updates_current_view() {
    let runtime = sample_runtime();
    let observation = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::West,
            },
        )
        .expect("move should succeed");

    assert_eq!(observation.view_id, "grid_road_xm1_y0");
    assert_eq!(observation.title, "West 1 Rd.");
}

#[test]
fn generated_grid_roads_extend_without_static_views() {
    let runtime = sample_runtime();
    let first = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::East,
            },
        )
        .expect("first grid move should succeed");
    let second = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::East,
            },
        )
        .expect("second grid move should succeed");
    let _back_to_first = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::West,
            },
        )
        .expect("west within grid should succeed");
    let harbor = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::West,
            },
        )
        .expect("west from East 1 should return to Harbor Square");

    assert_eq!(first.view_id, "grid_road_xp1_y0");
    assert_eq!(first.title, "East 1 Rd.");
    assert_eq!(second.view_id, "grid_road_xp2_y0");
    assert_eq!(second.title, "East 2 Rd.");
    assert!(second.description.contains("no fixed edge"));
    assert_eq!(harbor.view_id, "arrival_street");
}

#[test]
fn generated_grid_uses_configured_origin_anchor() {
    let mut world = sample_runtime().world().expect("world snapshot");
    let mut custom_arrival = world
        .views
        .get("arrival_street")
        .expect("sample arrival view")
        .clone();
    custom_arrival.id = "custom_arrival".to_owned();
    custom_arrival.title = "Custom Arrival".to_owned();
    world
        .views
        .insert(custom_arrival.id.clone(), custom_arrival);
    world
        .players
        .get_mut(LOCAL_PLAYER_ID)
        .expect("local player")
        .current_view = "grid_road_xp1_y0".to_owned();
    let runtime = GameRuntime::new_with_grid_origin(world, "custom_arrival")
        .expect("custom origin should build");

    let road = runtime
        .observe_view_json(LOCAL_PLAYER_ID, "grid_road_xp1_y0", Vec::new())
        .expect("grid observation");
    let west = road
        .exits
        .iter()
        .find(|exit| exit.direction == Direction::West)
        .expect("west exit");
    let observation = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::West,
            },
        )
        .expect("move into configured origin");

    assert_eq!(west.label.as_deref(), Some("Custom Arrival"));
    assert_eq!(observation.view_id, "custom_arrival");
    assert_eq!(observation.title, "Custom Arrival");
    assert_eq!(
        observation
            .exits
            .iter()
            .filter_map(|exit| exit.label.as_deref())
            .collect::<Vec<_>>(),
        vec!["North 1 Rd.", "South 1 Rd.", "West 1 Rd.", "East 1 Rd."]
    );
}

#[test]
fn runtime_rejects_missing_generated_grid_origin() {
    let world = sample_runtime().world().expect("world snapshot");

    let err = GameRuntime::new_with_grid_origin(world, "missing_origin")
        .expect_err("missing origin should fail");

    assert_eq!(err, RuntimeError::ViewNotFound("missing_origin".to_owned()));
}

#[test]
fn origin_map_is_generated_instead_of_read_from_world_ascii() {
    let mut world = sample_runtime().world().expect("world snapshot");
    let arrival = world.views.get_mut("arrival_street").expect("arrival view");
    arrival.ascii_art = vec!["STALE DATA MAP".to_owned()];
    arrival.exits = vec![Exit {
        direction: Direction::West,
        target: "legacy_wilderness".to_owned(),
        label: Some("wilderness".to_owned()),
        requirements: Vec::new(),
    }];
    let runtime = GameRuntime::new(world).expect("runtime should build");

    let observation = runtime
        .observe_json(LOCAL_PLAYER_ID, Vec::new())
        .expect("origin observation");
    let rendered = observation.ascii_art.join("\n");
    let exit_labels = observation
        .exits
        .iter()
        .filter_map(|exit| exit.label.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(observation.view_id, "arrival_street");
    assert!(!rendered.contains("STALE DATA MAP"));
    assert!(rendered.contains("+----+----+----+----+"));
    assert!(rendered.contains("North 1 Rd."));
    assert!(!rendered.contains("[C0-N1-01]"));
    assert!(rendered.contains("objects: bulletin board"));
    assert_eq!(
        exit_labels,
        vec!["North 1 Rd.", "South 1 Rd.", "West 1 Rd.", "East 1 Rd."]
    );
    assert!(
        !observation
            .exits
            .iter()
            .any(|exit| exit.label.as_deref() == Some("wilderness"))
    );
}

#[test]
fn generated_town_map_does_not_require_authored_ascii() {
    let mut world = sample_runtime().world().expect("world snapshot");
    world
        .views
        .get_mut("arrival_street")
        .expect("arrival view")
        .ascii_art = Vec::new();
    let runtime = GameRuntime::new(world).expect("runtime should build");

    let origin = runtime
        .observe_json(LOCAL_PLAYER_ID, Vec::new())
        .expect("origin observation");
    let road = runtime
        .execute(
            LOCAL_PLAYER_ID,
            &SemanticCommand::Move {
                direction: Direction::East,
            },
        )
        .expect("move to generated road");
    let parcel = runtime
        .observe_view_json(LOCAL_PLAYER_ID, "parcel_E1-C0-01", Vec::new())
        .expect("generated parcel observation");

    assert!(origin.ascii_art.join("\n").contains("Harbor Square"));
    assert!(road.ascii_art.join("\n").contains("East 1 Rd."));
    assert!(road.ascii_art.join("\n").contains("[E1-C0-01]"));
    assert!(parcel.ascii_art.join("\n").contains("[E1-C0-01]"));
}
