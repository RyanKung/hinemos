use crate::*;

pub(super) fn payment_request(action: &PayAction) -> AppRequest<'_> {
    match action {
        PayAction::Direct {
            target,
            amount,
            memo,
        } => AppRequest::PayDirect {
            target,
            amount: *amount,
            memo,
        },
        PayAction::Requests => AppRequest::PendingPayRequests,
        PayAction::Accept { request_id } => AppRequest::PayAccept {
            request_id: *request_id,
        },
    }
}

pub(super) fn inbox_request<'a>(
    action: &'a InboxAction,
    mail_domain: Option<&'a str>,
) -> AppRequest<'a> {
    match action {
        InboxAction::List { filter } => AppRequest::InboxList {
            title: "Inbox",
            filter,
            mail_domain,
        },
        InboxAction::Read { item_id } => AppRequest::InboxRead {
            item_id: *item_id,
            mail_domain,
        },
        InboxAction::Claim { item_id } => AppRequest::InboxClaim { item_id: *item_id },
        InboxAction::Ack { item_id } => AppRequest::InboxAck { item_id: *item_id },
        InboxAction::Archive { item_id } => AppRequest::InboxArchive { item_id: *item_id },
    }
}

fn build_request<'a>(action: &'a BuildAction, current_view: &'a str) -> AppRequest<'a> {
    match action {
        BuildAction::Help => AppRequest::ParcelBuildHelp,
        BuildAction::Apply { sheet } => AppRequest::ParcelBuildApply {
            current_view,
            sheet,
        },
        BuildAction::Set { field, value } => AppRequest::ParcelBuildSet {
            current_view,
            field,
            value,
        },
        BuildAction::Publish => AppRequest::ParcelBuildPublish { current_view },
    }
}

