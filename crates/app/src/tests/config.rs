use super::*;

#[test]
fn load_world_app_config_uses_defaults_and_meta_overrides() {
    let temp_root = std::env::temp_dir().join(format!(
        "hinemos-app-world-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).expect("create temp dir");

    let default_config = load_world_app_config_from_dir(&temp_root).expect("default config");
    assert_eq!(default_config, WorldAppConfig::default());

    fs::write(
        temp_root.join("meta.ron"),
        r#"(
admission_view_id: "custom_street",
admission_board_entity_id: "custom_board",
agreement_version: "2030-01-01",
hunger_loop_enabled: true,
builtin_service_rooms_enabled: true,
virtual_day_seconds: 600,
)"#,
    )
    .expect("write meta.ron");

    let loaded = load_world_app_config_from_dir(&temp_root).expect("loaded config");
    assert_eq!(
        loaded,
        WorldAppConfig {
            admission_view_id: "custom_street".to_owned(),
            admission_board_entity_id: "custom_board".to_owned(),
            agreement_version: "2030-01-01".to_owned(),
            hunger_loop_enabled: true,
            builtin_service_rooms_enabled: true,
            virtual_day_seconds: 600,
        }
    );

    let toml_root = std::env::temp_dir().join(format!(
        "hinemos-app-world-config-toml-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&toml_root).expect("create temp dir");
    fs::write(
        toml_root.join("world.toml"),
        r#"
admission_view_id = "toml_street"
admission_board_entity_id = "toml_board"
agreement_version = "2031-02-03"
hunger_loop_enabled = true
builtin_service_rooms_enabled = true
virtual_day_seconds = 900
"#,
    )
    .expect("write world.toml");

    let loaded = load_world_app_config_from_dir(&toml_root).expect("loaded toml config");
    assert_eq!(
        loaded,
        WorldAppConfig {
            admission_view_id: "toml_street".to_owned(),
            admission_board_entity_id: "toml_board".to_owned(),
            agreement_version: "2031-02-03".to_owned(),
            hunger_loop_enabled: true,
            builtin_service_rooms_enabled: true,
            virtual_day_seconds: 900,
        }
    );

    let _ = fs::remove_dir_all(&temp_root);
    let _ = fs::remove_dir_all(&toml_root);
}

#[test]
fn load_world_app_config_prefers_meta_over_world_toml() {
    let temp_root = std::env::temp_dir().join(format!(
        "hinemos-app-world-config-priority-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).expect("create temp dir");
    fs::write(
        temp_root.join("meta.ron"),
        r#"(
admission_view_id: "meta_street",
admission_board_entity_id: "meta_board",
agreement_version: "2040-01-01",
)"#,
    )
    .expect("write meta.ron");
    fs::write(
        temp_root.join("world.toml"),
        r#"
admission_view_id = "toml_street"
admission_board_entity_id = "toml_board"
agreement_version = "2041-02-02"
"#,
    )
    .expect("write world.toml");

    let loaded = load_world_app_config_from_dir(&temp_root).expect("loaded config");
    assert_eq!(
        loaded,
        WorldAppConfig {
            admission_view_id: "meta_street".to_owned(),
            admission_board_entity_id: "meta_board".to_owned(),
            agreement_version: "2040-01-01".to_owned(),
            ..WorldAppConfig::default()
        }
    );

    let _ = fs::remove_dir_all(&temp_root);
}
