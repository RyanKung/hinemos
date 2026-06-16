//! Connected-session presence tracking for the SSH adapter.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use hinemos_admin_protocol::{AdminSession, AdminUser};
use russh::ChannelId;
use russh::server::Handle;

#[derive(Debug, Default)]
pub(crate) struct PresenceRegistry {
    connections: HashMap<u64, PresenceRecord>,
    pending_kicks: HashSet<u64>,
}

impl PresenceRegistry {
    pub(crate) fn mark_online(
        &mut self,
        connection_id: u64,
        player_id: String,
        user: String,
        current_view: String,
    ) {
        self.connections.insert(
            connection_id,
            PresenceRecord {
                player_id,
                user,
                current_view,
                channel: None,
                connected_at: Instant::now(),
                last_seen_at: Instant::now(),
            },
        );
    }

    pub(crate) fn attach_channel(
        &mut self,
        connection_id: u64,
        handle: Handle,
        channel_id: ChannelId,
        mode: PresenceDeliveryMode,
    ) {
        if let Some(record) = self.connections.get_mut(&connection_id) {
            record.channel = Some(PresenceChannel {
                handle,
                channel_id,
                mode,
            });
        }
    }

    pub(crate) fn update_view(&mut self, connection_id: u64, current_view: String) {
        if let Some(record) = self.connections.get_mut(&connection_id) {
            record.current_view = current_view;
        }
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

    pub(crate) fn session_count(&self) -> usize {
        self.connections.len()
    }

    pub(crate) fn user_count(&self) -> usize {
        self.connections
            .values()
            .map(|record| record.user.as_str())
            .collect::<HashSet<_>>()
            .len()
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

    pub(crate) fn connection_views(&self) -> Vec<(u64, String)> {
        self.connections
            .iter()
            .map(|(&connection_id, record)| (connection_id, record.current_view.clone()))
            .collect()
    }

    pub(crate) fn connection_player_id(&self, connection_id: u64) -> Option<String> {
        self.connections
            .get(&connection_id)
            .map(|record| record.player_id.clone())
    }

    pub(crate) fn admin_users(&self) -> Vec<AdminUser> {
        let mut grouped = HashMap::<String, UserAccumulator>::new();
        for record in self.connections.values() {
            let entry = grouped.entry(record.user.clone()).or_default();
            entry.session_count += 1;
            entry.player_ids.insert(record.player_id.clone());
        }

        let mut users = grouped
            .into_iter()
            .map(|(user, accumulator)| {
                let mut player_ids = accumulator.player_ids.into_iter().collect::<Vec<_>>();
                player_ids.sort();
                AdminUser {
                    user,
                    session_count: accumulator.session_count,
                    player_ids,
                }
            })
            .collect::<Vec<_>>();
        users.sort_by(|left, right| left.user.cmp(&right.user));
        users
    }

    pub(crate) fn view_users(
        &self,
        current_connection_id: u64,
        view_id: &str,
    ) -> Vec<PresenceViewUser> {
        let mut grouped = HashMap::<String, Instant>::new();
        for (connection_id, record) in &self.connections {
            if *connection_id == current_connection_id || record.current_view != view_id {
                continue;
            }
            grouped
                .entry(record.user.clone())
                .and_modify(|last_seen_at| *last_seen_at = (*last_seen_at).max(record.last_seen_at))
                .or_insert(record.last_seen_at);
        }

        let mut users = grouped
            .into_iter()
            .map(|(user, last_seen_at)| PresenceViewUser { user, last_seen_at })
            .collect::<Vec<_>>();
        users.sort_by(|left, right| {
            right
                .last_seen_at
                .cmp(&left.last_seen_at)
                .then_with(|| left.user.cmp(&right.user))
        });
        users
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

    pub(crate) fn direct_recipients(
        &self,
        sender_connection_id: u64,
        target: &str,
    ) -> Vec<PresenceDelivery> {
        self.connections
            .iter()
            .filter(|(connection_id, record)| {
                **connection_id != sender_connection_id
                    && (record.user == target || record.player_id == target)
            })
            .filter_map(|(_, record)| record.delivery())
            .collect()
    }

    pub(crate) fn direct_recipients_in_view(
        &self,
        sender_connection_id: u64,
        target: &str,
        view_id: &str,
    ) -> Vec<PresenceDelivery> {
        self.connections
            .iter()
            .filter(|(connection_id, record)| {
                **connection_id != sender_connection_id
                    && record.current_view == view_id
                    && (record.user == target || record.player_id == target)
            })
            .filter_map(|(_, record)| record.delivery())
            .collect()
    }

    pub(crate) fn view_recipients(
        &self,
        sender_connection_id: u64,
        view_id: &str,
    ) -> Vec<PresenceDelivery> {
        self.connections
            .iter()
            .filter(|(connection_id, record)| {
                **connection_id != sender_connection_id && record.current_view == view_id
            })
            .filter_map(|(_, record)| record.delivery())
            .collect()
    }

    pub(crate) fn broadcast_recipients(&self, sender_connection_id: u64) -> Vec<PresenceDelivery> {
        self.connections
            .iter()
            .filter(|(connection_id, _)| **connection_id != sender_connection_id)
            .filter_map(|(_, record)| record.delivery())
            .collect()
    }
}

#[derive(Debug, Default)]
struct UserAccumulator {
    session_count: usize,
    player_ids: HashSet<String>,
}

#[derive(Debug)]
struct PresenceRecord {
    player_id: String,
    user: String,
    current_view: String,
    channel: Option<PresenceChannel>,
    connected_at: Instant,
    last_seen_at: Instant,
}

impl PresenceRecord {
    fn delivery(&self) -> Option<PresenceDelivery> {
        self.channel.as_ref().map(|channel| PresenceDelivery {
            handle: channel.handle.clone(),
            channel_id: channel.channel_id,
            mode: channel.mode,
        })
    }
}

#[derive(Debug, Clone)]
struct PresenceChannel {
    handle: Handle,
    channel_id: ChannelId,
    mode: PresenceDeliveryMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresenceDeliveryMode {
    Shell,
    Mailbox,
}

#[derive(Debug, Clone)]
pub(crate) struct PresenceDelivery {
    pub(crate) handle: Handle,
    pub(crate) channel_id: ChannelId,
    pub(crate) mode: PresenceDeliveryMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresenceViewUser {
    pub(crate) user: String,
    last_seen_at: Instant,
}

#[cfg(test)]
mod tests {
    use super::PresenceRegistry;

    #[test]
    fn admin_users_group_sessions_by_user() {
        let mut presence = PresenceRegistry::default();
        presence.mark_online(
            1,
            "player_a".to_owned(),
            "alice".to_owned(),
            "view_a".to_owned(),
        );
        presence.mark_online(
            2,
            "player_b".to_owned(),
            "alice".to_owned(),
            "view_a".to_owned(),
        );
        presence.mark_online(
            3,
            "player_c".to_owned(),
            "bob".to_owned(),
            "view_b".to_owned(),
        );

        let users = presence.admin_users();

        assert_eq!(users.len(), 2);
        assert_eq!(users[0].user, "alice");
        assert_eq!(users[0].session_count, 2);
        assert_eq!(
            users[0].player_ids,
            vec!["player_a".to_owned(), "player_b".to_owned()]
        );
        assert_eq!(users[1].user, "bob");
        assert_eq!(users[1].session_count, 1);
    }
}
