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

pub(super) fn land_request<'a>(action: &'a LandAction, token: &'a str) -> AppRequest<'a> {
    match action {
        LandAction::List => AppRequest::LandList,
        LandAction::Info { parcel_id } => AppRequest::LandInfo { parcel_id },
        LandAction::Claim { parcel_id } => AppRequest::LandClaim { parcel_id, token },
        LandAction::Transfer { parcel_id, target } => AppRequest::LandTransfer {
            parcel_id,
            target,
            token,
        },
        LandAction::Token { parcel_id } => AppRequest::LandRotateToken { parcel_id, token },
    }
}

pub(super) fn build_request<'a>(action: &'a BuildAction, current_view: &'a str) -> AppRequest<'a> {
    match action {
        BuildAction::Help => AppRequest::BuildHelp,
        BuildAction::Apply { sheet } => AppRequest::BuildApply {
            current_view,
            sheet,
        },
        BuildAction::Set { field, value } => AppRequest::BuildSet {
            current_view,
            field,
            value,
        },
        BuildAction::Publish => AppRequest::BuildPublish { current_view },
    }
}

pub(super) fn shop_request<'a>(action: &'a ShopAction, current_view: &'a str) -> AppRequest<'a> {
    match action {
        ShopAction::Inbox => AppRequest::ShopInbox,
        ShopAction::RequestPayment {
            command_id,
            amount,
            delivery,
        } => AppRequest::ShopRequestPayment {
            command_id: *command_id,
            amount: *amount,
            delivery,
        },
        ShopAction::MailingList { action } => match action {
            ShopMailingListAction::Create {
                parcel_id,
                slug,
                title,
            } => AppRequest::ShopMailingListCreate {
                parcel_id,
                slug,
                title,
            },
            ShopMailingListAction::List { parcel_id } => {
                AppRequest::ShopMailingListList { parcel_id }
            }
            ShopMailingListAction::Subscribers { parcel_id, slug } => {
                AppRequest::ShopMailingListSubscribers { parcel_id, slug }
            }
            ShopMailingListAction::Send {
                parcel_id,
                slug,
                subject,
                body,
            } => AppRequest::ShopMailingListSend {
                parcel_id,
                slug,
                subject,
                body,
            },
            ShopMailingListAction::Close { parcel_id, slug } => {
                AppRequest::ShopMailingListClose { parcel_id, slug }
            }
        },
        ShopAction::Desk { action } => match action {
            ShopDeskAction::Create {
                parcel_id,
                slug,
                title,
            } => AppRequest::ShopDeskCreate {
                current_view,
                parcel_id,
                slug,
                title,
            },
            ShopDeskAction::List { parcel_id } => AppRequest::ShopDeskList {
                current_view,
                parcel_id,
            },
        },
        ShopAction::Route { action } => match action {
            ShopRouteAction::Add {
                parcel_id,
                slug,
                command_prefix,
            } => AppRequest::ShopRouteAdd {
                current_view,
                parcel_id,
                slug,
                command_prefix,
            },
            ShopRouteAction::List { parcel_id } => AppRequest::ShopRouteList {
                current_view,
                parcel_id,
            },
            ShopRouteAction::Remove {
                parcel_id,
                slug,
                command_prefix,
            } => AppRequest::ShopRouteRemove {
                current_view,
                parcel_id,
                slug,
                command_prefix,
            },
        },
        ShopAction::Staff { action } => match action {
            ShopStaffAction::Add {
                parcel_id,
                slug,
                username,
            } => AppRequest::ShopStaffAdd {
                current_view,
                parcel_id,
                slug,
                username,
            },
            ShopStaffAction::List { parcel_id, slug } => AppRequest::ShopStaffList {
                current_view,
                parcel_id,
                slug,
            },
            ShopStaffAction::Remove {
                parcel_id,
                slug,
                username,
            } => AppRequest::ShopStaffRemove {
                current_view,
                parcel_id,
                slug,
                username,
            },
        },
        ShopAction::Shift { action } => match action {
            ShopShiftAction::Start { parcel_id, slug } => AppRequest::ShopShiftStart {
                current_view,
                parcel_id,
                slug,
            },
            ShopShiftAction::End { parcel_id, slug } => AppRequest::ShopShiftEnd {
                current_view,
                parcel_id,
                slug,
            },
        },
        ShopAction::Work { action } => match action {
            ShopWorkAction::List { parcel_id, slug } => AppRequest::ShopWorkList {
                current_view,
                parcel_id,
                slug: slug.as_deref(),
            },
            ShopWorkAction::Claim { parcel_id, work_id } => AppRequest::ShopWorkClaim {
                current_view,
                parcel_id,
                work_id: *work_id,
            },
            ShopWorkAction::Done {
                parcel_id,
                work_id,
                result,
            } => AppRequest::ShopWorkDone {
                current_view,
                parcel_id,
                work_id: *work_id,
                result,
            },
        },
        ShopAction::Badge { action } => match action {
            ShopBadgeAction::List { parcel_id } => AppRequest::ShopBadgeList { parcel_id },
            ShopBadgeAction::Create {
                parcel_id,
                slug,
                title,
                description,
            } => AppRequest::ShopBadgeCreate {
                parcel_id,
                slug,
                title,
                description: description.as_deref(),
            },
            ShopBadgeAction::Award {
                parcel_id,
                slug,
                target,
                note,
            } => AppRequest::ShopBadgeAward {
                parcel_id,
                slug,
                target,
                note: note.as_deref(),
            },
            ShopBadgeAction::Revoke {
                parcel_id,
                slug,
                target,
            } => AppRequest::ShopBadgeRevoke {
                parcel_id,
                slug,
                target,
            },
        },
    }
}

pub(super) fn badge_request(action: &BadgeAction) -> AppRequest<'_> {
    match action {
        BadgeAction::ListMine => AppRequest::BadgesMine,
        BadgeAction::ListUser { target } => AppRequest::BadgesUser { target },
    }
}

pub(super) fn subscription_request(action: &SubscriptionAction) -> AppRequest<'_> {
    match action {
        SubscriptionAction::Subscribe { target, slug } => {
            AppRequest::ShopMailingListSubscribe { target, slug }
        }
        SubscriptionAction::Unsubscribe { target, slug } => {
            AppRequest::ShopMailingListUnsubscribe { target, slug }
        }
        SubscriptionAction::Chat { target, slug, body } => {
            AppRequest::ShopMailingListChat { target, slug, body }
        }
        SubscriptionAction::List => AppRequest::ShopMailingListSubscriptions,
    }
}
