use hinemos_core::PlayerState;
use serde_json::json;
use sqlx::Row;

use super::{INITIAL_MARK_GRANT, room_mail_player_id, room_mail_user};
use crate::types::{
    credit_balance, debit_balance, ensure_balance_row, ensure_player_account, fetch_balance_pool,
    fetch_balance_tx, fetch_parcel_by_id, player_account_id, resolve_payment_target,
};
use crate::{
    NewInboxItem, NewMemoryAtom, NewMemoryEvent, PlayerStateRow, StorageError, StoredBalance,
    StoredInboxItem, StoredOperatorCommand, StoredParcel, StoredServiceRoom, StoredTransfer,
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

        let received_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: target_player_id.clone(),
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
                agent_id: target_player_id.clone(),
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
            &target_player_id,
            sender_user,
            received_atom.id,
            Some("payment_counterparty"),
        )
        .await?;

        Ok(transfer)
    }

    /// Lists all commercial parcels.
    pub async fn list_commercial_parcels(&self) -> Result<Vec<StoredParcel>, StorageError> {
        let parcels = sqlx::query_as::<_, StoredParcel>(
            r#"
            select parcel_id, view_id, district, position, owner_user, owner_player_id,
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
            select parcel_id, view_id, district, position, owner_user, owner_player_id,
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

    /// Sends player input to an externally hosted room service mailbox.
    pub async fn save_service_room_input(
        &self,
        room: &StoredServiceRoom,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
    ) -> Result<StoredInboxItem, StorageError> {
        let subject = format!("Room command for {}", room.view_id);
        self.save_mail_message_to_principal(
            sender_user,
            sender_player_id,
            &room.room_user,
            &room.room_player_id,
            &subject,
            raw_input,
        )
        .await
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
            returning parcel_id, view_id, district, position, owner_user, owner_player_id,
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
            returning parcel_id, view_id, district, position, owner_user, owner_player_id,
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
             returning parcel_id, view_id, district, position, owner_user, owner_player_id, \
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
            returning parcel_id, view_id, district, position, owner_user, owner_player_id,
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

    /// Stores a raw visitor command for a shop operator.
    pub async fn save_operator_command(
        &self,
        parcel: &StoredParcel,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        delivered: bool,
    ) -> Result<StoredOperatorCommand, StorageError> {
        let owner_user = parcel
            .owner_user
            .as_deref()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id.clone()))?;
        let owner_player_id = parcel
            .owner_player_id
            .as_deref()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id.clone()))?;
        let room_user = parcel
            .room_user
            .as_deref()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id.clone()))?;
        let room_player_id = parcel
            .room_player_id
            .as_deref()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id.clone()))?;
        let status = if delivered { "delivered" } else { "pending" };
        let command = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            insert into operator_commands (
                view_id, parcel_id, sender_user, sender_player_id,
                owner_user, owner_player_id, raw_input, status, delivered_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8,
                    case when $8 = 'delivered' then now() else null end)
            returning id, view_id, parcel_id, sender_user, sender_player_id,
                      owner_user, owner_player_id, raw_input, status,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(&parcel.view_id)
        .bind(&parcel.parcel_id)
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(owner_user)
        .bind(owner_player_id)
        .bind(raw_input)
        .bind(status)
        .fetch_one(&self.pool)
        .await?;
        let subject = format!("Shop command for {}", parcel.parcel_id);
        self.create_inbox_item(NewInboxItem {
            kind: "shop_command",
            recipient_user: room_user,
            recipient_player_id: room_player_id,
            sender_user,
            sender_player_id,
            subject: &subject,
            body: raw_input,
            source_kind: Some("operator_command"),
            source_id: Some(command.id),
            payload: json!({
                "parcelId": parcel.parcel_id,
                "viewId": parcel.view_id,
                "commandId": command.id,
                "rawInput": raw_input
            }),
        })
        .await?;
        self.save_mail_message_to_principal(
            sender_user,
            sender_player_id,
            room_user,
            room_player_id,
            &subject,
            raw_input,
        )
        .await?;

        let visitor_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: sender_player_id.to_owned(),
                source: "shop".to_owned(),
                event_type: "shop_command_sent".to_owned(),
                actors: json!([sender_user, owner_user]),
                content: format!(
                    "Sent shop command #{} to {} at {}: {}",
                    command.id, owner_user, parcel.parcel_id, raw_input
                ),
                world_refs: json!({
                    "kind": "shop_command",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id,
                    "view_id": parcel.view_id,
                    "owner_user": owner_user
                }),
                salience: 0.65,
            })
            .await?;
        let visitor_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: sender_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: owner_user.to_owned(),
                predicate: "shop_interaction".to_owned(),
                object: json!({
                    "direction": "sent",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id,
                    "raw_input": raw_input
                }),
                summary: format!(
                    "Asked {}'s shop at {}: {}",
                    owner_user, parcel.parcel_id, raw_input
                ),
                evidence_event_ids: vec![visitor_event.id],
                confidence: 0.85,
                importance: 0.65,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            sender_player_id,
            owner_user,
            visitor_atom.id,
            Some("shop_counterparty"),
        )
        .await?;

        let owner_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: owner_player_id.to_owned(),
                source: "shop".to_owned(),
                event_type: "shop_command_received".to_owned(),
                actors: json!([sender_user, owner_user]),
                content: format!(
                    "Received shop command #{} from {} at {}: {}",
                    command.id, sender_user, parcel.parcel_id, raw_input
                ),
                world_refs: json!({
                    "kind": "shop_command",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id,
                    "view_id": parcel.view_id,
                    "sender_user": sender_user
                }),
                salience: 0.75,
            })
            .await?;
        let owner_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: owner_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: sender_user.to_owned(),
                predicate: "shop_interaction".to_owned(),
                object: json!({
                    "direction": "received",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id,
                    "raw_input": raw_input
                }),
                summary: format!(
                    "{} asked my shop at {}: {}",
                    sender_user, parcel.parcel_id, raw_input
                ),
                evidence_event_ids: vec![owner_event.id],
                confidence: 0.9,
                importance: 0.75,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            owner_player_id,
            sender_user,
            owner_atom.id,
            Some("shop_counterparty"),
        )
        .await?;

        Ok(command)
    }

    /// Loads recent raw visitor commands for shops owned by a player.
    pub async fn recent_operator_commands(
        &self,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredOperatorCommand>, StorageError> {
        let commands = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            select id, view_id, parcel_id, sender_user, sender_player_id,
                   owner_user, owner_player_id, raw_input, status,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from operator_commands
            where owner_player_id = $1
            order by id desc
            limit $2
            "#,
        )
        .bind(owner_player_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(commands)
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
