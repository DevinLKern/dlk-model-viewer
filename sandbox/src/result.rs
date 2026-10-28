#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration file: {0}")]
    ConfigFileInvalid(&'static str),

    #[error("invalid window ID")]
    WindowIdInvalid,
}
