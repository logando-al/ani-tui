use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("Player error: {0}")]
    Player(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;
