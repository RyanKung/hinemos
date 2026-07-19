use crate::*;

/// Storage boundary for loading and deactivating service-room registrations.
pub trait RoomRegistrationStore {
    /// Store error type.
    type Error;

    /// Disables every service room not present in the provided registration list.
    async fn disable_service_rooms_except(&self, view_ids: Vec<String>)
    -> Result<u64, Self::Error>;

    /// Upserts a single service-room registration.
    async fn upsert_service_room(
        &self,
        registration: ServiceRoomRegistrationUpsert<'_>,
    ) -> Result<(), Self::Error>;

    /// Resolves a room player's id by room user name if it exists.
    async fn ssh_identity_player_id_for_user(
        &self,
        room_user: &str,
    ) -> Result<Option<String>, Self::Error>;
}

/// Rooms.ron registration entry for an externally hosted service room.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ServiceRoomRegistration {
    /// Runtime view id for the room.
    pub view_id: String,
    /// Optional view shown when the room is rendered as a street-facing parcel.
    #[serde(default)]
    pub front_view_id: Option<String>,
    /// Optional entity required to enter the room from the front view.
    #[serde(default)]
    pub front_entity_id: Option<String>,
    /// Short address users type with /enter.
    #[serde(default)]
    pub address: Option<String>,
    /// Player-facing label for the room.
    #[serde(default)]
    pub label: Option<String>,
    /// Extra aliases accepted by /enter.
    #[serde(default)]
    pub enter_aliases: Option<String>,
    /// Room service user name.
    pub room_user: String,
    /// Room service player id.
    pub room_player_id: String,
    /// Optional status text shown inside the room.
    #[serde(default)]
    pub status_text: Option<String>,
    /// Optional custom slash commands accepted inside the room.
    #[serde(default)]
    pub custom_commands: Option<String>,
    /// Optional custom slash commands that count as hunger recovery.
    #[serde(default)]
    pub recovery_commands: Option<String>,
    /// Whether the room is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Storage-ready service-room registration fields.
#[derive(Debug, Clone, Copy)]
pub struct ServiceRoomRegistrationUpsert<'a> {
    /// Runtime view id for the room.
    pub view_id: &'a str,
    /// Optional view where the room entrance appears.
    pub front_view_id: Option<&'a str>,
    /// Optional entity required to enter the room from the front view.
    pub front_entity_id: Option<&'a str>,
    /// Player-facing address.
    pub address: Option<&'a str>,
    /// Player-facing label.
    pub label: Option<&'a str>,
    /// Comma-separated aliases accepted by /enter.
    pub enter_aliases: Option<&'a str>,
    /// Room service user name.
    pub room_user: &'a str,
    /// Room service player id.
    pub room_player_id: &'a str,
    /// Optional status text shown inside the room.
    pub status_text: Option<&'a str>,
    /// Optional custom slash commands accepted inside the room.
    pub custom_commands: Option<&'a str>,
    /// Optional custom slash commands that count as hunger recovery.
    pub recovery_commands: Option<&'a str>,
    /// Whether the registration is enabled after validation.
    pub enabled: bool,
}

/// Cache invalidation hook for service-room registration reloads.
pub trait RoomRegistrationCache {
    /// Invalidates room cache state associated with one service room registration.
    async fn invalidate_room_cache_for_service_room(
        &self,
        view_id: &str,
        old_room_user: Option<&str>,
        old_front_view_id: Option<&str>,
        new_room_user: Option<&str>,
        new_front_view_id: Option<&str>,
    );
}

impl RoomRegistrationCache for () {
    async fn invalidate_room_cache_for_service_room(
        &self,
        _view_id: &str,
        _old_room_user: Option<&str>,
        _old_front_view_id: Option<&str>,
        _new_room_user: Option<&str>,
        _new_front_view_id: Option<&str>,
    ) {
    }
}

