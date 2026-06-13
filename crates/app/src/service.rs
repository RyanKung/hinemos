use crate::*;

/// Protocol-neutral application service that owns shared domain dependencies.
#[derive(Debug, Clone)]
pub struct AppService<S> {
    pub(crate) store: S,
    pub(crate) config: WorldAppConfig,
}

impl<S> AppService<S> {
    /// Builds an application service with default world app configuration.
    #[must_use]
    pub fn new(store: S) -> Self {
        Self {
            store,
            config: WorldAppConfig::default(),
        }
    }

    /// Builds an application service with explicit world app configuration.
    #[must_use]
    pub fn with_config(store: S, config: WorldAppConfig) -> Self {
        Self { store, config }
    }

    /// Returns the configured world app metadata.
    #[must_use]
    pub const fn config(&self) -> &WorldAppConfig {
        &self.config
    }

    /// Returns the underlying store.
    #[must_use]
    pub const fn store(&self) -> &S {
        &self.store
    }

    /// Consumes the service and returns the underlying store.
    #[must_use]
    pub fn into_store(self) -> S {
        self.store
    }

    /// Loads the world-level app metadata from `meta.ron` or `world.toml` when present.
    pub fn load_world_app_config(world_dir: &Path) -> Result<WorldAppConfig> {
        load_world_app_config_from_dir(world_dir)
    }
}
