use super::*;

pub(super) struct PendingAdmission;

#[derive(Debug, Clone)]
pub(super) struct TestAdmission {
    pub(super) admission_state: String,
    pub(super) agreement_version: Option<String>,
    pub(super) agreement_read_version: Option<String>,
    pub(super) role_card_name_valid: bool,
    pub(super) role_card_has_mbti: bool,
}

impl AdmissionView for TestAdmission {
    fn is_agreed(&self) -> bool {
        self.admission_state == ADMISSION_STATE_AGREED
    }

    fn has_read_version(&self, version: &str) -> bool {
        self.agreement_read_version.as_deref() == Some(version)
    }

    fn role_card_name_is_valid(&self) -> bool {
        self.role_card_name_valid
    }

    fn role_card_has_mbti(&self) -> bool {
        self.role_card_has_mbti
    }
}

#[derive(Debug)]
pub(super) struct TestAdmissionStore {
    pub(super) admission: Mutex<TestAdmission>,
}

#[derive(Debug, Clone)]
pub(super) struct TestServiceRoom {
    pub(super) view_id: &'static str,
    pub(super) label: Option<&'static str>,
    pub(super) address: Option<&'static str>,
    pub(super) front_view_id: Option<&'static str>,
    pub(super) room_user: &'static str,
    pub(super) status_text: Option<&'static str>,
    pub(super) custom_commands: Option<&'static str>,
}

pub(super) struct TestRoomBinding {
    pub(super) view_id: &'static str,
    pub(super) front_entity_id: Option<&'static str>,
    pub(super) address: &'static str,
    pub(super) label: &'static str,
    pub(super) enter_aliases: Vec<String>,
}

pub(super) struct TestRoomStore {
    pub(super) service_room: Option<TestServiceRoom>,
}

pub(super) struct TestBindingOnlyRoomStore {
    pub(super) room_binding: Option<TestRoomBinding>,
}

#[derive(Debug, Default)]
pub(super) struct TestServiceRoomCommandStore {
    pub(super) service_room: Option<TestServiceRoom>,
    pub(super) calls: Mutex<Vec<String>>,
}

#[derive(Default)]
pub(super) struct TestRegistrationStore {
    pub(super) existing_rooms: HashMap<String, RegistrationServiceRoom>,
    pub(super) parcels_by_front_view: HashMap<String, Vec<RegistrationParcel>>,
    pub(super) disable_calls: Mutex<Vec<Vec<String>>>,
    pub(super) upsert_calls: Mutex<Vec<(String, bool)>>,
}

#[derive(Debug, Clone)]
pub(super) struct RegistrationServiceRoom {
    pub(super) front_view_id: Option<&'static str>,
}

#[derive(Clone)]
pub(super) struct RegistrationParcel {
    pub(super) parcel_id: &'static str,
    pub(super) front_view_id: &'static str,
    pub(super) title: Option<&'static str>,
}

impl RoomBindingEntryView for TestRoomBinding {
    fn view_id(&self) -> &str {
        self.view_id
    }

    fn front_entity_id(&self) -> Option<&str> {
        self.front_entity_id
    }

    fn address(&self) -> &str {
        self.address
    }

    fn label(&self) -> &str {
        self.label
    }

    fn enter_aliases(&self) -> &[String] {
        &self.enter_aliases
    }
}

impl RoomBindingKindView for TestRoomBinding {
    fn is_commercial_parcel(&self) -> bool {
        false
    }

    fn is_service_room(&self) -> bool {
        true
    }
}

impl RoomMailboxView for TestRoomBinding {
    fn view_id(&self) -> &str {
        self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        Some("room-user")
    }

    fn room_player_id(&self) -> Option<&str> {
        Some("room-player")
    }
}

impl RoomCommandPolicyView for TestRoomBinding {
    fn forwards_all_input(&self) -> bool {
        false
    }

    fn listed_commands(&self) -> &[String] {
        &self.enter_aliases
    }
}

impl ServiceRoomView for TestRoomBinding {
    fn label(&self) -> Option<&str> {
        Some(self.label)
    }

    fn address(&self) -> Option<&str> {
        Some(self.address)
    }

    fn front_view_id(&self) -> Option<&str> {
        Some("arrival_street")
    }

    fn status_text(&self) -> Option<&str> {
        Some("Room status.")
    }

    fn custom_commands(&self) -> Option<&str> {
        Some("/room ask <question>")
    }

    fn recovery_commands(&self) -> Option<&str> {
        None
    }
}

impl ServiceRoomView for TestServiceRoom {
    fn label(&self) -> Option<&str> {
        self.label
    }

