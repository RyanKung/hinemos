use hinemos_app::{
    AccountSettingsView, AccountStore, AdmissionStore, AdmissionView, BalanceView, BuildStore,
    FromMailingListValidation, HungerStore, HungerView, InboxItemView, InboxStore, LandStore,
    MailAuthTokenView, MailDaemonStore, MailStore, MemoryAtomView, MemoryEventView, MemoryStore,
    MessageStore, ParcelStore, ParcelView, PaymentRequestView, PaymentStore,
    PlayerStateStore as AppPlayerStateStore, RecentPresenceUser, RoleCardUpdate,
    RoomBindingEntryView, RoomBindingKindView, RoomCommandPolicyView, RoomMailboxView,
    RoomRegistrationStore, RoomStore, SelfModelView, ServiceRoomRegistrationUpsert,
    ServiceRoomView, ShopMailingListPostView, ShopMailingListSend, ShopMailingListSubscriberPage,
    ShopMailingListSubscriberView, ShopMailingListSubscriptionView, ShopMailingListView, ShopStore,
    SocialEdgeView, TransferView, ViewPresenceStore, WorldMessageView,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

use crate::{
    PgStorage, ServiceRoomUpsert, StorageError, StoredAccountSettings, StoredAdmission,
    StoredAgentSelfModel, StoredBalance, StoredHungerState, StoredInboxItem, StoredMailAuthToken,
    StoredMemoryAtom, StoredMemoryEvent, StoredOperatorCommand, StoredParcel, StoredPaymentRequest,
    StoredRoomBinding, StoredRoomCommandPolicy, StoredServiceRoom, StoredShopMailingList,
    StoredShopMailingListPost, StoredShopMailingListSubscriber, StoredShopMailingListSubscription,
    StoredSocialEdge, StoredTransfer, StoredWorldMessage,
};

mod commerce;
mod hunger;
mod inbox;
mod memory_message;
mod rooms;
mod state;
