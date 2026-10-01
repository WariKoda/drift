#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0} changed in another drift process; reload before saving")]
    Conflict(String),
    #[error("configuration is being changed by another drift process; retry")]
    Busy,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Decode(#[from] toml::de::Error),
    #[error(transparent)]
    Encode(#[from] toml::ser::Error),
    #[error(transparent)]
    Mapping(#[from] crate::pathmap::MappingError),
}
pub type Result<T> = std::result::Result<T, Error>;
