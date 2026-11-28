use thiserror::Error;

#[derive(Debug, Error)]
pub enum MhchemError {
    #[error("mhchem: {0}")]
    Msg(String),
    #[error("extra close brace or missing open brace")]
    ExtraClose,
}

impl MhchemError {
    pub fn msg(s: impl Into<String>) -> Self {
        MhchemError::Msg(s.into())
    }
}

pub type MhchemResult<T> = Result<T, MhchemError>;
