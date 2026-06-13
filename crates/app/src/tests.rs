use super::*;
use hinemos_core::{ADMISSION_STATE_AGREED, ADMISSION_STATE_PENDING, JsonObservation};
use std::{fs, sync::Mutex};

mod fixtures_basic;
mod fixtures_rooms;

use fixtures_basic::*;
use fixtures_rooms::*;

mod admission;
mod commerce;
mod config;
mod memory;
mod registration;
mod rooms;
mod state;
