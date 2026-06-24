use sqlx::Row;

use crate::accounts::{
    debit_balance, ensure_balance_row, ensure_player_account, fetch_balance_tx, player_account_id,
    resolve_payment_target,
};
use crate::{NewInboxItem, PgStorage, StorageError, StoredMarriageCertificate, TEST_CURRENCY};

const REGISTRY_USER: &str = "room-hinemos_registry";
const REGISTRY_PLAYER_ID: &str = "room:hinemos_registry";
const REGISTRY_ACCOUNT_ID: &str = "room:hinemos_registry";

impl PgStorage {
    /// Registers a marriage when both players are recently present in the registry room.
    pub async fn register_marriage(
        &self,
        requester_user: &str,
        requester_player_id: &str,
        target: &str,
        fee_amount: i64,
        registry_view_id: &str,
    ) -> Result<StoredMarriageCertificate, StorageError> {
        if fee_amount <= 0 {
            return Err(StorageError::InvalidAmount(fee_amount));
        }

        let mut tx = self.pool.begin().await?;
        let target = resolve_payment_target(&mut tx, target).await?;
        if target.player_id == requester_player_id {
            return Err(StorageError::SelfMarriage);
        }

        require_recent_presence(
            &mut tx,
            requester_player_id,
            registry_view_id,
            requester_user,
        )
        .await?;
        require_recent_presence(
            &mut tx,
            &target.player_id,
            registry_view_id,
            &target.username,
        )
        .await?;
        ensure_no_active_marriage(&mut tx, requester_player_id, &target.player_id).await?;

        let requester_account_id = player_account_id(requester_player_id);
        let target_account_id = player_account_id(&target.player_id);
        ensure_player_account(
            &mut tx,
            &requester_account_id,
            requester_user,
            requester_player_id,
        )
        .await?;
        ensure_player_account(
            &mut tx,
            &target_account_id,
            &target.username,
            &target.player_id,
        )
        .await?;
        ensure_registry_account(&mut tx).await?;
        ensure_balance_row(&mut tx, &requester_account_id).await?;
        ensure_balance_row(&mut tx, &target_account_id).await?;
        ensure_balance_row(&mut tx, REGISTRY_ACCOUNT_ID).await?;

        debit_balance(&mut tx, &requester_account_id, fee_amount).await?;
        credit_registry_balance(&mut tx, fee_amount).await?;
        let requester_ledger_id = insert_fee_ledger(
            &mut tx,
            &requester_account_id,
            fee_amount,
            &format!(
                "Marriage registry fee for {requester_user} and {}",
                target.username
            ),
        )
        .await?;

        debit_balance(&mut tx, &target_account_id, fee_amount).await?;
        credit_registry_balance(&mut tx, fee_amount).await?;
        let target_ledger_id = insert_fee_ledger(
            &mut tx,
            &target_account_id,
            fee_amount,
            &format!(
                "Marriage registry fee for {requester_user} and {}",
                target.username
            ),
        )
        .await?;

        let ((party_a_user, party_a_player_id), (party_b_user, party_b_player_id)) =
            canonical_parties(
                (requester_user.to_owned(), requester_player_id.to_owned()),
                (target.username.clone(), target.player_id.clone()),
            );
        let certificate_text = render_certificate(
            &party_a_user,
            &party_b_user,
            fee_amount,
            requester_ledger_id,
            target_ledger_id,
        );
        let certificate = sqlx::query_as::<_, StoredMarriageCertificate>(
            r#"
            insert into marriage_certificates (
                party_a_user, party_a_player_id, party_b_user, party_b_player_id,
                status, fee_amount, fee_ledger_ids, certificate_text
            )
            values ($1, $2, $3, $4, 'active', $5, $6, $7)
            returning id, party_a_user, party_a_player_id, party_b_user, party_b_player_id,
                      status, fee_amount, fee_ledger_ids, certificate_text,
                      to_char(issued_at, 'YYYY-MM-DD HH24:MI:SS TZ') as issued_at
            "#,
        )
        .bind(&party_a_user)
        .bind(&party_a_player_id)
        .bind(&party_b_user)
        .bind(&party_b_player_id)
        .bind(fee_amount)
        .bind(vec![requester_ledger_id, target_ledger_id])
        .bind(&certificate_text)
        .fetch_one(&mut *tx)
        .await?;

        create_certificate_inbox_item(&mut tx, &certificate, requester_user, requester_player_id)
            .await?;
        create_certificate_inbox_item(&mut tx, &certificate, &target.username, &target.player_id)
            .await?;
        insert_certificate_participant(
            &mut tx,
            certificate.id,
            requester_player_id,
            requester_user,
        )
        .await?;
        insert_certificate_participant(
            &mut tx,
            certificate.id,
            &target.player_id,
            &target.username,
        )
        .await?;

        let _requester_balance = fetch_balance_tx(&mut tx, &requester_account_id).await?;
        let _target_balance = fetch_balance_tx(&mut tx, &target_account_id).await?;
        tx.commit().await?;
        Ok(certificate)
    }

