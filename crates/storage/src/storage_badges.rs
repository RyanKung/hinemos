use hinemos_core::{
    SHOP_BADGE_AWARD_ACTIVE, SHOP_BADGE_AWARD_REVOKED, SHOP_BADGES_PER_PARCEL_MAX,
    shop_badge_description_is_valid, shop_badge_note_is_valid, shop_badge_slug_is_valid,
    shop_badge_title_is_valid,
};

use crate::accounts::{PaymentTarget, resolve_payment_target};
use crate::parcels::canonical_parcel_id;
use crate::{PgStorage, StorageError, StoredShopBadgeAward, StoredShopBadgeDefinition};

impl PgStorage {
    /// Creates or updates a badge definition for an owned built shop parcel.
    pub async fn create_shop_badge(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
        description: Option<&str>,
    ) -> Result<StoredShopBadgeDefinition, StorageError> {
        validate_slug(slug)?;
        validate_title(title)?;
        validate_description(description)?;
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        if self
            .shop_badge_by_parcel_slug(&parcel.parcel_id, slug)
            .await?
            .is_none()
        {
            let badge_count = self.shop_badge_count(&parcel.parcel_id).await?;
            if badge_count >= SHOP_BADGES_PER_PARCEL_MAX {
                return Err(StorageError::InvalidShopBadge(format!(
                    "badge limit reached for parcel {}; maximum is {}",
                    parcel.parcel_id, SHOP_BADGES_PER_PARCEL_MAX
                )));
            }
        }
        let row = sqlx::query_as::<_, StoredShopBadgeDefinition>(
            r#"
            insert into shop_badges (parcel_id, owner_player_id, slug, title, description)
            values ($1, $2, $3, $4, $5)
            on conflict (parcel_id, slug) do update
            set owner_player_id = excluded.owner_player_id,
                title = excluded.title,
                description = excluded.description,
                updated_at = now()
            returning id, parcel_id, owner_player_id, slug, title, description,
                      (
                          select count(*)::bigint
                          from shop_badge_awards a
                          where a.badge_id = shop_badges.id
                            and a.status = $6
                      ) as active_award_count,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                      to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            "#,
        )
        .bind(&parcel.parcel_id)
        .bind(owner_player_id)
        .bind(slug)
        .bind(title.trim())
        .bind(description.map(str::trim).filter(|value| !value.is_empty()))
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Lists badge definitions for an owned shop parcel.
    pub async fn shop_badges(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<StoredShopBadgeDefinition>, StorageError> {
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        let rows = sqlx::query_as::<_, StoredShopBadgeDefinition>(
            r#"
            select b.id, b.parcel_id, b.owner_player_id, b.slug, b.title, b.description,
                   coalesce(active_awards.count, 0)::bigint as active_award_count,
                   to_char(b.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                   to_char(b.updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from shop_badges b
            left join lateral (
                select count(*) as count
                from shop_badge_awards a
                where a.badge_id = b.id
                  and a.status = $2
            ) active_awards on true
            where b.parcel_id = $1
            order by b.updated_at desc, b.slug
            "#,
        )
        .bind(&parcel.parcel_id)
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Awards a badge from an owned shop to a target player idempotently.
    pub async fn award_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        issuer_user: &str,
        issuer_player_id: &str,
        target: &str,
        note: Option<&str>,
    ) -> Result<StoredShopBadgeAward, StorageError> {
        validate_slug(slug)?;
        validate_note(note)?;
        let badge = self
            .owned_shop_badge(parcel_id, slug, issuer_player_id)
            .await?;
        let mut tx = self.pool.begin().await?;
        let recipient = resolve_badge_target(&mut tx, target).await?;
        let existing = sqlx::query_as::<_, StoredShopBadgeAward>(&award_select_sql(
            "where a.badge_id = $1 and a.recipient_player_id = $2 and a.status = $3",
        ))
        .bind(badge.id)
        .bind(&recipient.player_id)
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(existing) = existing {
            tx.commit().await?;
            return Ok(existing);
        }
        let award_id = sqlx::query_scalar::<_, i64>(
            r#"
            insert into shop_badge_awards (
                badge_id, issuer_user, issuer_player_id, recipient_user,
                recipient_player_id, note, status, awarded_at, revoked_at, updated_at
            )
            values ($1, $2, $3, $4, $5, $6, $7, now(), null, now())
            on conflict (badge_id, recipient_player_id) where status = 'active' do nothing
            returning id
            "#,
        )
        .bind(badge.id)
        .bind(issuer_user)
        .bind(issuer_player_id)
        .bind(&recipient.username)
        .bind(&recipient.player_id)
        .bind(note.map(str::trim).filter(|value| !value.is_empty()))
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .fetch_optional(&mut *tx)
        .await?;
        let row = match award_id {
            Some(award_id) => self.shop_badge_award_by_id_in_tx(&mut tx, award_id).await?,
            None => {
                sqlx::query_as::<_, StoredShopBadgeAward>(&award_select_sql(
                    "where a.badge_id = $1 and a.recipient_player_id = $2 and a.status = $3",
                ))
                .bind(badge.id)
                .bind(&recipient.player_id)
                .bind(SHOP_BADGE_AWARD_ACTIVE)
                .fetch_one(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;
        Ok(row)
    }

    /// Revokes an active badge award from an owned shop.
    pub async fn revoke_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        target: &str,
    ) -> Result<StoredShopBadgeAward, StorageError> {
        validate_slug(slug)?;
        let badge = self
            .owned_shop_badge(parcel_id, slug, owner_player_id)
            .await?;
        let mut tx = self.pool.begin().await?;
        let recipient = resolve_badge_target(&mut tx, target).await?;
        let existing = sqlx::query_as::<_, StoredShopBadgeAward>(&award_select_sql(
            "where a.badge_id = $1 and a.recipient_player_id = $2 and a.status = $3",
        ))
        .bind(badge.id)
        .bind(&recipient.player_id)
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .fetch_optional(&mut *tx)
        .await?;
        let existing = match existing {
            Some(existing) => existing,
            None => {
                let historical = sqlx::query_as::<_, StoredShopBadgeAward>(&award_select_sql(
                    "where a.badge_id = $1 and a.recipient_player_id = $2 order by a.awarded_at desc limit 1",
                ))
                .bind(badge.id)
                .bind(&recipient.player_id)
                .fetch_optional(&mut *tx)
                .await?;
                return match historical {
                    Some(_) => Err(StorageError::ShopBadgeAwardNotActive {
                        parcel_id: parcel_id.to_owned(),
                        slug: slug.to_owned(),
                        target: target.to_owned(),
                    }),
                    None => Err(StorageError::ShopBadgeAwardNotFound {
                        parcel_id: parcel_id.to_owned(),
                        slug: slug.to_owned(),
                        target: target.to_owned(),
                    }),
                };
            }
        };
        let award_id = sqlx::query_scalar::<_, i64>(
            r#"
            update shop_badge_awards
            set status = $2,
                revoked_at = now(),
                updated_at = now()
            where id = $1
            returning id
            "#,
        )
        .bind(existing.id)
        .bind(SHOP_BADGE_AWARD_REVOKED)
        .fetch_one(&mut *tx)
        .await?;
        let row = self.shop_badge_award_by_id_in_tx(&mut tx, award_id).await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Lists active badge awards for one player id.
    pub async fn shop_badges_for_player(
        &self,
        player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredShopBadgeAward>, StorageError> {
        let rows = sqlx::query_as::<_, StoredShopBadgeAward>(&award_select_sql(
            "where a.recipient_player_id = $1 and a.status = $2 order by a.awarded_at desc, b.slug limit $3",
        ))
        .bind(player_id)
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Lists active public badge awards for one username or player id.
    pub async fn shop_badges_for_target(
        &self,
        target: &str,
        limit: i64,
    ) -> Result<Vec<StoredShopBadgeAward>, StorageError> {
        let mut tx = self.pool.begin().await?;
        let recipient = resolve_badge_target(&mut tx, target).await?;
        tx.commit().await?;
        self.shop_badges_for_player(&recipient.player_id, limit)
            .await
    }

    async fn owned_shop_badge(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<StoredShopBadgeDefinition, StorageError> {
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        self.shop_badge_by_parcel_slug(&parcel.parcel_id, slug)
            .await?
            .ok_or_else(|| StorageError::ShopBadgeNotFound {
                parcel_id: parcel.parcel_id,
                slug: slug.to_owned(),
            })
    }

    async fn shop_badge_by_parcel_slug(
        &self,
        parcel_id: &str,
        slug: &str,
    ) -> Result<Option<StoredShopBadgeDefinition>, StorageError> {
        let parcel_id = canonical_parcel_id(parcel_id);
        let row = sqlx::query_as::<_, StoredShopBadgeDefinition>(
            r#"
            select b.id, b.parcel_id, b.owner_player_id, b.slug, b.title, b.description,
                   coalesce(active_awards.count, 0)::bigint as active_award_count,
                   to_char(b.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
                   to_char(b.updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from shop_badges b
            left join lateral (
                select count(*) as count
                from shop_badge_awards a
                where a.badge_id = b.id
                  and a.status = $3
            ) active_awards on true
            where b.parcel_id = $1
              and b.slug = $2
            "#,
        )
        .bind(parcel_id.as_ref())
        .bind(slug)
        .bind(SHOP_BADGE_AWARD_ACTIVE)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn shop_badge_count(&self, parcel_id: &str) -> Result<usize, StorageError> {
        let parcel_id = canonical_parcel_id(parcel_id);
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from shop_badges
            where parcel_id = $1
            "#,
        )
        .bind(parcel_id.as_ref())
        .fetch_one(&self.pool)
        .await?;
        usize::try_from(count).map_err(|_| {
            StorageError::InvalidShopBadge("badge count exceeds supported range".to_owned())
        })
    }

    async fn shop_badge_award_by_id_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        award_id: i64,
    ) -> Result<StoredShopBadgeAward, StorageError> {
        let row = sqlx::query_as::<_, StoredShopBadgeAward>(&award_select_sql("where a.id = $1"))
            .bind(award_id)
            .fetch_one(&mut **tx)
            .await?;
        Ok(row)
    }
}

fn award_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        select a.id,
               b.parcel_id,
               p.title as shop_title,
               b.slug,
               b.title as badge_title,
               b.description as badge_description,
               a.issuer_user,
               a.issuer_player_id,
               a.recipient_user,
               a.recipient_player_id,
               a.note,
               a.status,
               to_char(a.awarded_at, 'YYYY-MM-DD HH24:MI:SS TZ') as awarded_at,
               to_char(a.revoked_at, 'YYYY-MM-DD HH24:MI:SS TZ') as revoked_at
        from shop_badge_awards a
        join shop_badges b on b.id = a.badge_id
        join commercial_parcels p on p.parcel_id = b.parcel_id
        {where_clause}
        "#
    )
}

async fn resolve_badge_target(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target: &str,
) -> Result<PaymentTarget, StorageError> {
    resolve_payment_target(tx, target)
        .await
        .map_err(|error| match error {
            StorageError::PaymentTargetNotFound(target) => {
                StorageError::InvalidShopBadge(format!("badge target not found: {target}"))
            }
            other => other,
        })
}

fn validate_slug(slug: &str) -> Result<(), StorageError> {
    if shop_badge_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopBadge(
            "invalid badge slug".to_owned(),
        ))
    }
}

fn validate_title(title: &str) -> Result<(), StorageError> {
    if shop_badge_title_is_valid(title) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopBadge(
            "invalid badge title".to_owned(),
        ))
    }
}

fn validate_description(description: Option<&str>) -> Result<(), StorageError> {
    if description.is_none_or(shop_badge_description_is_valid) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopBadge(
            "invalid badge description".to_owned(),
        ))
    }
}

fn validate_note(note: Option<&str>) -> Result<(), StorageError> {
    if note.is_none_or(shop_badge_note_is_valid) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopBadge(
            "invalid badge note".to_owned(),
        ))
    }
}
