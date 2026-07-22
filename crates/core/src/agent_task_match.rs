use crate::{
    BadgeAction, BuildAction, InboxAction, ParcelAction, ParcelBadgeAction, ParcelDeskAction,
    ParcelJobAction, ParcelMailingListAction, ParcelRouteAction, ParcelShiftAction,
    ParcelStaffAction, ParcelWorkAction, PayAction, SemanticCommand, SettingsAction,
    extension_command_input_matches_template,
};

/// Returns true when `command` is permitted by an observed command template.
pub(crate) fn command_matches_template(
    command: &SemanticCommand,
    template: &SemanticCommand,
) -> bool {
    match (command, template) {
        (SemanticCommand::Say { text }, SemanticCommand::Say { text: template }) => {
            template_string_matches(text, template)
        }
        (
            SemanticCommand::Mail { target, text },
            SemanticCommand::Mail {
                target: template_target,
                text: template_text,
            },
        ) => {
            template_string_matches(target, template_target)
                && template_string_matches(text, template_text)
        }
        (SemanticCommand::Broadcast { text }, SemanticCommand::Broadcast { text: template }) => {
            template_string_matches(text, template)
        }
        (SemanticCommand::Settings { action }, SemanticCommand::Settings { action: template }) => {
            settings_action_matches(action, template)
        }
        (SemanticCommand::Inbox { action }, SemanticCommand::Inbox { action: template }) => {
            inbox_action_matches(action, template)
        }
        (SemanticCommand::Pay { action }, SemanticCommand::Pay { action: template }) => {
            pay_action_matches(action, template)
        }
        (SemanticCommand::Parcel { action }, SemanticCommand::Parcel { action: template }) => {
            parcel_action_matches(action, template)
        }
        (SemanticCommand::Badges { action }, SemanticCommand::Badges { action: template }) => {
            badge_action_matches(action, template)
        }
        (
            SemanticCommand::Extension { input, .. },
            SemanticCommand::Extension {
                input: template, ..
            },
        ) => extension_command_input_matches_template(template, input),
        (SemanticCommand::Memory { rest }, SemanticCommand::Memory { rest: template }) => {
            template_string_matches(rest, template)
        }
        (SemanticCommand::Agree { phrase }, SemanticCommand::Agree { phrase: template }) => {
            template_string_matches(phrase, template)
        }
        _ => command == template,
    }
}

fn template_string_matches(value: &str, template: &str) -> bool {
    template_string_is_wildcard(template) || value == template
}

fn option_string_matches(value: Option<&str>, template: Option<&str>) -> bool {
    match (value, template) {
        (_, None) => true,
        (Some(value), Some(template)) => template_string_matches(value, template),
        (None, Some(template)) => template_string_is_wildcard(template),
    }
}

fn template_string_is_wildcard(template: &str) -> bool {
    let template = template.trim();
    template.is_empty() || (template.starts_with('<') && template.ends_with('>'))
}

fn template_i64_matches(value: i64, template: i64) -> bool {
    template == 0 || value == template
}

fn optional_template_string_matches(value: Option<&str>, template: Option<&str>) -> bool {
    match template {
        None => value.is_none(),
        Some(template) if template_string_is_wildcard(template) => true,
        Some(template) => value == Some(template),
    }
}

fn settings_action_matches(action: &SettingsAction, template: &SettingsAction) -> bool {
    match (action, template) {
        (SettingsAction::Show, SettingsAction::Show)
        | (SettingsAction::MailToken, SettingsAction::MailToken) => true,
        (SettingsAction::Name { name }, SettingsAction::Name { name: template }) => {
            template_string_matches(name, template)
        }
        (SettingsAction::Gender { gender }, SettingsAction::Gender { gender: template }) => {
            gender == template
        }
        (SettingsAction::Mbti { mbti }, SettingsAction::Mbti { mbti: template }) => {
            mbti == template
        }
        (SettingsAction::Intro { intro }, SettingsAction::Intro { intro: template }) => {
            optional_template_string_matches(intro.as_deref(), template.as_deref())
        }
        _ => false,
    }
}

fn inbox_action_matches(action: &InboxAction, template: &InboxAction) -> bool {
    match (action, template) {
        (InboxAction::List { filter }, InboxAction::List { filter: template }) => {
            template_string_matches(filter, template)
        }
        (InboxAction::Read { item_id }, InboxAction::Read { item_id: template })
        | (InboxAction::Claim { item_id }, InboxAction::Claim { item_id: template })
        | (InboxAction::Ack { item_id }, InboxAction::Ack { item_id: template })
        | (InboxAction::Archive { item_id }, InboxAction::Archive { item_id: template }) => {
            template_i64_matches(*item_id, *template)
        }
        _ => false,
    }
}

