use sqlx::postgres::PgPool;

use crate::{StorageError, StoredParcel};

pub(crate) async fn seed_commercial_parcels(pool: &PgPool) -> Result<(), StorageError> {
    migrate_legacy_parcel_ids(pool).await?;
    for (district, prefix) in [("north", "N"), ("south", "S")] {
        for position in 1..=10 {
            let parcel_id = format!("{prefix}{position}");
            let view_id = format!("parcel_{parcel_id}");
            let front_view_id = parcel_front_view_id(district, position);
            sqlx::query(
                r#"
                insert into commercial_parcels (parcel_id, view_id, front_view_id, district, position)
                values ($1, $2, $3, $4, $5)
                on conflict (parcel_id) do update
                set front_view_id = excluded.front_view_id
                where commercial_parcels.front_view_id is null
                "#,
            )
            .bind(parcel_id)
            .bind(view_id)
            .bind(front_view_id)
            .bind(district)
            .bind(position)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

fn parcel_front_view_id(district: &str, position: i32) -> String {
    let segment = ((position - 1) / 2) + 1;
    format!("street_{district}_{segment:02}")
}

async fn migrate_legacy_parcel_ids(pool: &PgPool) -> Result<(), StorageError> {
    for (district, prefix) in [("north", "N"), ("south", "S")] {
        for position in (1..=5).rev() {
            let old_id = format!("{district}_{position:02}");
            let new_position = position * 2 - 1;
            let new_id = format!("{prefix}{new_position}");
            let new_view = format!("parcel_{new_id}");
            sqlx::query(
                r#"
                update commercial_parcels
                set parcel_id = $2, view_id = $3, position = $4, updated_at = now()
                where parcel_id = $1
                  and not exists (
                      select 1 from commercial_parcels existing
                      where existing.parcel_id = $2
                  )
                "#,
            )
            .bind(old_id)
            .bind(new_id)
            .bind(new_view)
            .bind(new_position)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

pub(crate) async fn fetch_parcel_by_id(
    pool: &PgPool,
    parcel_id: &str,
) -> Result<StoredParcel, StorageError> {
    let parcel = sqlx::query_as::<_, StoredParcel>(
        r#"
        select parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
               room_user, room_player_id,
               status, title, description, style, operator_prompt, custom_commands
        from commercial_parcels
        where parcel_id = $1
        "#,
    )
    .bind(parcel_id)
    .fetch_optional(pool)
    .await?;

    parcel.ok_or_else(|| StorageError::ParcelNotFound(parcel_id.to_owned()))
}
