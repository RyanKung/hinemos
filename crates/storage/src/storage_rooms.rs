use hinemos_app::RoomMailboxView;
use serde_json::json;
use sqlx::Row;

use super::room_command_subject;
use crate::{
    PgStorage, ServiceRoomUpsert, StorageError, StoredInboxItem, StoredMailAuthToken,
    StoredRoomBinding, StoredServiceRoom,
};

impl PgStorage {
    /// Loads an enabled external room service by view id.
    pub async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<StoredServiceRoom>, StorageError> {
        let room = sqlx::query_as::<_, StoredServiceRoom>(
            r#"
            select view_id, front_view_id, front_entity_id, address, label, enter_aliases,
                   room_user, room_player_id, status_text, custom_commands, enabled
            from service_rooms
            where view_id = $1
              and enabled
            "#,
        )
        .bind(view_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(room)
    }

    /// Loads an external room service by view id, including disabled registrations.
    pub async fn service_room_by_view_any(
        &self,
        view_id: &str,
    ) -> Result<Option<StoredServiceRoom>, StorageError> {
        let room = sqlx::query_as::<_, StoredServiceRoom>(
            r#"
            select view_id, front_view_id, front_entity_id, address, label, enter_aliases,
                   room_user, room_player_id, status_text, custom_commands, enabled
            from service_rooms
            where view_id = $1
            "#,
        )
        .bind(view_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(room)
    }

    /// Lists enabled external room services with entrances in a street view.
    pub async fn service_rooms_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<StoredServiceRoom>, StorageError> {
        let rooms = sqlx::query_as::<_, StoredServiceRoom>(
            r#"
            select view_id, front_view_id, front_entity_id, address, label, enter_aliases,
                   room_user, room_player_id, status_text, custom_commands, enabled
            from service_rooms
            where front_view_id = $1
              and enabled
            order by address, label, view_id
            "#,
        )
        .bind(front_view_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rooms)
    }

    /// Lists unified parcel and service-room bindings visible from a front view.
    pub async fn room_bindings_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<StoredRoomBinding>, StorageError> {
        let mut bindings = self
            .commercial_parcels_by_front_view(front_view_id)
            .await?
            .into_iter()
            .map(StoredRoomBinding::from_parcel)
            .collect::<Vec<_>>();
        bindings.extend(
            self.service_rooms_by_front_view(front_view_id)
                .await?
                .into_iter()
                .filter_map(StoredRoomBinding::from_service_room),
        );
        bindings.sort_by(|left, right| {
            left.address
                .cmp(&right.address)
                .then_with(|| left.label.cmp(&right.label))
                .then_with(|| left.view_id.cmp(&right.view_id))
        });
        Ok(bindings)
    }

    /// Loads a unified room binding by room view id.
    pub async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<StoredRoomBinding>, StorageError> {
        if let Some(parcel) = self.commercial_parcel_by_view(view_id).await? {
            return Ok(Some(StoredRoomBinding::from_parcel(parcel)));
        }
        Ok(self
            .service_room_by_view(view_id)
            .await?
            .and_then(StoredRoomBinding::from_service_room))
    }

    /// Lists enabled external room services that send mail as the given room user.
    pub async fn service_rooms_by_room_user(
        &self,
        room_user: &str,
    ) -> Result<Vec<StoredServiceRoom>, StorageError> {
        let rooms = sqlx::query_as::<_, StoredServiceRoom>(
            r#"
            select view_id, front_view_id, front_entity_id, address, label, enter_aliases,
                   room_user, room_player_id, status_text, custom_commands, enabled
            from service_rooms
            where room_user = $1
              and enabled
            order by view_id
            "#,
        )
        .bind(room_user)
        .fetch_all(&self.pool)
        .await?;
        Ok(rooms)
    }

    /// Registers or updates an externally hosted room service.
    pub async fn upsert_service_room(
        &self,
        params: ServiceRoomUpsert<'_>,
    ) -> Result<StoredServiceRoom, StorageError> {
        let room = sqlx::query_as::<_, StoredServiceRoom>(
            r#"
            insert into service_rooms (
                view_id, front_view_id, front_entity_id, address, label, enter_aliases,
                room_user, room_player_id, status_text, custom_commands, enabled
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            on conflict (view_id) do update
            set front_view_id = excluded.front_view_id,
                front_entity_id = excluded.front_entity_id,
                address = excluded.address,
                label = excluded.label,
                enter_aliases = excluded.enter_aliases,
                room_user = excluded.room_user,
                room_player_id = excluded.room_player_id,
                status_text = excluded.status_text,
                custom_commands = excluded.custom_commands,
                enabled = excluded.enabled,
                updated_at = now()
            returning view_id, front_view_id, front_entity_id, address, label, enter_aliases,
                      room_user, room_player_id, status_text, custom_commands, enabled
            "#,
        )
        .bind(params.view_id)
        .bind(params.front_view_id)
        .bind(params.front_entity_id)
        .bind(params.address)
        .bind(params.label)
        .bind(params.enter_aliases)
        .bind(params.room_user)
        .bind(params.room_player_id)
        .bind(params.status_text)
        .bind(params.custom_commands)
        .bind(params.enabled)
        .fetch_one(&self.pool)
        .await?;
        Ok(room)
    }

    /// Disables external service rooms that are not present in the latest registration set.
    pub async fn disable_service_rooms_except<'a, I>(
        &self,
        view_ids: I,
    ) -> Result<u64, StorageError>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let view_ids = view_ids.into_iter().collect::<Vec<_>>();
        let result = sqlx::query(
            r#"
            update service_rooms
            set enabled = false,
                updated_at = now()
            where not (view_id = any($1))
              and enabled
            "#,
        )
        .bind(&view_ids)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Loads the current SSH identity player id for a username if that user has logged in.
    pub async fn ssh_identity_player_id_for_user(
        &self,
        username: &str,
    ) -> Result<Option<String>, StorageError> {
        let row = sqlx::query(
            r#"
            select player_id
            from ssh_identities
            where username = $1
            order by last_seen_at desc
            limit 1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| row.get("player_id")))
    }

    /// Sets or rotates the SMTP/IMAP auth token for an externally registered service room.
    pub async fn set_service_room_mail_auth_token(
        &self,
        view_id: &str,
        token: &str,
    ) -> Result<StoredMailAuthToken, StorageError> {
        let Some(room) = self.service_room_by_view_any(view_id).await? else {
            return Err(StorageError::InvalidAccountSetting(format!(
                "service room not found: {view_id}"
            )));
        };
        self.set_room_mailbox_auth_token(&room, token).await
    }

    /// Sets or rotates the SMTP/IMAP auth token for a unified room mailbox.
    pub async fn set_room_mailbox_auth_token<M>(
        &self,
        mailbox: &M,
        token: &str,
    ) -> Result<StoredMailAuthToken, StorageError>
    where
        M: RoomMailboxView + Sync,
    {
        let Some(room_user) = mailbox.room_user() else {
            return Err(StorageError::RoomMailboxMissing(
                mailbox.view_id().to_owned(),
            ));
        };
        let Some(room_player_id) = mailbox.room_player_id() else {
            return Err(StorageError::RoomMailboxMissing(
                mailbox.view_id().to_owned(),
            ));
        };
        self.set_mail_auth_token(room_user, room_player_id, token)
            .await
    }

    /// Sends player input to a unified room mailbox principal.
    pub async fn save_room_mailbox_input<M>(
        &self,
        mailbox: &M,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
    ) -> Result<StoredInboxItem, StorageError>
    where
        M: RoomMailboxView + Sync,
    {
        let Some(room_user) = mailbox.room_user() else {
            return Err(StorageError::RoomMailboxMissing(
                mailbox.view_id().to_owned(),
            ));
        };
        let Some(room_player_id) = mailbox.room_player_id() else {
            return Err(StorageError::RoomMailboxMissing(
                mailbox.view_id().to_owned(),
            ));
        };
        let mut item = self
            .save_mail_message_to_principal(
                sender_user,
                sender_player_id,
                room_user,
                room_player_id,
                &format!("Room command for {}", mailbox.view_id()),
                raw_input,
            )
            .await?;
        let subject = room_command_subject(item.id, mailbox.view_id());
        sqlx::query(
            r#"
            update inbox_items
            set subject = $2,
                source_kind = 'room_command',
                source_id = $1,
                payload = $3,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(item.id)
        .bind(&subject)
        .bind(json!({
            "view_id": mailbox.view_id(),
            "room_user": room_user,
            "sender_user": sender_user
        }))
        .execute(&self.pool)
        .await?;
        item.subject = subject;
        item.source_kind = Some("room_command".to_owned());
        item.source_id = Some(item.id);
        item.payload = json!({
            "view_id": mailbox.view_id(),
            "room_user": room_user,
            "sender_user": sender_user
        });
        Ok(item)
    }
}
