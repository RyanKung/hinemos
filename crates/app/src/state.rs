use crate::*;

/// User presence with relative recency for online summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentPresenceUser {
    /// Display username.
    pub user: String,
    /// Milliseconds since this user was last observed.
    pub age_millis: u64,
}

impl<S, E> AppService<S>
where
    S: PlayerStateStore<Error = E>,
{
    /// Loads a saved player state if one exists.
    pub async fn load_player_state(&self, player_id: &str) -> Result<Option<PlayerState>, E> {
        self.store.load_player_state(player_id).await
    }

    /// Persists the current player state.
    pub async fn save_player_state(&self, player: &PlayerState) -> Result<(), E> {
        self.store.save_player_state(player).await
    }
}

impl<S, E> AppService<S>
where
    S: ViewPresenceStore<Error = E>,
{
    /// Records the player's latest observed view for presence hints.
    pub async fn record_view_presence(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<(), E> {
        self.store
            .record_view_presence(username, player_id, view_id)
            .await
    }

    /// Lists users with recent activity in any world view.
    pub async fn recent_active_users(
        &self,
        within_seconds: i64,
    ) -> Result<Vec<RecentPresenceUser>, E> {
        self.store.recent_active_users(within_seconds).await
    }

    /// Lists users with recent activity in the given world view.
    pub async fn recent_active_view_users(
        &self,
        view_id: &str,
        excluded_player_id: &str,
        within_seconds: i64,
    ) -> Result<Vec<RecentPresenceUser>, E> {
        self.store
            .recent_active_view_users(view_id, excluded_player_id, within_seconds)
            .await
    }
}

/// Storage boundary for view presence hints.
pub trait ViewPresenceStore {
    /// Store error type.
    type Error;

    /// Records the player's latest observed view.
    async fn record_view_presence(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<(), Self::Error>;

    /// Lists admitted usernames with recent activity in any world view.
    async fn recent_active_users(
        &self,
        within_seconds: i64,
    ) -> Result<Vec<RecentPresenceUser>, Self::Error>;

    /// Lists admitted usernames with recent activity in the given world view.
    async fn recent_active_view_users(
        &self,
        view_id: &str,
        excluded_player_id: &str,
        within_seconds: i64,
    ) -> Result<Vec<RecentPresenceUser>, Self::Error>;
}

/// Storage boundary for persisted player state.
pub trait PlayerStateStore {
    /// Store error type.
    type Error;

    /// Loads a player state if one has been saved.
    async fn load_player_state(&self, player_id: &str) -> Result<Option<PlayerState>, Self::Error>;

    /// Saves the current player state.
    async fn save_player_state(&self, player: &PlayerState) -> Result<(), Self::Error>;
}
