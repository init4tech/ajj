use crate::{ErrorPayload, RpcSend};
use std::borrow::Cow;

/// A trait for converting an error type into a JSON-RPC 2.0
/// [`ErrorPayload`].
///
/// By default, handler errors in `ajj` are mapped to JSON-RPC error code
/// `-32603` ("Internal error"). Implement this trait on your error types to
/// control the error code, message, and optional structured data that
/// appear in the response.
///
/// # Provided methods
///
/// Only [`error_code`] is required. The remaining methods have sensible
/// defaults:
///
/// | Method               | Default                                |
/// |----------------------|----------------------------------------|
/// | [`error_message`]    | `"Internal error"`                     |
/// | [`error_data`]       | `None`                                 |
/// | [`into_error_payload`] | Assembles from the other three methods |
///
/// Override [`into_error_payload`] when you can produce the
/// [`ErrorPayload`] more efficiently than calling each accessor
/// individually.
///
/// # Example
///
/// ```
/// use ajj::{IntoErrorPayload, ErrorPayload};
/// use std::borrow::Cow;
///
/// #[derive(Debug)]
/// enum AppError {
///     NotFound(String),
///     RateLimited { retry_after: u64 },
/// }
///
/// impl IntoErrorPayload for AppError {
///     type ErrData = String;
///
///     fn error_code(&self) -> i64 {
///         match self {
///             Self::NotFound(_) => 404,
///             Self::RateLimited { .. } => 429,
///         }
///     }
///
///     fn error_message(&self) -> Cow<'static, str> {
///         match self {
///             Self::NotFound(_) => "Not found".into(),
///             Self::RateLimited { .. } => "Rate limited".into(),
///         }
///     }
///
///     fn error_data(self) -> Option<String> {
///         match self {
///             Self::NotFound(resource) => Some(resource),
///             Self::RateLimited { retry_after } => {
///                 Some(format!("retry after {retry_after}s"))
///             }
///         }
///     }
/// }
///
/// let err = AppError::NotFound("user/42".into());
/// let payload = err.into_error_payload();
/// assert_eq!(payload.code, 404);
/// assert_eq!(payload.message, "Not found");
/// assert_eq!(payload.data.as_deref(), Some("user/42"));
/// ```
///
/// [`error_code`]: IntoErrorPayload::error_code
/// [`error_message`]: IntoErrorPayload::error_message
/// [`error_data`]: IntoErrorPayload::error_data
/// [`into_error_payload`]: IntoErrorPayload::into_error_payload
pub trait IntoErrorPayload {
    /// The type of structured data included in the error response.
    type ErrData: RpcSend;

    /// The JSON-RPC error code.
    fn error_code(&self) -> i64;

    /// A short human-readable description of the error.
    ///
    /// Defaults to `"Internal error"`.
    fn error_message(&self) -> Cow<'static, str> {
        Cow::Borrowed("Internal error")
    }

    /// Optional structured data to include in the error response.
    ///
    /// Defaults to `None`.
    fn error_data(self) -> Option<Self::ErrData>
    where
        Self: Sized,
    {
        None
    }

    /// Consume this value and produce an [`ErrorPayload`].
    ///
    /// The default implementation calls [`error_code`], [`error_message`],
    /// and [`error_data`].
    ///
    /// [`error_code`]: IntoErrorPayload::error_code
    /// [`error_message`]: IntoErrorPayload::error_message
    /// [`error_data`]: IntoErrorPayload::error_data
    fn into_error_payload(self) -> ErrorPayload<Self::ErrData>
    where
        Self: Sized,
    {
        ErrorPayload {
            code: self.error_code(),
            message: self.error_message(),
            data: self.error_data(),
        }
    }
}

/// A wrapper that maps any `T: RpcSend` into a JSON-RPC "Internal error"
/// (`-32603`) with `T` as the error data.
///
/// `InternalError` implements [`From<T>`] for ergonomic use with the `?`
/// operator: any error type can be converted into an `InternalError`
/// automatically.
///
/// # Example
///
/// ```
/// use ajj::{InternalError, IntoErrorPayload};
///
/// let err = InternalError("something went wrong");
/// let payload = err.into_error_payload();
/// assert_eq!(payload.code, -32603);
/// assert_eq!(payload.message, "Internal error");
/// assert_eq!(payload.data, Some("something went wrong"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InternalError<T>(pub T);

impl<T> From<T> for InternalError<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T: RpcSend> IntoErrorPayload for InternalError<T> {
    type ErrData = T;

    fn error_code(&self) -> i64 {
        -32603
    }

    fn error_data(self) -> Option<T> {
        Some(self.0)
    }
}

impl<E: RpcSend> IntoErrorPayload for ErrorPayload<E> {
    type ErrData = E;

    fn error_code(&self) -> i64 {
        self.code
    }

