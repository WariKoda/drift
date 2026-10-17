#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{challenge}: {cause}")]
    Certificate {
        challenge: Box<crate::tlstrust::Challenge>,
        cause: String,
    },
    #[error("connection lost: {0}")]
    Connection(String),
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
impl Error {
    /// Preserve terminal connection causes when close/cleanup also fails.
    pub(crate) fn join(first: Self, rest: impl IntoIterator<Item = Self>) -> Self {
        let mut errors = vec![first];
        errors.extend(rest);
        if errors.len() == 1 {
            return errors.pop().unwrap();
        }
        if let Some(challenge) = errors.iter().find_map(|error| match error {
            Self::Certificate { challenge, .. } => Some(challenge.clone()),
            _ => None,
        }) {
            return Self::Certificate {
                challenge,
                cause: errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; "),
            };
        }
        let terminal = errors
            .iter()
            .any(|error| matches!(error, Self::Connection(_)));
        let message = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        if terminal {
            Self::Connection(message)
        } else {
            Self::Invalid(message)
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;
