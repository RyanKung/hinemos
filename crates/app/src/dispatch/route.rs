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
    Land(LandAppRequest<'a>),
    Build(BuildAppRequest<'a>),
    Shop(ShopAppRequest<'a>),
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

pub(super) enum LandAppRequest<'a> {
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

pub(super) enum BuildAppRequest<'a> {
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

pub(super) enum ShopAppRequest<'a> {
    Inbox,
    RequestPayment {
        command_id: i64,
        amount: i64,
        delivery: &'a str,
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
            request @ (AppRequest::LandList
            | AppRequest::LandInfo { .. }
            | AppRequest::LandClaim { .. }
            | AppRequest::LandTransfer { .. }
            | AppRequest::LandRotateToken { .. }) => route_land(request),
            request @ (AppRequest::BuildHelp
            | AppRequest::BuildApply { .. }
            | AppRequest::BuildSet { .. }
            | AppRequest::BuildPublish { .. }) => route_build(request),
            request @ (AppRequest::ShopInbox | AppRequest::ShopRequestPayment { .. }) => {
                route_shop(request)
            }
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

fn route_land(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::LandList => RoutedAppRequest::Land(LandAppRequest::List),
        AppRequest::LandInfo { parcel_id } => {
            RoutedAppRequest::Land(LandAppRequest::Info { parcel_id })
        }
        AppRequest::LandClaim { parcel_id, token } => {
            RoutedAppRequest::Land(LandAppRequest::Claim { parcel_id, token })
        }
        AppRequest::LandTransfer {
            parcel_id,
            target,
            token,
        } => RoutedAppRequest::Land(LandAppRequest::Transfer {
            parcel_id,
            target,
            token,
        }),
        AppRequest::LandRotateToken { parcel_id, token } => {
            RoutedAppRequest::Land(LandAppRequest::RotateToken { parcel_id, token })
        }
        _ => unreachable!("land request route called with non-land request"),
    }
}

fn route_build(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::BuildHelp => RoutedAppRequest::Build(BuildAppRequest::Help),
        AppRequest::BuildApply {
            current_view,
            sheet,
        } => RoutedAppRequest::Build(BuildAppRequest::Apply {
            current_view,
            sheet,
        }),
        AppRequest::BuildSet {
            current_view,
            field,
            value,
        } => RoutedAppRequest::Build(BuildAppRequest::Set {
            current_view,
            field,
            value,
        }),
        AppRequest::BuildPublish { current_view } => {
            RoutedAppRequest::Build(BuildAppRequest::Publish { current_view })
        }
        _ => unreachable!("build request route called with non-build request"),
    }
}

fn route_shop(request: AppRequest<'_>) -> RoutedAppRequest<'_> {
    match request {
        AppRequest::ShopInbox => RoutedAppRequest::Shop(ShopAppRequest::Inbox),
        AppRequest::ShopRequestPayment {
            command_id,
            amount,
            delivery,
        } => RoutedAppRequest::Shop(ShopAppRequest::RequestPayment {
            command_id,
            amount,
            delivery,
        }),
        _ => unreachable!("shop request route called with non-shop request"),
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
            room_view: "room-blackstone",
            raw_input: "hello",
        });

        match routed {
            RoutedAppRequest::ServiceRoom(ServiceRoomAppRequest::Input {
                room_view,
                raw_input,
            }) => {
                assert_eq!(room_view, "room-blackstone");
                assert_eq!(raw_input, "hello");
            }
            _ => panic!("expected service room input route"),
        }
    }

    #[test]
    fn build_apply_preserves_sheet_reference() {
        let sheet = BuildSheet {
            title: Some("Test Shop".to_owned()),
            ..BuildSheet::default()
        };
        let routed = RoutedAppRequest::from(AppRequest::BuildApply {
            current_view: "parcel-test",
            sheet: &sheet,
        });

        match routed {
            RoutedAppRequest::Build(BuildAppRequest::Apply {
                current_view,
                sheet,
            }) => {
                assert_eq!(current_view, "parcel-test");
                assert_eq!(sheet.title.as_deref(), Some("Test Shop"));
            }
            _ => panic!("expected build apply route"),
        }
    }
}
