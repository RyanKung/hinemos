use std::collections::HashMap;

use hinemos_app::{
    ParcelView, ShopMailingListDelivery, ShopMailingListSend, ShopMailingListSubscriberPage,
};
use hinemos_core::{
    PARCEL_MAILING_LIST_STATUS_CLOSED, PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE,
    PARCEL_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED, PARCEL_MAILING_LISTS_PER_PARCEL_MAX,
    PARCEL_STATUS_BUILT, PARCEL_WORK_DESKS_PER_PARCEL_MAX, PARCEL_WORK_ITEM_CLAIMED,
    PARCEL_WORK_ITEM_DONE, PARCEL_WORK_ITEM_QUEUED, PARCEL_WORK_SHIFT_ACTIVE,
    PARCEL_WORK_SHIFT_ENDED, PARCEL_WORK_STAFF_ACTIVE, PARCEL_WORK_STAFF_REMOVED,
    parcel_command_route_prefix_is_valid, parcel_mailing_list_body_is_valid,
    parcel_mailing_list_slug_is_valid, parcel_mailing_list_subject_is_valid,
    parcel_mailing_list_title_is_valid, parcel_work_desk_slug_is_valid,
    parcel_work_desk_title_is_valid, parcel_work_result_is_valid,
};
use serde_json::json;

