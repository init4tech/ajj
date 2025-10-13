use crate::RpcSend;
use serde::Serialize;
use serde_json::value::{to_raw_value, RawValue};
use std::borrow::Cow;
use std::fmt;

const INTERNAL_ERROR: Cow<'_, str> = Cow::Borrowed("Internal error");

/// A JSON-RPC 2.0 response payload.
///
/// This is a thin wrapper around a [`Result`] type containing either
/// the successful payload or an error payload.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct ResponsePayload<Payload, ErrData>(pub Result<Payload, ErrorPayload<ErrData>>);

impl<T, E> From<Result<T, E>> for ResponsePayload<T, E>
where
    E: RpcSend,
{
    fn from(res: Result<T, E>) -> Self {
        match res {
            Ok(payload) => Self(Ok(payload)),
            Err(err) => Self(Err(ErrorPayload::internal_error_with_obj(err))),
        }
    }
}

impl<Payload, ErrData> ResponsePayload<Payload, ErrData> {
    /// Create a new error payload for a parse error.
    pub const fn parse_error() -> Self {
        Self(Err(ErrorPayload::parse_error()))
    }

    /// Create a new error payload for an invalid request.
    pub const fn invalid_request() -> Self {
        Self(Err(ErrorPayload::invalid_request()))
    }

    /// Create a new error payload for a method not found error.
    pub const fn method_not_found() -> Self {
        Self(Err(ErrorPayload::method_not_found()))
    }

    /// Create a new error payload for an invalid params error.
    pub const fn invalid_params() -> Self {
        Self(Err(ErrorPayload::invalid_params()))
    }

    /// Create a new error payload for an internal error.
    pub const fn internal_error() -> Self {
        Self(Err(ErrorPayload::internal_error()))
    }

    /// Create a new error payload for an internal error with a custom message.
    pub const fn internal_error_message(message: Cow<'static, str>) -> Self {
        Self(Err(ErrorPayload::internal_error_message(message)))
    }

    /// Create a new error payload for an internal error with a custom message
    /// and additional data.
    pub const fn internal_error_with_obj(data: ErrData) -> Self
    where
        ErrData: RpcSend,
    {
        Self(Err(ErrorPayload::internal_error_with_obj(data)))
    }

    /// Create a new error payload for an internal error with a custom message
    /// and additional data.
    pub const fn internal_error_with_message_and_obj(
        message: Cow<'static, str>,
        data: ErrData,
    ) -> Self
    where
        ErrData: RpcSend,
    {
        Self(Err(ErrorPayload::internal_error_with_message_and_obj(
            message, data,
        )))
    }

    /// Fallible conversion to the successful payload.
    pub const fn as_success(&self) -> Option<&Payload> {
        match self {
            Self(Ok(payload)) => Some(payload),
            _ => None,
        }
    }

    /// Fallible conversion to the error object.
    pub const fn as_error(&self) -> Option<&ErrorPayload<ErrData>> {
        match self {
            Self(Err(payload)) => Some(payload),
            _ => None,
        }
    }

    /// Returns `true` if the response payload is a success.
    pub const fn is_success(&self) -> bool {
        matches!(self, Self(Ok(_)))
    }

    /// Returns `true` if the response payload is an error.
    pub const fn is_error(&self) -> bool {
        matches!(self, Self(Err(_)))
    }

    /// Convert a result into a response payload, by converting the error into
    /// an internal error message.
    pub fn convert_internal_error_msg<T>(res: Result<Payload, T>) -> Self
    where
        T: Into<Cow<'static, str>>,
    {
        match res {
            Ok(payload) => Self(Ok(payload)),
            Err(err) => Self(Err(ErrorPayload::internal_error_message(err.into()))),
        }
    }
}

/// A JSON-RPC 2.0 error object.
///
/// This response indicates that the server received and handled the request,
/// but that there was an error in the processing of it. The error should be
/// included in the `message` field of the response payload.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ErrorPayload<ErrData = Box<RawValue>> {
    /// The error code.
    pub code: i64,
    /// The error message (if any).
    pub message: Cow<'static, str>,
    /// The error data (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ErrData>,
}

