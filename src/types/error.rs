/// Error when deserializing a request
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    /// Invalid UTF-8
    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] core::str::Utf8Error),
    /// Invalid JSON
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// Id is too large
    ///
    /// The limit is 80 bytes. 80 is selected as a reasonable limit for
    /// most use-cases, and will hold UUIDs as well as 0x-prefixed 256-bit
    /// hashes encoded as hex. If you need to send a large id, consider
    /// not doing that.
    #[error("Id is too large, limit of 80 bytes. Got: {0}")]
    IdTooLarge(usize),

    /// Method is not a valid JSON string.
    #[error("Method is not a valid JSON string.")]
    InvalidMethod,

    /// Method is too large
    ///
    /// The limit is 80 bytes. 80 is selected as a reasonable limit for
    /// most use-cases. If you need to send a large method name, consider
    /// not doing that.
    #[error("Method is too large, limit of 80 bytes. Got: {0}")]
    MethodTooLarge(usize),
}