    /// Loads the active marriage certificate for a player.
    pub async fn current_marriage_certificate(
        &self,
        player_id: &str,
    ) -> Result<Option<StoredMarriageCertificate>, StorageError> {
        sqlx::query_as::<_, StoredMarriageCertificate>(
            r#"
            select id, party_a_user, party_a_player_id, party_b_user, party_b_player_id,
                   status, fee_amount, fee_ledger_ids, certificate_text,
                   to_char(issued_at, 'YYYY-MM-DD HH24:MI:SS TZ') as issued_at
            from marriage_certificates
            where status = 'active'
              and (party_a_player_id = $1 or party_b_player_id = $1)
            order by id desc
            limit 1
            "#,
        )
        .bind(player_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from)
    }

    /// Dissolves the active marriage certificate for a player and notifies both parties.
    pub async fn divorce_marriage(
        &self,
        requester_user: &str,
        requester_player_id: &str,
    ) -> Result<StoredMarriageCertificate, StorageError> {
        let mut tx = self.pool.begin().await?;
        let active = sqlx::query_as::<_, StoredMarriageCertificate>(
            r#"
            select id, party_a_user, party_a_player_id, party_b_user, party_b_player_id,
                   status, fee_amount, fee_ledger_ids, certificate_text,
                   to_char(issued_at, 'YYYY-MM-DD HH24:MI:SS TZ') as issued_at
            from marriage_certificates
            where status = 'active'
              and (party_a_player_id = $1 or party_b_player_id = $1)
            order by id desc
            limit 1
            for update
            "#,
        )
        .bind(requester_player_id)
        .fetch_optional(&mut *tx)
        .await?;
        let active = match active {
            Some(active) => active,
            None => return Err(StorageError::NoActiveMarriage(requester_user.to_owned())),
        };

        let certificate = sqlx::query_as::<_, StoredMarriageCertificate>(
            r#"
            update marriage_certificates
            set status = 'divorced',
                divorced_at = now()
            where id = $1
              and status = 'active'
            returning id, party_a_user, party_a_player_id, party_b_user, party_b_player_id,
                      status, fee_amount, fee_ledger_ids, certificate_text,
                      to_char(issued_at, 'YYYY-MM-DD HH24:MI:SS TZ') as issued_at
            "#,
        )
        .bind(active.id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            update marriage_certificate_participants
            set status = 'divorced'
            where certificate_id = $1
            "#,
        )
        .bind(certificate.id)
        .execute(&mut *tx)
        .await?;

        create_divorce_inbox_item(
            &mut tx,
            &certificate,
            &certificate.party_a_user,
            &certificate.party_a_player_id,
            requester_user,
        )
        .await?;
        create_divorce_inbox_item(
            &mut tx,
            &certificate,
            &certificate.party_b_user,
            &certificate.party_b_player_id,
            requester_user,
        )
        .await?;
        tx.commit().await?;
        Ok(certificate)
    }
}

