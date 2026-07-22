#![deny(missing_docs)]
#![allow(async_fn_in_trait)]

//! Protocol-neutral application primitives for Hinemos sessions.

mod account;
mod admission;
mod commerce;
mod config;
mod dispatch;
mod events;
mod hunger;
mod identity;
mod inbox;
mod memory;
mod messages;
mod registration;
mod request;
mod resident_loop;
mod rooms;
mod service;
mod state;

#[cfg(test)]
mod tests;

pub use account::*;
pub use admission::*;
pub use commerce::*;
pub use config::*;
pub use dispatch::{AppDispatchStore, AppViewCommandContext};
pub use events::*;
pub use hunger::*;
pub use identity::*;
pub use inbox::*;
pub use memory::*;
pub use messages::*;
pub use registration::*;
pub use request::*;
pub use rooms::*;
pub use service::*;
pub use state::*;

pub(crate) use anyhow::{Context, Result};
#[cfg(test)]
pub(crate) use commerce::render_parcel_list;
pub(crate) use hinemos_core::{
    BadgeAction, BuildAction, BuildSheet, Direction, EntityRef, ExitObservation, FEEDBACK_QUIT,
    Gender, HungerPolicy, HungerSignal, INBOX_STATUS_ACKED, INBOX_STATUS_ARCHIVED, InboxAction,
    JsonObservation, MbtiType, PARCEL_STATUS_BUILT, PARCEL_STATUS_CLAIMED, ParcelAction,
    ParcelBadgeAction, ParcelDeskAction, ParcelJobAction, ParcelMailingListAction,
    ParcelRouteAction, ParcelShiftAction, ParcelStaffAction, ParcelWorkAction, PayAction,
    PlayerState, SemanticCommand, SettingsAction, TaskCommandRecord, TaskMode, TaskSnapshot,
    TaskStepEvaluation, WorldMetadata, WorldState, extension_commands,
    parcel_badge_description_is_valid, parcel_badge_note_is_valid, parcel_badge_slug_is_valid,
    parcel_badge_title_is_valid, parcel_command_route_prefix_is_valid,
    parcel_job_guide_body_is_valid, parcel_job_guide_slug_is_valid,
    parcel_job_guide_title_is_valid, parcel_mailing_list_body_is_valid,
    parcel_mailing_list_slug_is_valid, parcel_mailing_list_subject_is_valid,
    parcel_mailing_list_title_is_valid, parcel_work_desk_slug_is_valid,
    parcel_work_desk_title_is_valid, parcel_work_result_is_valid, role_card_name_is_valid,
};
pub(crate) use inbox::{enabled_label, format_mail_user};
pub(crate) use memory::memory_command_rest;
pub(crate) use messages::{
    render_inventory, render_message_list, render_player_balance, render_who,
};
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::Value;
pub(crate) use std::collections::{HashMap, HashSet};
pub(crate) use std::fs;
pub(crate) use std::future::Future;
pub(crate) use std::path::Path;
pub(crate) use std::pin::Pin;
