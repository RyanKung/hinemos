//! Stable identifiers used by the world model.

use serde::{Deserialize, Serialize};

/// Stable identifier for a view node in the world graph.
pub type ViewId = String;

/// Stable identifier for an entity.
pub type EntityId = String;

/// Stable identifier for a player session.
pub type PlayerId = String;

/// Stable key used to look up localized text.
pub type TextKey = String;

/// Creates an owned identifier from a static string.
pub fn id(value: &str) -> String {
    value.to_owned()
}

/// A serializable pair of canonical id and localized label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedId {
    /// Stable canonical id.
    pub id: String,
    /// Localized display label.
    pub label: String,
}
