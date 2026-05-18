//! Connected-session presence tracking for the SSH adapter.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use xagora_admin_protocol::AdminSession;

#[derive(Debug, Default)]
pub(crate) struct PresenceRegistry {
    connections: HashMap<u64, PresenceRecord>,
    pending_kicks: HashSet<u64>,
}

impl PresenceRegistry {
    pub(crate) fn mark_online(&mut self, connection_id: u64, player_id: String, user: String) {
        self.connections.insert(
            connection_id,
            PresenceRecord {
                player_id,
                user,
                connected_at: Instant::now(),
                last_seen_at: Instant::now(),
            },
        );
    }

    pub(crate) fn touch(&mut self, connection_id: u64) {
        if let Some(record) = self.connections.get_mut(&connection_id) {
            let _session_age = record.connected_at.elapsed();
            record.last_seen_at = Instant::now();
        }
    }

    pub(crate) fn remove(&mut self, connection_id: u64) {
        self.connections.remove(&connection_id);
        self.pending_kicks.remove(&connection_id);
    }

    pub(crate) fn online_count_for_player(&self, player_id: &str) -> usize {
        self.connections
            .values()
            .filter(|record| record.player_id == player_id)
            .count()
    }

    pub(crate) fn users(&self) -> Vec<&str> {
        self.connections
            .values()
            .map(|record| record.user.as_str())
            .collect()
    }

    pub(crate) fn admin_sessions(&self) -> Vec<AdminSession> {
        self.connections
            .iter()
            .map(|(&connection_id, record)| AdminSession {
                connection_id,
                player_id: record.player_id.clone(),
                user: record.user.clone(),
            })
            .collect()
    }

    pub(crate) fn request_kick(&mut self, connection_id: u64) -> bool {
        if self.connections.contains_key(&connection_id) {
            self.pending_kicks.insert(connection_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn poll_kick(&mut self, connection_id: u64) -> bool {
        self.pending_kicks.remove(&connection_id)
    }
}

#[derive(Debug)]
struct PresenceRecord {
    player_id: String,
    user: String,
    connected_at: Instant,
    last_seen_at: Instant,
}