use crate::parcels::{canonical_parcel_id, fetch_parcel_by_id};
use crate::{
    NewInboxItem, PgStorage, StorageError, StoredInboxItem, StoredOperatorCommand, StoredParcel,
    StoredShopCommandRoute, StoredShopMailingList, StoredShopMailingListPost,
    StoredShopMailingListSubscriber, StoredShopMailingListSubscription, StoredShopShift,
    StoredShopStaff, StoredShopWorkDesk, StoredShopWorkItem,
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
    desk_id: i64,
    parcel_id: String,
    slug: String,
    desk_title: String,
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
        if list_count >= PARCEL_MAILING_LISTS_PER_PARCEL_MAX {
            return Err(StorageError::InvalidMailingList(format!(
                "mailing-list limit reached for parcel {}; maximum is {}",
                parcel.parcel_id, PARCEL_MAILING_LISTS_PER_PARCEL_MAX
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Lists recent active subscribers for an owned parcel mailing list.
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(ShopMailingListSubscriberPage {
            total: list.subscriber_count,
            subscribers,
        })
    }

    /// Closes an owned parcel mailing list to new subscriptions.
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
        .bind(PARCEL_MAILING_LIST_STATUS_CLOSED)
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Resolves a built shop target and mailing-list slug without changing membership.
    pub async fn shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
    ) -> Result<StoredShopMailingList, StorageError> {
        validate_slug(slug)?;
        self.resolve_shop_mailing_list(target, slug).await
    }

    /// Subscribes a player to an open parcel mailing list.
    pub async fn subscribe_shop_mailing_list(
        &self,
        target: &str,
        slug: &str,
        subscriber_user: &str,
        subscriber_player_id: &str,
    ) -> Result<StoredShopMailingListSubscription, StorageError> {
        validate_slug(slug)?;
        let list = self.resolve_shop_mailing_list(target, slug).await?;
        if list.status == PARCEL_MAILING_LIST_STATUS_CLOSED {
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .execute(&self.pool)
        .await?;
        self.subscription_for_player(list.id, subscriber_player_id)
            .await
    }

    /// Unsubscribes a player from a parcel mailing list.
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_UNSUBSCRIBED)
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
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

    /// Creates a shop-local work desk for an owned built shop parcel.
    pub async fn create_shop_work_desk(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        title: &str,
    ) -> Result<StoredShopWorkDesk, StorageError> {
        validate_work_slug(slug)?;
        validate_work_title(title)?;
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        if self
            .shop_work_desk_by_parcel_slug(&parcel.parcel_id, slug)
            .await?
            .is_some()
        {
            return Err(StorageError::ShopWorkDeskAlreadyExists {
                parcel_id: parcel.parcel_id,
                slug: slug.to_owned(),
            });
        }
        let desk_count = self.shop_work_desk_count(&parcel.parcel_id).await?;
        if desk_count >= PARCEL_WORK_DESKS_PER_PARCEL_MAX {
            return Err(StorageError::InvalidShopWork(format!(
                "work-desk limit reached for parcel {}; maximum is {}",
                parcel.parcel_id, PARCEL_WORK_DESKS_PER_PARCEL_MAX
            )));
        }
        let row = sqlx::query_as::<_, StoredShopWorkDesk>(
            r#"
            insert into shop_work_desks (parcel_id, owner_player_id, slug, title)
            values ($1, $2, $3, $4)
            returning id, parcel_id, owner_player_id, slug, title, status,
                      0::bigint as queued_count,
                      0::bigint as active_worker_count,
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

    /// Lists shop-local work desks for an owned shop parcel.
    pub async fn shop_work_desks(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
    ) -> Result<Vec<StoredShopWorkDesk>, StorageError> {
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        self.shop_work_desks_for_parcel(&parcel.parcel_id).await
    }

    /// Adds or reactivates a worker assignment for one work desk.
    pub async fn add_shop_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<StoredShopStaff, StorageError> {
        validate_work_slug(slug)?;
        let username = username.trim();
        if username.is_empty() || username.contains(char::is_whitespace) {
            return Err(StorageError::InvalidShopWork(
                "invalid parcel staff username".to_owned(),
            ));
        }
        let desk = self
            .owned_shop_work_desk(parcel_id, slug, owner_player_id)
            .await?;
        let staff = sqlx::query_as::<_, StoredShopStaff>(
            r#"
            insert into shop_work_staff (desk_id, staff_user, status)
            values ($1, $2, $3)
            on conflict (desk_id, staff_user) do update
            set status = excluded.status,
                updated_at = now()
            returning staff_user, status,
                      to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            "#,
        )
        .bind(desk.id)
        .bind(username)
        .bind(PARCEL_WORK_STAFF_ACTIVE)
        .fetch_one(&self.pool)
        .await?;
        Ok(staff)
    }

    /// Lists staff assignments for one owned work desk.
    pub async fn shop_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredShopStaff>, StorageError> {
        validate_work_slug(slug)?;
        let desk = self
            .owned_shop_work_desk(parcel_id, slug, owner_player_id)
            .await?;
        let rows = sqlx::query_as::<_, StoredShopStaff>(
            r#"
            select staff_user, status,
                   to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            from shop_work_staff
            where desk_id = $1
            order by updated_at desc, staff_user
            limit $2
            "#,
        )
        .bind(desk.id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Removes a worker assignment from one work desk.
    pub async fn remove_shop_staff(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
        username: &str,
    ) -> Result<StoredShopStaff, StorageError> {
        validate_work_slug(slug)?;
        let desk = self
            .owned_shop_work_desk(parcel_id, slug, owner_player_id)
            .await?;
        let desk_id = desk.id;
        let staff_user = username.trim();
        let staff = sqlx::query_as::<_, StoredShopStaff>(
            r#"
            update shop_work_staff
            set status = $3, updated_at = now()
            where desk_id = $1
              and staff_user = $2
            returning staff_user, status,
                      to_char(updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
            "#,
        )
        .bind(desk_id)
        .bind(staff_user)
        .bind(PARCEL_WORK_STAFF_REMOVED)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::ShopWorkerNotAssigned {
            parcel_id: desk.parcel_id.clone(),
            slug: desk.slug.clone(),
        })?;
        sqlx::query(
            r#"
            update shop_work_shifts
            set status = $3, ended_at = now(), updated_at = now()
            where desk_id = $1
              and worker_user = $2
              and status = $4
            "#,
        )
        .bind(desk_id)
        .bind(staff_user)
        .bind(PARCEL_WORK_SHIFT_ENDED)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .execute(&self.pool)
        .await?;
        Ok(staff)
    }

    /// Starts an active shift for an assigned worker.
    pub async fn start_shop_shift(
        &self,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<StoredShopShift, StorageError> {
        validate_work_slug(slug)?;
        let desk = self.shop_work_desk(parcel_id, slug).await?;
        self.ensure_worker_assigned(&desk, worker_user, worker_player_id)
            .await?;
        if let Some(shift) = self.active_shop_shift(desk.id, worker_player_id).await? {
            return Ok(shift);
        }
        let shift = sqlx::query_as::<_, StoredShopShift>(
            r#"
            insert into shop_work_shifts (desk_id, worker_user, worker_player_id, status)
            values ($1, $2, $3, $4)
            returning id,
                      $5::text as parcel_id,
                      $6::text as slug,
                      worker_user, worker_player_id, status,
                      to_char(started_at, 'YYYY-MM-DD HH24:MI:SS TZ') as started_at,
                      to_char(ended_at, 'YYYY-MM-DD HH24:MI:SS TZ') as ended_at
            "#,
        )
        .bind(desk.id)
        .bind(worker_user)
        .bind(worker_player_id)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .bind(&desk.parcel_id)
        .bind(&desk.slug)
        .fetch_one(&self.pool)
        .await?;
        Ok(shift)
    }

    /// Ends the worker's active shift for one desk.
    pub async fn end_shop_shift(
        &self,
        parcel_id: &str,
        slug: &str,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<StoredShopShift, StorageError> {
        validate_work_slug(slug)?;
        let desk = self.shop_work_desk(parcel_id, slug).await?;
        let shift = sqlx::query_as::<_, StoredShopShift>(
            r#"
            update shop_work_shifts
            set status = $4, ended_at = now(), updated_at = now()
            where desk_id = $1
              and worker_player_id = $2
              and status = $3
            returning id,
                      $5::text as parcel_id,
                      $6::text as slug,
                      $7::text as worker_user,
                      worker_player_id, status,
                      to_char(started_at, 'YYYY-MM-DD HH24:MI:SS TZ') as started_at,
                      to_char(ended_at, 'YYYY-MM-DD HH24:MI:SS TZ') as ended_at
            "#,
        )
        .bind(desk.id)
        .bind(worker_player_id)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .bind(PARCEL_WORK_SHIFT_ENDED)
        .bind(&desk.parcel_id)
        .bind(&desk.slug)
        .bind(worker_user)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::ShopShiftNotActive {
            parcel_id: desk.parcel_id,
            slug: desk.slug,
        })?;
        Ok(shift)
    }

    /// Lists work items visible to an active in-parcel worker.
    pub async fn shop_work_items(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        slug: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredShopWorkItem>, StorageError> {
        if let Some(slug) = slug {
            validate_work_slug(slug)?;
        }
        let desk_ids = self
            .active_worker_desk_ids(parcel_id, worker_user, worker_player_id, slug)
            .await?;
        if desk_ids.is_empty() {
            let slug = slug.unwrap_or("*").to_owned();
            return Err(StorageError::ShopShiftNotActive {
                parcel_id: canonical_parcel_id(parcel_id).into_owned(),
                slug,
            });
        }
        self.shop_work_items_for_desks(&desk_ids, worker_player_id, limit)
            .await
    }

    /// Claims one queued work item for an active in-parcel worker.
    pub async fn claim_shop_work(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
    ) -> Result<StoredShopWorkItem, StorageError> {
        let item = self.shop_work_item(work_id).await?;
        if item.parcel_id != canonical_parcel_id(parcel_id).as_ref() {
            return Err(StorageError::ShopWorkItemNotFound(work_id));
        }
        self.ensure_active_shift_for_work(&item, worker_user, worker_player_id)
            .await?;
        if item.status != PARCEL_WORK_ITEM_QUEUED {
            return Err(StorageError::ShopWorkItemInvalidState(work_id));
        }
        let claim_query = format!(
            r#"
            update shop_work_items item
            set status = $3,
                assignee_user = $4,
                assignee_player_id = $5,
                updated_at = now()
            from shop_work_desks d, operator_commands cmd
            where item.id = $1
              and item.status = $2
              and d.id = item.desk_id
              and cmd.id = item.operator_command_id
            returning {}
            "#,
            shop_work_item_projection()
        );
        let claimed = sqlx::query_as::<_, StoredShopWorkItem>(&claim_query)
            .bind(work_id)
            .bind(PARCEL_WORK_ITEM_QUEUED)
            .bind(PARCEL_WORK_ITEM_CLAIMED)
            .bind(worker_user)
            .bind(worker_player_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StorageError::ShopWorkItemInvalidState(work_id))?;
        Ok(claimed)
    }

    /// Completes one claimed work item for an active in-parcel worker.
    pub async fn finish_shop_work(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        work_id: i64,
        result: &str,
    ) -> Result<StoredShopWorkItem, StorageError> {
        if !parcel_work_result_is_valid(result) {
            return Err(StorageError::InvalidShopWork(
                "invalid parcel work result".to_owned(),
            ));
        }
        let item = self.shop_work_item(work_id).await?;
        if item.parcel_id != canonical_parcel_id(parcel_id).as_ref() {
            return Err(StorageError::ShopWorkItemNotFound(work_id));
        }
        self.ensure_active_shift_for_work(&item, worker_user, worker_player_id)
            .await?;
        if item.status != PARCEL_WORK_ITEM_CLAIMED
            || item.assignee_player_id.as_deref() != Some(worker_player_id)
        {
            return Err(StorageError::ShopWorkItemInvalidState(work_id));
        }
        let done_query = format!(
            r#"
            update shop_work_items item
            set status = $3,
                result = $4,
                updated_at = now()
            from shop_work_desks d, operator_commands cmd
            where item.id = $1
              and item.status = $2
              and item.assignee_player_id = $5
              and d.id = item.desk_id
              and cmd.id = item.operator_command_id
            returning {}
            "#,
            shop_work_item_projection()
        );
        let done = sqlx::query_as::<_, StoredShopWorkItem>(&done_query)
            .bind(work_id)
            .bind(PARCEL_WORK_ITEM_CLAIMED)
            .bind(PARCEL_WORK_ITEM_DONE)
            .bind(result.trim())
            .bind(worker_player_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StorageError::ShopWorkItemInvalidState(work_id))?;
        Ok(done)
    }

    /// Adds or returns a command route from an owned shop into one work desk.
    pub async fn add_shop_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<StoredShopCommandRoute, StorageError> {
        validate_work_slug(slug)?;
        validate_command_prefix(command_prefix)?;
        let desk = self
            .owned_shop_work_desk(parcel_id, slug, owner_player_id)
            .await?;
        let route = sqlx::query_as::<_, StoredShopCommandRoute>(
            r#"
            insert into shop_work_routes (
                parcel_id, desk_id, owner_player_id, command_prefix
            )
            values ($1, $2, $3, $4)
            on conflict (parcel_id, desk_id, command_prefix) do update
            set owner_player_id = excluded.owner_player_id
            returning id, parcel_id,
                      $5::text as slug,
                      $6::text as desk_title,
                      command_prefix,
                      to_char(created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            "#,
        )
        .bind(&desk.parcel_id)
        .bind(desk.id)
        .bind(owner_player_id)
        .bind(command_prefix.trim())
        .bind(&desk.slug)
        .bind(&desk.title)
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
            select r.id, r.parcel_id, d.slug, d.title as desk_title,
                   r.command_prefix,
                   to_char(r.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_work_routes r
            join shop_work_desks d on d.id = r.desk_id
            where r.parcel_id = $1
            order by r.created_at desc, d.slug, r.command_prefix
            "#,
        )
        .bind(&parcel.parcel_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(routes)
    }

    /// Removes a command route from an owned parcel work desk.
    pub async fn remove_shop_command_route(
        &self,
        parcel_id: &str,
        owner_player_id: &str,
        slug: &str,
        command_prefix: &str,
    ) -> Result<StoredShopCommandRoute, StorageError> {
        validate_work_slug(slug)?;
        validate_command_prefix(command_prefix)?;
        let desk = self
            .owned_shop_work_desk(parcel_id, slug, owner_player_id)
            .await?;
        let route = sqlx::query_as::<_, StoredShopCommandRoute>(
            r#"
            select r.id, r.parcel_id, d.slug, d.title as desk_title,
                   r.command_prefix,
                   to_char(r.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_work_routes r
            join shop_work_desks d on d.id = r.desk_id
            where r.parcel_id = $1
              and r.desk_id = $2
              and r.command_prefix = $3
            "#,
        )
        .bind(&desk.parcel_id)
        .bind(desk.id)
        .bind(command_prefix.trim())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            StorageError::InvalidShopWork(format!(
                "parcel command route not found: {}/{} {}",
                desk.parcel_id,
                desk.slug,
                command_prefix.trim()
            ))
        })?;
        sqlx::query("delete from shop_work_routes where id = $1")
            .bind(route.id)
            .execute(&self.pool)
            .await?;
        Ok(route)
    }

    /// Dispatches one saved operator command into matching work queues.
    pub async fn dispatch_shop_command_routes<P>(
        &self,
        parcel: &P,
        command_id: i64,
    ) -> Result<Vec<StoredShopWorkItem>, StorageError>
    where
        P: ParcelView,
    {
        let command = self.operator_command(command_id).await?;
        if command.parcel_id != parcel.parcel_id() {
            return Err(StorageError::InvalidShopWork(format!(
                "parcel command {} belongs to {}, not {}",
                command.id,
                command.parcel_id,
                parcel.parcel_id()
            )));
        }
        let targets = matching_shop_command_route_targets(
            self.shop_command_route_targets(parcel.parcel_id()).await?,
            &command.raw_input,
        );
        let mut items = Vec::new();
        for target in targets {
            let item = self.insert_shop_work_item(&target, command.id).await?;
            items.push(item);
        }
        Ok(items)
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
                "[{} / {}]\nFrom: {}\n{}\n\nReply: /parcel chat {} {} -- <message>\nUnsubscribe: /parcel unsubscribe {} {}",
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
                    source_kind: Some("parcel_mailing_list_post"),
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

    async fn owned_shop_work_desk(
        &self,
        parcel_id: &str,
        slug: &str,
        owner_player_id: &str,
    ) -> Result<StoredShopWorkDesk, StorageError> {
        validate_work_slug(slug)?;
        let parcel = self.owned_built_parcel(parcel_id, owner_player_id).await?;
        self.shop_work_desk_by_parcel_slug(&parcel.parcel_id, slug)
            .await?
            .ok_or_else(|| StorageError::ShopWorkDeskNotFound {
                parcel_id: parcel.parcel_id,
                slug: slug.to_owned(),
            })
    }

    async fn shop_work_desk(
        &self,
        parcel_id: &str,
        slug: &str,
    ) -> Result<StoredShopWorkDesk, StorageError> {
        validate_work_slug(slug)?;
        let parcel_id = canonical_parcel_id(parcel_id);
        self.shop_work_desk_by_parcel_slug(parcel_id.as_ref(), slug)
            .await?
            .ok_or_else(|| StorageError::ShopWorkDeskNotFound {
                parcel_id: parcel_id.into_owned(),
                slug: slug.to_owned(),
            })
    }

    async fn shop_work_desk_by_parcel_slug(
        &self,
        parcel_id: &str,
        slug: &str,
    ) -> Result<Option<StoredShopWorkDesk>, StorageError> {
        let row = sqlx::query_as::<_, StoredShopWorkDesk>(
            r#"
            select d.id, d.parcel_id, d.owner_player_id, d.slug, d.title, d.status,
                   coalesce(queued_items.count, 0)::bigint as queued_count,
                   coalesce(active_workers.count, 0)::bigint as active_worker_count,
                   to_char(d.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_work_desks d
            left join lateral (
                select count(*) as count
                from shop_work_items item
                where item.desk_id = d.id
                  and item.status = $3
            ) queued_items on true
            left join lateral (
                select count(*) as count
                from shop_work_shifts shift
                where shift.desk_id = d.id
                  and shift.status = $4
            ) active_workers on true
            where d.parcel_id = $1
              and d.slug = $2
            "#,
        )
        .bind(parcel_id)
        .bind(slug)
        .bind(PARCEL_WORK_ITEM_QUEUED)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn shop_work_desks_for_parcel(
        &self,
        parcel_id: &str,
    ) -> Result<Vec<StoredShopWorkDesk>, StorageError> {
        let parcel_id = canonical_parcel_id(parcel_id);
        let rows = sqlx::query_as::<_, StoredShopWorkDesk>(
            r#"
            select d.id, d.parcel_id, d.owner_player_id, d.slug, d.title, d.status,
                   coalesce(queued_items.count, 0)::bigint as queued_count,
                   coalesce(active_workers.count, 0)::bigint as active_worker_count,
                   to_char(d.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at
            from shop_work_desks d
            left join lateral (
                select count(*) as count
                from shop_work_items item
                where item.desk_id = d.id
                  and item.status = $2
            ) queued_items on true
            left join lateral (
                select count(*) as count
                from shop_work_shifts shift
                where shift.desk_id = d.id
                  and shift.status = $3
            ) active_workers on true
            where d.parcel_id = $1
            order by d.created_at desc, d.slug
            "#,
        )
        .bind(parcel_id.as_ref())
        .bind(PARCEL_WORK_ITEM_QUEUED)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn shop_work_desk_count(&self, parcel_id: &str) -> Result<usize, StorageError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            select count(*)::bigint
            from shop_work_desks
            where parcel_id = $1
            "#,
        )
        .bind(parcel_id)
        .fetch_one(&self.pool)
        .await?;
        usize::try_from(count).map_err(|_| {
            StorageError::InvalidShopWork("work-desk count exceeds supported range".to_owned())
        })
    }

    async fn ensure_worker_assigned(
        &self,
        desk: &StoredShopWorkDesk,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<(), StorageError> {
        if desk.owner_player_id == worker_player_id {
            return Ok(());
        }
        let assigned = sqlx::query_scalar::<_, bool>(
            r#"
            select exists (
                select 1
                from shop_work_staff
                where desk_id = $1
                  and staff_user = $2
                  and status = $3
            )
            "#,
        )
        .bind(desk.id)
        .bind(worker_user)
        .bind(PARCEL_WORK_STAFF_ACTIVE)
        .fetch_one(&self.pool)
        .await?;
        if assigned {
            Ok(())
        } else {
            Err(StorageError::ShopWorkerNotAssigned {
                parcel_id: desk.parcel_id.clone(),
                slug: desk.slug.clone(),
            })
        }
    }

    async fn active_shop_shift(
        &self,
        desk_id: i64,
        worker_player_id: &str,
    ) -> Result<Option<StoredShopShift>, StorageError> {
        let row = sqlx::query_as::<_, StoredShopShift>(
            r#"
            select shift.id, d.parcel_id, d.slug,
                   shift.worker_user, shift.worker_player_id, shift.status,
                   to_char(shift.started_at, 'YYYY-MM-DD HH24:MI:SS TZ') as started_at,
                   to_char(shift.ended_at, 'YYYY-MM-DD HH24:MI:SS TZ') as ended_at
            from shop_work_shifts shift
            join shop_work_desks d on d.id = shift.desk_id
            where shift.desk_id = $1
              and shift.worker_player_id = $2
              and shift.status = $3
            "#,
        )
        .bind(desk_id)
        .bind(worker_player_id)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn active_worker_desk_ids(
        &self,
        parcel_id: &str,
        worker_user: &str,
        worker_player_id: &str,
        slug: Option<&str>,
    ) -> Result<Vec<i64>, StorageError> {
        let parcel_id = canonical_parcel_id(parcel_id);
        let rows = sqlx::query_scalar::<_, i64>(
            r#"
            select distinct d.id
            from shop_work_desks d
            join shop_work_shifts shift on shift.desk_id = d.id
            where d.parcel_id = $1
              and ($2::text is null or d.slug = $2)
              and shift.worker_user = $3
              and shift.worker_player_id = $4
              and shift.status = $5
              and (
                    d.owner_player_id = $4
                 or exists (
                        select 1
                        from shop_work_staff staff
                        where staff.desk_id = d.id
                          and staff.staff_user = $3
                          and staff.status = $6
                    )
              )
            order by d.id
            "#,
        )
        .bind(parcel_id.as_ref())
        .bind(slug)
        .bind(worker_user)
        .bind(worker_player_id)
        .bind(PARCEL_WORK_SHIFT_ACTIVE)
        .bind(PARCEL_WORK_STAFF_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn shop_work_items_for_desks(
        &self,
        desk_ids: &[i64],
        worker_player_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredShopWorkItem>, StorageError> {
        let query = format!(
            r#"
            select {}
            from shop_work_items item
            join shop_work_desks d on d.id = item.desk_id
            join operator_commands cmd on cmd.id = item.operator_command_id
            where item.desk_id = any($1)
              and (
                    item.status = $2
                 or (item.status = $3 and item.assignee_player_id = $4)
                 or item.status = $5
              )
            order by case item.status
                         when $2 then 0
                         when $3 then 1
                         when $5 then 2
                         else 3
                     end,
                     case when item.status = $5 then item.updated_at end desc,
                     item.updated_at asc,
                     item.id
            limit $6
            "#,
            shop_work_item_projection()
        );
        let rows = sqlx::query_as::<_, StoredShopWorkItem>(&query)
            .bind(desk_ids)
            .bind(PARCEL_WORK_ITEM_QUEUED)
            .bind(PARCEL_WORK_ITEM_CLAIMED)
            .bind(worker_player_id)
            .bind(PARCEL_WORK_ITEM_DONE)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn shop_work_item(&self, work_id: i64) -> Result<StoredShopWorkItem, StorageError> {
        let query = format!(
            r#"
            select {}
            from shop_work_items item
            join shop_work_desks d on d.id = item.desk_id
            join operator_commands cmd on cmd.id = item.operator_command_id
            where item.id = $1
            "#,
            shop_work_item_projection()
        );
        sqlx::query_as::<_, StoredShopWorkItem>(&query)
            .bind(work_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StorageError::ShopWorkItemNotFound(work_id))
    }

    async fn ensure_active_shift_for_work(
        &self,
        item: &StoredShopWorkItem,
        worker_user: &str,
        worker_player_id: &str,
    ) -> Result<(), StorageError> {
        let desk = self.shop_work_desk(&item.parcel_id, &item.slug).await?;
        self.ensure_worker_assigned(&desk, worker_user, worker_player_id)
            .await?;
        if self
            .active_shop_shift(desk.id, worker_player_id)
            .await?
            .is_some()
        {
            Ok(())
        } else {
            Err(StorageError::ShopShiftNotActive {
                parcel_id: item.parcel_id.clone(),
                slug: item.slug.clone(),
            })
        }
    }

    async fn insert_shop_work_item(
        &self,
        target: &ShopCommandRouteTarget,
        command_id: i64,
    ) -> Result<StoredShopWorkItem, StorageError> {
        let query = format!(
            r#"
            insert into shop_work_items as item (
                parcel_id, desk_id, operator_command_id, command_prefix
            )
            values ($1, $2, $3, $4)
            on conflict (desk_id, operator_command_id) do update
            set command_prefix = excluded.command_prefix
            returning {}
            "#,
            shop_work_item_projection_with_subqueries()
        );
        sqlx::query_as::<_, StoredShopWorkItem>(&query)
            .bind(&target.parcel_id)
            .bind(target.desk_id)
            .bind(command_id)
            .bind(&target.command_prefix)
            .fetch_one(&self.pool)
            .await
            .map_err(StorageError::from)
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
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
        .bind(PARCEL_MAILING_LIST_SUBSCRIPTION_ACTIVE)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Loads one stored operator command by id.
    pub async fn operator_command(
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
            select r.id, r.desk_id, r.parcel_id, d.slug, d.title as desk_title,
                   r.command_prefix
            from shop_work_routes r
            join shop_work_desks d on d.id = r.desk_id
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
    if parcel_mailing_list_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list slug".to_owned(),
        ))
    }
}

fn validate_title(title: &str) -> Result<(), StorageError> {
    if parcel_mailing_list_title_is_valid(title) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list title".to_owned(),
        ))
    }
}

fn validate_subject(subject: &str) -> Result<(), StorageError> {
    if parcel_mailing_list_subject_is_valid(subject) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list subject".to_owned(),
        ))
    }
}

fn validate_body(body: &str) -> Result<(), StorageError> {
    if parcel_mailing_list_body_is_valid(body) {
        Ok(())
    } else {
        Err(StorageError::InvalidMailingList(
            "invalid mailing-list body".to_owned(),
        ))
    }
}

fn validate_work_slug(slug: &str) -> Result<(), StorageError> {
    if parcel_work_desk_slug_is_valid(slug) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopWork(
            "invalid parcel work desk slug".to_owned(),
        ))
    }
}

fn validate_work_title(title: &str) -> Result<(), StorageError> {
    if parcel_work_desk_title_is_valid(title) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopWork(
            "invalid parcel work desk title".to_owned(),
        ))
    }
}

fn validate_command_prefix(command_prefix: &str) -> Result<(), StorageError> {
    if parcel_command_route_prefix_is_valid(command_prefix) {
        Ok(())
    } else {
        Err(StorageError::InvalidShopWork(
            "invalid parcel command route prefix".to_owned(),
        ))
    }
}

fn shop_work_item_projection() -> &'static str {
    r#"
    item.id, item.parcel_id, d.slug, d.title as desk_title,
    item.operator_command_id, item.command_prefix, item.status,
    cmd.sender_user, cmd.raw_input,
    item.assignee_user, item.assignee_player_id, item.result,
    to_char(item.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
    to_char(item.updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
    "#
}

fn shop_work_item_projection_with_subqueries() -> &'static str {
    r#"
    item.id, item.parcel_id,
    (select d.slug from shop_work_desks d where d.id = item.desk_id) as slug,
    (select d.title from shop_work_desks d where d.id = item.desk_id) as desk_title,
    item.operator_command_id, item.command_prefix, item.status,
    (select cmd.sender_user from operator_commands cmd where cmd.id = item.operator_command_id) as sender_user,
    (select cmd.raw_input from operator_commands cmd where cmd.id = item.operator_command_id) as raw_input,
    item.assignee_user, item.assignee_player_id, item.result,
    to_char(item.created_at, 'YYYY-MM-DD HH24:MI:SS TZ') as created_at,
    to_char(item.updated_at, 'YYYY-MM-DD HH24:MI:SS TZ') as updated_at
    "#
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
        if let Some(index) = target_indexes.get(&target.desk_id).copied() {
            let current_prefix = &matches[index].command_prefix;
            if route_prefix_specificity(&target.command_prefix)
                > route_prefix_specificity(current_prefix)
            {
                matches[index] = target;
            }
        } else {
            target_indexes.insert(target.desk_id, matches.len());
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

fn sender_can_post_to_shop_chat(
    current_owner_player_id: Option<&str>,
    sender_player_id: &str,
    sender_has_subscription: bool,
) -> bool {
    current_owner_player_id == Some(sender_player_id) || sender_has_subscription
}
