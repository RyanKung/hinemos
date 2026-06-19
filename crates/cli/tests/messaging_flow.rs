mod common;

use std::sync::{Mutex, MutexGuard};

static MESSAGING_FLOW_LOCK: Mutex<()> = Mutex::new(());

fn serial_messaging_flow() -> MutexGuard<'static, ()> {
    MESSAGING_FLOW_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[path = "messaging_flow/early.rs"]
mod messaging_flow_early;
#[path = "messaging_flow/late.rs"]
mod messaging_flow_late;