impl<E> ErrorPayload<E> {
    /// Create a new error payload for a parse error.
    pub const fn parse_error() -> Self {
        Self {
            code: -32700,
            message: Cow::Borrowed("Parse error"),
            data: None,
        }
    }

    /// Create a new error payload for an invalid request.
    pub const fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: Cow::Borrowed("Invalid Request"),
            data: None,
        }
    }

    /// Create a new error payload for a method not found error.
    pub const fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: Cow::Borrowed("Method not found"),
            data: None,
        }
    }

    /// Create a new error payload for an invalid params error.
    pub const fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: Cow::Borrowed("Invalid params"),
            data: None,
        }
    }

    /// Create a new error payload for an internal error.
    pub const fn internal_error() -> Self {
        Self {
            code: -32603,
            message: INTERNAL_ERROR,
            data: None,
        }
    }

    /// Create a new error payload for an internal error with a custom message.
    pub const fn internal_error_message(message: Cow<'static, str>) -> Self {
        Self {
            code: -32603,
            message,
            data: None,
        }
    }

    /// Create a new error payload for an internal error with a custom message
    /// and additional data.
    pub const fn internal_error_with_obj(data: E) -> Self
    where
        E: RpcSend,
    {
        Self {
            code: -32603,
            message: INTERNAL_ERROR,
            data: Some(data),
        }
    }

    /// Create a new error payload for an internal error with a custom message
    pub const fn internal_error_with_message_and_obj(message: Cow<'static, str>, data: E) -> Self
    where
        E: RpcSend,
    {
        Self {
            code: -32603,
            message,
            data: Some(data),
        }
    }

    /// Analyzes the [ErrorPayload] and decides if the request should be
    /// retried based on the error code or the message.
    pub fn is_retry_err(&self) -> bool {
        // alchemy throws it this way
        if self.code == 429 {
            return true;
        }

        // This is an infura error code for `exceeded project rate limit`
        if self.code == -32005 {
            return true;
        }

        // alternative alchemy error for specific IPs
        if self.code == -32016 && self.message.contains("rate limit") {
            return true;
        }

        // quick node error `"credits limited to 6000/sec"`
        // <https://github.com/foundry-rs/foundry/pull/6712#issuecomment-1951441240>
        if self.code == -32012 && self.message.contains("credits") {
            return true;
        }

        // quick node rate limit error: `100/second request limit reached - reduce calls per second
        // or upgrade your account at quicknode.com` <https://github.com/foundry-rs/foundry/issues/4894>
        if self.code == -32007 && self.message.contains("request limit reached") {
            return true;
        }

        match self.message.as_ref() {
            // this is commonly thrown by infura and is apparently a load balancer issue, see also <https://github.com/MetaMask/metamask-extension/issues/7234>
            "header not found" => true,
            // also thrown by infura if out of budget for the day and ratelimited
            "daily request count exceeded, request rate limited" => true,
            msg => {
                msg.contains("rate limit")
                    || msg.contains("rate exceeded")
                    || msg.contains("too many requests")
                    || msg.contains("credits limited")
                    || msg.contains("request limit")
            }
        }
    }
}

impl<T> From<T> for ErrorPayload<T>
where
    T: std::error::Error + RpcSend,
{
    fn from(value: T) -> Self {
        Self {
            code: -32603,
            message: INTERNAL_ERROR,
            data: Some(value),
        }
    }
}

impl<E> ErrorPayload<E>
where
    E: RpcSend,
{
    /// Serialize the inner data into a [`RawValue`].
    pub fn serialize_payload(&self) -> serde_json::Result<ErrorPayload> {
        Ok(ErrorPayload {
            code: self.code,
            message: self.message.clone(),
            data: match self.data.as_ref() {
                Some(data) => Some(to_raw_value(data)?),
                None => None,
            },
        })
    }
}

impl<ErrData: fmt::Display> fmt::Display for ErrorPayload<ErrData> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error code {}: {}{}",
            self.code,
            self.message,
            self.data
                .as_ref()
                .map(|data| format!(", data: {}", data))
                .unwrap_or_default()
        )
    }
}
