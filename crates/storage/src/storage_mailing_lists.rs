use std::collections::HashMap;

use hinemos_app::{
    ParcelView, ShopCommandRouteDispatch, ShopMailingListDelivery, ShopMailingListSend,
    ShopMailingListSubscriberPage,
};
use hinemos_core::{
    PARCEL_STATUS_BUILT, SHOP_MAILING_LIST_BODY_MAX_CHARS, SHOP_MAILING_LIST_STATUS_CLOSED,
    SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE, SHOP_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED,
    SHOP_MAILING_LISTS_PER_PARCEL_MAX, shop_command_route_prefix_is_valid,
    shop_mailing_list_body_is_valid, shop_mailing_list_slug_is_valid,
    shop_mailing_list_subject_is_valid, shop_mailing_list_title_is_valid,
};
use serde_json::json;

use crate::parcels::{canonical_parcel_id, fetch_parcel_by_id};
use crate::{
    NewInboxItem, PgStorage, StorageError, StoredInboxItem, StoredOperatorCommand, StoredParcel,
    StoredShopCommandRoute, StoredShopMailingList, StoredShopMailingListPost,
    StoredShopMailingListSubscriber, StoredShopMailingListSubscription,
};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct MailingListDeliveryRecipient {
    id: i64,
    recipient_user: String,
    recipient_player_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct ShopCommandRouteTarget {
    id: i64,
    list_id: i64,
    parcel_id: String,
    slug: String,
    list_title: String,
    command_prefix: String,
}

struct MailingListPostInsert<'a> {
    list_id: i64,
    parcel_id: &'a str,
    slug: &'a str,
    list_title: &'a str,
    recipients: &'a [MailingListDeliveryRecipient],
    sender_user: &'a str,
    sender_player_id: &'a str,
    subject: &'a str,
    body: &'a str,
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
        let parcel_id = canonical_parcel_id(parcel_id);
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
        .bind(parcel_id.as_ref())
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

        let post = self
            .insert_shop_mailing_list_post_for_recipients(MailingListPostInsert {
                list_id: list.id,
                parcel_id: &list.parcel_id,
                slug: &list.slug,
                list_title: &list.title,
                recipients: &recipients,
                sender_user,
                sender_player_id,
                subject: subject.trim(),
                body: body.trim(),
            })
            .await?;

        let deliveries = self.deliver_shop_mailing_list_post(post.id).await?;
        Ok(ShopMailingListSend { post, deliveries })
    }

    /// Adds or returns a command route from an owned shop into one mailing list.
    pub async fn add_shop_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<StoredShopCommandRoute, StorageError> {
        validate_slug(slug)?;
        validate_command_prefix(command_prefix)?;
        let list = self
            .owned_shop_mailing_list(parcel_id, slug, owner_player_id)
            .await?;
        let route = sqlx::query_as::<_, StoredShopCommandRoute>(
            r#"
            insert into shop_command_routes (
                parcel_id, list_id, owner_player_id, command_prefix
            )
            values ($1, $2, $3, $4)
            on conflict (parcel_id, list_id, command_prefix) do update
            set owner_player_id = excluded.owner_player_id
            returning id, parcel_id,
                      $5::text as slug,
                      $6::text as list_title,
                      command_prefix,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(&list.parcel_id)
        .bind(list.id)
        .bind(owner_player_id)
        .bind(command_prefix.trim())
        .bind(&list.slug)
        .bind(&list.title)
        .fetch_one(&self.pool)
        .await?;
        Ok(route)
    }

    /// Lists command routes for an owned shop parcel.
    pub async fn shop_command_routes(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<StoredShopCommandRoute>, StorageError> {
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        let routes = sqlx::query_as::<_, StoredShopCommandRoute>(
            r#"
            select r.id, r.parcel_id, l.slug, l.title as list_title,
                   r.command_prefix,
                   to_char(r.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_command_routes r
            join shop_mailing_lists l on l.id = r.list_id
            where r.parcel_id = $1
            order by r.created_at desc, l.slug, r.command_prefix
            "#,
        )
        .bind(&parcel.parcel_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routes)
    }

    /// Removes a command route from an owned shop mailing list.
    pub async fn remove_shop_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<StoredShopCommandRoute, StorageError> {
        validate_slug(slug)?;
        validate_command_prefix(command_prefix)?;
        let list = self
            .owned_shop_mailing_list(parcel_id, slug, owner_player_id)
            .await?;
        let route = sqlx::query_as::<_, StoredShopCommandRoute>(
            r#"
            select r.id, r.parcel_id, l.slug, l.title as list_title,
                   r.command_prefix,
                   to_char(r.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_command_routes r
            join shop_mailing_lists l on l.id = r.list_id
            where r.parcel_id = $1
              and r.list_id = $2
              and r.command_prefix = $3
            "#,
        )
        .bind(&list.parcel_id)
        .bind(list.id)
        .bind(command_prefix.trim())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            StorageError::InvalidMailingList(format!(
                "shop command route not found: {}/{} {}",
                list.parcel_id,
                list.slug,
                command_prefix.trim()
            ))
        })?;
        sqlx::query("delete from shop_command_routes where id = $1")
            .bind(route.id)
            .execute(&self.pool)
            .await?;
        Ok(route)
    }

    /// Dispatches one saved operator command into matching route streams.
    pub async fn dispatch_shop_command_routes<P>(
        &self,
        parcel: &P,
        command_id: i64,
    ) -> Result<
        Vec<ShopCommandRouteDispatch<StoredShopMailingListPost, StoredInboxItem>>,
        StorageError,
    >
    where
        P: ParcelView,
    {
        let command = self.operator_command(command_id).await?;
        if command.parcel_id != parcel.parcel_id() {
            return Err(StorageError::InvalidMailingList(format!(
                "shop command {} belongs to {}, not {}",
                command.id,
                command.parcel_id,
                parcel.parcel_id()
            )));
        }
        let targets = matching_shop_command_route_targets(
            self.shop_command_route_targets(parcel.parcel_id()).await?,
            &command.raw_input,
        );
        let mut routed = Vec::new();
        for target in targets {
            let recipients = self.active_subscription_recipients(target.list_id).await?;
            if recipients.is_empty() {
                continue;
            }
            let body = routed_command_body(
                command.id,
                &target.parcel_id,
                &command.sender_user,
                &command.raw_input,
                &target.command_prefix,
                &target.slug,
            );
            let post = self
                .insert_shop_mailing_list_post_for_recipients(MailingListPostInsert {
                    list_id: target.list_id,
                    parcel_id: &target.parcel_id,
                    slug: &target.slug,
                    list_title: &target.list_title,
                    recipients: &recipients,
                    sender_user: &command.sender_user,
                    sender_player_id: &command.sender_player_id,
                    subject: &target.command_prefix,
                    body: &body,
                })
                .await?;
            let deliveries = self.deliver_shop_mailing_list_post(post.id).await?;
            routed.push(ShopCommandRouteDispatch { post, deliveries });
        }
        Ok(routed)
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
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        self.shop_mailing_list_by_parcel_slug(&parcel.parcel_id, slug)
            .await?
            .ok_or_else(|| StorageError::MailingListNotFound {
                parcel_id: parcel.parcel_id,
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
        let target = canonical_parcel_id(target);
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
        .bind(target.as_ref())
        .bind(slug)
        .bind(SHOP_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .bind(PARCEL_STATUS_BUILT)
        .fetch_all(&self.pool)
        .await?;
        let mut rows = rows.into_iter();
        let Some(first) = rows.next() else {
            return Err(StorageError::MailingListNotFound {
                parcel_id: target.into_owned(),
                slug: slug.to_owned(),
            });
        };
        if rows.next().is_some() {
            return Err(StorageError::InvalidMailingList(format!(
                "ambiguous shop target: {}",
                target.as_ref()
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

    async fn operator_command(
        &self,
        command_id: i64,
    ) -> Result<StoredOperatorCommand, StorageError> {
        let command = sqlx::query_as::<_, StoredOperatorCommand>(
            r#"
            select id, view_id, parcel_id, sender_user, sender_player_id,
                   owner_user, owner_player_id, raw_input, status,
                   to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from operator_commands
            where id = $1
            "#,
        )
        .bind(command_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StorageError::OperatorCommandNotFound(command_id))?;
        Ok(command)
    }

    async fn shop_command_route_targets(
        &self,
        parcel_id: &str,
    ) -> Result<Vec<ShopCommandRouteTarget>, StorageError> {
        let parcel_id = canonical_parcel_id(parcel_id);
        let targets = sqlx::query_as::<_, ShopCommandRouteTarget>(
            r#"
            select r.id, r.list_id, r.parcel_id, l.slug, l.title as list_title,
                   r.command_prefix
            from shop_command_routes r
            join shop_mailing_lists l on l.id = r.list_id
            where r.parcel_id = $1
            order by r.created_at, r.id
            "#,
        )
        .bind(parcel_id.as_ref())
        .fetch_all(&self.pool)
        .await?;
        Ok(targets)
    }

    async fn insert_shop_mailing_list_post_for_recipients(
        &self,
        insert: MailingListPostInsert<'_>,
    ) -> Result<StoredShopMailingListPost, StorageError> {
        let recipient_count = i64::try_from(insert.recipients.len()).map_err(|_| {
            StorageError::InvalidMailingList("subscriber count exceeds supported range".to_owned())
        })?;
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
        .bind(insert.list_id)
        .bind(insert.sender_user)
        .bind(insert.sender_player_id)
        .bind(insert.subject)
        .bind(insert.body)
        .bind(recipient_count)
        .bind(insert.parcel_id)
        .bind(insert.slug)
        .bind(insert.list_title)
        .fetch_one(&mut *tx)
        .await?;
        for recipient in insert.recipients {
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
        Ok(post)
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

fn validate_command_prefix(command_prefix: &str) -> Result<(), StorageError> {
    if shop_command_route_prefix_is_valid(command_prefix) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid shop command route prefix".to_owned(),
        ))
    }
}

fn matching_shop_command_route_targets(
    targets: impl IntoIterator<Item = ShopCommandRouteTarget>,
    raw_input: &str,
) -> Vec<ShopCommandRouteTarget> {
    let mut target_indexes = HashMap::<i64, usize>::new();
    let mut matches = Vec::<ShopCommandRouteTarget>::new();
    for target in targets {
        if !command_prefix_matches(raw_input, &target.command_prefix) {
            continue;
        }
        if let Some(index) = target_indexes.get(&target.list_id).copied() {
            let current_prefix = &matches[index].command_prefix;
            if route_prefix_specificity(&target.command_prefix)
                > route_prefix_specificity(current_prefix)
            {
                matches[index] = target;
            }
        } else {
            target_indexes.insert(target.list_id, matches.len());
            matches.push(target);
        }
    }
    matches
}

fn command_prefix_matches(raw_input: &str, command_prefix: &str) -> bool {
    let raw_input = raw_input.trim();
    let command_prefix = command_prefix.trim();
    let Some(head) = raw_input.get(..command_prefix.len()) else {
        return false;
    };
    if !head.eq_ignore_ascii_case(command_prefix) {
        return false;
    }
    raw_input
        .get(command_prefix.len()..)
        .is_some_and(|tail| tail.is_empty() || tail.chars().next().is_some_and(char::is_whitespace))
}

fn route_prefix_specificity(command_prefix: &str) -> usize {
    command_prefix.trim().chars().count()
}

fn routed_command_body(
    command_id: i64,
    parcel_id: &str,
    sender_user: &str,
    raw_input: &str,
    command_prefix: &str,
    slug: &str,
) -> String {
    let body = format!(
        "Shop command #{command_id} from {sender_user} in {parcel_id}:\n{raw_input}\n\nMatched route: {command_prefix}\nReply in stream: /chat {parcel_id} {slug} -- <message>"
    );
    truncate_mailing_list_body(&body)
}

fn truncate_mailing_list_body(body: &str) -> String {
    let body = body.trim();
    if body.chars().count() <= SHOP_MAILING_LIST_BODY_MAX_CHARS {
        body.to_owned()
    } else {
        body.chars()
            .take(SHOP_MAILING_LIST_BODY_MAX_CHARS)
            .collect()
    }
}

fn sender_can_post_to_shop_chat(
    current_owner_player_id: Option<&str>,
    sender_player_id: &str,
    sender_has_subscription: bool,
) -> bool {
    current_owner_player_id == Some(sender_player_id) || sender_has_subscription
}
