use serde_json::json;
use sqlx::Row;

use crate::accounts::{
    credit_balance, debit_balance, ensure_balance_row, ensure_player_account, fetch_balance_tx,
    player_account_id,
};
use crate::{
    NewInboxItem, NewMemoryAtom, NewMemoryEvent, OPERATOR_COMMAND_STATUS_HANDLED,
    PAYMENT_REQUEST_STATUS_PAID, PAYMENT_REQUEST_STATUS_PENDING, PgStorage, StorageError,
    StoredOperatorCommand, StoredPaymentRequest,
};

impl PgStorage {
    /// Creates a payment request from a shop command owned by the operator.
    pub async fn create_payment_request(
        &self,
        operator_command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<StoredPaymentRequest, StorageError> {
        if amount <= 0 {
            return Err(StorageError::InvalidAmount(amount));
        }
        let request = self
            .insert_payment_request(operator_command_id, owner_player_id, amount, delivery)
            .await?;
        self.create_payment_request_inbox_item(&request).await?;
        self.record_payment_request_created_memory(&request).await?;
        Ok(request)
    }

    async fn insert_payment_request(
        &self,
        operator_command_id: i64,
        owner_player_id: &str,
        amount: i64,
        delivery: &str,
    ) -> Result<StoredPaymentRequest, StorageError> {
        let mut tx = self.pool.begin().await?;
        let command = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            select id, view_id, parcel_id, sender_user, sender_player_id,
                   owner_user, owner_player_id, raw_input, status,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from operator_commands
            where id = $1
            for update
            "#,
        )
        .bind(operator_command_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StorageError::OperatorCommandNotFound(operator_command_id))?;

        if command.owner_player_id != owner_player_id {
            return Err(StorageError::NotParcelOwner(command.parcel_id));
        }

        let memo = format!("shop command #{}", command.id);
        let request = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            insert into payment_requests (
                operator_command_id, parcel_id, payer_user, payer_player_id,
                payee_user, payee_player_id, asset, amount, memo, delivery
            )
            values ($1, $2, $3, $4, $5, $6, 'MARK', $7, $8, $9)
            returning id, operator_command_id, parcel_id, payer_user, payer_player_id,
                      payee_user, payee_player_id, asset, amount, memo, delivery,
                      status, ledger_id,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(command.id)
        .bind(command.parcel_id)
        .bind(command.sender_user)
        .bind(command.sender_player_id)
        .bind(command.owner_user)
        .bind(command.owner_player_id)
        .bind(amount)
        .bind(memo)
        .bind(delivery)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            update operator_commands
            set status = $2
            where id = $1
            "#,
        )
        .bind(command.id)
        .bind(OPERATOR_COMMAND_STATUS_HANDLED)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(request)
    }

    async fn create_payment_request_inbox_item(
        &self,
        request: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        let subject = format!("Payment request #{}", request.id);
        let body = format!(
            "{} requests {} {} for shop command #{} in {}. Accept with /pay accept {}.",
            request.payee_user,
            request.amount,
            request.asset,
            request.operator_command_id,
            request.parcel_id,
            request.id
        );
        self.create_inbox_item(NewInboxItem {
            kind: "payment_request",
            recipient_user: &request.payer_user,
            recipient_player_id: &request.payer_player_id,
            sender_user: &request.payee_user,
            sender_player_id: &request.payee_player_id,
            subject: &subject,
            body: &body,
            source_kind: Some("payment_request"),
            source_id: Some(request.id),
            payload: json!({
                "requestId": request.id,
                "operatorCommandId": request.operator_command_id,
                "parcelId": request.parcel_id,
                "amount": request.amount,
                "asset": request.asset
            }),
        })
        .await?;
        Ok(())
    }

    async fn record_payment_request_created_memory(
        &self,
        request: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        self.record_created_request_payee_memory(request).await?;
        self.record_received_request_payer_memory(request).await?;
        Ok(())
    }

    async fn record_created_request_payee_memory(
        &self,
        request: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        let payee_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: request.payee_player_id.clone(),
                source: "shop".to_owned(),
                event_type: "payment_request_created".to_owned(),
                actors: json!([request.payee_user, request.payer_user]),
                content: format!(
                    "Requested {} {} from {} for shop command #{}.",
                    request.amount, request.asset, request.payer_user, request.operator_command_id
                ),
                world_refs: json!({
                    "kind": "payment_request",
                    "request_id": request.id,
                    "operator_command_id": request.operator_command_id,
                    "parcel_id": request.parcel_id,
                    "payer_user": request.payer_user
                }),
                salience: 0.8,
            })
            .await?;
        let payee_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: request.payee_player_id.clone(),
                kind: "commitment".to_owned(),
                subject: request.payer_user.clone(),
                predicate: format!("payment_requested:{}", request.id),
                object: json!({
                    "request_id": request.id,
                    "amount": request.amount,
                    "asset": request.asset,
                    "status": request.status,
                    "delivery": request.delivery
                }),
                summary: format!(
                    "{} owes {} {} for payment request #{}.",
                    request.payer_user, request.amount, request.asset, request.id
                ),
                evidence_event_ids: vec![payee_event.id],
                confidence: 0.95,
                importance: 0.8,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            &request.payee_player_id,
            &request.payer_user,
            payee_atom.id,
            Some("payment_counterparty"),
        )
        .await?;
        Ok(())
    }

    async fn record_received_request_payer_memory(
        &self,
        request: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        let payer_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: request.payer_player_id.clone(),
                source: "shop".to_owned(),
                event_type: "payment_request_received".to_owned(),
                actors: json!([request.payee_user, request.payer_user]),
                content: format!(
                    "Received payment request #{} from {} for {} {}.",
                    request.id, request.payee_user, request.amount, request.asset
                ),
                world_refs: json!({
                    "kind": "payment_request",
                    "request_id": request.id,
                    "operator_command_id": request.operator_command_id,
                    "parcel_id": request.parcel_id,
                    "payee_user": request.payee_user
                }),
                salience: 0.85,
            })
            .await?;
        let payer_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: request.payer_player_id.clone(),
                kind: "commitment".to_owned(),
                subject: request.payee_user.clone(),
                predicate: format!("payment_due:{}", request.id),
                object: json!({
                    "request_id": request.id,
                    "amount": request.amount,
                    "asset": request.asset,
                    "status": request.status,
                    "delivery": request.delivery
                }),
                summary: format!(
                    "I owe {} {} to {} for payment request #{}.",
                    request.amount, request.asset, request.payee_user, request.id
                ),
                evidence_event_ids: vec![payer_event.id],
                confidence: 0.95,
                importance: 0.85,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            &request.payer_player_id,
            &request.payee_user,
            payer_atom.id,
            Some("payment_counterparty"),
        )
        .await?;
        Ok(())
    }

    /// Loads pending payment requests for a player.
    pub async fn pending_payment_requests(
        &self,
        payer_player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredPaymentRequest>, StorageError> {
        let requests = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            select id, operator_command_id, parcel_id, payer_user, payer_player_id,
                   payee_user, payee_player_id, asset, amount, memo, delivery,
                   status, ledger_id,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from payment_requests
            where payer_player_id = $1
              and status = $3
            order by id desc
            limit $2
            "#,
        )
        .bind(payer_player_id)
        .bind(limit)
        .bind(PAYMENT_REQUEST_STATUS_PENDING)
        .fetch_all(&self.pool)
        .await?;
        Ok(requests)
    }

    /// Accepts a pending payment request, transfers MARK, and returns the paid request.
    pub async fn accept_payment_request(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<(StoredPaymentRequest, i64), StorageError> {
        let (paid, sender_balance) = self
            .execute_payment_request_acceptance(payer_user, payer_player_id, request_id)
            .await?;
        self.record_payment_request_paid_memory(payer_user, payer_player_id, &paid)
            .await?;
        Ok((paid, sender_balance))
    }

    async fn execute_payment_request_acceptance(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        request_id: i64,
    ) -> Result<(StoredPaymentRequest, i64), StorageError> {
        let mut tx = self.pool.begin().await?;
        let request = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            select id, operator_command_id, parcel_id, payer_user, payer_player_id,
                   payee_user, payee_player_id, asset, amount, memo, delivery,
                   status, ledger_id,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from payment_requests
            where id = $1
            for update
            "#,
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StorageError::PaymentRequestNotFound(request_id))?;

        if request.payer_player_id != payer_player_id {
            return Err(StorageError::PaymentRequestForbidden(request_id));
        }
        if request.status != PAYMENT_REQUEST_STATUS_PENDING {
            return Err(StorageError::PaymentRequestNotPending(request_id));
        }

        let sender_account_id = player_account_id(payer_player_id);
        let target_account_id = player_account_id(&request.payee_player_id);
        ensure_player_account(&mut tx, &sender_account_id, payer_user, payer_player_id).await?;
        ensure_player_account(
            &mut tx,
            &target_account_id,
            &request.payee_user,
            &request.payee_player_id,
        )
        .await?;
        ensure_balance_row(&mut tx, &sender_account_id).await?;
        ensure_balance_row(&mut tx, &target_account_id).await?;

        debit_balance(&mut tx, &sender_account_id, request.amount).await?;
        credit_balance(&mut tx, &target_account_id, request.amount).await?;
        let ledger_id = sqlx::query(
            r#"
            insert into world_ledger_entries (
                asset, debit_account_id, credit_account_id, amount, reason, memo
            )
            values ('MARK', $1, $2, $3, 'shop_payment_request', $4)
            returning id
            "#,
        )
        .bind(&sender_account_id)
        .bind(&target_account_id)
        .bind(request.amount)
        .bind(&request.memo)
        .fetch_one(&mut *tx)
        .await?
        .get::<i64, _>("id");

        let paid = sqlx::query_as::<_, StoredPaymentRequest>(
            r#"
            update payment_requests
            set status = $3,
                ledger_id = $2,
                paid_at = now()
            where id = $1
            returning id, operator_command_id, parcel_id, payer_user, payer_player_id,
                      payee_user, payee_player_id, asset, amount, memo, delivery,
                      status, ledger_id,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(request_id)
        .bind(ledger_id)
        .bind(PAYMENT_REQUEST_STATUS_PAID)
        .fetch_one(&mut *tx)
        .await?;
        let sender_balance = fetch_balance_tx(&mut tx, &sender_account_id).await?.amount;
        tx.commit().await?;
        Ok((paid, sender_balance))
    }

    async fn record_payment_request_paid_memory(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        paid: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        self.record_paid_request_payer_memory(payer_user, payer_player_id, paid)
            .await?;
        self.record_collected_request_payee_memory(payer_user, paid)
            .await?;
        Ok(())
    }

    async fn record_paid_request_payer_memory(
        &self,
        payer_user: &str,
        payer_player_id: &str,
        paid: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        let payer_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: payer_player_id.to_owned(),
                source: "trade".to_owned(),
                event_type: "payment_request_paid".to_owned(),
                actors: json!([payer_user, paid.payee_user]),
                content: format!(
                    "Paid payment request #{}: {} {} to {}.",
                    paid.id, paid.amount, paid.asset, paid.payee_user
                ),
                world_refs: json!({
                    "kind": "payment_request",
                    "request_id": paid.id,
                    "ledger_id": paid.ledger_id,
                    "operator_command_id": paid.operator_command_id,
                    "parcel_id": paid.parcel_id
                }),
                salience: 0.9,
            })
            .await?;
        let payer_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: payer_player_id.to_owned(),
                kind: "social".to_owned(),
                subject: paid.payee_user.clone(),
                predicate: "paid_request".to_owned(),
                object: json!({
                    "request_id": paid.id,
                    "amount": paid.amount,
                    "asset": paid.asset,
                    "ledger_id": paid.ledger_id,
                    "delivery": paid.delivery
                }),
                summary: format!(
                    "Paid {} {} to {} for payment request #{}.",
                    paid.amount, paid.asset, paid.payee_user, paid.id
                ),
                evidence_event_ids: vec![payer_event.id],
                confidence: 0.98,
                importance: 0.85,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            payer_player_id,
            &paid.payee_user,
            payer_atom.id,
            Some("payment_counterparty"),
        )
        .await?;
        self.upsert_memory_atom(NewMemoryAtom {
            agent_id: payer_player_id.to_owned(),
            kind: "commitment".to_owned(),
            subject: paid.payee_user.clone(),
            predicate: format!("payment_due:{}", paid.id),
            object: json!({
                "request_id": paid.id,
                "amount": paid.amount,
                "asset": paid.asset,
                "status": paid.status,
                "ledger_id": paid.ledger_id
            }),
            summary: format!(
                "I paid {} {} to {} for payment request #{}.",
                paid.amount, paid.asset, paid.payee_user, paid.id
            ),
            evidence_event_ids: vec![payer_event.id],
            confidence: 0.98,
            importance: 0.85,
            emotional_valence: 0.0,
        })
        .await?;
        Ok(())
    }

    async fn record_collected_request_payee_memory(
        &self,
        payer_user: &str,
        paid: &StoredPaymentRequest,
    ) -> Result<(), StorageError> {
        let payee_event = self
            .append_memory_event(NewMemoryEvent {
                agent_id: paid.payee_player_id.clone(),
                source: "trade".to_owned(),
                event_type: "payment_request_collected".to_owned(),
                actors: json!([payer_user, paid.payee_user]),
                content: format!(
                    "Collected payment request #{}: {} {} from {}.",
                    paid.id, paid.amount, paid.asset, payer_user
                ),
                world_refs: json!({
                    "kind": "payment_request",
                    "request_id": paid.id,
                    "ledger_id": paid.ledger_id,
                    "operator_command_id": paid.operator_command_id,
                    "parcel_id": paid.parcel_id
                }),
                salience: 0.9,
            })
            .await?;
        let payee_atom = self
            .upsert_memory_atom(NewMemoryAtom {
                agent_id: paid.payee_player_id.clone(),
                kind: "social".to_owned(),
                subject: payer_user.to_owned(),
                predicate: "collected_payment".to_owned(),
                object: json!({
                    "request_id": paid.id,
                    "amount": paid.amount,
                    "asset": paid.asset,
                    "ledger_id": paid.ledger_id
                }),
                summary: format!(
                    "{} paid {} {} for payment request #{}.",
                    payer_user, paid.amount, paid.asset, paid.id
                ),
                evidence_event_ids: vec![payee_event.id],
                confidence: 0.98,
                importance: 0.85,
                emotional_valence: 0.0,
            })
            .await?;
        self.touch_social_edge(
            &paid.payee_player_id,
            payer_user,
            payee_atom.id,
            Some("payment_counterparty"),
        )
        .await?;
        self.upsert_memory_atom(NewMemoryAtom {
            agent_id: paid.payee_player_id.clone(),
            kind: "commitment".to_owned(),
            subject: payer_user.to_owned(),
            predicate: format!("payment_requested:{}", paid.id),
            object: json!({
                "request_id": paid.id,
                "amount": paid.amount,
                "asset": paid.asset,
                "status": paid.status,
                "ledger_id": paid.ledger_id
            }),
            summary: format!(
                "{} paid {} {} for payment request #{}.",
                payer_user, paid.amount, paid.asset, paid.id
            ),
            evidence_event_ids: vec![payee_event.id],
            confidence: 0.98,
            importance: 0.85,
            emotional_valence: 0.0,
        })
        .await?;
        Ok(())
    }
}