pub(super) fn parcel_request<'a>(
    action: &'a ParcelAction,
    current_view: &'a str,
    token: &'a str,
) -> AppRequest<'a> {
    match action {
        ParcelAction::List => AppRequest::ParcelList,
        ParcelAction::Info { parcel_id } => AppRequest::ParcelInfo { parcel_id },
        ParcelAction::Claim { parcel_id } => AppRequest::ParcelClaim { parcel_id, token },
        ParcelAction::Transfer { parcel_id, target } => AppRequest::ParcelTransfer {
            parcel_id,
            target,
            token,
        },
        ParcelAction::Token { parcel_id } => AppRequest::ParcelRotateToken { parcel_id, token },
        ParcelAction::Build { action } => build_request(action, current_view),
        ParcelAction::Inbox => AppRequest::ParcelInbox { current_view },
        ParcelAction::RequestPayment {
            command_id,
            amount,
            delivery,
        } => AppRequest::ParcelRequestPayment {
            current_view,
            command_id: *command_id,
            amount: *amount,
            delivery,
        },
        ParcelAction::MailingList { action } => match action {
            ParcelMailingListAction::Create {
                parcel_id,
                slug,
                title,
            } => AppRequest::ParcelMailingListCreate {
                current_view,
                parcel_id,
                slug,
                title,
            },
            ParcelMailingListAction::List { parcel_id } => AppRequest::ParcelMailingListList {
                current_view,
                parcel_id,
            },
            ParcelMailingListAction::Subscribers { parcel_id, slug } => {
                AppRequest::ParcelMailingListSubscribers {
                    current_view,
                    parcel_id,
                    slug,
                }
            }
            ParcelMailingListAction::Send {
                parcel_id,
                slug,
                subject,
                body,
            } => AppRequest::ParcelMailingListSend {
                current_view,
                parcel_id,
                slug,
                subject,
                body,
            },
            ParcelMailingListAction::Close { parcel_id, slug } => {
                AppRequest::ParcelMailingListClose {
                    current_view,
                    parcel_id,
                    slug,
                }
            }
        },
        ParcelAction::Desk { action } => match action {
            ParcelDeskAction::Create {
                parcel_id,
                slug,
                title,
            } => AppRequest::ParcelDeskCreate {
                current_view,
                parcel_id,
                slug,
                title,
            },
            ParcelDeskAction::List { parcel_id } => AppRequest::ParcelDeskList {
                current_view,
                parcel_id,
            },
        },
        ParcelAction::Job { action } => match action {
            ParcelJobAction::Publish {
                parcel_id,
                slug,
                title,
                body,
            } => AppRequest::ParcelJobPublish {
                current_view,
                parcel_id,
                slug,
                title,
                body,
            },
            ParcelJobAction::List { parcel_id } => AppRequest::ParcelJobList {
                current_view,
                parcel_id,
            },
            ParcelJobAction::Read { parcel_id, slug } => AppRequest::ParcelJobRead {
                current_view,
                parcel_id,
                slug,
            },
        },
        ParcelAction::Route { action } => match action {
            ParcelRouteAction::Add {
                parcel_id,
                slug,
                command_prefix,
            } => AppRequest::ParcelRouteAdd {
                current_view,
                parcel_id,
                slug,
                command_prefix,
            },
            ParcelRouteAction::List { parcel_id } => AppRequest::ParcelRouteList {
                current_view,
                parcel_id,
            },
            ParcelRouteAction::Remove {
                parcel_id,
                slug,
                command_prefix,
            } => AppRequest::ParcelRouteRemove {
                current_view,
                parcel_id,
                slug,
                command_prefix,
            },
        },
        ParcelAction::Staff { action } => match action {
            ParcelStaffAction::Add {
                parcel_id,
                slug,
                username,
            } => AppRequest::ParcelStaffAdd {
                current_view,
                parcel_id,
                slug,
                username,
            },
            ParcelStaffAction::List { parcel_id, slug } => AppRequest::ParcelStaffList {
                current_view,
                parcel_id,
                slug,
            },
            ParcelStaffAction::Remove {
                parcel_id,
                slug,
                username,
            } => AppRequest::ParcelStaffRemove {
                current_view,
                parcel_id,
                slug,
                username,
            },
        },
        ParcelAction::Shift { action } => match action {
            ParcelShiftAction::Start { parcel_id, slug } => AppRequest::ParcelShiftStart {
                current_view,
                parcel_id,
                slug,
            },
            ParcelShiftAction::End { parcel_id, slug } => AppRequest::ParcelShiftEnd {
                current_view,
                parcel_id,
                slug,
            },
        },
        ParcelAction::Work { action } => match action {
            ParcelWorkAction::List { parcel_id, slug } => AppRequest::ParcelWorkList {
                current_view,
                parcel_id,
                slug: slug.as_deref(),
            },
            ParcelWorkAction::Claim { parcel_id, work_id } => AppRequest::ParcelWorkClaim {
                current_view,
                parcel_id,
                work_id: *work_id,
            },
            ParcelWorkAction::Done {
                parcel_id,
                work_id,
                result,
            } => AppRequest::ParcelWorkDone {
                current_view,
                parcel_id,
                work_id: *work_id,
                result,
            },
        },
        ParcelAction::Badge { action } => match action {
            ParcelBadgeAction::List { parcel_id } => AppRequest::ParcelBadgeList {
                current_view,
                parcel_id,
            },
            ParcelBadgeAction::Create {
                parcel_id,
                slug,
                title,
                description,
            } => AppRequest::ParcelBadgeCreate {
                current_view,
                parcel_id,
                slug,
                title,
                description: description.as_deref(),
            },
            ParcelBadgeAction::Award {
                parcel_id,
                slug,
                target,
                note,
            } => AppRequest::ParcelBadgeAward {
                current_view,
                parcel_id,
                slug,
                target,
                note: note.as_deref(),
            },
            ParcelBadgeAction::Revoke {
                parcel_id,
                slug,
                target,
            } => AppRequest::ParcelBadgeRevoke {
                current_view,
                parcel_id,
                slug,
                target,
            },
        },
        ParcelAction::Subscribe { target, slug } => AppRequest::ParcelMailingListSubscribe {
            current_view,
            target,
            slug,
        },
        ParcelAction::Unsubscribe { target, slug } => AppRequest::ParcelMailingListUnsubscribe {
            current_view,
            target,
            slug,
        },
        ParcelAction::Chat { target, slug, body } => AppRequest::ParcelMailingListChat {
            current_view,
            target,
            slug,
            body,
        },
        ParcelAction::Subscriptions => AppRequest::ParcelMailingListSubscriptions,
    }
}

pub(super) fn badge_request(action: &BadgeAction) -> AppRequest<'_> {
    match action {
        BadgeAction::ListMine => AppRequest::BadgesMine,
        BadgeAction::ListUser { target } => AppRequest::BadgesUser { target },
    }
}
