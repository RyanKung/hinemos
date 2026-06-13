use hinemos_app::ParcelView;
use serde_json::json;

use crate::{
    NewInboxItem, NewMemoryAtom, NewMemoryEvent, OPERATOR_COMMAND_STATUS_DELIVERED,
    OPERATOR_COMMAND_STATUS_PENDING, PgStorage, StorageError, StoredOperatorCommand,
};

struct OperatorCommandParties<'a> {
    owner_user: &'a str,
    owner_player_id: &'a str,
    room_user: &'a str,
    room_player_id: &'a str,
}

fn operator_command_parties<'a, P>(
    parcel: &'a P,
) -> Result<OperatorCommandParties<'a>, StorageError>
where
    P: ParcelView,
{
    Ok(OperatorCommandParties {
        owner_user: parcel
            .owner_user()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id().to_owned()))?,
        owner_player_id: parcel
            .owner_player_id()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id().to_owned()))?,
        room_user: parcel
            .room_user()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id().to_owned()))?,
        room_player_id: parcel
            .room_player_id()
            .ok_or_else(|| StorageError::ParcelNotBuilt(parcel.parcel_id().to_owned()))?,
    })
}

impl PgStorage {
    /// Stores a raw visitor command for a shop operator.
    pub async fn save_operator_command<P>(
        &self,
        parcel: &P,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        delivered: bool,
    ) -> Result<StoredOperatorCommand, StorageError>
    where
        P: ParcelView,
    {
        let parties = operator_command_parties(parcel)?;
        let command = self
            .insert_operator_command(
                parcel,
                &parties,
                sender_user,
                sender_player_id,
                raw_input,
                delivered,
            )
            .await?;
        self.notify_shop_operator(
            parcel,
            &parties,
            sender_user,
            sender_player_id,
            raw_input,
            &command,
        )
        .await?;
        self.record_shop_command_memory(
            parcel,
            &parties,
            sender_user,
            sender_player_id,
            raw_input,
            &command,
        )
        .await?;
        Ok(command)
    }

    async fn insert_operator_command<P>(
        &self,
        parcel: &P,
        parties: &OperatorCommandParties<'_>,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        delivered: bool,
    ) -> Result<StoredOperatorCommand, StorageError>
    where
        P: ParcelView,
    {
        let status = if delivered {
            OPERATOR_COMMAND_STATUS_DELIVERED
        } else {
            OPERATOR_COMMAND_STATUS_PENDING
        };
        let command = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            insert into operator_commands (
                view_id, parcel_id, sender_user, sender_player_id,
                owner_user, owner_player_id, raw_input, status, delivered_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8,
                    case when $8 = $9 then now() else null end)
            returning id, view_id, parcel_id, sender_user, sender_player_id,
                      owner_user, owner_player_id, raw_input, status,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(parcel.view_id())
        .bind(parcel.parcel_id())
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(parties.owner_user)
        .bind(parties.owner_player_id)
        .bind(raw_input)
        .bind(status)
        .bind(OPERATOR_COMMAND_STATUS_DELIVERED)
        .fetch_one(&self.pool)
        .await?;
        Ok(command)
    }

    async fn notify_shop_operator<P>(
        &self,
        parcel: &P,
        parties: &OperatorCommandParties<'_>,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        command: &StoredOperatorCommand,
    ) -> Result<(), StorageError>
    where
        P: ParcelView,
    {
        let subject = format!("Shop command for {}", parcel.parcel_id());
        self.create_inbox_item(NewInboxItem {
            kind: "shop_command",
            recipient_user: parties.room_user,
            recipient_player_id: parties.room_player_id,
            sender_user,
            sender_player_id,
            subject: &subject,
            body: raw_input,
            source_kind: Some("operator_command"),
            source_id: Some(command.id),
            payload: json!({
                "parcelId": parcel.parcel_id(),
                "viewId": parcel.view_id(),
                "commandId": command.id,
                "rawInput": raw_input
            }),
        })
        .await?;
        self.save_mail_message_to_principal(
            sender_user,
            sender_player_id,
            parties.room_user,
            parties.room_player_id,
            &subject,
            raw_input,
        )
        .await?;
        Ok(())
    }

