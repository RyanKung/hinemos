use std::borrow::Cow;

use hinemos_core::{GridParcelAddress, GridRoad, PARCEL_STATUS_VACANT};
use sqlx::postgres::PgPool;

use crate::{StorageError, StoredParcel};

pub(crate) async fn seed_parcels(pool: &PgPool) -> Result<(), StorageError> {
    migrate_legacy_parcel_ids(pool).await?;
    delete_unowned_legacy_seed_parcels(pool).await
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
                update parcels
                set parcel_id = $2, view_id = $3, position = $4, updated_at = now()
                where parcel_id = $1
                  and not exists (
                      select 1 from parcels existing
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

async fn delete_unowned_legacy_seed_parcels(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        delete from parcels
        where status = $1
          and owner_player_id is null
          and district in ('north', 'south')
          and position between 1 and 10
        "#,
    )
    .bind(PARCEL_STATUS_VACANT)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn fetch_parcel_by_id(
    pool: &PgPool,
    parcel_id: &str,
) -> Result<StoredParcel, StorageError> {
    let lookup_id = canonical_parcel_id(parcel_id);
    let parcel = sqlx::query_as::<_, StoredParcel>(
        r#"
        select parcel_id, view_id, front_view_id, district, position, owner_user, owner_player_id,
               room_user, room_player_id,
               status, title, description, style, operator_prompt, custom_commands
        from parcels
        where parcel_id = $1
        "#,
    )
    .bind(lookup_id.as_ref())
    .fetch_optional(pool)
    .await?;

    if let Some(parcel) = parcel {
        return Ok(parcel);
    }

    if let Some(address) = GridParcelAddress::from_parcel_id(parcel_id) {
        return Ok(virtual_grid_parcel(address));
    }

    Err(StorageError::ParcelNotFound(parcel_id.to_owned()))
}

pub(crate) fn canonical_parcel_id(parcel_id: &str) -> Cow<'_, str> {
    GridParcelAddress::canonical_parcel_id(parcel_id)
        .map(Cow::Owned)
        .unwrap_or_else(|| Cow::Borrowed(parcel_id))
}

pub(crate) async fn ensure_grid_parcel(pool: &PgPool, parcel_id: &str) -> Result<(), StorageError> {
    let Some(address) = GridParcelAddress::from_parcel_id(parcel_id) else {
        return Ok(());
    };
    sqlx::query(
        r#"
        insert into parcels (parcel_id, view_id, front_view_id, district, position)
        values ($1, $2, $3, $4, $5)
        on conflict (parcel_id) do nothing
        "#,
    )
    .bind(address.parcel_id())
    .bind(address.view_id())
    .bind(address.front_view_id())
    .bind(address.district())
    .bind(address.position())
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) fn virtual_grid_parcels_for_front_view(
    front_view_id: &str,
) -> Option<Vec<StoredParcel>> {
    let road = GridRoad::from_view_id(front_view_id)?;
    Some(
        road.parcel_addresses()
            .into_iter()
            .map(virtual_grid_parcel)
            .collect(),
    )
}

pub(crate) fn virtual_grid_parcel_by_view(view_id: &str) -> Option<StoredParcel> {
    GridParcelAddress::from_view_id(view_id).map(virtual_grid_parcel)
}

fn virtual_grid_parcel(address: GridParcelAddress) -> StoredParcel {
    StoredParcel {
        parcel_id: address.parcel_id(),
        view_id: address.view_id(),
        front_view_id: address.front_view_id(),
        district: address.district(),
        position: address.position(),
        owner_user: None,
        owner_player_id: None,
        room_user: None,
        room_player_id: None,
        status: PARCEL_STATUS_VACANT.to_owned(),
        title: None,
        description: None,
        style: None,
        operator_prompt: None,
        custom_commands: None,
    }
}
