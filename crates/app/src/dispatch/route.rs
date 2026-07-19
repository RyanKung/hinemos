use crate::{AppRequest, BuildSheet, RoleCardUpdate, WhoPopulation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InboxMutation {
    Claim,
    Ack,
    Archive,
}

pub(super) enum RoutedAppRequest<'a> {
    Read(ReadAppRequest<'a>),
    Inbox(InboxAppRequest<'a>),
    Payment(PaymentAppRequest<'a>),
    Message(MessageAppRequest<'a>),
    ParcelRegistry(ParcelRegistryAppRequest<'a>),
    ParcelBuild(ParcelBuildAppRequest<'a>),
    ParcelOperation(ParcelOperationAppRequest<'a>),
    ServiceRoom(ServiceRoomAppRequest<'a>),
    Admission(AdmissionAppRequest),
    Settings(SettingsAppRequest<'a>),
}

pub(super) enum ReadAppRequest<'a> {
    MemoryContext,
    MemoryCommand {
        rest: &'a str,
    },
    RoomHistory {
        current_view: &'a str,
        title: &'a str,
    },
    Inventory {
        items: &'a [String],
    },
    Who {
        current_view: &'a str,
        users: &'a [String],
        population: WhoPopulation,
    },
    News,
    Balance,
}

pub(super) enum InboxAppRequest<'a> {
    List {
        title: &'a str,
        filter: &'a str,
        mail_domain: Option<&'a str>,
    },
    Read {
        item_id: i64,
        mail_domain: Option<&'a str>,
    },
    Mutate {
        item_id: i64,
        mutation: InboxMutation,
    },
}

pub(super) enum PaymentAppRequest<'a> {
    PendingRequests,
    Direct {
        target: &'a str,
        amount: i64,
        memo: &'a str,
    },
    Accept {
        request_id: i64,
    },
}

pub(super) enum MessageAppRequest<'a> {
    Say {
        current_view: &'a str,
        text: &'a str,
    },
    Mail {
        target: &'a str,
        text: &'a str,
    },
    Broadcast {
        text: &'a str,
    },
}

pub(super) enum ParcelRegistryAppRequest<'a> {
    List,
    Info {
        parcel_id: &'a str,
    },
    Claim {
        parcel_id: &'a str,
        token: &'a str,
    },
    Transfer {
        parcel_id: &'a str,
        target: &'a str,
        token: &'a str,
    },
    RotateToken {
        parcel_id: &'a str,
        token: &'a str,
    },
}

pub(super) enum ParcelBuildAppRequest<'a> {
    Help,
    Apply {
        current_view: &'a str,
        sheet: &'a BuildSheet,
    },
    Set {
        current_view: &'a str,
        field: &'a str,
        value: &'a str,
    },
    Publish {
        current_view: &'a str,
    },
}

pub(super) enum ParcelOperationAppRequest<'a> {
    Inbox,
    RequestPayment {
        current_view: &'a str,
        command_id: i64,
        amount: i64,
        delivery: &'a str,
    },
    MailingListCreate {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        title: &'a str,
    },
    MailingListList {
        current_view: &'a str,
        parcel_id: &'a str,
    },
    MailingListSubscribers {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
    },
    MailingListSend {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        subject: &'a str,
        body: &'a str,
    },
    MailingListChat {
        current_view: &'a str,
        target: &'a str,
        slug: &'a str,
        body: &'a str,
    },
    MailingListClose {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
    },
    MailingListSubscribe {
        current_view: &'a str,
        target: &'a str,
        slug: &'a str,
    },
    MailingListUnsubscribe {
        current_view: &'a str,
        target: &'a str,
        slug: &'a str,
    },
    MailingListSubscriptions,
    DeskCreate {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        title: &'a str,
    },
    DeskList {
        current_view: &'a str,
        parcel_id: &'a str,
    },
    StaffAdd {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        username: &'a str,
    },
    StaffList {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
    },
    StaffRemove {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        username: &'a str,
    },
    ShiftStart {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
    },
    ShiftEnd {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
    },
    WorkList {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: Option<&'a str>,
    },
    WorkClaim {
        current_view: &'a str,
        parcel_id: &'a str,
        work_id: i64,
    },
    WorkDone {
        current_view: &'a str,
        parcel_id: &'a str,
        work_id: i64,
        result: &'a str,
    },
    RouteAdd {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        command_prefix: &'a str,
    },
    RouteList {
        current_view: &'a str,
        parcel_id: &'a str,
    },
    RouteRemove {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        command_prefix: &'a str,
    },
    BadgeList {
        current_view: &'a str,
        parcel_id: &'a str,
    },
    BadgeCreate {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        title: &'a str,
        description: Option<&'a str>,
    },
    BadgeAward {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        target: &'a str,
        note: Option<&'a str>,
    },
    BadgeRevoke {
        current_view: &'a str,
        parcel_id: &'a str,
        slug: &'a str,
        target: &'a str,
    },
    BadgesMine,
    BadgesUser {
        target: &'a str,
    },
}

pub(super) enum ServiceRoomAppRequest<'a> {
    Input {
        room_view: &'a str,
        raw_input: &'a str,
    },
    Help {
        room_view: &'a str,
    },
    Observation {
        room_view: &'a str,
    },
    BlockedExit,
    Unavailable,
    Quit {
        feedback: &'a str,
    },
}

pub(super) enum AdmissionAppRequest {
    Read,
    Accept,
}

pub(super) enum SettingsAppRequest<'a> {
    Show {
        mail_address: &'a str,
    },
    RotateMailToken {
        mail_address: &'a str,
        token: &'a str,
    },
    UpdateRoleCard {
        mail_address: &'a str,
        update: RoleCardUpdate,
    },
}

