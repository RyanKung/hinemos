use super::*;

impl RoomStore for PgStorage {
    type Error = StorageError;
    type ServiceRoom = StoredServiceRoom;
    type RoomBinding = StoredRoomBinding;

    async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error> {
        PgStorage::service_room_by_view(self, view_id).await
    }

    async fn room_bindings_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error> {
        PgStorage::room_bindings_by_front_view(self, front_view_id).await
    }

    async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error> {
        PgStorage::room_binding_by_view(self, view_id).await
    }
}

impl RoomRegistrationStore for PgStorage {
    type Error = StorageError;

    async fn disable_service_rooms_except(
        &self,
        view_ids: Vec<String>,
    ) -> Result<u64, Self::Error> {
        let view_ids = view_ids.iter().map(String::as_str).collect::<Vec<_>>();
        PgStorage::disable_service_rooms_except(self, view_ids).await
    }

    async fn upsert_service_room(
        &self,
        registration: ServiceRoomRegistrationUpsert<'_>,
    ) -> Result<(), Self::Error> {
        PgStorage::upsert_service_room(
            self,
            ServiceRoomUpsert {
                view_id: registration.view_id,
                front_view_id: registration.front_view_id,
                front_entity_id: registration.front_entity_id,
                address: registration.address,
                label: registration.label,
                enter_aliases: registration.enter_aliases,
                room_user: registration.room_user,
                room_player_id: registration.room_player_id,
                status_text: registration.status_text,
                custom_commands: registration.custom_commands,
                builtin_handler: registration.builtin_handler,
                enabled: registration.enabled,
            },
        )
        .await?;
        Ok(())
    }

    async fn ssh_identity_player_id_for_user(
        &self,
        room_user: &str,
    ) -> Result<Option<String>, Self::Error> {
        PgStorage::ssh_identity_player_id_for_user(self, room_user).await
    }
}

impl RoomCommandPolicyView for StoredRoomBinding {
    fn forwards_all_input(&self) -> bool {
        matches!(self.command_policy, StoredRoomCommandPolicy::ForwardAll)
    }

    fn listed_commands(&self) -> &[String] {
        match &self.command_policy {
            StoredRoomCommandPolicy::ForwardAll => &[],
            StoredRoomCommandPolicy::ForwardListed(commands) => commands,
        }
    }
}

impl RoomBindingEntryView for StoredRoomBinding {
    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn front_entity_id(&self) -> Option<&str> {
        self.front_entity_id.as_deref()
    }

    fn address(&self) -> &str {
        &self.address
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn enter_aliases(&self) -> &[String] {
        &self.enter_aliases
    }
}

impl RoomBindingKindView for StoredRoomBinding {
    fn is_commercial_parcel(&self) -> bool {
        matches!(self.kind, crate::StoredRoomBindingKind::CommercialParcel)
    }

    fn is_service_room(&self) -> bool {
        matches!(self.kind, crate::StoredRoomBindingKind::ServiceRoom)
    }
}

impl RoomMailboxView for StoredServiceRoom {
    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        Some(&self.room_user)
    }

    fn room_player_id(&self) -> Option<&str> {
        Some(&self.room_player_id)
    }
}

impl RoomMailboxView for StoredRoomBinding {
    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        self.room_user.as_deref()
    }

    fn room_player_id(&self) -> Option<&str> {
        self.room_player_id.as_deref()
    }
}

impl ParcelView for StoredRoomBinding {
    fn parcel_id(&self) -> &str {
        &self.address
    }

    fn view_id(&self) -> &str {
        &self.view_id
    }

    fn front_view_id(&self) -> &str {
        &self.front_view_id
    }

    fn district(&self) -> &str {
        ""
    }

    fn position(&self) -> i32 {
        0
    }

    fn owner_user(&self) -> Option<&str> {
        self.owner_user.as_deref()
    }

    fn owner_player_id(&self) -> Option<&str> {
        self.owner_player_id.as_deref()
    }

    fn room_user(&self) -> Option<&str> {
        self.room_user.as_deref()
    }

    fn room_player_id(&self) -> Option<&str> {
        self.room_player_id.as_deref()
    }

    fn status(&self) -> &str {
        self.parcel_status.as_deref().unwrap_or("unknown")
    }

    fn title(&self) -> Option<&str> {
        self.parcel_title.as_deref()
    }

    fn description(&self) -> Option<&str> {
        self.parcel_description.as_deref()
    }

    fn style(&self) -> Option<&str> {
        self.parcel_style.as_deref()
    }

    fn operator_prompt(&self) -> Option<&str> {
        self.parcel_operator_prompt.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.parcel_custom_commands.as_deref()
    }
}

impl ServiceRoomView for StoredRoomBinding {
    fn label(&self) -> Option<&str> {
        Some(&self.label)
    }

    fn address(&self) -> Option<&str> {
        Some(&self.address)
    }

    fn front_view_id(&self) -> Option<&str> {
        Some(&self.front_view_id)
    }

    fn status_text(&self) -> Option<&str> {
        self.status_text.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.custom_commands.as_deref()
    }
}

impl ServiceRoomView for StoredServiceRoom {
    fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }

    fn front_view_id(&self) -> Option<&str> {
        self.front_view_id.as_deref()
    }

    fn status_text(&self) -> Option<&str> {
        self.status_text.as_deref()
    }

    fn custom_commands(&self) -> Option<&str> {
        self.custom_commands.as_deref()
    }
}
