//! Stable identifiers used in the world model.

/// Stable identifier for a view node in the world graph.
pub type ViewId = String;

/// Stable identifier for an entity.
pub type EntityId = String;

/// Stable identifier for a player session.
pub type PlayerId = String;

/// Creates an owned identifier from a static string.
pub fn id(value: &str) -> String {
    value.to_owned()
}