impl<'a> From<AppRequest<'a>> for RoutedAppRequest<'a> {
    fn from(request: AppRequest<'a>) -> Self {
        match request {
            request @ (AppRequest::MemoryContext
            | AppRequest::MemoryCommand { .. }
            | AppRequest::RoomHistory { .. }
            | AppRequest::Inventory { .. }
            | AppRequest::Who { .. }
            | AppRequest::News
            | AppRequest::Balance) => route_read(request),
            request @ (AppRequest::InboxList { .. }
            | AppRequest::InboxRead { .. }
            | AppRequest::InboxClaim { .. }
            | AppRequest::InboxAck { .. }
            | AppRequest::InboxArchive { .. }) => route_inbox(request),
            request @ (AppRequest::PendingPayRequests
            | AppRequest::PayDirect { .. }
            | AppRequest::PayAccept { .. }) => route_payment(request),
            request @ (AppRequest::Say { .. }
            | AppRequest::Mail { .. }
            | AppRequest::Broadcast { .. }) => route_message(request),
            request @ (AppRequest::ParcelList
            | AppRequest::ParcelInfo { .. }
            | AppRequest::ParcelClaim { .. }
            | AppRequest::ParcelTransfer { .. }
            | AppRequest::ParcelRotateToken { .. }) => route_parcel_registry(request),
            request @ (AppRequest::ParcelBuildHelp
            | AppRequest::ParcelBuildApply { .. }
            | AppRequest::ParcelBuildSet { .. }
            | AppRequest::ParcelBuildPublish { .. }) => route_parcel_build(request),
            request @ (AppRequest::ParcelInbox
            | AppRequest::ParcelRequestPayment { .. }
            | AppRequest::ParcelMailingListCreate { .. }
            | AppRequest::ParcelMailingListList { .. }
            | AppRequest::ParcelMailingListSubscribers { .. }
            | AppRequest::ParcelMailingListSend { .. }
            | AppRequest::ParcelMailingListChat { .. }
            | AppRequest::ParcelMailingListClose { .. }
            | AppRequest::ParcelMailingListSubscribe { .. }
            | AppRequest::ParcelMailingListUnsubscribe { .. }
            | AppRequest::ParcelMailingListSubscriptions
            | AppRequest::ParcelDeskCreate { .. }
            | AppRequest::ParcelDeskList { .. }
            | AppRequest::ParcelStaffAdd { .. }
            | AppRequest::ParcelStaffList { .. }
            | AppRequest::ParcelStaffRemove { .. }
            | AppRequest::ParcelShiftStart { .. }
            | AppRequest::ParcelShiftEnd { .. }
            | AppRequest::ParcelWorkList { .. }
            | AppRequest::ParcelWorkClaim { .. }
            | AppRequest::ParcelWorkDone { .. }
            | AppRequest::ParcelRouteAdd { .. }
            | AppRequest::ParcelRouteList { .. }
            | AppRequest::ParcelRouteRemove { .. }
            | AppRequest::ParcelBadgeList { .. }
            | AppRequest::ParcelBadgeCreate { .. }
            | AppRequest::ParcelBadgeAward { .. }
            | AppRequest::ParcelBadgeRevoke { .. }
            | AppRequest::BadgesMine
            | AppRequest::BadgesUser { .. }) => route_parcel_operation(request),
            request @ (AppRequest::ServiceRoomInput { .. }
            | AppRequest::ServiceRoomHelp { .. }
            | AppRequest::ServiceRoomObservation { .. }
            | AppRequest::ServiceRoomBlockedExit
            | AppRequest::ServiceRoomUnavailable
            | AppRequest::ServiceRoomQuit { .. }) => route_service_room(request),
            request @ (AppRequest::AdmissionRead | AppRequest::AdmissionAccept) => {
                route_admission(request)
            }
            request @ (AppRequest::Settings { .. }
            | AppRequest::SettingsRotateMailToken { .. }
            | AppRequest::SettingsUpdateRoleCard { .. }) => route_settings(request),
        }
    }
}

