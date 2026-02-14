use crate::{ErrorPayload, ResponsePayload, RpcSend};
use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::value::RawValue;
use std::{borrow::Cow, cell::Cell};

/// Response struct. The payload is held in a [`Cell`] so that the
/// [`Serialize`] impl can consume it via [`ResponsePayload::into_raw`].
pub(crate) struct Response<'b, T, E> {
    id: &'b RawValue,
    payload: Cell<Option<ResponsePayload<T, E>>>,
}

impl<'b, T, E> Response<'b, T, E> {
    const fn new(id: &'b RawValue, payload: ResponsePayload<T, E>) -> Self {
        Self {
            id,
            payload: Cell::new(Some(payload)),
        }
    }
}

impl Response<'_, (), ()> {
    /// Parse error response, used when the request is not valid JSON.
    pub(crate) fn parse_error() -> Box<RawValue> {
        Response::new(RawValue::NULL, ResponsePayload::<(), ()>::parse_error()).to_json()
    }

    /// Method not found response, used in default fallback handler.
    pub(crate) fn method_not_found(id: &RawValue) -> Box<RawValue> {
        Response::new(id, ResponsePayload::<(), ()>::method_not_found()).to_json()
    }

    /// Method not found response, used in default fallback handler. This
    /// function exists to simplify notification responses, which should be
    /// omitted.
    pub(crate) fn maybe_method_not_found(id: Option<&RawValue>) -> Option<Box<RawValue>> {
        id.map(Self::method_not_found)
    }

    /// Response failed to serialize. This bypasses [`Self::to_json`] to
    /// avoid recursion, and uses [`serde_json::value::to_raw_value`]
    /// directly.
    pub(crate) fn serialization_failure(id: &RawValue) -> Box<RawValue> {
        let resp = Response::new(
            id,
            ResponsePayload::<(), ()>(Err(ErrorPayload {
                code: -32700,
                message: Cow::Borrowed("response serialization error"),
                data: None,
            })),
        );
        serde_json::value::to_raw_value(&resp).expect("serialization_failure is infallible")
    }

    /// Build a JSON-RPC response from an id and a payload.
    ///
    /// The payload is consumed inside the [`Serialize`] impl via
    /// [`ResponsePayload::into_raw`], which calls
    /// [`RpcSend::into_raw_value`] on the result and error data. The
    /// pre-serialized [`Box<RawValue>`] values are then embedded directly
    /// into the JSON-RPC envelope without re-serialization.
    pub(crate) fn build_response<T, E>(
        id: Option<&RawValue>,
        payload: ResponsePayload<T, E>,
    ) -> Option<Box<RawValue>>
    where
        T: RpcSend,
        E: RpcSend,
    {
        let id = id?;
        Some(Response::new(id, payload).to_json())
    }
}

impl<T: RpcSend, E: RpcSend> Response<'_, T, E> {
    fn to_json(&self) -> Box<RawValue> {
        serde_json::value::to_raw_value(self).unwrap_or_else(|err| {
            tracing::debug!(%err, id = ?self.id, "failed to serialize response");
            Response::serialization_failure(self.id)
        })
    }
}

impl<T: RpcSend, E: RpcSend> Serialize for Response<'_, T, E> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let payload = self
            .payload
            .take()
            .ok_or_else(|| serde::ser::Error::custom("payload already consumed"))?;
        let raw = payload.into_raw().map_err(serde::ser::Error::custom)?;

        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("jsonrpc", "2.0")?;
        map.serialize_entry("id", &self.id)?;
        match raw.0 {
            Ok(result) => {
                map.serialize_entry("result", &result)?;
            }
            Err(error) => {
                map.serialize_entry("error", &error)?;
            }
        }
        map.end()
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
