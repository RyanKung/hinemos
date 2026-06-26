use hinemos_app::{
    AccountSettingsView, AccountStore, AdmissionStore, AdmissionView, BalanceView, BuildStore,
    InboxItemView, InboxStore, LandStore, MailAuthTokenView, MailDaemonStore, MailStore,
    MemoryAtomView, MemoryEventView, MemoryStore, MessageStore, ParcelStore, ParcelView,
    PaymentRequestView, PaymentStore, PlayerStateStore as AppPlayerStateStore, RecentPresenceUser,
    RoomBindingEntryView, RoomBindingKindView, RoomCommandPolicyView, RoomMailboxView,
    RoomRegistrationStore, RoomStore, SelfModelView, ServiceRoomRegistrationUpsert,
    ServiceRoomView, ShopStore, SocialEdgeView, TransferView, ViewPresenceStore, WorldMessageView,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

use crate::{
    PgStorage, ServiceRoomUpsert, StorageError, StoredAccountSettings, StoredAdmission,
    StoredAgentSelfModel, StoredBalance, StoredInboxItem, StoredMailAuthToken, StoredMemoryAtom,
    StoredMemoryEvent, StoredOperatorCommand, StoredParcel, StoredPaymentRequest,
    StoredRoomBinding, StoredRoomCommandPolicy, StoredServiceRoom, StoredSocialEdge,
    StoredTransfer, StoredWorldMessage,
};

mod commerce;
mod inbox;
mod memory_message;
mod rooms;
mod state;