fn route_read(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::MemoryContext => RoutedAppRequest::Read(ReadAppRequest::MemoryContext),
        AppRequest::MemoryCommand { rest } => {
            RoutedAppRequest::Read(ReadAppRequest::MemoryCommand { rest })
        }
        AppRequest::RoomHistory {
            current_view,
            title,
        } => RoutedAppRequest::Read(ReadAppRequest::RoomHistory {
            current_view,
            title,
        }),
        AppRequest::Inventory { items } => {
            RoutedAppRequest::Read(ReadAppRequest::Inventory { items })
        }
        AppRequest::Who {
            current_view,
            users,
            population,
        } => RoutedAppRequest::Read(ReadAppRequest::Who {
            current_view,
            users,
            population,
        }),
        AppRequest::News => RoutedAppRequest::Read(ReadAppRequest::News),
        AppRequest::Balance => RoutedAppRequest::Read(ReadAppRequest::Balance),
        _ => unreachable!("read request route called with non-read request"),
    }
}

fn route_inbox(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::InboxList {
            title,
            filter,
            mail_domain,
        } => RoutedAppRequest::Inbox(InboxAppRequest::List {
            title,
            filter,
            mail_domain,
        }),
        AppRequest::InboxRead {
            item_id,
            mail_domain,
        } => RoutedAppRequest::Inbox(InboxAppRequest::Read {
            item_id,
            mail_domain,
        }),
        AppRequest::InboxClaim { item_id } => inbox_mutation(item_id, InboxMutation::Claim),
        AppRequest::InboxAck { item_id } => inbox_mutation(item_id, InboxMutation::Ack),
        AppRequest::InboxArchive { item_id } => inbox_mutation(item_id, InboxMutation::Archive),
        _ => unreachable!("inbox request route called with non-inbox request"),
    }
}

fn inbox_mutation(item_id: i64, mutation: InboxMutation) -> RoutedAppRequest<'static> {
    RoutedAppRequest::Inbox(InboxAppRequest::Mutate { item_id, mutation })
}

fn route_payment(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::PendingPayRequests => {
            RoutedAppRequest::Payment(PaymentAppRequest::PendingRequests)
        }
        AppRequest::PayDirect {
            target,
            amount,
            memo,
        } => RoutedAppRequest::Payment(PaymentAppRequest::Direct {
            target,
            amount,
            memo,
        }),
        AppRequest::PayAccept { request_id } => {
            RoutedAppRequest::Payment(PaymentAppRequest::Accept { request_id })
        }
        _ => unreachable!("payment request route called with non-payment request"),
    }
}

fn route_message(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::Say { current_view, text } => {
            RoutedAppRequest::Message(MessageAppRequest::Say { current_view, text })
        }
        AppRequest::Mail { target, text } => {
            RoutedAppRequest::Message(MessageAppRequest::Mail { target, text })
        }
        AppRequest::Broadcast { text } => {
            RoutedAppRequest::Message(MessageAppRequest::Broadcast { text })
        }
        _ => unreachable!("message request route called with non-message request"),
    }
}

fn route_parcel_registry(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::ParcelList => RoutedAppRequest::ParcelRegistry(ParcelRegistryAppRequest::List),
        AppRequest::ParcelInfo { parcel_id } => {
            RoutedAppRequest::ParcelRegistry(ParcelRegistryAppRequest::Info { parcel_id })
        }
        AppRequest::ParcelClaim { parcel_id, token } => {
            RoutedAppRequest::ParcelRegistry(ParcelRegistryAppRequest::Claim { parcel_id, token })
        }
        AppRequest::ParcelTransfer {
            parcel_id,
            target,
            token,
        } => RoutedAppRequest::ParcelRegistry(ParcelRegistryAppRequest::Transfer {
            parcel_id,
            target,
            token,
        }),
        AppRequest::ParcelRotateToken { parcel_id, token } => {
            RoutedAppRequest::ParcelRegistry(ParcelRegistryAppRequest::RotateToken {
                parcel_id,
                token,
            })
        }
        _ => unreachable!("land request route called with non-land request"),
    }
}