    fn error_message(&self) -> Cow<'static, str> {
        self.message.clone()
    }

    fn error_data(self) -> Option<E> {
        self.data
    }

    fn into_error_payload(self) -> ErrorPayload<E> {
        self
    }
}

impl IntoErrorPayload for String {
    type ErrData = ();

    fn error_code(&self) -> i64 {
        -32603
    }

    fn error_message(&self) -> Cow<'static, str> {
        Cow::Owned(self.clone())
    }

    fn into_error_payload(self) -> ErrorPayload<()> {
        ErrorPayload {
            code: -32603,
            message: Cow::Owned(self),
            data: None,
        }
    }
}

impl IntoErrorPayload for &'static str {
    type ErrData = ();

    fn error_code(&self) -> i64 {
        -32603
    }

    fn error_message(&self) -> Cow<'static, str> {
        Cow::Borrowed(self)
    }

    fn into_error_payload(self) -> ErrorPayload<()> {
        ErrorPayload {
            code: -32603,
            message: Cow::Borrowed(self),
            data: None,
        }
    }
}

impl IntoErrorPayload for () {
    type ErrData = ();

    fn error_code(&self) -> i64 {
        -32603
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_error_wrapper() {
        let err = InternalError(42u64);
        assert_eq!(err.error_code(), -32603);
        assert_eq!(err.error_message(), "Internal error");

        let payload = err.into_error_payload();
        assert_eq!(payload.code, -32603);
        assert_eq!(payload.message, "Internal error");
        assert_eq!(payload.data, Some(42u64));
    }

    #[test]
    fn internal_error_from() {
        let err: InternalError<u64> = 42u64.into();
        assert_eq!(err.0, 42u64);
    }

    #[test]
    fn error_payload_identity() {
        let original = ErrorPayload {
            code: 404,
            message: Cow::Borrowed("Not found"),
            data: Some("missing".to_string()),
        };
        assert_eq!(original.error_code(), 404);
        assert_eq!(original.error_message(), "Not found");

        let payload = original.into_error_payload();
        assert_eq!(payload.code, 404);
        assert_eq!(payload.message, "Not found");
        assert_eq!(payload.data.as_deref(), Some("missing"));
    }

    #[test]
    fn error_payload_no_data() {
        let original: ErrorPayload<()> = ErrorPayload {
            code: -32603,
            message: Cow::Borrowed("Internal error"),
            data: None,
        };
        let payload = original.into_error_payload();
        assert_eq!(payload.code, -32603);
        assert_eq!(payload.data, None);
    }

    #[test]
    fn string_error() {
        let err = "bad input".to_string();
        assert_eq!(err.error_code(), -32603);
        assert_eq!(err.error_message(), "bad input");

        let err2 = "bad input".to_string();
        let payload = err2.into_error_payload();
        assert_eq!(payload.code, -32603);
        assert_eq!(payload.message, "bad input");
        assert_eq!(payload.data, None);
    }

    #[test]
    fn static_str_error() {
        let err: &'static str = "oops";
        assert_eq!(err.error_code(), -32603);
        assert_eq!(err.error_message(), "oops");

        let payload = err.into_error_payload();
        assert_eq!(payload.code, -32603);
        assert_eq!(payload.message, "oops");
        assert_eq!(payload.data, None);
    }

    #[test]
    fn unit_error() {
        let err = ();
        assert_eq!(err.error_code(), -32603);
        assert_eq!(err.error_message(), "Internal error");

        let payload = err.into_error_payload();
        assert_eq!(payload.code, -32603);
        assert_eq!(payload.message, "Internal error");
        assert_eq!(payload.data, None);
    }

    #[test]
    fn custom_error_type() {
        #[derive(Debug)]
        enum AppError {
            NotFound(String),
            Internal,
        }

        impl IntoErrorPayload for AppError {
            type ErrData = String;

            fn error_code(&self) -> i64 {
                match self {
                    Self::NotFound(_) => 404,
                    Self::Internal => -32603,
                }
            }

            fn error_message(&self) -> Cow<'static, str> {
                match self {
                    Self::NotFound(_) => "Not found".into(),
                    Self::Internal => "Internal error".into(),
                }
            }

            fn error_data(self) -> Option<String> {
                match self {
                    Self::NotFound(resource) => Some(resource),
                    Self::Internal => None,
                }
            }
        }

        let err = AppError::NotFound("user/42".into());
        let payload = err.into_error_payload();
        assert_eq!(payload.code, 404);
        assert_eq!(payload.message, "Not found");
        assert_eq!(payload.data.as_deref(), Some("user/42"));

        let err = AppError::Internal;
        let payload = err.into_error_payload();
        assert_eq!(payload.code, -32603);
        assert_eq!(payload.message, "Internal error");
        assert_eq!(payload.data, None);
    }
}
