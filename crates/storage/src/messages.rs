//! World message persistence.

use serde_json::json;

use crate::{
    INBOX_FILTER_ALL, INBOX_FILTER_CLAIMED, INBOX_FILTER_DONE, INBOX_FILTER_OPEN,
    INBOX_FILTER_UNREAD, INBOX_STATUS_ACKED, INBOX_STATUS_ARCHIVED, INBOX_STATUS_CLAIMED,
    INBOX_STATUS_UNREAD, NewMemoryAtom, NewMemoryEvent, PgStorage, StorageError, StoredInboxItem,
    StoredWorldMessage,
};

impl PgStorage {
    /// Persists a mailbox message. Mail has no expiry.
    pub async fn save_mail_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        body: &str,
    ) -> Result<StoredInboxItem, StorageError> {
        self.save_mail_message_with_subject(
            sender_user,
            sender_player_id,
            target,
            "Private mail",
            body,
        )
        .await
    }

    /// Persists a mailbox message with a caller-provided inbox subject.
    pub async fn save_mail_message_with_subject(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target: &str,
        subject: &str,
        body: &str,
    ) -> Result<StoredInboxItem, StorageError> {
        self.save_mail_message_to_principal(
            sender_user,
            sender_player_id,
            target,
            target,
            subject,
            body,
        )
        .await
    }

    /// Persists a mailbox message with an explicit recipient principal.
    pub async fn save_mail_message_to_principal(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        recipient_user: &str,
        recipient_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<StoredInboxItem, StorageError> {
        self.insert_mail_world_message(
            sender_user,
            sender_player_id,
            recipient_user,
            recipient_player_id,
            body,
        )
        .await?;
        let inbox_item = self
            .create_mail_inbox_item(
                sender_user,
                sender_player_id,
                recipient_user,
                recipient_player_id,
                subject,
                body,
            )
            .await?;
        self.record_mail_memory(
            sender_user,
            sender_player_id,
            recipient_user,
            recipient_player_id,
            subject,
            body,
            inbox_item.id,
        )
        .await?;
        Ok(inbox_item)
    }

    async fn insert_mail_world_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        recipient_user: &str,
        recipient_player_id: &str,
        body: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into world_messages (
                kind, sender_user, sender_player_id, target_user, target_player_id, body
            )
            values ('mail', $1, $2, $3, $4, $5)
            "#,
        )
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(recipient_user)
        .bind(recipient_player_id)
        .bind(body)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn create_mail_inbox_item(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        recipient_user: &str,
        recipient_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<StoredInboxItem, StorageError> {
        self.create_inbox_item(NewInboxItem {
            kind: "mail",
            recipient_user,
            recipient_player_id,
            sender_user,
            sender_player_id,
            subject,
            body,
            source_kind: None,
            source_id: None,
            payload: json!({ "target": recipient_user }),
        })
        .await
    }

    async fn record_mail_memory(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        recipient_user: &str,
        recipient_player_id: &str,
        subject: &str,
        body: &str,
        inbox_item_id: i64,
    ) -> Result<(), StorageError> {
        let sent_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: sender_player_id.to_owned(),
                source: "mail".to_owned(),
                event_type: "mail_sent".to_owned(),
                actors: json!([sender_user, recipient_user]),
                content: format!("Sent mail to {recipient_user}: {body}"),
                world_refs: json!({
                    "kind": "mail",
                    "inbox_item_id": inbox_item_id,
                    "subject": subject,
                    "target_user": recipient_user
                }),
                salience: 0.6,
            })
            .await?;

        let received_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: recipient_player_id.to_owned(),
                source: "mail".to_owned(),
                event_type: "mail_received".to_owned(),
                actors: json!([sender_user, recipient_user]),
                content: format!("Received mail from {sender_user}: {body}"),
                world_refs: json!({
                    "kind": "mail",
                    "inbox_item_id": inbox_item_id,
                    "subject": subject,
                    "sender_user": sender_user
                }),
                salience: 0.7,
            })
            .await?;

        let sent_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: sender_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: recipient_user.to_owned(),
                predicate: "messaged".to_owned(),
                object: json!({
                    "direction": "sent",
                    "subject": subject,
                    "last_body": body
                }),
                summary: format!("Sent mail to {recipient_user}: {body}"),
                evidence_event_ids: vec![sent_event.id],
                confidence: 0.7,
                importance: 0.6,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            sender_player_id,
            recipient_player_id,
            sent_atom.id,
            Some("mail_contact"),
        )
        .await?;

        let received_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: recipient_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: sender_user.to_owned(),
                predicate: "messaged".to_owned(),
                object: json!({
                    "direction": "received",
                    "subject": subject,
                    "last_body": body
                }),
                summary: format!("Received mail from {sender_user}: {body}"),
                evidence_event_ids: vec![received_event.id],
                confidence: 0.75,
                importance: 0.7,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            recipient_player_id,
            sender_player_id,
            received_atom.id,
            Some("mail_contact"),
        )
        .await?;
        Ok(())
    }

    /// Persists a same-view say message with a 24 hour expiry.
    pub async fn save_say_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        target_view: &str,
        body: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into world_messages (
                kind, sender_user, sender_player_id, target_view, body, expires_at
            )
            values ('say', $1, $2, $3, $4, now() + interval '24 hours')
            "#,
        )
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(target_view)
        .bind(body)
        .execute(&self.pool)
        .await?;

        self.append_memory_event(NewMemoryEvent {
            agent_id: sender_player_id.to_owned(),
            source: "chat".to_owned(),
            event_type: "say_message_sent".to_owned(),
            actors: json!([sender_user]),
            content: format!("Said in {target_view}: {body}"),
            world_refs: json!({
                "kind": "say",
                "target_view": target_view
            }),
            salience: 0.35,
        })
        .await?;

        Ok(())
    }

    /// Persists a broadcast message. Broadcasts are kept permanently.
    pub async fn save_broadcast_message(
        &self,
        sender_user: &str,
        sender_player_id: &str,
        body: &str,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            insert into world_messages (
                kind, sender_user, sender_player_id, body
            )
            values ('broadcast', $1, $2, $3)
            "#,
        )
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(body)
        .execute(&self.pool)
        .await?;

        self.append_memory_event(NewMemoryEvent {
            agent_id: sender_player_id.to_owned(),
            source: "broadcast".to_owned(),
            event_type: "broadcast_sent".to_owned(),
            actors: json!([sender_user]),
            content: format!("Broadcast: {body}"),
            world_refs: json!({
                "kind": "broadcast"
            }),
            salience: 0.75,
        })
        .await?;

        Ok(())
    }

    /// Loads recent mailbox messages for a player identity.
    pub async fn recent_mailbox_messages(
        &self,
        username: &str,
        player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredWorldMessage>, StorageError> {
        let messages = sqlx::query_as::<_, StoredWorldMessage>(
            r#"
            select
                kind,
                sender_user,
                coalesce(target_user, '') as target_user,
                body,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from world_messages
            where kind = 'mail'
              and (target_user = $1 or target_player_id = $2)
            order by created_at desc
            limit $3
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Loads recent unexpired say messages for a view.
    pub async fn recent_view_messages(
        &self,
        view_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredWorldMessage>, StorageError> {
        let messages = sqlx::query_as::<_, StoredWorldMessage>(
            r#"
            select
                kind,
                sender_user,
                coalesce(target_user, '') as target_user,
                body,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from world_messages
            where kind = 'say'
              and target_view = $1
              and expires_at > now()
            order by created_at desc
            limit $2
            "#,
        )
        .bind(view_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Loads recent broadcast messages for news.
    pub async fn recent_news_messages(
        &self,
        limit: i64,
    ) -> Result<Vec<StoredWorldMessage>, StorageError> {
        let messages = sqlx::query_as::<_, StoredWorldMessage>(
            r#"
            select
                kind,
                sender_user,
                coalesce(target_user, '') as target_user,
                body,
                to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                to_char(expires_at, 'YYYY-MM-DD HH24:MI:SS TZ') as expires_at
            from world_messages
            where kind = 'broadcast'
            order by created_at desc
            limit $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Creates or returns a persistent actionable inbox item.
    pub async fn create_inbox_item(
        &self,
        item: NewInboxItem<'_>,
    ) -> Result<StoredInboxItem, StorageError> {
        let row = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            insert into inbox_items (
                kind, recipient_user, recipient_player_id,
                sender_user, sender_player_id, subject, body,
                source_kind, source_id, payload
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            on conflict (source_kind, source_id, recipient_player_id)
            do update set updated_at = inbox_items.updated_at
            returning id, kind, recipient_user, recipient_player_id,
                      sender_user, sender_player_id, subject, body, status, attempts,
                      to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(item.kind)
        .bind(item.recipient_user)
        .bind(item.recipient_player_id)
        .bind(item.sender_user)
        .bind(item.sender_player_id)
        .bind(item.subject)
        .bind(item.body)
        .bind(item.source_kind)
        .bind(item.source_id)
        .bind(item.payload)
        .fetch_one(&self.pool)
        .await?;
        if row.kind == "mail" {
            sqlx::query("select pg_notify('hinemos_inbox_mail', $1)")
                .bind(row.id.to_string())
                .execute(&self.pool)
                .await?;
        }
        Ok(row)
    }

    /// Lists actionable inbox items for a player.
    pub async fn list_inbox_items(
        &self,
        username: &str,
        player_id: &str,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredInboxItem>, StorageError> {
        let status = status.unwrap_or(INBOX_FILTER_OPEN);
        if !matches!(
            status,
            INBOX_FILTER_OPEN
                | INBOX_FILTER_UNREAD
                | INBOX_FILTER_CLAIMED
                | INBOX_FILTER_DONE
                | INBOX_FILTER_ALL
        ) {
            return Err(StorageError::InvalidInboxFilter(status.to_owned()));
        }
        let items = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            select id, kind, recipient_user, recipient_player_id,
                   sender_user, sender_player_id, subject, body, status, attempts,
                   to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from inbox_items
            where (recipient_user = $1 or recipient_player_id = $2)
              and (
                    ($3 = $5 and status in ($6, $7))
                 or ($3 = $8 and status = $6)
                 or ($3 = $9 and status = $7)
                 or ($3 = $10 and status in ($11, $12))
                 or ($3 = $13)
              )
            order by id desc
            limit $4
            "#,
        )
        .bind(username)
        .bind(player_id)
        .bind(status)
        .bind(limit)
        .bind(INBOX_FILTER_OPEN)
        .bind(INBOX_STATUS_UNREAD)
        .bind(INBOX_STATUS_CLAIMED)
        .bind(INBOX_FILTER_UNREAD)
        .bind(INBOX_FILTER_CLAIMED)
        .bind(INBOX_FILTER_DONE)
        .bind(INBOX_STATUS_ACKED)
        .bind(INBOX_STATUS_ARCHIVED)
        .bind(INBOX_FILTER_ALL)
        .fetch_all(&self.pool)
        .await?;
        Ok(items)
    }

    /// Reads one inbox item visible to a player.
    pub async fn read_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<StoredInboxItem, StorageError> {
        self.inbox_item_for_player(username, player_id, item_id)
            .await
    }

    /// Reads one inbox item by id.
    pub async fn inbox_item(&self, item_id: i64) -> Result<StoredInboxItem, StorageError> {
        let item = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            select id, kind, recipient_user, recipient_player_id,
                   sender_user, sender_player_id, subject, body, status, attempts,
                   to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from inbox_items
            where id = $1
            "#,
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::InboxItemNotFound(item_id))?;
        Ok(item)
    }

    /// Reads one inbox item by its idempotent source.
    pub async fn inbox_item_by_source(
        &self,
        recipient_player_id: &str,
        source_kind: &str,
        source_id: i64,
    ) -> Result<StoredInboxItem, StorageError> {
        let item = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            select id, kind, recipient_user, recipient_player_id,
                   sender_user, sender_player_id, subject, body, status, attempts,
                   to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from inbox_items
            where recipient_player_id = $1
              and source_kind = $2
              and source_id = $3
            "#,
        )
        .bind(recipient_player_id)
        .bind(source_kind)
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::InboxItemNotFound(source_id))?;
        Ok(item)
    }

    /// Claims an inbox item for processing with a short lease.
    pub async fn claim_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<StoredInboxItem, StorageError> {
        let item = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            update inbox_items
            set status = 'claimed',
                attempts = attempts + 1,
                lease_until = now() + interval '5 minutes',
                updated_at = now()
            where id = $1
              and (recipient_user = $2 or recipient_player_id = $3)
              and status in ('unread', 'claimed')
            returning id, kind, recipient_user, recipient_player_id,
                      sender_user, sender_player_id, subject, body, status, attempts,
                      to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(item_id)
        .bind(username)
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::InboxItemNotFound(item_id))?;
        Ok(item)
    }

    /// Acknowledges or archives an inbox item after processing.
    pub async fn finish_inbox_item(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
        status: &str,
    ) -> Result<StoredInboxItem, StorageError> {
        let item = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            update inbox_items
            set status = $4,
                lease_until = null,
                updated_at = now()
            where id = $1
              and (recipient_user = $2 or recipient_player_id = $3)
              and $4 in ('acked', 'archived')
            returning id, kind, recipient_user, recipient_player_id,
                      sender_user, sender_player_id, subject, body, status, attempts,
                      to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(item_id)
        .bind(username)
        .bind(player_id)
        .bind(status)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::InboxItemNotFound(item_id))?;
        Ok(item)
    }

    async fn inbox_item_for_player(
        &self,
        username: &str,
        player_id: &str,
        item_id: i64,
    ) -> Result<StoredInboxItem, StorageError> {
        let item = sqlx::query_as::<_, StoredInboxItem>(
            r#"
            select id, kind, recipient_user, recipient_player_id,
                   sender_user, sender_player_id, subject, body, status, attempts,
                   to_char(lease_until, 'YYYY-MM-DD HH24:MI:SS TZ') as lease_until,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from inbox_items
            where id = $1
              and (recipient_user = $2 or recipient_player_id = $3)
            "#,
        )
        .bind(item_id)
        .bind(username)
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::InboxItemNotFound(item_id))?;
        Ok(item)
    }
}

/// New inbox item input.
pub struct NewInboxItem<'a> {
    /// Kind such as mail, shop_command, or payment_request.
    pub kind: &'a str,
    /// Recipient user.
    pub recipient_user: &'a str,
    /// Recipient player id.
    pub recipient_player_id: &'a str,
    /// Sender user.
    pub sender_user: &'a str,
    /// Sender player id.
    pub sender_player_id: &'a str,
    /// Short subject.
    pub subject: &'a str,
    /// Full body.
    pub body: &'a str,
    /// Optional source kind for idempotency.
    pub source_kind: Option<&'a str>,
    /// Optional source id for idempotency.
    pub source_id: Option<i64>,
    /// Structured payload.
    pub payload: serde_json::Value,
}