fn pay_action_matches(action: &PayAction, template: &PayAction) -> bool {
    match (action, template) {
        (PayAction::Requests, PayAction::Requests) => true,
        (
            PayAction::Accept { request_id },
            PayAction::Accept {
                request_id: template,
            },
        ) => template_i64_matches(*request_id, *template),
        (
            PayAction::Direct {
                target,
                amount,
                memo,
            },
            PayAction::Direct {
                target: template_target,
                amount: template_amount,
                memo: template_memo,
            },
        ) => {
            template_string_matches(target, template_target)
                && template_i64_matches(*amount, *template_amount)
                && template_string_matches(memo, template_memo)
        }
        _ => false,
    }
}

fn parcel_action_matches(action: &ParcelAction, template: &ParcelAction) -> bool {
    match (action, template) {
        (ParcelAction::List, ParcelAction::List) => true,
        (
            ParcelAction::Info { parcel_id },
            ParcelAction::Info {
                parcel_id: template,
            },
        )
        | (
            ParcelAction::Claim { parcel_id },
            ParcelAction::Claim {
                parcel_id: template,
            },
        )
        | (
            ParcelAction::Token { parcel_id },
            ParcelAction::Token {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        (
            ParcelAction::Transfer { parcel_id, target },
            ParcelAction::Transfer {
                parcel_id: template_parcel,
                target: template_target,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(target, template_target)
        }
        (ParcelAction::Build { action }, ParcelAction::Build { action: template }) => {
            build_action_matches(action, template)
        }
        (ParcelAction::Inbox, ParcelAction::Inbox) => true,
        (
            ParcelAction::RequestPayment {
                command_id, amount, ..
            },
            ParcelAction::RequestPayment {
                command_id: template_command,
                amount: template_amount,
                ..
            },
        ) => {
            template_i64_matches(*command_id, *template_command)
                && template_i64_matches(*amount, *template_amount)
        }
        (ParcelAction::MailingList { action }, ParcelAction::MailingList { action: template }) => {
            parcel_mailing_list_action_matches(action, template)
        }
        (ParcelAction::Desk { action }, ParcelAction::Desk { action: template }) => {
            parcel_desk_action_matches(action, template)
        }
        (ParcelAction::Job { action }, ParcelAction::Job { action: template }) => {
            parcel_job_action_matches(action, template)
        }
        (ParcelAction::Route { action }, ParcelAction::Route { action: template }) => {
            parcel_route_action_matches(action, template)
        }
        (ParcelAction::Staff { action }, ParcelAction::Staff { action: template }) => {
            parcel_staff_action_matches(action, template)
        }
        (ParcelAction::Shift { action }, ParcelAction::Shift { action: template }) => {
            parcel_shift_action_matches(action, template)
        }
        (ParcelAction::Work { action }, ParcelAction::Work { action: template }) => {
            parcel_work_action_matches(action, template)
        }
        (ParcelAction::Badge { action }, ParcelAction::Badge { action: template }) => {
            parcel_badge_action_matches(action, template)
        }
        (ParcelAction::Subscriptions, ParcelAction::Subscriptions) => true,
        (
            ParcelAction::Subscribe { target, slug },
            ParcelAction::Subscribe {
                target: template_target,
                slug: template_slug,
            },
        )
        | (
            ParcelAction::Unsubscribe { target, slug },
            ParcelAction::Unsubscribe {
                target: template_target,
                slug: template_slug,
            },
        )
        | (
            ParcelAction::Chat { target, slug, .. },
            ParcelAction::Chat {
                target: template_target,
                slug: template_slug,
                ..
            },
        ) => {
            template_string_matches(target, template_target)
                && template_string_matches(slug, template_slug)
        }
        _ => false,
    }
}

fn build_action_matches(action: &BuildAction, template: &BuildAction) -> bool {
    match (action, template) {
        (BuildAction::Help, BuildAction::Help) | (BuildAction::Publish, BuildAction::Publish) => {
            true
        }
        (
            BuildAction::Set { field, .. },
            BuildAction::Set {
                field: template, ..
            },
        ) => template_string_matches(field, template),
        (BuildAction::Apply { .. }, BuildAction::Apply { .. }) => true,
        _ => false,
    }
}

fn parcel_mailing_list_action_matches(
    action: &ParcelMailingListAction,
    template: &ParcelMailingListAction,
) -> bool {
    match (action, template) {
        (
            ParcelMailingListAction::Create {
                parcel_id, slug, ..
            },
            ParcelMailingListAction::Create {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ParcelMailingListAction::Send {
                parcel_id, slug, ..
            },
            ParcelMailingListAction::Send {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ParcelMailingListAction::Subscribers { parcel_id, slug },
            ParcelMailingListAction::Subscribers {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        )
        | (
            ParcelMailingListAction::Close { parcel_id, slug },
            ParcelMailingListAction::Close {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        (
            ParcelMailingListAction::List { parcel_id },
            ParcelMailingListAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn parcel_desk_action_matches(action: &ParcelDeskAction, template: &ParcelDeskAction) -> bool {
    match (action, template) {
        (
            ParcelDeskAction::Create {
                parcel_id, slug, ..
            },
            ParcelDeskAction::Create {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        (
            ParcelDeskAction::List { parcel_id },
            ParcelDeskAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn parcel_job_action_matches(action: &ParcelJobAction, template: &ParcelJobAction) -> bool {
    match (action, template) {
        (
            ParcelJobAction::Publish {
                parcel_id, slug, ..
            },
            ParcelJobAction::Publish {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ParcelJobAction::Read { parcel_id, slug },
            ParcelJobAction::Read {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        (
            ParcelJobAction::List { parcel_id },
            ParcelJobAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn parcel_route_action_matches(action: &ParcelRouteAction, template: &ParcelRouteAction) -> bool {
    match (action, template) {
        (
            ParcelRouteAction::Add {
                parcel_id,
                slug,
                command_prefix,
            },
            ParcelRouteAction::Add {
                parcel_id: template_parcel,
                slug: template_slug,
                command_prefix: template_prefix,
            },
        )
        | (
            ParcelRouteAction::Remove {
                parcel_id,
                slug,
                command_prefix,
            },
            ParcelRouteAction::Remove {
                parcel_id: template_parcel,
                slug: template_slug,
                command_prefix: template_prefix,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
                && template_string_matches(command_prefix, template_prefix)
        }
        (
            ParcelRouteAction::List { parcel_id },
            ParcelRouteAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn parcel_staff_action_matches(action: &ParcelStaffAction, template: &ParcelStaffAction) -> bool {
    match (action, template) {
        (
            ParcelStaffAction::Add {
                parcel_id,
                slug,
                username,
            },
            ParcelStaffAction::Add {
                parcel_id: template_parcel,
                slug: template_slug,
                username: template_user,
            },
        )
        | (
            ParcelStaffAction::Remove {
                parcel_id,
                slug,
                username,
            },
            ParcelStaffAction::Remove {
                parcel_id: template_parcel,
                slug: template_slug,
                username: template_user,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
                && template_string_matches(username, template_user)
        }
        (
            ParcelStaffAction::List { parcel_id, slug },
            ParcelStaffAction::List {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        _ => false,
    }
}

fn parcel_shift_action_matches(action: &ParcelShiftAction, template: &ParcelShiftAction) -> bool {
    match (action, template) {
        (
            ParcelShiftAction::Start { parcel_id, slug },
            ParcelShiftAction::Start {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        )
        | (
            ParcelShiftAction::End { parcel_id, slug },
            ParcelShiftAction::End {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        _ => false,
    }
}

fn parcel_work_action_matches(action: &ParcelWorkAction, template: &ParcelWorkAction) -> bool {
    match (action, template) {
        (
            ParcelWorkAction::List { parcel_id, slug },
            ParcelWorkAction::List {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && option_string_matches(slug.as_deref(), template_slug.as_deref())
        }
        (
            ParcelWorkAction::Claim { parcel_id, work_id },
            ParcelWorkAction::Claim {
                parcel_id: template_parcel,
                work_id: template_id,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_i64_matches(*work_id, *template_id)
        }
        (
            ParcelWorkAction::Done {
                parcel_id, work_id, ..
            },
            ParcelWorkAction::Done {
                parcel_id: template_parcel,
                work_id: template_id,
                ..
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_i64_matches(*work_id, *template_id)
        }
        _ => false,
    }
}

fn parcel_badge_action_matches(action: &ParcelBadgeAction, template: &ParcelBadgeAction) -> bool {
    match (action, template) {
        (
            ParcelBadgeAction::List { parcel_id },
            ParcelBadgeAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        (
            ParcelBadgeAction::Create {
                parcel_id, slug, ..
            },
            ParcelBadgeAction::Create {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ParcelBadgeAction::Award {
                parcel_id, slug, ..
            },
            ParcelBadgeAction::Award {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ParcelBadgeAction::Revoke {
                parcel_id, slug, ..
            },
            ParcelBadgeAction::Revoke {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        _ => false,
    }
}

fn badge_action_matches(action: &BadgeAction, template: &BadgeAction) -> bool {
    match (action, template) {
        (BadgeAction::ListMine, BadgeAction::ListMine) => true,
        (BadgeAction::ListUser { target }, BadgeAction::ListUser { target: template }) => {
            template_string_matches(target, template)
        }
        _ => false,
    }
}
