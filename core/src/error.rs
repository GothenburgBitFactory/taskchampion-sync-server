/// An error from the [`crate::Server`] type.
///
/// This type represents only circumstances outside the realm of the protocol, and not the specific
/// results descriebd in the protocol documentation.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// There is no client with the given ClientId.
    #[error("No such client")]
    NoSuchClient,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
