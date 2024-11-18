#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// There is no client with the given ClientId.
    #[error("No such client")]
    NoSuchClient,

    #[error(transparent)]
    Other(#[from] anyhow::Error), // source and Display delegate to anyhow::Error
}