fn route_parcel_build(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::ParcelBuildHelp => RoutedAppRequest::ParcelBuild(ParcelBuildAppRequest::Help),
        AppRequest::ParcelBuildApply {
            current_view,
            sheet,
        } => RoutedAppRequest::ParcelBuild(ParcelBuildAppRequest::Apply {
            current_view,
            sheet,
        }),
        AppRequest::ParcelBuildSet {
            current_view,
            field,
            value,
        } => RoutedAppRequest::ParcelBuild(ParcelBuildAppRequest::Set {
            current_view,
            field,
            value,
        }),
        AppRequest::ParcelBuildPublish { current_view } => {
            RoutedAppRequest::ParcelBuild(ParcelBuildAppRequest::Publish { current_view })
        }
        _ => unreachable!("build request route called with non-build request"),
    }
}

fn route_parcel_operation(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::ParcelInbox => {
            RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::Inbox)
        }
        AppRequest::ParcelRequestPayment {
            current_view,
            command_id,
            amount,
            delivery,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::RequestPayment {
            current_view,
            command_id,
            amount,
            delivery,
        }),
        AppRequest::ParcelMailingListCreate {
            current_view,
            parcel_id,
            slug,
            title,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListCreate {
            current_view,
            parcel_id,
            slug,
            title,
        }),
        AppRequest::ParcelMailingListList {
            current_view,
            parcel_id,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListList {
            current_view,
            parcel_id,
        }),
        AppRequest::ParcelMailingListSubscribers {
            current_view,
            parcel_id,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListSubscribers {
            current_view,
            parcel_id,
            slug,
        }),
        AppRequest::ParcelMailingListSend {
            current_view,
            parcel_id,
            slug,
            subject,
            body,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListSend {
            current_view,
            parcel_id,
            slug,
            subject,
            body,
        }),
        AppRequest::ParcelMailingListChat {
            current_view,
            target,
            slug,
            body,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListChat {
            current_view,
            target,
            slug,
            body,
        }),
        AppRequest::ParcelMailingListClose {
            current_view,
            parcel_id,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListClose {
            current_view,
            parcel_id,
            slug,
        }),
        AppRequest::ParcelMailingListSubscribe {
            current_view,
            target,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListSubscribe {
            current_view,
            target,
            slug,
        }),
        AppRequest::ParcelMailingListUnsubscribe {
            current_view,
            target,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListUnsubscribe {
            current_view,
            target,
            slug,
        }),
        AppRequest::ParcelMailingListSubscriptions => {
            RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::MailingListSubscriptions)
        }
        AppRequest::ParcelDeskCreate {
            current_view,
            parcel_id,
            slug,
            title,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::DeskCreate {
            current_view,
            parcel_id,
            slug,
            title,
        }),
        AppRequest::ParcelDeskList {
            current_view,
            parcel_id,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::DeskList {
            current_view,
            parcel_id,
        }),
        AppRequest::ParcelStaffAdd {
            current_view,
            parcel_id,
            slug,
            username,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::StaffAdd {
            current_view,
            parcel_id,
            slug,
            username,
        }),
        AppRequest::ParcelStaffList {
            current_view,
            parcel_id,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::StaffList {
            current_view,
            parcel_id,
            slug,
        }),
        AppRequest::ParcelStaffRemove {
            current_view,
            parcel_id,
            slug,
            username,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::StaffRemove {
            current_view,
            parcel_id,
            slug,
            username,
        }),
        AppRequest::ParcelShiftStart {
            current_view,
            parcel_id,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::ShiftStart {
            current_view,
            parcel_id,
            slug,
        }),
        AppRequest::ParcelShiftEnd {
            current_view,
            parcel_id,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::ShiftEnd {
            current_view,
            parcel_id,
            slug,
        }),
        AppRequest::ParcelWorkList {
            current_view,
            parcel_id,
            slug,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::WorkList {
            current_view,
            parcel_id,
            slug,
        }),
        AppRequest::ParcelWorkClaim {
            current_view,
            parcel_id,
            work_id,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::WorkClaim {
            current_view,
            parcel_id,
            work_id,
        }),
        AppRequest::ParcelWorkDone {
            current_view,
            parcel_id,
            work_id,
            result,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::WorkDone {
            current_view,
            parcel_id,
            work_id,
            result,
        }),
        AppRequest::ParcelRouteAdd {
            current_view,
            parcel_id,
            slug,
            command_prefix,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::RouteAdd {
            current_view,
            parcel_id,
            slug,
            command_prefix,
        }),
        AppRequest::ParcelRouteList {
            current_view,
            parcel_id,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::RouteList {
            current_view,
            parcel_id,
        }),
        AppRequest::ParcelRouteRemove {
            current_view,
            parcel_id,
            slug,
            command_prefix,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::RouteRemove {
            current_view,
            parcel_id,
            slug,
            command_prefix,
        }),
        AppRequest::ParcelBadgeList {
            current_view,
            parcel_id,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::BadgeList {
            current_view,
            parcel_id,
        }),
        AppRequest::ParcelBadgeCreate {
            current_view,
            parcel_id,
            slug,
            title,
            description,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::BadgeCreate {
            current_view,
            parcel_id,
            slug,
            title,
            description,
        }),
        AppRequest::ParcelBadgeAward {
            current_view,
            parcel_id,
            slug,
            target,
            note,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::BadgeAward {
            current_view,
            parcel_id,
            slug,
            target,
            note,
        }),
        AppRequest::ParcelBadgeRevoke {
            current_view,
            parcel_id,
            slug,
            target,
        } => RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::BadgeRevoke {
            current_view,
            parcel_id,
            slug,
            target,
        }),
        AppRequest::BadgesMine => {
            RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::BadgesMine)
        }
        AppRequest::BadgesUser { target } => {
            RoutedAppRequest::ParcelOperation(ParcelOperationAppRequest::BadgesUser { target })
        }
        _ => unreachable!("parcel request route called with non-parcel request"),
    }
}