impl<S> AppService<S> {
    /// Returns the normalized slash command inputs declared by a service room.
    #[must_use]
    pub fn service_room_enter_tokens(registration: &ServiceRoomRegistration) -> Vec<String> {
        let mut tokens = Vec::new();
        if let Some(address) = registration.address.as_deref() {
            tokens.push(normalize_enter_token(address));
        }
        if let Some(label) = registration.label.as_deref() {
            tokens.push(normalize_enter_token(label));
        }
        tokens.extend(
            registration
                .enter_aliases
                .as_deref()
                .unwrap_or_default()
                .split([',', ';', '\n', ' '])
                .map(normalize_enter_token),
        );
        tokens
            .into_iter()
            .filter(|token| !token.is_empty())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }
}

impl<S, E> AppService<S>
where
    S: ParcelRegistryStore<Error = E>,
{
    /// Checks whether the proposed room aliases conflict with parcels or previously claimed aliases.
    pub async fn service_room_alias_conflict(
        storage: &S,
        registration: &ServiceRoomRegistration,
        claimed_aliases: &mut HashMap<(String, String), String>,
    ) -> Result<Option<String>, E> {
        let Some(front_view_id) = registration.front_view_id.as_deref() else {
            return Ok(None);
        };
        let tokens = Self::service_room_enter_tokens(registration);
        if tokens.is_empty() {
            return Ok(None);
        }

        let parcels = storage.parcels_by_front_view(front_view_id).await?;
        for token in &tokens {
            if let Some(parcel) = parcels.iter().find(|parcel| {
                normalize_enter_token(parcel.parcel_id()) == *token
                    || parcel
                        .title()
                        .is_some_and(|title| normalize_enter_token(title) == *token)
            }) {
                return Ok(Some(format!(
                    "`{token}` conflicts with parcel {} in {front_view_id}",
                    parcel.parcel_id()
                )));
            }
            let key = (front_view_id.to_owned(), token.clone());
            if let Some(existing_view_id) = claimed_aliases.get(&key) {
                return Ok(Some(format!(
                    "`{token}` conflicts with service room {existing_view_id} in {front_view_id}"
                )));
            }
        }
        for token in tokens {
            claimed_aliases.insert(
                (front_view_id.to_owned(), token),
                registration.view_id.clone(),
            );
        }
        Ok(None)
    }
}

impl<S> AppService<S> {
    /// Loads room registrations from `rooms.ron`, deactivates stale rooms, and upserts the current set.
    pub async fn load_service_room_registrations<I>(
        storage: &S,
        world_dir: &Path,
        world: &WorldState,
        shared: Option<&I>,
    ) -> Result<()>
    where
        S: RoomRegistrationStore
            + RoomStore<Error = <S as RoomRegistrationStore>::Error>
            + ParcelRegistryStore<Error = <S as RoomRegistrationStore>::Error>,
        <S as RoomStore>::RoomBinding: ServiceRoomView,
        S::ServiceRoom: ServiceRoomView,
        <S as RoomRegistrationStore>::Error: std::error::Error + Send + Sync + 'static,
        I: RoomRegistrationCache,
    {
        let Some(registrations) = Self::read_service_room_registrations(world_dir)? else {
            return Ok(());
        };
        storage
            .disable_service_rooms_except(Self::registered_service_room_view_ids(&registrations))
            .await?;
        let mut claimed_aliases = HashMap::<(String, String), String>::new();
        for registration in registrations {
            Self::upsert_service_room_registration(
                storage,
                world,
                shared,
                &mut claimed_aliases,
                registration,
            )
            .await?;
        }
        Ok(())
    }