    fn address(&self) -> Option<&str> {
        self.address
    }

    fn front_view_id(&self) -> Option<&str> {
        self.front_view_id
    }

    fn status_text(&self) -> Option<&str> {
        self.status_text
    }

    fn custom_commands(&self) -> Option<&str> {
        self.custom_commands
    }

    fn recovery_commands(&self) -> Option<&str> {
        None
    }
}

impl RoomMailboxView for TestServiceRoom {
    fn view_id(&self) -> &str {
        self.view_id
    }

    fn room_user(&self) -> Option<&str> {
        Some(self.room_user)
    }

    fn room_player_id(&self) -> Option<&str> {
        Some("room-player")
    }
}

impl RoomCommandPolicyView for TestServiceRoom {
    fn forwards_all_input(&self) -> bool {
        false
    }

    fn listed_commands(&self) -> &[String] {
        &[]
    }
}

impl RoomBindingKindView for TestServiceRoom {
    fn is_commercial_parcel(&self) -> bool {
        false
    }

    fn is_service_room(&self) -> bool {
        true
    }
}

impl RoomStore for TestRoomStore {
    type Error = std::convert::Infallible;
    type ServiceRoom = TestServiceRoom;
    type RoomBinding = TestRoomBinding;

    async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error> {
        Ok(self
            .service_room
            .as_ref()
            .filter(|room| room.view_id == view_id)
            .map(|room| TestServiceRoom {
                view_id: room.view_id,
                label: room.label,
                address: room.address,
                front_view_id: room.front_view_id,
                room_user: room.room_user,
                status_text: room.status_text,
                custom_commands: room.custom_commands,
            }))
    }

