use crate::{
    BadgeAction, BuildAction, InboxAction, LandAction, PayAction, SemanticCommand, SettingsAction,
    ShopAction, ShopBadgeAction, ShopDeskAction, ShopMailingListAction, ShopRouteAction,
    ShopShiftAction, ShopStaffAction, ShopWorkAction, SubscriptionAction,
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
        (SemanticCommand::Land { action }, SemanticCommand::Land { action: template }) => {
            land_action_matches(action, template)
        }
        (SemanticCommand::Build { action }, SemanticCommand::Build { action: template }) => {
            build_action_matches(action, template)
        }
        (SemanticCommand::Shop { action }, SemanticCommand::Shop { action: template }) => {
            shop_action_matches(action, template)
        }
        (SemanticCommand::Badges { action }, SemanticCommand::Badges { action: template }) => {
            badge_action_matches(action, template)
        }
        (
            SemanticCommand::Subscription { action },
            SemanticCommand::Subscription { action: template },
        ) => subscription_action_matches(action, template),
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

fn land_action_matches(action: &LandAction, template: &LandAction) -> bool {
    match (action, template) {
        (LandAction::List, LandAction::List) => true,
        (
            LandAction::Info { parcel_id },
            LandAction::Info {
                parcel_id: template,
            },
        )
        | (
            LandAction::Claim { parcel_id },
            LandAction::Claim {
                parcel_id: template,
            },
        )
        | (
            LandAction::Token { parcel_id },
            LandAction::Token {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        (
            LandAction::Transfer { parcel_id, target },
            LandAction::Transfer {
                parcel_id: template_parcel,
                target: template_target,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(target, template_target)
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

fn shop_action_matches(action: &ShopAction, template: &ShopAction) -> bool {
    match (action, template) {
        (ShopAction::Inbox, ShopAction::Inbox) => true,
        (
            ShopAction::RequestPayment {
                command_id, amount, ..
            },
            ShopAction::RequestPayment {
                command_id: template_command,
                amount: template_amount,
                ..
            },
        ) => {
            template_i64_matches(*command_id, *template_command)
                && template_i64_matches(*amount, *template_amount)
        }
        (ShopAction::MailingList { action }, ShopAction::MailingList { action: template }) => {
            shop_mailing_list_action_matches(action, template)
        }
        (ShopAction::Desk { action }, ShopAction::Desk { action: template }) => {
            shop_desk_action_matches(action, template)
        }
        (ShopAction::Route { action }, ShopAction::Route { action: template }) => {
            shop_route_action_matches(action, template)
        }
        (ShopAction::Staff { action }, ShopAction::Staff { action: template }) => {
            shop_staff_action_matches(action, template)
        }
        (ShopAction::Shift { action }, ShopAction::Shift { action: template }) => {
            shop_shift_action_matches(action, template)
        }
        (ShopAction::Work { action }, ShopAction::Work { action: template }) => {
            shop_work_action_matches(action, template)
        }
        (ShopAction::Badge { action }, ShopAction::Badge { action: template }) => {
            shop_badge_action_matches(action, template)
        }
        _ => false,
    }
}

fn shop_mailing_list_action_matches(
    action: &ShopMailingListAction,
    template: &ShopMailingListAction,
) -> bool {
    match (action, template) {
        (
            ShopMailingListAction::Create {
                parcel_id, slug, ..
            },
            ShopMailingListAction::Create {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ShopMailingListAction::Send {
                parcel_id, slug, ..
            },
            ShopMailingListAction::Send {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ShopMailingListAction::Subscribers { parcel_id, slug },
            ShopMailingListAction::Subscribers {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        )
        | (
            ShopMailingListAction::Close { parcel_id, slug },
            ShopMailingListAction::Close {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        (
            ShopMailingListAction::List { parcel_id },
            ShopMailingListAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn shop_desk_action_matches(action: &ShopDeskAction, template: &ShopDeskAction) -> bool {
    match (action, template) {
        (
            ShopDeskAction::Create {
                parcel_id, slug, ..
            },
            ShopDeskAction::Create {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_string_matches(slug, template_slug)
        }
        (
            ShopDeskAction::List { parcel_id },
            ShopDeskAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn shop_route_action_matches(action: &ShopRouteAction, template: &ShopRouteAction) -> bool {
    match (action, template) {
        (
            ShopRouteAction::Add {
                parcel_id,
                slug,
                command_prefix,
            },
            ShopRouteAction::Add {
                parcel_id: template_parcel,
                slug: template_slug,
                command_prefix: template_prefix,
            },
        )
        | (
            ShopRouteAction::Remove {
                parcel_id,
                slug,
                command_prefix,
            },
            ShopRouteAction::Remove {
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
            ShopRouteAction::List { parcel_id },
            ShopRouteAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        _ => false,
    }
}

fn shop_staff_action_matches(action: &ShopStaffAction, template: &ShopStaffAction) -> bool {
    match (action, template) {
        (
            ShopStaffAction::Add {
                parcel_id,
                slug,
                username,
            },
            ShopStaffAction::Add {
                parcel_id: template_parcel,
                slug: template_slug,
                username: template_user,
            },
        )
        | (
            ShopStaffAction::Remove {
                parcel_id,
                slug,
                username,
            },
            ShopStaffAction::Remove {
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
            ShopStaffAction::List { parcel_id, slug },
            ShopStaffAction::List {
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

fn shop_shift_action_matches(action: &ShopShiftAction, template: &ShopShiftAction) -> bool {
    match (action, template) {
        (
            ShopShiftAction::Start { parcel_id, slug },
            ShopShiftAction::Start {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        )
        | (
            ShopShiftAction::End { parcel_id, slug },
            ShopShiftAction::End {
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

fn shop_work_action_matches(action: &ShopWorkAction, template: &ShopWorkAction) -> bool {
    match (action, template) {
        (
            ShopWorkAction::List { parcel_id, slug },
            ShopWorkAction::List {
                parcel_id: template_parcel,
                slug: template_slug,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && option_string_matches(slug.as_deref(), template_slug.as_deref())
        }
        (
            ShopWorkAction::Claim { parcel_id, work_id },
            ShopWorkAction::Claim {
                parcel_id: template_parcel,
                work_id: template_id,
            },
        ) => {
            template_string_matches(parcel_id, template_parcel)
                && template_i64_matches(*work_id, *template_id)
        }
        (
            ShopWorkAction::Done {
                parcel_id, work_id, ..
            },
            ShopWorkAction::Done {
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

fn shop_badge_action_matches(action: &ShopBadgeAction, template: &ShopBadgeAction) -> bool {
    match (action, template) {
        (
            ShopBadgeAction::List { parcel_id },
            ShopBadgeAction::List {
                parcel_id: template,
            },
        ) => template_string_matches(parcel_id, template),
        (
            ShopBadgeAction::Create {
                parcel_id, slug, ..
            },
            ShopBadgeAction::Create {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ShopBadgeAction::Award {
                parcel_id, slug, ..
            },
            ShopBadgeAction::Award {
                parcel_id: template_parcel,
                slug: template_slug,
                ..
            },
        )
        | (
            ShopBadgeAction::Revoke {
                parcel_id, slug, ..
            },
            ShopBadgeAction::Revoke {
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

fn subscription_action_matches(action: &SubscriptionAction, template: &SubscriptionAction) -> bool {
    match (action, template) {
        (SubscriptionAction::List, SubscriptionAction::List) => true,
        (
            SubscriptionAction::Subscribe { target, slug },
            SubscriptionAction::Subscribe {
                target: template_target,
                slug: template_slug,
            },
        )
        | (
            SubscriptionAction::Unsubscribe { target, slug },
            SubscriptionAction::Unsubscribe {
                target: template_target,
                slug: template_slug,
            },
        )
        | (
            SubscriptionAction::Chat { target, slug, .. },
            SubscriptionAction::Chat {
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