async fn insert_certificate_participant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    certificate_id: i64,
    player_id: &str,
    username: &str,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        insert into marriage_certificate_participants (
            certificate_id, player_id, username, status
        )
        values ($1, $2, $3, 'active')
        "#,
    )
    .bind(certificate_id)
    .bind(player_id)
    .bind(username)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn require_recent_presence(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    player_id: &str,
    view_id: &str,
    label: &str,
) -> Result<(), StorageError> {
    let present = sqlx::query(
        r#"
        select 1
        from view_presence
        where player_id = $1
          and view_id = $2
          and last_seen_at >= now() - interval '2 minutes'
        limit 1
        "#,
    )
    .bind(player_id)
    .bind(view_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some();
    if !present {
        return Err(StorageError::MarriagePartnerNotPresent(label.to_owned()));
    }
    Ok(())
}

async fn ensure_no_active_marriage(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    left_player_id: &str,
    right_player_id: &str,
) -> Result<(), StorageError> {
    let existing = sqlx::query(
        r#"
        select party_a_user, party_b_user
        from marriage_certificates
        where status = 'active'
          and (
              party_a_player_id = $1 or party_b_player_id = $1
              or party_a_player_id = $2 or party_b_player_id = $2
          )
        for update
        limit 1
        "#,
    )
    .bind(left_player_id)
    .bind(right_player_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = existing {
        return Err(StorageError::MarriageAlreadyActive(format!(
            "{} or {}",
            row.get::<String, _>("party_a_user"),
            row.get::<String, _>("party_b_user")
        )));
    }
    Ok(())
}

async fn ensure_registry_account(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        insert into world_accounts (account_id, kind, owner_id, display_name)
        values ($1, 'room', $2, 'Hinemos Registry Office')
        on conflict (account_id) do update
        set display_name = excluded.display_name
        "#,
    )
    .bind(REGISTRY_ACCOUNT_ID)
    .bind(REGISTRY_PLAYER_ID)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn credit_registry_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    amount: i64,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        update world_balances
        set amount = amount + $2,
            updated_at = now()
        where account_id = $1 and asset = 'MARK'
        "#,
    )
    .bind(REGISTRY_ACCOUNT_ID)
    .bind(amount)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_fee_ledger(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    payer_account_id: &str,
    amount: i64,
    memo: &str,
) -> Result<i64, StorageError> {
    let ledger_id = sqlx::query(
        r#"
        insert into world_ledger_entries (
            asset, debit_account_id, credit_account_id, amount, reason, memo
        )
        values ('MARK', $1, $2, $3, 'marriage_registration_fee', $4)
        returning id
        "#,
    )
    .bind(payer_account_id)
    .bind(REGISTRY_ACCOUNT_ID)
    .bind(amount)
    .bind(memo)
    .fetch_one(&mut **tx)
    .await?
    .get::<i64, _>("id");
    Ok(ledger_id)
}

async fn create_certificate_inbox_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    certificate: &StoredMarriageCertificate,
    recipient_user: &str,
    recipient_player_id: &str,
) -> Result<(), StorageError> {
    let subject = format!("Marriage certificate #{}", certificate.id);
    let item = NewInboxItem {
        kind: "marriage_certificate",
        recipient_user,
        recipient_player_id,
        sender_user: REGISTRY_USER,
        sender_player_id: REGISTRY_PLAYER_ID,
        subject: &subject,
        body: &certificate.certificate_text,
        source_kind: Some("marriage_certificate"),
        source_id: Some(certificate.id),
        payload: serde_json::json!({
            "certificateId": certificate.id,
            "partyA": certificate.party_a_user,
            "partyB": certificate.party_b_user,
            "feeAmount": certificate.fee_amount,
            "asset": TEST_CURRENCY
        }),
    };
    sqlx::query(
        r#"
        insert into inbox_items (
            kind, recipient_user, recipient_player_id, sender_user, sender_player_id,
            subject, body, source_kind, source_id, payload
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        on conflict (source_kind, source_id, recipient_player_id) do nothing
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn create_divorce_inbox_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    certificate: &StoredMarriageCertificate,
    recipient_user: &str,
    recipient_player_id: &str,
    requester_user: &str,
) -> Result<(), StorageError> {
    let subject = format!("Marriage dissolved #{}", certificate.id);
    let body = format!(
        "Hinemos Registry Office Divorce Notice\n\
         Certificate: #{}\n\
         Parties: {} and {}\n\
         Filed by: {}\n\
         Status: divorced",
        certificate.id, certificate.party_a_user, certificate.party_b_user, requester_user
    );
    let item = NewInboxItem {
        kind: "marriage_divorce",
        recipient_user,
        recipient_player_id,
        sender_user: REGISTRY_USER,
        sender_player_id: REGISTRY_PLAYER_ID,
        subject: &subject,
        body: &body,
        source_kind: Some("marriage_divorce"),
        source_id: Some(certificate.id),
        payload: serde_json::json!({
            "certificateId": certificate.id,
            "partyA": certificate.party_a_user,
            "partyB": certificate.party_b_user,
            "filedBy": requester_user,
            "status": certificate.status
        }),
    };
    sqlx::query(
        r#"
        insert into inbox_items (
            kind, recipient_user, recipient_player_id, sender_user, sender_player_id,
            subject, body, source_kind, source_id, payload
        )
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        on conflict (source_kind, source_id, recipient_player_id) do nothing
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn canonical_parties(
    left: (String, String),
    right: (String, String),
) -> ((String, String), (String, String)) {
    if left.1 <= right.1 {
        (left, right)
    } else {
        (right, left)
    }
}

fn render_certificate(
    party_a_user: &str,
    party_b_user: &str,
    fee_amount: i64,
    first_ledger_id: i64,
    second_ledger_id: i64,
) -> String {
    format!(
        "Hinemos Registry Office Marriage Certificate\n\
         Parties: {party_a_user} and {party_b_user}\n\
         Fee: {fee_amount} MARK each\n\
         Ledger entries: #{first_ledger_id}, #{second_ledger_id}\n\
         Status: active"
    )
}