fn route_service_room(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::ServiceRoomInput {
            room_view,
            raw_input,
        } => RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Input {
            room_view,
            raw_input,
        }),
        AppRequest::ServiceRoomHelp { room_view } => {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Help { room_view })
        }
        AppRequest::ServiceRoomObservation { room_view } => {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Observation { room_view })
        }
        AppRequest::ServiceRoomBlockedExit => {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::BlockedExit)
        }
        AppRequest::ServiceRoomUnavailable => {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Unavailable)
        }
        AppRequest::ServiceRoomQuit { feedback } => {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Quit { feedback })
        }
        _ => unreachable!("service-room request route called with non-service-room request"),
    }
}

fn route_admission(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::AdmissionRead => RoutedAppRequest::Admission(AdmissionAppRequest::Read),
        AppRequest::AdmissionAccept => RoutedAppRequest::Admission(AdmissionAppRequest::Accept),
        _ => unreachable!("admission request route called with non-admission request"),
    }
}

fn route_settings(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::Settings { mail_address } => {
            RoutedAppRequest::Settings(SettingsAppRequest::Show { mail_address })
        }
        AppRequest::SettingsRotateMailToken {
            mail_address,
            token,
        } => RoutedAppRequest::Settings(SettingsAppRequest::RotateMailToken {
            mail_address,
            token,
        }),
        AppRequest::SettingsUpdateRoleCard {
            mail_address,
            update,
        } => RoutedAppRequest::Settings(SettingsAppRequest::UpdateRoleCard {
            mail_address,
            update,
        }),
        _ => unreachable!("settings request route called with non-settings request"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_mutations_route_to_inbox_domain() {
        let routed = RoutedAppRequest::from(AppRequest::InboxAck { item_id: 42 });

        match routed {
            RoutedAppRequest::Inbox(InboxAppRequest::Mutate {
                item_id,
                mutation: InboxMutation::Ack,
            }) => assert_eq!(item_id, 42),
            _ => panic!("expected inbox ack route"),
        }
    }

    #[test]
    fn service_room_input_routes_without_interpreting_raw_text() {
        let routed = RoutedAppRequest::from(AppRequest::ServiceRoomInput {
            room_view: "external-room",
            raw_input: "hello",
        });

        match routed {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Input {
                room_view,
                raw_input,
            }) => {
                assert_eq!(room_view, "external-room");
                assert_eq!(raw_input, "hello");
            }
            _ => panic!("expected service room input route"),
        }
    }

    #[test]
    fn build_apply_preserves_sheet_reference() {
        let sheet = BuildSheet {
            title: Some("Test Parcel".to_owned()),
            ..BuildSheet::default()
        };
        let routed = RoutedAppRequest::from(AppRequest::ParcelBuildApply {
            current_view: "parcel-test",
            sheet: &sheet,
        });

        match routed {
            RoutedAppRequest::ParcelBuild(ParcelBuildAppRequest::Apply {
                current_view,
                sheet,
            }) => {
                assert_eq!(current_view, "parcel-test");
                assert_eq!(sheet.title.as_deref(), Some("Test Parcel"));
            }
            _ => panic!("expected build apply route"),
        }
    }
}
