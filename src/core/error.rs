use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResonaError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid JDF header: {0}")]
    InvalidJdfHeader(String),

    #[error("Processed data error: {0}")]
    ProcessedDataNotSupported(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Parsing error: {0}")]
    ParseError(String),

    #[error("Signal processing error: {0}")]
    ProcessingError(String),

    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, ResonaError>;
