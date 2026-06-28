use hinemos_app::{ShopMailingListDelivery, ShopMailingListSend, ShopMailingListSubscriberPage};
use hinemos_core::{
    PARCEL_STATUS_BUILT, SHOP_MAILING_LIST_STATUS_CLOSED, SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE,
    SHOP_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED, SHOP_MAILING_LISTS_PER_PARCEL_MAX,
    shop_mailing_list_body_is_valid, shop_mailing_list_slug_is_valid,
    shop_mailing_list_subject_is_valid, shop_mailing_list_title_is_valid,
};
use serde_json::json;

use crate::parcels::fetch_parcel_by_id;
use crate::{
    NewInboxItem, PgStorage, StorageError, StoredInboxItem, StoredParcel, StoredShopMailingList,
    StoredShopMailingListPost, StoredShopMailingListSubscriber, StoredShopMailingListSubscription,
};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct MailingListDeliveryRecipient {
    id: i64,
    recipient_user: String,
    recipient_player_id: String,
}

impl PgStorage {
    /// Creates a mailing list for an owned built shop parcel.
    pub async fn create_shop_mailing_list(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<StoredShopMailingList, StorageError> {
        validate_slug(slug)?;
        validate_title(title)?;
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        if self
            .shop_mailing_list_by_parcel_slug(parcel.parcel_id.as_str(), slug)
            .await?
            .is_some()
        {
            return Err(StorageError::MailingListAlreadyExists {
                parcel_id: parcel.parcel_id,
                slug: slug.to_owned(),
            });
        }
        let list_count = self.shop_mailing_list_count(&parcel.parcel_id).await?;
        if list_count >= SHOP_MAILING_LISTS_PER_PARCEL_MAX {
            return Err(StorageError::InvalidMailingList(format!(
                "mailing-list limit reached for parcel {}; maximum is {}",
                parcel.parcel_id, SHOP_MAILING_LISTS_PER_PARCEL_MAX
            )));
        }
        let row = sqlx::query_as::<_, StoredShopMailingList>(
            r#"
            insert into shop_mailing_lists (parcel_id, owner_player_id, slug, title)
            values ($1, $2, $3, $4)
            returning id, parcel_id, owner_player_id, slug, title, status,
                      0::bigint as subscriber_count,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(&parcel.parcel_id)
        .bind(owner_player_id)
        .bind(slug)
        .bind(title.trim())
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Lists mailing lists for an owned shop parcel.
    pub async fn shop_mailing_lists(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<StoredShopMailingList>, StorageError> {
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        self.shop_mailing_lists_for_parcel(&parcel.parcel_id).await
    }

    /// Lists mailing lists for a parcel without applying owner authorization.
    pub async fn shop_mailing_lists_for_parcel(
        &self,
        parcel_id: &str,
    ) -> Result<Vec<StoredShopMailingList>, StorageError> {
        let rows = sqlx::query_as::<_, StoredShopMailingList>(
            r#"
            select l.id, l.parcel_id, l.owner_player_id, l.slug, l.title, l.status,
                   coalesce(active_subscribers.count, 0)::bigint as subscriber_count,
                   to_char(l.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_mailing_lists l
            left join lateral (
                select count(*) as count
                from shop_mailing_list_subscriptions s
                where s.list_id = l.id
                  and s.status = $2
            ) active_subscribers on true
            where l.parcel_id = $1
            order by l.created_at desc, l.slug
            "#,
        )
        .bind(parcel_id)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Lists recent active subscribers for an owned shop mailing list.
    pub async fn shop_mailing_list_subscribers(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<ShopMailingListSubscriberPage<StoredShopMailingListSubscriber>, StorageError> {
        let list = self
            .owned_shop_mailing_list(parcel_id, slug, owner_player_id)
            .await?;
        let subscribers = sqlx::query_as::<_, StoredShopMailingListSubscriber>(
            r#"
            select subscriber_user, subscriber_player_id,
                   to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from shop_mailing_list_subscriptions
            where list_id = $1
              and status = $3
            order by updated_at desc, subscriber_user
            limit $2
            "#,
        )
        .bind(list.id)
        .bind(limit)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(ShopMailingListSubscriberPage {
            total: list.subscriber_count,
            subscribers,
        })
    }

    /// Closes an owned shop mailing list to new subscriptions.
    pub async fn close_shop_mailing_list(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<StoredShopMailingList, StorageError> {
        let list = self
            .owned_shop_mailing_list(parcel_id, slug, owner_player_id)
            .await?;
        let row = sqlx::query_as::<_, StoredShopMailingList>(
            r#"
            update shop_mailing_lists
            set status = $2, updated_at = now()
            where id = $1
            returning id, parcel_id, owner_player_id, slug, title, status,
                      (
                          select count(*)::bigint
                          from shop_mailing_list_subscriptions s
                          where s.list_id = shop_mailing_lists.id
                            and s.status = $3
                      ) as subscriber_count,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(list.id)
        .bind(SHOP_MAILING_LIST_STATUS_CLOSED)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Subscribes a player to an open shop mailing list.
    pub async fn subscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<StoredShopMailingListSubscription, StorageError> {
        validate_slug(slug)?;
        let list = self.resolve_shop_mailing_list(target, slug).await?;
        if list.status == SHOP_MAILING_LIST_STATUS_CLOSED {
            return Err(StorageError::MailingListClosed {
                parcel_id: list.parcel_id,
                slug: list.slug,
            });
        }
        if self
            .active_subscription_exists(list.id, subscriber_player_id)
            .await?
        {
            return Err(StorageError::MailingListAlreadySubscribed {
                parcel_id: list.parcel_id,
                slug: list.slug,
            });
        }
        sqlx::query(
            r#"
            insert into shop_mailing_list_subscriptions (
                list_id, subscriber_user, subscriber_player_id, status
            )
            values ($1, $2, $3, $4)
            on conflict (list_id, subscriber_player_id) do update
            set subscriber_user = excluded.subscriber_user,
                status = excluded.status,
                updated_at = now()
            "#,
        )
        .bind(list.id)
        .bind(subscriber_user)
        .bind(subscriber_player_id)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .execute(&self.pool)
        .await?;
        self.subscription_for_player(list.id, subscriber_player_id)
            .await
    }

    /// Unsubscribes a player from a shop mailing list.
    pub async fn unsubscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<StoredShopMailingListSubscription, StorageError> {
        validate_slug(slug)?;
        let list = self.resolve_shop_mailing_list(target, slug).await?;
        sqlx::query(
            r#"
            insert into shop_mailing_list_subscriptions (
                list_id, subscriber_user, subscriber_player_id, status
            )
            values ($1, $2, $3, $4)
            on conflict (list_id, subscriber_player_id) do update
            set subscriber_user = excluded.subscriber_user,
                status = excluded.status,
                updated_at = now()
            "#,
        )
        .bind(list.id)
        .bind(subscriber_user)
        .bind(subscriber_player_id)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED)
        .execute(&self.pool)
        .await?;
        self.subscription_for_player(list.id, subscriber_player_id)
            .await
    }

    /// Lists active subscriptions for a player.
    pub async fn shop_mailing_list_subscriptions(
        &self,
        subscriber_player_id: &str,
    ) -> Result<Vec<StoredShopMailingListSubscription>, StorageError> {
        let rows = sqlx::query_as::<_, StoredShopMailingListSubscription>(
            r#"
            select l.parcel_id, p.title as shop_title, l.slug, l.title as list_title,
                   s.status,
                   to_char(s.updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from shop_mailing_list_subscriptions s
            join shop_mailing_lists l on l.id = s.list_id
            join commercial_parcels p on p.parcel_id = l.parcel_id
            where s.subscriber_player_id = $1
              and s.status = $2
            order by s.updated_at desc, l.parcel_id, l.slug
            "#,
        )
        .bind(subscriber_player_id)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Sends one mailing-list post to all active shop-chat members.
    pub async fn send_shop_mailing_list_post(
        &self,
        target: &str,
        slug: &str,
        sender_user: &str,
        sender_player_id: &str,
        subject: &str,
        body: &str,
    ) -> Result<ShopMailingListSend<StoredShopMailingListPost, StoredInboxItem>, StorageError> {
        validate_slug(slug)?;
        validate_subject(subject)?;
        validate_body(body)?;
        let list = self.resolve_shop_mailing_list(target, slug).await?;
        let parcel = fetch_parcel_by_id(&self.pool, &list.parcel_id).await?;
        let sender_has_subscription = self
            .active_subscription_exists(list.id, sender_player_id)
            .await?;
        if !sender_can_post_to_shop_chat(
            parcel.owner_player_id.as_deref(),
            sender_player_id,
            sender_has_subscription,
        ) {
            return Err(StorageError::MailingListNotMember {
                parcel_id: list.parcel_id,
                slug: list.slug,
            });
        }
        let recipients = self.active_subscription_recipients(list.id).await?;
        let recipient_count = i64::try_from(recipients.len()).map_err(|_| {
            StorageError::InvalidMailingList("subscriber count exceeds supported range".to_owned())
        })?;
        if recipient_count == 0 {
            return Err(StorageError::MailingListNoSubscribers {
                parcel_id: list.parcel_id,
                slug: list.slug,
            });
        }

        let mut tx = self.pool.begin().await?;
        let post = sqlx::query_as::<_, StoredShopMailingListPost>(
            r#"
            insert into shop_mailing_list_posts (
                list_id, sender_user, sender_player_id, subject, body, recipient_count
            )
            values ($1, $2, $3, $4, $5, $6)
            returning id,
                      $7::text as parcel_id,
                      $8::text as slug,
                      $9::text as list_title,
                      sender_user, sender_player_id, subject, body, recipient_count,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(list.id)
        .bind(sender_user)
        .bind(sender_player_id)
        .bind(subject.trim())
        .bind(body.trim())
        .bind(recipient_count)
        .bind(&list.parcel_id)
        .bind(&list.slug)
        .bind(&list.title)
        .fetch_one(&mut *tx)
        .await?;
        for recipient in &recipients {
            sqlx::query(
                r#"
                insert into shop_mailing_list_deliveries (
                    post_id, recipient_user, recipient_player_id
                )
                values ($1, $2, $3)
                on conflict (post_id, recipient_player_id) do nothing
                "#,
            )
            .bind(post.id)
            .bind(&recipient.recipient_user)
            .bind(&recipient.recipient_player_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        let deliveries = self.deliver_shop_mailing_list_post(post.id).await?;
        Ok(ShopMailingListSend { post, deliveries })
    }

    /// Delivers an existing mailing-list post into subscriber inboxes idempotently.
    pub async fn deliver_shop_mailing_list_post(
        &self,
        post_id: i64,
    ) -> Result<Vec<ShopMailingListDelivery<StoredInboxItem>>, StorageError> {
        let post = self.shop_mailing_list_post(post_id).await?;
        let recipients = sqlx::query_as::<_, MailingListDeliveryRecipient>(
            r#"
            select id, recipient_user, recipient_player_id
            from shop_mailing_list_deliveries
            where post_id = $1
            order by id
            "#,
        )
        .bind(post_id)
        .fetch_all(&self.pool)
        .await?;
        let mut deliveries = Vec::with_capacity(recipients.len());
        for recipient in recipients {
            let body = format!(
                "[{} / {}]\nFrom: {}\n{}\n\nReply: /chat {} {} -- <message>\nUnsubscribe: /unsubscribe {} {}",
                post.parcel_id,
                post.list_title,
                post.sender_user,
                post.body,
                post.parcel_id,
                post.slug,
                post.parcel_id,
                post.slug
            );
            let inbox_item = self
                .create_inbox_item(NewInboxItem {
                    kind: "mail",
                    recipient_user: &recipient.recipient_user,
                    recipient_player_id: &recipient.recipient_player_id,
                    sender_user: &post.sender_user,
                    sender_player_id: &post.sender_player_id,
                    subject: &post.subject,
                    body: &body,
                    source_kind: Some("shop_mailing_list_post"),
                    source_id: Some(post.id),
                    payload: json!({
                        "parcelId": post.parcel_id.as_str(),
                        "listSlug": post.slug.as_str(),
                        "listTitle": post.list_title.as_str(),
                        "postId": post.id,
                        "deliveryId": recipient.id
                    }),
                })
                .await?;
            sqlx::query(
                r#"
                update shop_mailing_list_deliveries
                set inbox_item_id = $2
                where id = $1
                "#,
            )
            .bind(recipient.id)
            .bind(inbox_item.id)
            .execute(&self.pool)
            .await?;
            deliveries.push(ShopMailingListDelivery {
                recipient_player_id: recipient.recipient_player_id,
                inbox_item,
            });
        }
        Ok(deliveries)
    }

    pub(crate) async fn owned_built_parcel(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<StoredParcel, StorageError> {
        let parcel = fetch_parcel_by_id(&self.pool, parcel_id).await?;
        if parcel.owner_player_id.as_deref() != Some(owner_player_id) {
            return Err(StorageError::NotParcelOwner(parcel.parcel_id));
        }
        if parcel.status != PARCEL_STATUS_BUILT {
            return Err(StorageError::ParcelNotBuilt(parcel.parcel_id));
        }
        Ok(parcel)
    }

    async fn owned_shop_mailing_list(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<StoredShopMailingList, StorageError> {
        validate_slug(slug)?;
        self.owned_built_parcel(parcel_id, owner_player_id).await?;
        self.shop_mailing_list_by_parcel_slug(parcel_id, slug)
            .await?
            .ok_or_else(|| StorageError::MailingListNotFound {
                parcel_id: parcel_id.to_owned(),
                slug: slug.to_owned(),
            })
    }

    async fn shop_mailing_list_by_parcel_slug(
        &self,
        parcel_id: &str,
        slug: &str,
    ) -> Result<Option<StoredShopMailingList>, StorageError> {
        let row = sqlx::query_as::<_, StoredShopMailingList>(
            r#"
            select l.id, l.parcel_id, l.owner_player_id, l.slug, l.title, l.status,
                   coalesce(active_subscribers.count, 0)::bigint as subscriber_count,
                   to_char(l.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_mailing_lists l
            left join lateral (
                select count(*) as count
                from shop_mailing_list_subscriptions s
                where s.list_id = l.id
                  and s.status = $3
            ) active_subscribers on true
            where l.parcel_id = $1
              and l.slug = $2
            "#,
        )
        .bind(parcel_id)
        .bind(slug)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn shop_mailing_list_count(&self, parcel_id: &str) -> Result<usize, StorageError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from shop_mailing_lists
            where parcel_id = $1
            "#,
        )
        .bind(parcel_id)
        .fetch_one(&self.pool)
        .await?;
        usize::try_from(count).map_err(|_| {
            StorageError::InvalidMailingList(
                "mailing-list count exceeds supported range".to_owned(),
            )
        })
    }

    async fn resolve_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
    ) -> Result<StoredShopMailingList, StorageError> {
        let rows = sqlx::query_as::<_, StoredShopMailingList>(
            r#"
            select l.id, l.parcel_id, l.owner_player_id, l.slug, l.title, l.status,
                   coalesce(active_subscribers.count, 0)::bigint as subscriber_count,
                   to_char(l.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_mailing_lists l
            join commercial_parcels p on p.parcel_id = l.parcel_id
            left join lateral (
                select count(*) as count
                from shop_mailing_list_subscriptions s
                where s.list_id = l.id
                  and s.status = $3
            ) active_subscribers on true
            where l.slug = $2
              and p.status = $4
              and (
                    lower(p.parcel_id) = lower($1)
                 or lower(coalesce(p.title, '')) = lower($1)
              )
            order by p.parcel_id
            limit 2
            "#,
        )
        .bind(target)
        .bind(slug)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .bind(PARCEL_STATUS_BUILT)
        .fetch_all(&self.pool)
        .await?;
        let mut rows = rows.into_iter();
        let Some(first) = rows.next() else {
            return Err(StorageError::MailingListNotFound {
                parcel_id: target.to_owned(),
                slug: slug.to_owned(),
            });
        };
        if rows.next().is_some() {
            return Err(StorageError::InvalidMailingList(format!(
                "ambiguous shop target: {target}"
            )));
        }
        Ok(first)
    }

    async fn active_subscription_exists(
        &self,
        list_id: i64,
        subscriber_player_id: &str,
    ) -> Result<bool, StorageError> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            select exists (
                select 1
                from shop_mailing_list_subscriptions
                where list_id = $1
                  and subscriber_player_id = $2
                  and status = $3
            )
            "#,
        )
        .bind(list_id)
        .bind(subscriber_player_id)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_one(&self.pool)
        .await?;
        Ok(exists)
    }

    async fn subscription_for_player(
        &self,
        list_id: i64,
        subscriber_player_id: &str,
    ) -> Result<StoredShopMailingListSubscription, StorageError> {
        let row = sqlx::query_as::<_, StoredShopMailingListSubscription>(
            r#"
            select l.parcel_id, p.title as shop_title, l.slug, l.title as list_title,
                   s.status,
                   to_char(s.updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from shop_mailing_list_subscriptions s
            join shop_mailing_lists l on l.id = s.list_id
            join commercial_parcels p on p.parcel_id = l.parcel_id
            where s.list_id = $1
              and s.subscriber_player_id = $2
            "#,
        )
        .bind(list_id)
        .bind(subscriber_player_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    async fn active_subscription_recipients(
        &self,
        list_id: i64,
    ) -> Result<Vec<MailingListDeliveryRecipient>, StorageError> {
        let rows = sqlx::query_as::<_, MailingListDeliveryRecipient>(
            r#"
            select id, subscriber_user as recipient_user,
                   subscriber_player_id as recipient_player_id
            from shop_mailing_list_subscriptions
            where list_id = $1
              and status = $2
            order by id
            "#,
        )
        .bind(list_id)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn shop_mailing_list_post(
        &self,
        post_id: i64,
    ) -> Result<StoredShopMailingListPost, StorageError> {
        let post = sqlx::query_as::<_, StoredShopMailingListPost>(
            r#"
            select post.id, list.parcel_id, list.slug, list.title as list_title,
                   post.sender_user, post.sender_player_id, post.subject, post.body,
                   post.recipient_count,
                   to_char(post.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_mailing_list_posts post
            join shop_mailing_lists list on list.id = post.list_id
            where post.id = $1
            "#,
        )
        .bind(post_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::InvalidMailingList(format!("post not found: {post_id}")))?;
        Ok(post)
    }
}

fn validate_slug(slug: &str) -> Result<(), StorageError> {
    if shop_mailing_list_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list slug".to_owned(),
        ))
    }
}

fn validate_title(title: &str) -> Result<(), StorageError> {
    if shop_mailing_list_title_is_valid(title) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list title".to_owned(),
        ))
    }
}

fn validate_subject(subject: &str) -> Result<(), StorageError> {
    if shop_mailing_list_subject_is_valid(subject) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list subject".to_owned(),
        ))
    }
}

fn validate_body(body: &str) -> Result<(), StorageError> {
    if shop_mailing_list_body_is_valid(body) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list body".to_owned(),
        ))
    }
}

fn sender_can_post_to_shop_chat(
    current_owner_player_id: Option<&str>,
    sender_player_id: &str,
    sender_has_subscription: bool,
) -> bool {
    current_owner_player_id == Some(sender_player_id) || sender_has_subscription
}