    async fn record_shop_command_memory<P>(
        &self,
        parcel: &P,
        parties: &OperatorCommandParties<'_>,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        command: &StoredOperatorCommand,
    ) -> Result<(), StorageError>
    where
        P: ParcelView,
    {
        self.record_shop_command_visitor_memory(
            parcel,
            parties,
            sender_user,
            sender_player_id,
            raw_input,
            command,
        )
        .await?;
        self.record_shop_command_owner_memory(parcel, parties, sender_user, raw_input, command)
            .await?;
        Ok(())
    }

    async fn record_shop_command_visitor_memory<P>(
        &self,
        parcel: &P,
        parties: &OperatorCommandParties<'_>,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
        command: &StoredOperatorCommand,
    ) -> Result<(), StorageError>
    where
        P: ParcelView,
    {
        let visitor_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: sender_player_id.to_owned(),
                source: "shop".to_owned(),
                event_type: "shop_command_sent".to_owned(),
                actors: json!([sender_user, parties.owner_user]),
                content: format!(
                    "Sent shop command #{} to {} at {}: {}",
                    command.id,
                    parties.owner_user,
                    parcel.parcel_id(),
                    raw_input
                ),
                world_refs: json!({
                    "kind": "shop_command",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id(),
                    "view_id": parcel.view_id(),
                    "owner_user": parties.owner_user
                }),
                salience: 0.65,
            })
            .await?;
        let visitor_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: sender_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: parties.owner_user.to_owned(),
                predicate: "shop_interaction".to_owned(),
                object: json!({
                    "direction": "sent",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id(),
                    "raw_input": raw_input
                }),
                summary: format!(
                    "Asked {}'s shop at {}: {}",
                    parties.owner_user,
                    parcel.parcel_id(),
                    raw_input
                ),
                evidence_event_ids: vec![visitor_event.id],
                confidence: 0.85,
                importance: 0.65,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            sender_player_id,
            parties.owner_user,
            visitor_atom.id,
            Some("shop_counterparty"),
        )
        .await?;
        Ok(())
    }

    async fn record_shop_command_owner_memory<P>(
        &self,
        parcel: &P,
        parties: &OperatorCommandParties<'_>,
        sender_user: &str,
        raw_input: &str,
        command: &StoredOperatorCommand,
    ) -> Result<(), StorageError>
    where
        P: ParcelView,
    {
        let owner_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: parties.owner_player_id.to_owned(),
                source: "shop".to_owned(),
                event_type: "shop_command_received".to_owned(),
                actors: json!([sender_user, parties.owner_user]),
                content: format!(
                    "Received shop command #{} from {} at {}: {}",
                    command.id,
                    sender_user,
                    parcel.parcel_id(),
                    raw_input
                ),
                world_refs: json!({
                    "kind": "shop_command",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id(),
                    "view_id": parcel.view_id(),
                    "sender_user": sender_user
                }),
                salience: 0.75,
            })
            .await?;
        let owner_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: parties.owner_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: sender_user.to_owned(),
                predicate: "shop_interaction".to_owned(),
                object: json!({
                    "direction": "received",
                    "command_id": command.id,
                    "parcel_id": parcel.parcel_id(),
                    "raw_input": raw_input
                }),
                summary: format!(
                    "{} asked my shop at {}: {}",
                    sender_user,
                    parcel.parcel_id(),
                    raw_input
                ),
                evidence_event_ids: vec![owner_event.id],
                confidence: 0.9,
                importance: 0.75,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            parties.owner_player_id,
            sender_user,
            owner_atom.id,
            Some("shop_counterparty"),
        )
        .await?;
        Ok(())
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
}