    fn read_service_room_registrations(
        world_dir: &Path,
    ) -> Result<Option<Vec<ServiceRoomRegistration>>> {
        let path = world_dir.join("rooms.ron");
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path).with_context(|| {
            format!("failed to read room registrations from {}", path.display())
        })?;
        let registrations = ron::from_str(&content).with_context(|| {
            format!("failed to parse room registrations from {}", path.display())
        })?;
        Ok(Some(registrations))
    }

    fn registered_service_room_view_ids(registrations: &[ServiceRoomRegistration]) -> Vec<String> {
        registrations
            .iter()
            .map(|registration| registration.view_id.clone())
            .collect()
    }

    async fn upsert_service_room_registration<I>(
        storage: &S,
        world: &WorldState,
        shared: Option<&I>,
        claimed_aliases: &mut HashMap<(String, String), String>,
        registration: ServiceRoomRegistration,
    ) -> Result<()>
    where
        S: RoomRegistrationStore
            + RoomStore<Error = <S as RoomRegistrationStore>::Error>
            + ParcelRegistryStore<Error = <S as RoomRegistrationStore>::Error>,
        <S as RoomStore>::RoomBinding: ServiceRoomView,
        S::ServiceRoom: ServiceRoomView,
        <S as RoomRegistrationStore>::Error: std::error::Error + Send + Sync + 'static,
        I: RoomRegistrationCache,
    {
        let existing_room = storage.room_binding_by_view(&registration.view_id).await?;
        let existing_front_view_id = existing_room
            .as_ref()
            .and_then(|room| room.front_view_id().map(str::to_owned));
        let enabled =
            Self::service_room_registration_enabled(storage, world, &registration, claimed_aliases)
                .await?;
        Self::warn_on_room_player_mismatch(storage, &registration).await?;
        storage
            .upsert_service_room(ServiceRoomRegistrationUpsert {
                view_id: &registration.view_id,
                front_view_id: registration.front_view_id.as_deref(),
                front_entity_id: registration.front_entity_id.as_deref(),
                address: registration.address.as_deref(),
                label: registration.label.as_deref(),
                enter_aliases: registration.enter_aliases.as_deref(),
                room_user: &registration.room_user,
                room_player_id: &registration.room_player_id,
                status_text: registration.status_text.as_deref(),
                custom_commands: registration.custom_commands.as_deref(),
                recovery_commands: registration.recovery_commands.as_deref(),
                enabled,
            })
            .await?;
        if let Some(shared) = shared {
            shared
                .invalidate_room_cache_for_service_room(
                    &registration.view_id,
                    existing_room.as_ref().and_then(|room| room.room_user()),
                    existing_front_view_id.as_deref(),
                    Some(&registration.room_user),
                    registration.front_view_id.as_deref(),
                )
                .await;
        }
        Ok(())
    }

    async fn service_room_registration_enabled(
        storage: &S,
        world: &WorldState,
        registration: &ServiceRoomRegistration,
        claimed_aliases: &mut HashMap<(String, String), String>,
    ) -> Result<bool>
    where
        S: ParcelRegistryStore,
        <S as ParcelRegistryStore>::Error: std::error::Error + Send + Sync + 'static,
    {
        if !registration
            .front_view_id
            .as_deref()
            .is_some_and(|front_view_id| world.views.contains_key(front_view_id))
        {
            if registration.enabled {
                eprintln!(
                    "disabling service room {}: front_view_id is missing or not in world",
                    registration.view_id
                );
            }
            return Ok(false);
        }
        if registration.enabled
            && let Some(reason) =
                Self::service_room_alias_conflict(storage, registration, claimed_aliases).await?
        {
            eprintln!(
                "disabling service room {}: enter alias conflict: {reason}",
                registration.view_id
            );
            return Ok(false);
        }
        Ok(registration.enabled)
    }

    async fn warn_on_room_player_mismatch(
        storage: &S,
        registration: &ServiceRoomRegistration,
    ) -> Result<()>
    where
        S: RoomRegistrationStore,
        <S as RoomRegistrationStore>::Error: std::error::Error + Send + Sync + 'static,
    {
        if let Some(player_id) = storage
            .ssh_identity_player_id_for_user(&registration.room_user)
            .await?
            .filter(|player_id| player_id != &registration.room_player_id)
        {
            eprintln!(
                "service room {} room_player_id mismatch for room_user {}: rooms.ron={} ssh_identity={}",
                registration.view_id,
                registration.room_user,
                registration.room_player_id,
                player_id
            );
        }
        Ok(())
    }
}

fn default_enabled() -> bool {
    true
}

fn normalize_enter_token(token: &str) -> String {
    token.trim().to_ascii_lowercase()
}