    async fn room_bindings_by_front_view(
        &self,
        _front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error> {
        Ok(Vec::new())
    }

    async fn room_binding_by_view(
        &self,
        _view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error> {
        Ok(None)
    }
}

impl RoomStore for TestBindingOnlyRoomStore {
    type Error = std::convert::Infallible;
    type ServiceRoom = TestServiceRoom;
    type RoomBinding = TestRoomBinding;

    async fn service_room_by_view(
        &self,
        _view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error> {
        Ok(None)
    }

    async fn room_bindings_by_front_view(
        &self,
        _front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error> {
        Ok(Vec::new())
    }

    async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error> {
        Ok(self
            .room_binding
            .as_ref()
            .filter(|binding| binding.view_id == view_id)
            .map(|binding| TestRoomBinding {
                view_id: binding.view_id,
                front_entity_id: binding.front_entity_id,
                address: binding.address,
                label: binding.label,
                enter_aliases: binding.enter_aliases.clone(),
            }))
    }
}

impl RoomStore for TestServiceRoomCommandStore {
    type Error = std::convert::Infallible;
    type ServiceRoom = TestServiceRoom;
    type RoomBinding = TestServiceRoom;

    async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error> {
        Ok(self
            .service_room
            .as_ref()
            .filter(|room| room.view_id == view_id)
            .cloned())
    }

    async fn room_bindings_by_front_view(
        &self,
        _front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error> {
        Ok(Vec::new())
    }

    async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error> {
        Ok(self
            .service_room
            .as_ref()
            .filter(|room| room.view_id == view_id)
            .cloned())
    }
}

impl MailStore for TestServiceRoomCommandStore {
    type Error = std::convert::Infallible;
    type InboxItem = TestInboxItem;

    async fn save_room_mailbox_input<M>(
        &self,
        mailbox: &M,
        sender_user: &str,
        sender_player_id: &str,
        raw_input: &str,
    ) -> Result<Self::InboxItem, Self::Error>
    where
        M: RoomMailboxView + Sync,
    {
        self.calls.lock().unwrap().push(format!(
            "mailbox:{}:{}:{}:{}",
            mailbox.view_id(),
            sender_user,
            sender_player_id,
            raw_input
        ));
        Ok(TestInboxItem {
            id: 17,
            kind: "room_command",
            sender_user: "room-user",
            subject: "Room command #17 for external_room",
            body: "body",
        })
    }
}

impl RoomStore for TestRegistrationStore {
    type Error = std::convert::Infallible;
    type ServiceRoom = RegistrationServiceRoom;
    type RoomBinding = RegistrationServiceRoom;

    async fn service_room_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::ServiceRoom>, Self::Error> {
        Ok(self.existing_rooms.get(view_id).cloned())
    }

    async fn room_bindings_by_front_view(
        &self,
        _front_view_id: &str,
    ) -> Result<Vec<Self::RoomBinding>, Self::Error> {
        Ok(Vec::new())
    }

    async fn room_binding_by_view(
        &self,
        view_id: &str,
    ) -> Result<Option<Self::RoomBinding>, Self::Error> {
        Ok(self.existing_rooms.get(view_id).cloned())
    }
}

impl ServiceRoomView for RegistrationServiceRoom {
    fn label(&self) -> Option<&str> {
        None
    }

    fn address(&self) -> Option<&str> {
        None
    }

    fn front_view_id(&self) -> Option<&str> {
        self.front_view_id
    }

    fn status_text(&self) -> Option<&str> {
        None
    }

    fn custom_commands(&self) -> Option<&str> {
        None
    }

    fn recovery_commands(&self) -> Option<&str> {
        None
    }
}

impl RoomRegistrationStore for TestRegistrationStore {
    type Error = std::convert::Infallible;

    async fn disable_service_rooms_except(
        &self,
        view_ids: Vec<String>,
    ) -> Result<u64, Self::Error> {
        self.disable_calls.lock().unwrap().push(view_ids);
        Ok(0)
    }

    async fn upsert_service_room(
        &self,
        registration: ServiceRoomRegistrationUpsert<'_>,
    ) -> Result<(), Self::Error> {
        self.upsert_calls
            .lock()
            .unwrap()
            .push((registration.view_id.to_owned(), registration.enabled));
        Ok(())
    }

    async fn ssh_identity_player_id_for_user(
        &self,
        _room_user: &str,
    ) -> Result<Option<String>, Self::Error> {
        Ok(None)
    }
}

impl ParcelStore for TestRegistrationStore {
    type Error = std::convert::Infallible;
    type Parcel = RegistrationParcel;

    async fn list_commercial_parcels(&self) -> Result<Vec<Self::Parcel>, Self::Error> {
        Ok(Vec::new())
    }

    async fn commercial_parcels_by_front_view(
        &self,
        front_view_id: &str,
    ) -> Result<Vec<Self::Parcel>, Self::Error> {
        Ok(self
            .parcels_by_front_view
            .get(front_view_id)
            .cloned()
            .unwrap_or_default())
    }
}

impl RoomMailboxView for RegistrationServiceRoom {
    fn view_id(&self) -> &str {
        "registration-room"
    }

    fn room_user(&self) -> Option<&str> {
        Some("room-user")
    }

    fn room_player_id(&self) -> Option<&str> {
        Some("room-player")
    }
}

impl ParcelView for RegistrationParcel {
    fn parcel_id(&self) -> &str {
        self.parcel_id
    }

    fn view_id(&self) -> &str {
        "parcel-view"
    }

    fn front_view_id(&self) -> &str {
        self.front_view_id
    }

    fn district(&self) -> &str {
        "north"
    }

    fn position(&self) -> i32 {
        1
    }

    fn owner_user(&self) -> Option<&str> {
        None
    }

    fn owner_player_id(&self) -> Option<&str> {
        None
    }

    fn room_user(&self) -> Option<&str> {
        None
    }

    fn room_player_id(&self) -> Option<&str> {
        None
    }

    fn status(&self) -> &str {
        "vacant"
    }

    fn title(&self) -> Option<&str> {
        self.title
    }

    fn description(&self) -> Option<&str> {
        None
    }

    fn style(&self) -> Option<&str> {
        None
    }

    fn operator_prompt(&self) -> Option<&str> {
        None
    }

    fn custom_commands(&self) -> Option<&str> {
        None
    }
}

impl AdmissionStore for TestAdmissionStore {
    type Error = std::convert::Infallible;
    type Admission = TestAdmission;

    async fn player_admission(&self, _player_id: &str) -> Result<Self::Admission, Self::Error> {
        Ok(self.admission.lock().unwrap().clone())
    }

    async fn mark_agreement_read(
        &self,
        _player_id: &str,
        agreement_version: &str,
    ) -> Result<(), Self::Error> {
        let mut admission = self.admission.lock().unwrap();
        admission.agreement_read_version = Some(agreement_version.to_owned());
        Ok(())
    }

    async fn admit_player(
        &self,
        _player_id: &str,
        agreement_version: &str,
    ) -> Result<(), Self::Error> {
        let mut admission = self.admission.lock().unwrap();
        admission.admission_state = ADMISSION_STATE_AGREED.to_owned();
        admission.agreement_version = Some(agreement_version.to_owned());
        Ok(())
    }
}

impl AdmissionView for PendingAdmission {
    fn is_agreed(&self) -> bool {
        false
    }

    fn has_read_version(&self, _version: &str) -> bool {
        false
    }

    fn role_card_name_is_valid(&self) -> bool {
        false
    }

    fn role_card_has_mbti(&self) -> bool {
        false
    }
}
