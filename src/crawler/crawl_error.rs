#[derive(Debug, thiserror::Error)]
pub enum CrawlError {
    #[error("HTTP Error Status Code = {0}")]
    HttpError(u16),

    #[error(transparent)]
    AnyError(#[from] anyhow::Error),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error(transparent)]
    MimeParseError(#[from] mime::FromStrError),
}