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
}
