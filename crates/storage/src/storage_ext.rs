use hinemos_app::RoomMailboxView;
use hinemos_core::PlayerState;
use serde_json::json;
use sqlx::Row;

use super::{INITIAL_MARK_GRANT, room_command_subject, room_mail_player_id, room_mail_user};
use crate::types::{
    credit_balance, debit_balance, ensure_balance_row, ensure_player_account, fetch_balance_pool,
    fetch_balance_tx, fetch_parcel_by_id, player_account_id, resolve_payment_target,
};
use crate::{
    NewMemoryAtom, NewMemoryEvent, PlayerStateRow, StorageError, StoredBalance, StoredInboxItem,
    StoredMailAuthToken, StoredParcel, StoredRoomBinding, StoredServiceRoom, StoredTransfer,
};
use crate::{PgStorage, ServiceRoomUpsert, TEST_CURRENCY};

impl PgStorage {
    /// Ensures the player has a MARK wallet and receives the one-time test grant.
    pub async fn ensure_player_wallet(
        &self,
        username: &str,
        player_id: &str,
    ) -> Result<StoredBalance, StorageError> {
        let mut tx = self.pool.begin().await?;
        let account_id = player_account_id(player_id);
        ensure_player_account(&mut tx, &account_id, username, player_id).await?;
        ensure_balance_row(&mut tx, &account_id).await?;

        let grant_key = format!("initial_mark_grant:{account_id}");
        let inserted = sqlx::query(
            r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount, reason, memo, idempotency_key
            )
            values ('MARK', null, $1, $2, 'initial_grant', 'Initial MARK grant', $3)
            on conflict (idempotency_key) do nothing
            returning id
            "#,
        )
        .bind(&account_id)
        .bind(INITIAL_MARK_GRANT)
        .bind(&grant_key)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();

        if inserted {
            credit_balance(&mut tx, &account_id, INITIAL_MARK_GRANT).await?;
        }

