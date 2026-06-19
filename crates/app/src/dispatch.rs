use crate::*;

mod account_admission;
mod commerce;
mod events;
mod inbox;
mod messaging;
mod payment;
mod read;
mod request_mapping;
mod route;
mod semantic;
mod service_room;

use route::RoutedAppRequest;

/// Store capability set required by the high-level app dispatch entrypoints.
pub trait AppDispatchStore:
    AccountStore<Error = <Self as AppDispatchStore>::Error>
    + AdmissionStore<Error = <Self as AppDispatchStore>::Error>
    + BuildStore<Error = <Self as AppDispatchStore>::Error>
    + InboxStore<Error = <Self as AppDispatchStore>::Error>
    + LandStore<Error = <Self as AppDispatchStore>::Error>
    + MailStore<Error = <Self as AppDispatchStore>::Error>
    + MemoryStore<Error = <Self as AppDispatchStore>::Error>
    + MessageStore<Error = <Self as AppDispatchStore>::Error>
    + ParcelStore<Error = <Self as AppDispatchStore>::Error>
    + PaymentStore<Error = <Self as AppDispatchStore>::Error>
    + RoomStore<Error = <Self as AppDispatchStore>::Error>
    + ShopStore<Error = <Self as AppDispatchStore>::Error>
{
    /// Shared store error type across all dispatch subdomains.
    type Error;
}

impl<T, E> AppDispatchStore for T
where
    T: AccountStore<Error = E>
        + AdmissionStore<Error = E>
        + BuildStore<Error = E>
        + InboxStore<Error = E>
        + LandStore<Error = E>
        + MailStore<Error = E>
        + MemoryStore<Error = E>
        + MessageStore<Error = E>
        + ParcelStore<Error = E>
        + PaymentStore<Error = E>
        + RoomStore<Error = E>
        + ShopStore<Error = E>,
{
    type Error = E;
}

/// Context needed to route a command in the current rendered view.
#[derive(Debug, Clone, Copy)]
pub struct AppViewCommandContext<'a, B> {
    /// Current rendered view id.
    pub current_view: &'a str,
    /// Current rendered view title.
    pub current_title: &'a str,
    /// Current player inventory.
    pub inventory: &'a [String],
    /// Usernames currently visible in this view.
    pub online_users: &'a [String],
    /// Entity ids visible in the current observation.
    pub visible_entity_ids: &'a [String],
    /// Optional active room binding for this view.
    pub room_binding: Option<&'a B>,
    /// Optional configured mail domain.
    pub mail_domain: Option<&'a str>,
    /// Context for business commands.
    pub business: AppCommandContext<'a>,
}

impl<S> AppService<S>
where
    S: AppDispatchStore,
    <S as RoomStore>::RoomBinding: RoomBindingKindView + RoomMailboxView + ServiceRoomView + Sync,
    <S as RoomStore>::ServiceRoom: ServiceRoomView,
{
    /// Handles a protocol-neutral app request and returns UI events.
    pub async fn handle(
        &self,
        identity: &AppIdentity,
        request: AppRequest<'_>,
    ) -> Result<Vec<UiEvent>, <S as AppDispatchStore>::Error> {
        match RoutedAppRequest::from(request) {
            RoutedAppRequest::Read(request) => self.handle_read_request(identity, request).await,
            RoutedAppRequest::Inbox(request) => self.handle_inbox_request(identity, request).await,
            RoutedAppRequest::Payment(request) => {
                self.handle_payment_request(identity, request).await
            }
            RoutedAppRequest::Message(request) => {
                self.handle_message_request(identity, request).await
            }
            RoutedAppRequest::Land(request) => self.handle_land_request(identity, request).await,
            RoutedAppRequest::Build(request) => self.handle_build_request(identity, request).await,
            RoutedAppRequest::Shop(request) => self.handle_shop_request(identity, request).await,
            RoutedAppRequest::ServiceRoom(request) => {
                self.handle_service_room_request(identity, request).await
            }
            RoutedAppRequest::Admission(request) => {
                self.handle_admission_request(identity, request).await
            }
            RoutedAppRequest::Settings(request) => {
                self.handle_settings_request(identity, request).await
            }
        }
    }
}
