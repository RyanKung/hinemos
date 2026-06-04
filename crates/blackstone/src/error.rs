//! Error types for the Blackstone Izakaya extension.

use hinemos_storage::StorageError;
use thiserror::Error;

/// Blackstone service error.
#[derive(Debug, Error)]
pub enum BlackstoneError {
    /// Storage failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// SQL failed.
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    /// Missing command argument.
    #[error("missing command argument")]
    MissingArgument,
    /// Unknown command.
    #[error("unknown command")]
    UnknownCommand,
}
