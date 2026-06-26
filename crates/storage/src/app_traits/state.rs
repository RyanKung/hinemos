use super::*;

impl AppPlayerStateStore for PgStorage {
    type Error = StorageError;

    async fn load_player_state(
        &self,
        player_id: &str,
    ) -> Result<Option<hinemos_core::PlayerState>, Self::Error> {
        PgStorage::load_player_state(self, player_id).await
    }

    async fn save_player_state(
        &self,
        player: &hinemos_core::PlayerState,
    ) -> Result<(), Self::Error> {
        PgStorage::save_player_state(self, player).await
    }
}

impl ViewPresenceStore for PgStorage {
    type Error = StorageError;

    async fn record_view_presence(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<(), Self::Error> {
        PgStorage::record_view_presence(self, username, player_id, view_id).await
    }

    async fn recent_active_users(&self, within_seconds: i64) -> Result<Vec<String>, Self::Error> {
        PgStorage::recent_active_users(self, within_seconds).await
    }

    async fn recent_active_view_users(
        &self,
        view_id: &str,
        excluded_player_id: &str,
        within_seconds: i64,
    ) -> Result<Vec<String>, Self::Error> {
        PgStorage::recent_active_view_users(self, view_id, excluded_player_id, within_seconds).await
    }
}
