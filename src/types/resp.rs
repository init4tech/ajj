use crate::RpcSend;
use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::value::{to_raw_value, RawValue};
use std::borrow::Cow;
use std::fmt;

const INTERNAL_ERROR: Cow<'_, str> = Cow::Borrowed("Internal error");

/// Response struct.
#[derive(Debug, Clone)]
pub(crate) struct Response<'a, 'b, T, E> {
    pub(crate) id: &'b RawValue,
    pub(crate) payload: &'a ResponsePayload<T, E>,
}

impl Response<'_, '_, (), ()> {
    /// Parse error response, used when the request is not valid JSON.
    #[allow(dead_code)] // used in features
    pub(crate) fn parse_error() -> Box<RawValue> {
        Response::<(), ()> {
            id: RawValue::NULL,
            payload: &ResponsePayload::parse_error(),
        }
        .to_json()
    }

    /// Invalid params response, used when the params field does not
    /// deserialize into the expected type.
    pub(crate) fn invalid_params(id: &RawValue) -> Box<RawValue> {
        Response::<(), ()> {
            id,
            payload: &ResponsePayload::invalid_params(),
        }
        .to_json()
    }

    /// Invalid params response, used when the params field does not
    /// deserialize into the expected type. This function exists to simplify
    /// notification responses, which should be omitted.
    pub(crate) fn maybe_invalid_params(id: Option<&RawValue>) -> Option<Box<RawValue>> {
        id.map(|id| Self::invalid_params(id))
    }

    /// Method not found response, used in default fallback handler.
    pub(crate) fn method_not_found(id: &RawValue) -> Box<RawValue> {
        Response::<(), ()> {
            id,
            payload: &ResponsePayload::method_not_found(),
        }
        .to_json()
    }

    /// Method not found response, used in default fallback handler. This
    /// function exists to simplify notification responses, which should be
    /// omitted.
    pub(crate) fn maybe_method_not_found(id: Option<&RawValue>) -> Option<Box<RawValue>> {
        id.map(|id| Self::method_not_found(id))
    }

    /// Response failed to serialize
    pub(crate) fn serialization_failure(id: &RawValue) -> Box<RawValue> {
        RawValue::from_string(format!(
            r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32700,"message":"response serialization error"}}}}"#,
            id.get()
        ))
        .expect("valid json")
    }
}

impl<'a, 'b, T, E> Response<'a, 'b, T, E>
where
    T: Serialize,
    E: Serialize,
{
    pub(crate) fn maybe(
        id: Option<&'b RawValue>,
        payload: &'a ResponsePayload<T, E>,
    ) -> Option<Box<RawValue>> {
        id.map(|id| Self { id, payload }.to_json())
    }

    pub(crate) fn to_json(&self) -> Box<RawValue> {
        serde_json::value::to_raw_value(self).unwrap_or_else(|err| {
            tracing::debug!(%err, id = ?self.id, "failed to serialize response");
            Response::serialization_failure(self.id)
        })
    }
}

impl<T, E> Serialize for Response<'_, '_, T, E>
where
    T: Serialize,
    E: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("jsonrpc", "2.0")?;
        map.serialize_entry("id", &self.id)?;
        match &self.payload {
            ResponsePayload::Success(result) => {
                map.serialize_entry("result", result)?;
            }
            ResponsePayload::Failure(error) => {
                map.serialize_entry("error", error)?;
            }
        }
        map.end()
    }
}

/// A JSON-RPC 2.0 response payload.
///
/// This enum covers both the success and error cases of a JSON-RPC 2.0
/// response. It is used to represent the `result` and `error` fields of a
/// response object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponsePayload<Payload, ErrData> {
    /// A successful response payload.
    Success(Payload),
    /// An error response payload.
    Failure(ErrorPayload<ErrData>),
}

impl<Payload, ErrData> ResponsePayload<Payload, ErrData> {
    /// Create a new error payload for a parse error.
    pub const fn parse_error() -> Self {
        Self::Failure(ErrorPayload::parse_error())
    }

    /// Create a new error payload for an invalid request.
    pub const fn invalid_request() -> Self {
        Self::Failure(ErrorPayload::invalid_request())
    }

    /// Create a new error payload for a method not found error.
    pub const fn method_not_found() -> Self {
        Self::Failure(ErrorPayload::method_not_found())
    }

    /// Create a new error payload for an invalid params error.
    pub const fn invalid_params() -> Self {
        Self::Failure(ErrorPayload::invalid_params())
    }

    /// Create a new error payload for an internal error.
    pub const fn internal_error() -> Self {
        Self::Failure(ErrorPayload::internal_error())
    }

    /// Create a new error payload for an internal error with a custom message.
    pub const fn internal_error_message(message: Cow<'static, str>) -> Self {
        Self::Failure(ErrorPayload::internal_error_message(message))
    }

    /// Create a new error payload for an internal error with a custom message
    /// and additional data.
    pub const fn internal_error_with_obj(data: ErrData) -> Self
    where
        ErrData: RpcSend,
    {
        Self::Failure(ErrorPayload::internal_error_with_obj(data))
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
        Self::Failure(ErrorPayload::internal_error_with_message_and_obj(
            message, data,
        ))
    }

    /// Fallible conversion to the successful payload.
    pub const fn as_success(&self) -> Option<&Payload> {
        match self {
            Self::Success(payload) => Some(payload),
            _ => None,
        }
    }

    /// Fallible conversion to the error object.
    pub const fn as_error(&self) -> Option<&ErrorPayload<ErrData>> {
        match self {
            Self::Failure(payload) => Some(payload),
            _ => None,
        }
    }

    /// Returns `true` if the response payload is a success.
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Returns `true` if the response payload is an error.
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Failure(_))
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::assert_rv_eq;

    #[test]
    fn ser_failure() {
        let id = RawValue::from_string("1".to_string()).unwrap();
        let res = Response::<(), ()>::serialization_failure(&id);
        assert_rv_eq(
            &res,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32700,"message":"response serialization error"}}"#,
        );
    }
}

// Some code is this file is reproduced under the terms of the MIT license. It
// originates from the `alloy` crate. The original source code can be found at
// the following URL, and the original license is included below.
//
// https://github.com/alloy-rs/alloy
//
// The MIT License (MIT)
//
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.
