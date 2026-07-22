use hinemos_app::{
    AccountSettingsView, AccountStore, AdmissionStore, AdmissionView, BalanceView, BuildStore,
    FromMailingListValidation, HungerStore, HungerView, InboxItemView, InboxStore,
    MailAuthTokenView, MailDaemonStore, MailStore, MemoryAtomView, MemoryEventView, MemoryStore,
    MessageStore, ParcelMailingListPostView, ParcelMailingListSend,
    ParcelMailingListSubscriberPage, ParcelMailingListSubscriberView,
    ParcelMailingListSubscriptionView, ParcelMailingListView, ParcelOwnershipStore, ParcelStore,
    ParcelView, PaymentRequestCreation, PaymentRequestView, PaymentStore,
    PlayerStateStore as AppPlayerStateStore, RecentPresenceUser, RoleCardUpdate,
    RoomBindingEntryView, RoomBindingKindView, RoomCommandPolicyView, RoomMailboxView,
    RoomRegistrationStore, RoomStore, SelfModelView, ServiceRoomRegistrationUpsert,
    ServiceRoomView, SocialEdgeView, TransferView, ViewPresenceStore, WorldMessageView,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

use crate::{
    PgStorage, ServiceRoomUpsert, StorageError, StoredAccountSettings, StoredAdmission,
    StoredAgentSelfModel, StoredBalance, StoredHungerState, StoredInboxItem, StoredMailAuthToken,
    StoredMemoryAtom, StoredMemoryEvent, StoredOperatorCommand, StoredParcel,
    StoredParcelMailingList, StoredParcelMailingListPost, StoredParcelMailingListSubscriber,
    StoredParcelMailingListSubscription, StoredPaymentRequest, StoredRoomBinding,
    StoredRoomCommandPolicy, StoredServiceRoom, StoredSocialEdge, StoredTransfer,
    StoredWorldMessage,
};

mod commerce;
mod hunger;
mod inbox;
mod memory_message;
mod rooms;
mod state;
