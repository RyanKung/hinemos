use super::*;

impl HungerView for StoredHungerState {
    fn player_id(&self) -> &str {
        &self.player_id
    }

    fn hunger_points(&self) -> i32 {
        self.hunger_points
    }
}

impl HungerStore for PgStorage {
    type Error = StorageError;
    type Hunger = StoredHungerState;

    async fn player_hunger(&self, player_id: &str) -> Result<Self::Hunger, Self::Error> {
        PgStorage::player_hunger(self, player_id).await
    }

    async fn record_hunger_interaction(
        &self,
        player_id: &str,
        points: i32,
    ) -> Result<Self::Hunger, Self::Error> {
        PgStorage::record_hunger_interaction(self, player_id, points).await
    }

    async fn restore_player_hunger(
        &self,
        player_id: &str,
        food: &str,
    ) -> Result<Self::Hunger, Self::Error> {
        PgStorage::restore_player_hunger(self, player_id, food).await
    }

    async fn try_record_hungry_broke_interaction(
        &self,
        player_id: &str,
        cooldown_seconds: i64,
    ) -> Result<bool, Self::Error> {
        PgStorage::try_record_hungry_broke_interaction(self, player_id, cooldown_seconds).await
    }
}