        let balance = fetch_balance_tx(&mut tx, &account_id).await?;
        tx.commit().await?;
        Ok(balance)
    }

    /// Records the player's latest observed view for cross-session presence hints.
    pub async fn record_view_presence(
        &self,
        username: &str,
        player_id: &str,
        view_id: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into view_presence (player_id, username, view_id, last_seen_at)
            values ($1, $2, $3, now())
            on conflict (player_id) do update
            set username = excluded.username,
                view_id = excluded.view_id,
                last_seen_at = now()
            where view_presence.view_id is distinct from excluded.view_id
               or view_presence.username is distinct from excluded.username
               or view_presence.last_seen_at < now() - interval '5 seconds'
            "#,
        )
        .bind(player_id)
        .bind(username)
        .bind(view_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Loads a player's MARK balance.
    pub async fn player_balance(&self, player_id: &str) -> Result<StoredBalance, StorageError> {
        let account_id = player_account_id(player_id);
        fetch_balance_pool(&self.pool, &account_id).await
    }

    /// Transfers MARK from a player to a user or player account.
    pub async fn transfer_mark(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<StoredTransfer, StorageError> {
        if amount <= 0 {
            return Err(StorageError::InvalidAmount(amount));
        }

        let (transfer, target_player_id) = self
            .execute_mark_transfer(sender_user, sender_player_id, target, amount, memo)
            .await?;
        self.record_mark_transfer_memory(
            sender_user,
            sender_player_id,
            &target_player_id,
            &transfer,
        )
        .await?;
        Ok(transfer)
    }

    async fn execute_mark_transfer(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        amount: i64,
        memo: &str,
    ) -> Result<(StoredTransfer, String), StorageError> {
        let mut tx = self.pool.begin().await?;
        let sender_account_id = player_account_id(sender_player_id);
        ensure_player_account(&mut tx, &sender_account_id, sender_user, sender_player_id).await?;
        ensure_balance_row(&mut tx, &sender_account_id).await?;

        let target = resolve_payment_target(&mut tx, target).await?;
        let target_account_id = player_account_id(&target.player_id);
        ensure_player_account(
            &mut tx,
            &target_account_id,
            &target.username,
            &target.player_id,
        )
        .await?;
        ensure_balance_row(&mut tx, &target_account_id).await?;

        if sender_account_id == target_account_id {
            return Err(StorageError::SelfPayment);
        }

        debit_balance(&mut tx, &sender_account_id, amount).await?;
        credit_balance(&mut tx, &target_account_id, amount).await?;
        let ledger_id = sqlx::query(
            r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount, reason, memo
            )
            values ('MARK', $1, $2, $3, 'player_payment', $4)
            returning id
            "#,
        )
        .bind(&sender_account_id)
        .bind(&target_account_id)
        .bind(amount)
        .bind(memo)
        .fetch_one(&mut *tx)
        .await?
        .get::<i64, _>("id");
        let sender_balance = fetch_balance_tx(&mut tx, &sender_account_id).await?.amount;
        tx.commit().await?;

        let target_player_id = target.player_id.clone();
        let transfer = StoredTransfer {
            ledger_id,
            asset: TEST_CURRENCY.to_owned(),
            amount,
            sender_account_id,
            target_account_id,
            target_user: target.username,
            memo: memo.to_owned(),
            sender_balance,
        };

        Ok((transfer, target_player_id))
    }

    async fn record_mark_transfer_memory(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target_player_id: &str,
        transfer: &StoredTransfer,
    ) -> Result<(), StorageError> {
        self.record_sent_mark_transfer_memory(sender_user, sender_player_id, transfer)
            .await?;
        self.record_received_mark_transfer_memory(sender_user, target_player_id, transfer)
            .await?;
        Ok(())
    }

    async fn record_sent_mark_transfer_memory(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        transfer: &StoredTransfer,
    ) -> Result<(), StorageError> {
        let sent_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: sender_player_id.to_owned(),
                source: "trade".to_owned(),
                event_type: "mark_transfer_sent".to_owned(),
                actors: json!([sender_user, transfer.target_user]),
                content: format!(
                    "Sent {} {} to {}. Memo: {}",
                    transfer.amount, transfer.asset, transfer.target_user, transfer.memo
                ),
                world_refs: json!({
                    "kind": "transfer",
                    "ledger_id": transfer.ledger_id,
                    "target_user": transfer.target_user
                }),
                salience: 0.75,
            })
            .await?;
        let sent_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: sender_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: transfer.target_user.clone(),
                predicate: "paid".to_owned(),
                object: json!({
                    "direction": "sent",
                    "asset": transfer.asset,
                    "amount": transfer.amount,
                    "memo": transfer.memo,
                    "ledger_id": transfer.ledger_id
                }),
                summary: format!(
                    "Paid {} {} to {} for: {}",
                    transfer.amount, transfer.asset, transfer.target_user, transfer.memo
                ),
                evidence_event_ids: vec![sent_event.id],
                confidence: 0.95,
                importance: 0.75,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            sender_player_id,
            &transfer.target_user,
            sent_atom.id,
            Some("payment_counterparty"),
        )
        .await?;
        Ok(())
    }

    async fn record_received_mark_transfer_memory(
        &self,
        sender_user: &str,
        target_player_id: &str,
        transfer: &StoredTransfer,
    ) -> Result<(), StorageError> {
        let received_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: target_player_id.to_owned(),
                source: "trade".to_owned(),
                event_type: "mark_transfer_received".to_owned(),
                actors: json!([sender_user, transfer.target_user]),
                content: format!(
                    "Received {} {} from {}. Memo: {}",
                    transfer.amount, transfer.asset, sender_user, transfer.memo
                ),
                world_refs: json!({
                    "kind": "transfer",
                    "ledger_id": transfer.ledger_id,
                    "sender_user": sender_user
                }),
                salience: 0.75,
            })
            .await?;
        let received_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: target_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: sender_user.to_owned(),
                predicate: "received_payment".to_owned(),
                object: json!({
                    "direction": "received",
                    "asset": transfer.asset,
                    "amount": transfer.amount,
                    "memo": transfer.memo,
                    "ledger_id": transfer.ledger_id
                }),
                summary: format!(
                    "Received {} {} from {} for: {}",
                    transfer.amount, transfer.asset, sender_user, transfer.memo
                ),
                evidence_event_ids: vec![received_event.id],
                confidence: 0.95,
                importance: 0.75,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            target_player_id,
            sender_user,
            received_atom.id,
            Some("payment_counterparty"),
        )
        .await?;
        Ok(())
    }

    /// Lists all commercial parcels.
    pub async fn list_commercial_parcels(&self) -> Result<Vec<StoredParcel>, StorageError> {
        let parcels = sqlx::query_as::<_, StoredParcel>(
            r#"
            select parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
                   room_user, room_player_id,
                   status, title, description, style, operator_prompt, custom_commands
            from commercial_parcels
            order by district, position
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(parcels)
    }

    /// Lists commercial parcels whose entrances are visible from a front view.
    pub async fn commercial_parcels_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<StoredParcel>, StorageError> {
        let parcels = sqlx::query_as::<_, StoredParcel>(
            r#"
            select parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
                   room_user, room_player_id,
                   status, title, description, style, operator_prompt, custom_commands
            from commercial_parcels
            where front_view_id = $1
            order by district, position
            "#,
        )
        .bind(front_view_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(parcels)
    }

    /// Loads a commercial parcel by parcel id.
    pub async fn commercial_parcel(&self, parcel_id: &str) -> Result<StoredParcel, StorageError> {
        fetch_parcel_by_id(&self.pool, parcel_id).await
    }

    /// Loads a commercial parcel by view id.
    pub async fn commercial_parcel_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<StoredParcel>, StorageError> {
        let parcel = sqlx::query_as::<_, StoredParcel>(
            r#"
            select parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
                   room_user, room_player_id,
                   status, title, description, style, operator_prompt, custom_commands
            from commercial_parcels
            where view_id = $1
            "#,
        )
        .bind(view_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(parcel)
    }

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
            set subject = $2
            where id = $1
            "#,
        )
        .bind(item.id)
        .bind(&subject)
        .execute(&self.pool)
        .await?;
        item.subject = subject;
        Ok(item)
    }

    /// Claims a free commercial parcel.
    pub async fn claim_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_user: &str,
        owner_player_id: &str,
    ) -> Result<StoredParcel, StorageError> {
        let updated = sqlx::query_as::<_, StoredParcel>(
            r#"
            update commercial_parcels
            set owner_user = $2,
                owner_player_id = $3,
                room_user = coalesce(room_user, $4),
                room_player_id = coalesce(room_player_id, $5),
                status = 'claimed',
                updated_at = now()
            where parcel_id = $1
              and owner_player_id is null
            returning parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
                      room_user, room_player_id,
                      status, title, description, style, operator_prompt, custom_commands
            "#,
        )
        .bind(parcel_id)
        .bind(owner_user)
        .bind(owner_player_id)
        .bind(room_mail_user(parcel_id))
        .bind(room_mail_player_id(parcel_id))
        .fetch_optional(&self.pool)
        .await?;

        if let Some(parcel) = updated {
            return Ok(parcel);
        }

        let existing = self.commercial_parcel(parcel_id).await?;
        if existing.owner_player_id.is_some() {
            Err(StorageError::ParcelAlreadyOwned(parcel_id.to_owned()))
        } else {
            Ok(existing)
        }
    }

    /// Transfers a commercial parcel to another known player.
    pub async fn transfer_commercial_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<StoredParcel, StorageError> {
        let mut tx = self.pool.begin().await?;
        let target = resolve_payment_target(&mut tx, target).await?;
        let updated = sqlx::query_as::<_, StoredParcel>(
            r#"
            update commercial_parcels
            set owner_user = $3,
                owner_player_id = $4,
                updated_at = now()
            where parcel_id = $1
              and owner_player_id = $2
            returning parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
                      room_user, room_player_id,
                      status, title, description, style, operator_prompt, custom_commands
            "#,
        )
        .bind(parcel_id)
        .bind(owner_player_id)
        .bind(&target.username)
        .bind(&target.player_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        updated.ok_or_else(|| StorageError::NotParcelOwner(parcel_id.to_owned()))
    }

    /// Updates one build sheet field for an owned parcel.
    pub async fn update_parcel_build_field(
        &self,
        view_id: &str,
        owner_player_id: &str,
        field: &str,
        value: &str,
    ) -> Result<StoredParcel, StorageError> {
        let column = match field {
            "title" => "title",
            "description" => "description",
            "style" => "style",
            "prompt" => "operator_prompt",
            "commands" => "custom_commands",
            _ => return Err(StorageError::UnknownBuildField(field.to_owned())),
        };
        let query = format!(
            "update commercial_parcels set {column} = $3, updated_at = now() \
             where view_id = $1 and owner_player_id = $2 \
             returning parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id, \
                       room_user, room_player_id, \
                       status, title, description, style, operator_prompt, custom_commands"
        );
        let updated = sqlx::query_as::<_, StoredParcel>(&query)
            .bind(view_id)
            .bind(owner_player_id)
            .bind(value)
            .fetch_optional(&self.pool)
            .await?;

        updated.ok_or_else(|| StorageError::NotParcelOwner(view_id.to_owned()))
    }

    /// Publishes an owned parcel build sheet.
    pub async fn publish_parcel_build(
        &self,
        view_id: &str,
        owner_player_id: &str,
    ) -> Result<StoredParcel, StorageError> {
        let updated = sqlx::query_as::<_, StoredParcel>(
            r#"
            update commercial_parcels
            set status = 'built',
                updated_at = now()
            where view_id = $1
              and owner_player_id = $2
              and coalesce(title, '') <> ''
              and coalesce(description, '') <> ''
            returning parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
                      room_user, room_player_id,
                      status, title, description, style, operator_prompt, custom_commands
            "#,
        )
        .bind(view_id)
        .bind(owner_player_id)
        .fetch_optional(&self.pool)
        .await?;

        updated.ok_or_else(|| StorageError::BuildNotPublishable(view_id.to_owned()))
    }

    /// Loads a player state if one has been saved.
    pub async fn load_player_state(
        &self,
        player_id: &str,
    ) -> Result<Option<PlayerState>, StorageError> {
        let Some(row) = sqlx::query_as::<_, PlayerStateRow>(
            r#"
            select player_id, current_view, inventory
            from player_states
            where player_id = $1
            "#,
        )
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(row.try_into()?))
    }

    /// Saves the current player state.
    pub async fn save_player_state(&self, player: &PlayerState) -> Result<(), StorageError> {
        let inventory = serde_json::to_value(&player.inventory)?;
        sqlx::query(
            r#"
            insert into player_states (player_id, current_view, inventory)
            values ($1, $2, $3)
            on conflict (player_id) do update
            set current_view = excluded.current_view,
                inventory = excluded.inventory,
                updated_at = now()
            "#,
        )
        .bind(&player.id)
        .bind(&player.current_view)
        .bind(inventory)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
