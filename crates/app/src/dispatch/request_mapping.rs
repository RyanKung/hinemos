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

pub(super) fn shop_request(action: &ShopAction) -> AppRequest<'_> {
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
    }
}
