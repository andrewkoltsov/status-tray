#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected HTTP status {0}")]
    Status(u16),
    #[error("invalid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid catalog: {0}")]
    Catalog(String),
}

pub type Result<T> = std::result::Result<T, Error>;
