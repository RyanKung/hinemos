use crate::*;

/// Protocol-neutral world/admission configuration.
pub type WorldAppConfig = WorldMetadata;

/// Loads the world-level app metadata from `meta.ron` or `world.toml` when present.
pub(crate) fn load_world_app_config_from_dir(world_dir: &Path) -> Result<WorldAppConfig> {
    let meta_ron = world_dir.join("meta.ron");
    if meta_ron.exists() {
        return load_world_app_config_from_file(&meta_ron, "ron");
    }

    let world_toml = world_dir.join("world.toml");
    if world_toml.exists() {
        return load_world_app_config_from_file(&world_toml, "toml");
    }

    Ok(WorldAppConfig::default())
}

fn load_world_app_config_from_file(path: &Path, format: &str) -> Result<WorldAppConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read world metadata from {}", path.display()))?;
    match format {
        "ron" => ron::from_str(&content)
            .with_context(|| format!("failed to parse world metadata from {}", path.display())),
        "toml" => toml::from_str(&content)
            .with_context(|| format!("failed to parse world metadata from {}", path.display())),
        _ => unreachable!("unsupported world metadata format"),
    }
}
